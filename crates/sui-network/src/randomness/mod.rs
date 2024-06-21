// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use self::{auth::AllowedPeersUpdatable, metrics::Metrics};
use anemo::PeerId;
use anyhow::Result;
use fastcrypto::groups::bls12381;
use fastcrypto_tbls::{
    dkg, nodes::PartyId, tbls::ThresholdBls, types::ShareIndex, types::ThresholdBls12381MinSig,
};
use mysten_metrics::spawn_monitored_task;
use mysten_network::anemo_ext::NetworkExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::{btree_map::BTreeMap, BTreeSet, HashMap},
    ops::Bound,
    sync::Arc,
    time::{self, Duration},
};
use sui_config::p2p::RandomnessConfig;
use sui_macros::fail_point_if;
use sui_types::{
    base_types::AuthorityName,
    committee::EpochId,
    crypto::{RandomnessPartialSignature, RandomnessRound, RandomnessSignature},
};
use tokio::sync::mpsc;
use tracing::{debug, error, info, instrument, warn};

mod auth;
mod builder;
mod generated {
    include!(concat!(env!("OUT_DIR"), "/sui.Randomness.rs"));
}
mod metrics;
mod server;
#[cfg(test)]
mod tests;

pub use builder::{Builder, UnstartedRandomness};
pub use generated::{
    randomness_client::RandomnessClient,
    randomness_server::{Randomness, RandomnessServer},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendSignaturesRequest {
    epoch: EpochId,
    round: RandomnessRound,
    // BCS-serialized `RandomnessPartialSignature` values. We store raw bytes here to enable
    // defenses against too-large messages.
    // The protocol requires the signatures to be ordered by share index (as provided by fastcrypto).
    partial_sigs: Vec<Vec<u8>>,
    // TODO: add support for receiving full signature from validators who have already
    // reconstructed it.
    sig: Option<RandomnessSignature>,
}

/// A handle to the Randomness network subsystem.
///
/// This handle can be cloned and shared. Once all copies of a Randomness system's Handle have been
/// dropped, the Randomness system will be gracefully shutdown.
#[derive(Clone, Debug)]
pub struct Handle {
    sender: mpsc::Sender<RandomnessMessage>,
}

impl Handle {
    /// Transitions the Randomness system to a new epoch. Cancels all partial signature sends for
    /// prior epochs.
    pub fn update_epoch(
        &self,
        new_epoch: EpochId,
        authority_info: HashMap<AuthorityName, (PeerId, PartyId)>,
        dkg_output: dkg::Output<bls12381::G2Element, bls12381::G2Element>,
        aggregation_threshold: u16,
        recovered_last_completed_round: Option<RandomnessRound>, // set to None if not starting up mid-epoch
    ) {
        self.sender
            .try_send(RandomnessMessage::UpdateEpoch(
                new_epoch,
                authority_info,
                dkg_output,
                aggregation_threshold,
                recovered_last_completed_round,
            ))
            .expect("RandomnessEventLoop mailbox should not overflow or be closed")
    }

    /// Begins transmitting partial signatures for the given epoch and round until completed.
    pub fn send_partial_signatures(&self, epoch: EpochId, round: RandomnessRound) {
        self.sender
            .try_send(RandomnessMessage::SendPartialSignatures(epoch, round))
            .expect("RandomnessEventLoop mailbox should not overflow or be closed")
    }

    /// Records the given round as complete, stopping any partial signature sends.
    pub fn complete_round(&self, epoch: EpochId, round: RandomnessRound) {
        self.sender
            .try_send(RandomnessMessage::CompleteRound(epoch, round))
            .expect("RandomnessEventLoop mailbox should not overflow or be closed")
    }

    // For testing.
    pub fn new_stub() -> Self {
        let (sender, mut receiver) = mpsc::channel(1);
        // Keep receiver open until all senders are closed.
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    m = receiver.recv() => {
                        if m.is_none() {
                            break;
                        }
                    },
                }
            }
        });
        Self { sender }
    }
}

