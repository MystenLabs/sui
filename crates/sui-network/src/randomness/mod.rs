// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use self::{auth::AllowedPeersUpdatable, metrics::Metrics};
use anemo::PeerId;
use anyhow::Result;
use fastcrypto::groups::bls12381;
use fastcrypto_tbls::{
    dkg,
    nodes::PartyId,
    tbls::ThresholdBls,
    types::{ShareIndex, ThresholdBls12381MinSig},
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
use tokio::sync::{mpsc, oneshot};
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

    /// Admin interface handler: generates partial signatures for the given round at the
    /// current epoch.
    pub fn admin_get_partial_signatures(
        &self,
        round: RandomnessRound,
        tx: oneshot::Sender<Vec<u8>>,
    ) {
        self.sender
            .try_send(RandomnessMessage::AdminGetPartialSignatures(round, tx))
            .expect("RandomnessEventLoop mailbox should not overflow or be closed")
    }

    /// Admin interface handler: injects partial signatures for the given round at the
    /// current epoch, skipping validity checks.
    pub fn admin_inject_partial_signatures(
        &self,
        authority_name: AuthorityName,
        round: RandomnessRound,
        sigs: Vec<RandomnessPartialSignature>,
    ) {
        self.sender
            .try_send(RandomnessMessage::AdminInjectPartialSignatures(
                authority_name,
                round,
                sigs,
            ))
            .expect("RandomnessEventLoop mailbox should not overflow or be closed")
    }

    /// Admin interface handler: injects full signature for the given round at the
    /// current epoch, skipping validity checks.
    pub fn admin_inject_full_signature(&self, round: RandomnessRound, sig: RandomnessSignature) {
        self.sender
            .try_send(RandomnessMessage::AdminInjectFullSignature(round, sig))
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

#[derive(Debug)]
enum RandomnessMessage {
    UpdateEpoch(
        EpochId,
        HashMap<AuthorityName, (PeerId, PartyId)>,
        dkg::Output<bls12381::G2Element, bls12381::G2Element>,
        u16,                     // aggregation_threshold
        Option<RandomnessRound>, // recovered_last_completed_round
    ),
    SendPartialSignatures(EpochId, RandomnessRound),
    CompleteRound(EpochId, RandomnessRound),
    ReceivePartialSignatures(PeerId, EpochId, RandomnessRound, Vec<Vec<u8>>),
    AdminGetPartialSignatures(RandomnessRound, oneshot::Sender<Vec<u8>>),
    AdminInjectPartialSignatures(
        AuthorityName,
        RandomnessRound,
        Vec<RandomnessPartialSignature>,
    ),
    AdminInjectFullSignature(RandomnessRound, RandomnessSignature),
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
    pending_tasks: BTreeSet<(EpochId, RandomnessRound)>,
    send_tasks: BTreeMap<(EpochId, RandomnessRound), tokio::task::JoinHandle<()>>,
    round_request_time: BTreeMap<(EpochId, RandomnessRound), time::Instant>,
    future_epoch_partial_sigs: BTreeMap<(EpochId, RandomnessRound, PeerId), Vec<Vec<u8>>>,
    received_partial_sigs:
        BTreeMap<(EpochId, RandomnessRound, PeerId), Vec<RandomnessPartialSignature>>,
    completed_sigs: BTreeSet<(EpochId, RandomnessRound)>,
    completed_rounds: BTreeSet<(EpochId, RandomnessRound)>,
    recovered_last_completed_round: Option<RandomnessRound>, // reported by RandomnessManager on crash recovery
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
                recovered_last_completed_round,
            ) => {
                if let Err(e) = self.update_epoch(
                    epoch,
                    authority_info,
                    dkg_output,
                    aggregation_threshold,
                    recovered_last_completed_round,
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
            RandomnessMessage::AdminGetPartialSignatures(round, tx) => {
                self.admin_get_partial_signatures(round, tx)
            }
            RandomnessMessage::AdminInjectPartialSignatures(authority_name, round, sigs) => {
                self.admin_inject_partial_signatures(authority_name, round, sigs)
            }
            RandomnessMessage::AdminInjectFullSignature(round, sig) => {
                self.admin_inject_full_signature(round, sig)
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
        recovered_last_completed_round: Option<RandomnessRound>,
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
        self.recovered_last_completed_round = recovered_last_completed_round;
        for (_, task) in std::mem::take(&mut self.send_tasks) {
            task.abort();
        }
        self.metrics.set_epoch(new_epoch);

        // Throw away info from old epochs.
        self.round_request_time = self
            .round_request_time
            .split_off(&(new_epoch, RandomnessRound(0)));
        self.received_partial_sigs =
            self.received_partial_sigs
                .split_off(&(new_epoch, RandomnessRound(0), PeerId([0; 32])));
        self.completed_sigs = self
            .completed_sigs
            .split_off(&(new_epoch, RandomnessRound(0)));
        self.completed_rounds = self
            .completed_rounds
            .split_off(&(new_epoch, RandomnessRound(0)));

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
        let mut aggregate_rounds = BTreeSet::new();
        for (epoch, round, _) in self.received_partial_sigs.keys() {
            if *epoch < new_epoch {
                error!("BUG: received partial sigs for old epoch still present after attempting to remove them");
                debug_assert!(false, "received partial sigs for old epoch still present after attempting to remove them");
                continue;
            }
            if *epoch > new_epoch {
                break;
            }
            if !self.completed_sigs.contains(&(*epoch, *round)) {
                aggregate_rounds.insert(*round);
            }
        }
        for round in aggregate_rounds {
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
        if self.completed_rounds.contains(&(epoch, round)) {
            info!("skipping sending partial sigs, we already have completed this round");
            return;
        }

        self.pending_tasks.insert((epoch, round));
        self.round_request_time
            .insert((epoch, round), time::Instant::now());
        self.maybe_start_pending_tasks();
    }

    #[instrument(level = "debug", skip_all, fields(?epoch, ?round))]
    fn complete_round(&mut self, epoch: EpochId, round: RandomnessRound) {
        debug!("completing randomness round");
        self.pending_tasks.remove(&(epoch, round));
        self.round_request_time.remove(&(epoch, round));
        self.completed_rounds.insert((epoch, round));

        // In case we first received the full sig from a checkpoint instead of aggregating it
        // locally, update related data structures here.
        self.completed_sigs.insert((epoch, round));
        self.remove_partial_sigs_in_range((
            Bound::Included((epoch, round, PeerId([0; 32]))),
            Bound::Excluded((epoch, round + 1, PeerId([0; 32]))),
        ));

        if let Some(task) = self.send_tasks.remove(&(epoch, round)) {
            task.abort();
            self.maybe_start_pending_tasks();
        } else {
            self.update_rounds_pending_metric();
        }
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
        let (last_completed_epoch, last_completed_round) = match self.completed_sigs.last() {
            Some((last_completed_epoch, last_completed_round)) => {
                (*last_completed_epoch, *last_completed_round)
            }
            // If we just changed epochs and haven't completed any sigs yet, or if we
            // restarted mid-epoch, this will be used.
            None => (
                self.epoch,
                // We don't store completed sigs durably outside of checkpoints, so after a
                // restart we use the last completed round instead. This is okay because
                // incomplete rounds with previously-completed sigs will be re-opened
                // by the RandomnessManager on restart, and we'll simply repeat the process.
                self.recovered_last_completed_round
                    .unwrap_or(RandomnessRound(0)),
            ),
        };
        if epoch == last_completed_epoch
            && round.0
                >= last_completed_round
                    .0
                    .saturating_add(self.config.max_partial_sigs_rounds_ahead())
        {
            debug!(
                    "skipping received partial sigs, most recent round we completed was only {last_completed_round}",
                );
            return;
        }
        if epoch > last_completed_epoch && round.0 >= self.config.max_partial_sigs_rounds_ahead() {
            debug!(
                    "skipping received partial sigs, most recent epoch we completed was only {last_completed_epoch}",
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
            .insert((epoch, round, peer_id), partial_sigs);

        self.maybe_aggregate_partial_signatures(epoch, round);
    }

    #[instrument(level = "debug", skip_all, fields(?epoch, ?round))]
    fn maybe_aggregate_partial_signatures(&mut self, epoch: EpochId, round: RandomnessRound) {
        if self.completed_sigs.contains(&(epoch, round)) {
            error!("BUG: called maybe_aggregate_partial_signatures for already-completed round");
            debug_assert!(
                false,
                "called maybe_aggregate_partial_signatures for already-completed round"
            );
            return;
        }

        if !(self.send_tasks.contains_key(&(epoch, round))
            || self.pending_tasks.contains(&(epoch, round)))
        {
            // We have to wait here, because even if we have enough information from other nodes
            // to complete the signature, local shared object versions are not set until consensus
            // finishes processing the corresponding commit. This function will be called again
            // after maybe_start_pending_tasks begins this round locally.
            debug!("waiting to aggregate randomness partial signatures until local consensus catches up");
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
            Bound::Included((epoch, round, PeerId([0; 32]))),
            Bound::Excluded((epoch, round + 1, PeerId([0; 32]))),
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
                .retain(|&(e, r, peer_id), partial_sigs| {
                    if epoch != e || round != r {
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
            return; // can't start tasks until first DKG completes
        };
        let shares = if let Some(shares) = &dkg_output.shares {
            shares
        } else {
            return; // can't participate in randomness generation without shares
        };

        let mut last_handled_key = None;
        let mut rounds_to_aggregate = Vec::new();
        for (epoch, round) in &self.pending_tasks {
            if epoch > &self.epoch {
                break; // wait for DKG in new epoch
            }

            if self.send_tasks.len() >= self.config.max_partial_sigs_concurrent_sends() {
                break; // limit concurrent tasks
            }

            last_handled_key = Some((*epoch, *round));

            if epoch < &self.epoch {
                info!(
                    "skipping sending partial sigs for epoch {epoch} round {round}, we are already up to epoch {}",
                    self.epoch
                );
                continue;
            }

            if self.completed_rounds.contains(&(*epoch, *round)) {
                info!(
                    "skipping sending partial sigs for epoch {epoch} round {round}, we already have completed this round",
                );
                continue;
            }

            self.send_tasks.entry((*epoch, *round)).or_insert_with(|| {
                let name = self.name;
                let network = self.network.clone();
                let retry_interval = self.config.partial_signature_retry_interval();
                let metrics = self.metrics.clone();
                let authority_info = self.authority_info.clone();
                let epoch = *epoch;
                let round = *round;
                let partial_sigs = ThresholdBls12381MinSig::partial_sign_batch(
                    shares.iter(),
                    &round.signature_message(),
                );

                // Record own partial sigs.
                if !self.completed_sigs.contains(&(epoch, round)) {
                    self.received_partial_sigs
                        .insert((epoch, round, self.network.peer_id()), partial_sigs.clone());
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

        if let Some(last_handled_key) = last_handled_key {
            // Remove stuff from the pending_tasks map that we've handled.
            let split_point = self
                .pending_tasks
                .range((Bound::Excluded(last_handled_key), Bound::Unbounded))
                .next()
                .cloned();
            if let Some(key) = split_point {
                self.pending_tasks = self.pending_tasks.split_off(&key);
            } else {
                self.pending_tasks.clear();
            }
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
            Bound<(u64, RandomnessRound, PeerId)>,
            Bound<(u64, RandomnessRound, PeerId)>,
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
        let num_rounds_pending = (self.pending_tasks.len() + self.send_tasks.len()) as i64;
        let prev_value = self.metrics.num_rounds_pending().unwrap_or_default();
        if num_rounds_pending / 100 > prev_value / 100 {
            warn!(
                // Recording multiples of 100 so tests can match on the log message.
                "RandomnessEventLoop randomness generation backlog: over {} rounds are pending (oldest is {:?})",
                (num_rounds_pending / 100) * 100,
                match (self.pending_tasks.first(), self.send_tasks.first_key_value()) {
                    (Some(p), Some((s, _))) => {
                        std::cmp::min(p, s)
                    }
                    (Some(p), None) => p,
                    (None, Some((s, _))) => s,
                    (None, None) => &(0, RandomnessRound(0)),
                },
            );
        }
        self.metrics.set_num_rounds_pending(num_rounds_pending);
    }

    fn admin_get_partial_signatures(&self, round: RandomnessRound, tx: oneshot::Sender<Vec<u8>>) {
        let shares = if let Some(shares) = self.dkg_output.as_ref().and_then(|d| d.shares.as_ref())
        {
            shares
        } else {
            let _ = tx.send(Vec::new()); // no error handling needed if receiver is already dropped
            return;
        };

        let partial_sigs =
            ThresholdBls12381MinSig::partial_sign_batch(shares.iter(), &round.signature_message());
        // no error handling needed if receiver is already dropped
        let _ = tx.send(bcs::to_bytes(&partial_sigs).expect("serialization should not fail"));
    }

    fn admin_inject_partial_signatures(
        &mut self,
        authority_name: AuthorityName,
        round: RandomnessRound,
        sigs: Vec<RandomnessPartialSignature>,
    ) {
        let peer_id = match self
            .authority_info
            .get(&authority_name)
            .map(|(peer_id, _)| *peer_id)
        {
            Some(peer_id) => peer_id,
            None => {
                warn!(
                    "ignoring admin request to inject partial sigs: unknown AuthorityName {authority_name:?}"
                );
                return;
            }
        };
        self.received_partial_sigs
            .insert((self.epoch, round, peer_id), sigs);
        self.maybe_aggregate_partial_signatures(self.epoch, round);
    }

    fn admin_inject_full_signature(&mut self, round: RandomnessRound, sig: RandomnessSignature) {
        self.completed_sigs.insert((self.epoch, round));
        self.remove_partial_sigs_in_range((
            Bound::Included((self.epoch, round, PeerId([0; 32]))),
            Bound::Excluded((self.epoch, round + 1, PeerId([0; 32]))),
        ));
        let bytes = bcs::to_bytes(&sig).expect("signature serialization should not fail");
        self.randomness_tx
            .try_send((self.epoch, round, bytes))
            .expect("RandomnessRoundReceiver mailbox should not overflow or be closed");
    }
}