#[derive(Clone, Debug)]
enum RandomnessMessage {
    UpdateEpoch(
        EpochId,
        HashMap<AuthorityName, (PeerId, PartyId)>,
        dkg::Output<bls12381::G2Element, bls12381::G2Element>,
        u16,                     // aggregation_threshold
        Option<RandomnessRound>, // recovered_highest_completed_round
    ),
    SendPartialSignatures(EpochId, RandomnessRound),
    CompleteRound(EpochId, RandomnessRound),
    ReceivePartialSignatures(PeerId, EpochId, RandomnessRound, Vec<Vec<u8>>),
}

struct RandomnessEventLoop {
    name: AuthorityName,
    config: RandomnessConfig,
    mailbox: mpsc::Receiver<RandomnessMessage>,
    network: anemo::Network,
    allowed_peers: AllowedPeersUpdatable,
    metrics: Metrics,
    randomness_tx: mpsc::Sender<(EpochId, RandomnessRound, Vec<u8>)>,

    epoch: EpochId,
    authority_info: Arc<HashMap<AuthorityName, (PeerId, PartyId)>>,
    peer_share_ids: Option<HashMap<PeerId, Vec<ShareIndex>>>,
    dkg_output: Option<dkg::Output<bls12381::G2Element, bls12381::G2Element>>,
    aggregation_threshold: u16,
    highest_requested_round: BTreeMap<EpochId, RandomnessRound>,
    send_tasks: BTreeMap<RandomnessRound, tokio::task::JoinHandle<()>>,
    round_request_time: BTreeMap<(EpochId, RandomnessRound), time::Instant>,
    future_epoch_partial_sigs: BTreeMap<(EpochId, RandomnessRound, PeerId), Vec<Vec<u8>>>,
    received_partial_sigs: BTreeMap<(RandomnessRound, PeerId), Vec<RandomnessPartialSignature>>,
    completed_sigs: BTreeSet<(EpochId, RandomnessRound)>,
    highest_completed_round: BTreeMap<EpochId, RandomnessRound>,
}

impl RandomnessEventLoop {
    pub async fn start(mut self) {
        info!("Randomness network event loop started");

        loop {
            tokio::select! {
                maybe_message = self.mailbox.recv() => {
                    // Once all handles to our mailbox have been dropped this
                    // will yield `None` and we can terminate the event loop.
                    if let Some(message) = maybe_message {
                        self.handle_message(message);
                    } else {
                        break;
                    }
                },
            }
        }

        info!("Randomness network event loop ended");
    }

    fn handle_message(&mut self, message: RandomnessMessage) {
        match message {
            RandomnessMessage::UpdateEpoch(
                epoch,
                authority_info,
                dkg_output,
                aggregation_threshold,
                recovered_highest_completed_round,
            ) => {
                if let Err(e) = self.update_epoch(
                    epoch,
                    authority_info,
                    dkg_output,
                    aggregation_threshold,
                    recovered_highest_completed_round,
                ) {
                    error!("BUG: failed to update epoch in RandomnessEventLoop: {e:?}");
                }
            }
            RandomnessMessage::SendPartialSignatures(epoch, round) => {
                self.send_partial_signatures(epoch, round)
            }
            RandomnessMessage::CompleteRound(epoch, round) => self.complete_round(epoch, round),
            RandomnessMessage::ReceivePartialSignatures(peer_id, epoch, round, sigs) => {
                self.receive_partial_signatures(peer_id, epoch, round, sigs)
            }
        }
    }

    #[instrument(level = "debug", skip_all, fields(?new_epoch))]
    fn update_epoch(
        &mut self,
        new_epoch: EpochId,
        authority_info: HashMap<AuthorityName, (PeerId, PartyId)>,
        dkg_output: dkg::Output<bls12381::G2Element, bls12381::G2Element>,
        aggregation_threshold: u16,
        recovered_highest_completed_round: Option<RandomnessRound>,
    ) -> Result<()> {
        assert!(self.dkg_output.is_none() || new_epoch > self.epoch);

        debug!("updating randomness network loop to new epoch");

        self.peer_share_ids = Some(authority_info.iter().try_fold(
            HashMap::new(),
            |mut acc, (_name, (peer_id, party_id))| -> Result<_> {
                let ids = dkg_output
                    .nodes
                    .share_ids_of(*party_id)
                    .expect("party_id should be valid");
                acc.insert(*peer_id, ids);
                Ok(acc)
            },
        )?);
        self.allowed_peers.update(Arc::new(
            authority_info
                .values()
                .map(|(peer_id, _)| *peer_id)
                .collect(),
        ));
        self.epoch = new_epoch;
        self.authority_info = Arc::new(authority_info);
        self.dkg_output = Some(dkg_output);
        self.aggregation_threshold = aggregation_threshold;
        if let Some(round) = recovered_highest_completed_round {
            self.highest_completed_round
                .entry(new_epoch)
                .and_modify(|r| *r = std::cmp::max(*r, round))
                .or_insert(round);
        }
        for (_, task) in std::mem::take(&mut self.send_tasks) {
            task.abort();
        }
        self.metrics.set_epoch(new_epoch);

        // Throw away info from old epochs.
        self.highest_requested_round = self.highest_requested_round.split_off(&new_epoch);
        self.round_request_time = self
            .round_request_time
            .split_off(&(new_epoch, RandomnessRound(0)));
        self.received_partial_sigs.clear();
        self.completed_sigs = self
            .completed_sigs
            .split_off(&(new_epoch, RandomnessRound(0)));
        self.highest_completed_round = self.highest_completed_round.split_off(&new_epoch);

        // Start any pending tasks for the new epoch.
        self.maybe_start_pending_tasks();

        // Aggregate any sigs received early from the new epoch.
        // (We can't call `maybe_aggregate_partial_signatures` directly while iterating,
        // because it takes `&mut self`, so we store in a Vec first.)
        for ((epoch, round, peer_id), sig_bytes) in
            std::mem::take(&mut self.future_epoch_partial_sigs)
        {
            // We can fully validate these now that we have current epoch DKG output.
            self.receive_partial_signatures(peer_id, epoch, round, sig_bytes);
        }
        let rounds_to_aggregate: Vec<_> =
            self.received_partial_sigs.keys().map(|(r, _)| *r).collect();
        for round in rounds_to_aggregate {
            self.maybe_aggregate_partial_signatures(new_epoch, round);
        }

        Ok(())
    }

    #[instrument(level = "debug", skip_all, fields(?epoch, ?round))]
    fn send_partial_signatures(&mut self, epoch: EpochId, round: RandomnessRound) {
        if epoch < self.epoch {
            error!(
                "BUG: skipping sending partial sigs, we are already up to epoch {}",
                self.epoch
            );
            debug_assert!(
                false,
                "skipping sending partial sigs, we are already up to higher epoch"
            );
            return;
        }
        if epoch == self.epoch {
            if let Some(highest_completed_round) = self.highest_completed_round.get(&epoch) {
                if round <= *highest_completed_round {
                    info!("skipping sending partial sigs, we already have completed this round");
                    return;
                }
            }
        }

        self.highest_requested_round
            .entry(epoch)
            .and_modify(|r| *r = std::cmp::max(*r, round))
            .or_insert(round);
        self.round_request_time
            .insert((epoch, round), time::Instant::now());
        self.maybe_start_pending_tasks();
    }

    #[instrument(level = "debug", skip_all, fields(?epoch, ?round))]
    fn complete_round(&mut self, epoch: EpochId, round: RandomnessRound) {
        debug!("completing randomness round");
        let new_highest_round = *self
            .highest_completed_round
            .entry(epoch)
            .and_modify(|r| *r = std::cmp::max(*r, round))
            .or_insert(round);
        if round != new_highest_round {
            // This round completion came out of order, and we're already ahead. Nothing more
            // to do in that case.
            return;
        }

        self.round_request_time = self.round_request_time.split_off(&(epoch, round + 1));

        if epoch == self.epoch {
            self.remove_partial_sigs_in_range((
                Bound::Included((RandomnessRound(0), PeerId([0; 32]))),
                Bound::Excluded((round + 1, PeerId([0; 32]))),
            ));
            for (_, task) in self.send_tasks.iter().take_while(|(r, _)| **r <= round) {
                task.abort();
            }
            self.send_tasks = self.send_tasks.split_off(&(round + 1));
            self.maybe_start_pending_tasks();
        }

        self.update_rounds_pending_metric();
    }

    #[instrument(level = "debug", skip_all, fields(?peer_id, ?epoch, ?round))]
    fn receive_partial_signatures(
        &mut self,
        peer_id: PeerId,
        epoch: EpochId,
        round: RandomnessRound,
        sig_bytes: Vec<Vec<u8>>,
    ) {
        // Basic validity checks.
        if epoch < self.epoch {
            debug!(
                "skipping received partial sigs, we are already up to epoch {}",
                self.epoch
            );
            return;
        }
        if epoch > self.epoch + 1 {
            debug!(
                "skipping received partial sigs, we are still on epoch {}",
                self.epoch
            );
            return;
        }
        if self.completed_sigs.contains(&(epoch, round)) {
            debug!("skipping received partial sigs, we already have completed this sig");
            return;
        }
        let highest_completed_round = self.highest_completed_round.get(&epoch).copied();
        if let Some(highest_completed_round) = &highest_completed_round {
            if *highest_completed_round >= round {
                debug!("skipping received partial sigs, we already have completed this round");
                return;
            }
        }

        // If sigs are for a future epoch, we can't fully verify them without DKG output.
        // Save them for later use.
        if epoch != self.epoch || self.peer_share_ids.is_none() {
            if round.0 >= self.config.max_partial_sigs_rounds_ahead() {
                debug!("skipping received partial sigs for future epoch, round too far ahead",);
                return;
            }

            debug!("saving partial sigs from future epoch for later use");
            self.future_epoch_partial_sigs
                .insert((epoch, round, peer_id), sig_bytes);
            return;
        }

        // Verify shape of sigs matches what we expect for the peer.
        let peer_share_ids = self.peer_share_ids.as_ref().expect("checked above");
        let expected_share_ids = if let Some(expected_share_ids) = peer_share_ids.get(&peer_id) {
            expected_share_ids
        } else {
            debug!("received partial sigs from unknown peer");
            return;
        };
        if sig_bytes.len() != expected_share_ids.len() as usize {
            warn!(
                "received partial sigs with wrong share ids count: expected {}, got {}",
                expected_share_ids.len(),
                sig_bytes.len(),
            );
            return;
        }

        // Accept partial signatures up to `max_partial_sigs_rounds_ahead` past the round of the
        // last completed signature, or the highest completed round, whichever is greater.
        let last_completed_signature = self
            .completed_sigs
            .range(..&(epoch + 1, RandomnessRound(0)))
            .next_back()
            .map(|(e, r)| if *e == epoch { *r } else { RandomnessRound(0) });
        let last_completed_round = std::cmp::max(last_completed_signature, highest_completed_round)
            .unwrap_or(RandomnessRound(0));
        if round.0
            >= last_completed_round
                .0
                .saturating_add(self.config.max_partial_sigs_rounds_ahead())
        {
            debug!(
                "skipping received partial sigs, most recent round we completed was only {last_completed_round}",
            );
            return;
        }

        // Deserialize the partial sigs.
        let partial_sigs =
            match sig_bytes
                .iter()
                .try_fold(Vec::new(), |mut acc, bytes| -> Result<_> {
                    let sig: RandomnessPartialSignature = bcs::from_bytes(bytes)?;
                    acc.push(sig);
                    Ok(acc)
                }) {
                Ok(partial_sigs) => partial_sigs,
                Err(e) => {
                    warn!("failed to deserialize partial sigs: {e:?}");
                    return;
                }
            };
        // Verify we received the expected share IDs (to protect against a validator that sends
        // valid signatures of other peers which will be successfully verified below).
        let received_share_ids = partial_sigs.iter().map(|s| s.index);
        if received_share_ids
            .zip(expected_share_ids.iter())
            .any(|(a, b)| a != *b)
        {
            let received_share_ids = partial_sigs.iter().map(|s| s.index).collect::<Vec<_>>();
            warn!("received partial sigs with wrong share ids: expected {expected_share_ids:?}, received {received_share_ids:?}");
            return;
        }

        // We passed all the checks, save the partial sigs.
        debug!("recording received partial signatures");
        self.received_partial_sigs
            .insert((round, peer_id), partial_sigs);

        self.maybe_aggregate_partial_signatures(epoch, round);
    }

    #[instrument(level = "debug", skip_all, fields(?epoch, ?round))]
    fn maybe_aggregate_partial_signatures(&mut self, epoch: EpochId, round: RandomnessRound) {
        if self.completed_sigs.contains(&(epoch, round)) {
            info!("skipping aggregation for already-completed signature");
            return;
        }

        if let Some(highest_completed_round) = self.highest_completed_round.get(&epoch) {
            if round <= *highest_completed_round {
                info!("skipping aggregation for already-completed round");
                return;
            }
        }

        let highest_requested_round = self.highest_requested_round.get(&epoch);
        if highest_requested_round.is_none() || round > *highest_requested_round.unwrap() {
            // We have to wait here, because even if we have enough information from other nodes
            // to complete the signature, local shared object versions are not set until consensus
            // finishes processing the corresponding commit. This function will be called again
            // after maybe_start_pending_tasks begins this round locally.
            debug!("waiting to aggregate randomness partial signatures until local consensus catches up");
            return;
        }

        if epoch != self.epoch {
            debug!(
                "waiting to aggregate randomness partial signatures until DKG completes for epoch"
            );
            return;
        }

        let vss_pk = {
            let Some(dkg_output) = &self.dkg_output else {
                debug!("called maybe_aggregate_partial_signatures before DKG completed");
                return;
            };
            &dkg_output.vss_pk
        };

        let sig_bounds = (
            Bound::Included((round, PeerId([0; 32]))),
            Bound::Excluded((round + 1, PeerId([0; 32]))),
        );

        // If we have enough partial signatures, aggregate them.
        let sig_range = self
            .received_partial_sigs
            .range(sig_bounds)
            .flat_map(|(_, sigs)| sigs);
        let mut sig =
            match ThresholdBls12381MinSig::aggregate(self.aggregation_threshold, sig_range) {
                Ok(sig) => sig,
                Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => return, // wait for more input
                Err(e) => {
                    error!("error while aggregating randomness partial signatures: {e:?}");
                    return;
                }
            };

        // Try to verify the aggregated signature all at once. (Should work in the happy path.)
        if ThresholdBls12381MinSig::verify(vss_pk.c0(), &round.signature_message(), &sig).is_err() {
            // If verifiation fails, some of the inputs must be invalid. We have to go through
            // one-by-one to find which.
            // TODO: add test for individual sig verification.
            self.received_partial_sigs
                .retain(|&(r, peer_id), partial_sigs| {
                    if round != r {
                        return true;
                    }
                    if ThresholdBls12381MinSig::partial_verify_batch(
                        vss_pk,
                        &round.signature_message(),
                        partial_sigs.iter(),
                        &mut rand::thread_rng(),
                    )
                    .is_err()
                    {
                        warn!(
                            "received invalid partial signatures from possibly-Byzantine peer {peer_id}"
                        );
                        // TODO: Ignore future messages from peers sending bad signatures.
                        return false;
                    }
                    true
                });
            let sig_range = self
                .received_partial_sigs
                .range(sig_bounds)
                .flat_map(|(_, sigs)| sigs);
            sig = match ThresholdBls12381MinSig::aggregate(self.aggregation_threshold, sig_range) {
                Ok(sig) => sig,
                Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => return, // wait for more input
                Err(e) => {
                    error!("error while aggregating randomness partial signatures: {e:?}");
                    return;
                }
            };
            if let Err(e) =
                ThresholdBls12381MinSig::verify(vss_pk.c0(), &round.signature_message(), &sig)
            {
                error!("error while verifying randomness partial signatures after removing invalid partials: {e:?}");
                debug_assert!(false, "error while verifying randomness partial signatures after removing invalid partials");
                return;
            }
        }

        debug!("successfully generated randomness full signature");
        self.completed_sigs.insert((epoch, round));
        self.remove_partial_sigs_in_range(sig_bounds);
        self.metrics.record_completed_round(round);
        if let Some(start_time) = self.round_request_time.get(&(epoch, round)) {
            if let Some(metric) = self.metrics.round_generation_latency_metric() {
                metric.observe(start_time.elapsed().as_secs_f64());
            }
        }

        let bytes = bcs::to_bytes(&sig).expect("signature serialization should not fail");
        self.randomness_tx
            .try_send((epoch, round, bytes))
            .expect("RandomnessRoundReceiver mailbox should not overflow or be closed");
    }

    fn maybe_start_pending_tasks(&mut self) {
        let dkg_output = if let Some(dkg_output) = &self.dkg_output {
            dkg_output
        } else {
            return; // wait for DKG
        };
        let shares = if let Some(shares) = &dkg_output.shares {
            shares
        } else {
            return; // can't participate in randomness generation without shares
        };
        let highest_requested_round =
            if let Some(highest_requested_round) = self.highest_requested_round.get(&self.epoch) {
                highest_requested_round
            } else {
                return; // no rounds to start
            };
        // Begin from the next round after the most recent one we've started (or, if none are running,
        // after the highest completed round in the epoch).
        let start_round = std::cmp::max(
            if let Some(highest_completed_round) = self.highest_completed_round.get(&self.epoch) {
                highest_completed_round.checked_add(1).unwrap()
            } else {
                RandomnessRound(0)
            },
            self.send_tasks
                .last_key_value()
                .map(|(r, _)| r.checked_add(1).unwrap())
                .unwrap_or(RandomnessRound(0)),
        );

        let mut rounds_to_aggregate = Vec::new();
        for round in start_round.0..=highest_requested_round.0 {
            let round = RandomnessRound(round);

            if self.send_tasks.len() >= self.config.max_partial_sigs_concurrent_sends() {
                break; // limit concurrent tasks
            }

            self.send_tasks.entry(round).or_insert_with(|| {
                let name = self.name;
                let network = self.network.clone();
                let retry_interval = self.config.partial_signature_retry_interval();
                let metrics = self.metrics.clone();
                let authority_info = self.authority_info.clone();
                let epoch = self.epoch;
                let partial_sigs = ThresholdBls12381MinSig::partial_sign_batch(
                    shares.iter(),
                    &round.signature_message(),
                );

                // Record own partial sigs.
                if !self.completed_sigs.contains(&(epoch, round)) {
                    self.received_partial_sigs
                        .insert((round, self.network.peer_id()), partial_sigs.clone());
                    rounds_to_aggregate.push((epoch, round));
                }

                debug!("sending partial sigs for epoch {epoch}, round {round}");
                spawn_monitored_task!(RandomnessEventLoop::send_partial_signatures_task(
                    name,
                    network,
                    retry_interval,
                    metrics,
                    authority_info,
                    epoch,
                    round,
                    partial_sigs
                ))
            });
        }

        self.update_rounds_pending_metric();

        // After starting a round, we have generated our own partial sigs. Check if that's
        // enough for us to aggregate already.
        for (epoch, round) in rounds_to_aggregate {
            self.maybe_aggregate_partial_signatures(epoch, round);
        }
    }

    #[allow(clippy::type_complexity)]
    fn remove_partial_sigs_in_range(
        &mut self,
        range: (
            Bound<(RandomnessRound, PeerId)>,
            Bound<(RandomnessRound, PeerId)>,
        ),
    ) {
        let keys_to_remove: Vec<_> = self
            .received_partial_sigs
            .range(range)
            .map(|(key, _)| *key)
            .collect();
        for key in keys_to_remove {
            // Have to remove keys one-by-one because BTreeMap does not support range-removal.
            self.received_partial_sigs.remove(&key);
        }
    }

    async fn send_partial_signatures_task(
        name: AuthorityName,
        network: anemo::Network,
        retry_interval: Duration,
        metrics: Metrics,
        authority_info: Arc<HashMap<AuthorityName, (PeerId, PartyId)>>,
        epoch: EpochId,
        round: RandomnessRound,
        partial_sigs: Vec<RandomnessPartialSignature>,
    ) {
        // For simtests, we may test not sending partial signatures.
        #[allow(unused_mut)]
        let mut fail_point_skip_sending = false;
        fail_point_if!("rb-send-partial-signatures", || {
            fail_point_skip_sending = true;
        });
        if fail_point_skip_sending {
            warn!("skipping sending partial sigs due to simtest fail point");
            return;
        }

        let _metrics_guard = metrics
            .round_observation_latency_metric()
            .map(|metric| metric.start_timer());

        let peers: HashMap<_, _> = authority_info
            .iter()
            .map(|(name, (peer_id, _party_id))| (name, network.waiting_peer(*peer_id)))
            .collect();
        let partial_sigs: Vec<_> = partial_sigs
            .iter()
            .map(|sig| bcs::to_bytes(sig).expect("message serialization should not fail"))
            .collect();

        loop {
            let mut requests = Vec::new();
            for (peer_name, peer) in &peers {
                if name == **peer_name {
                    continue; // don't send partial sigs to self
                }
                let mut client = RandomnessClient::new(peer.clone());
                const SEND_PARTIAL_SIGNATURES_TIMEOUT: Duration = Duration::from_secs(10);
                let request = anemo::Request::new(SendSignaturesRequest {
                    epoch,
                    round,
                    partial_sigs: partial_sigs.clone(),
                    sig: None,
                })
                .with_timeout(SEND_PARTIAL_SIGNATURES_TIMEOUT);
                requests.push(async move {
                    let result = client.send_signatures(request).await;
                    if let Err(e) = result {
                        debug!("failed to send partial signatures to {peer_name}: {e:?}");
                    }
                });
            }

            // Process all requests.
            futures::future::join_all(requests).await;

            // Keep retrying send to all peers until task is aborted via external message.
            tokio::time::sleep(retry_interval).await;
        }
    }

    fn update_rounds_pending_metric(&self) {
        let highest_requested_round = self
            .highest_requested_round
            .get(&self.epoch)
            .map(|r| r.0)
            .unwrap_or(0);
        let highest_completed_round = self
            .highest_completed_round
            .get(&self.epoch)
            .map(|r| r.0)
            .unwrap_or(0);
        let num_rounds_pending =
            highest_requested_round.saturating_sub(highest_completed_round) as i64;
        let prev_value = self.metrics.num_rounds_pending().unwrap_or_default();
        if num_rounds_pending / 100 > prev_value / 100 {
            warn!(
                // Recording multiples of 100 so tests can match on the log message.
                "RandomnessEventLoop randomness generation backlog: over {} rounds are pending (oldest is {:?})",
                (num_rounds_pending / 100) * 100,
                highest_completed_round+1,
            );
        }
        self.metrics.set_num_rounds_pending(num_rounds_pending);
    }
}
