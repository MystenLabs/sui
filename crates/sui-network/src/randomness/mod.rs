// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use self::{auth::AllowedPeersUpdatable, metrics::Metrics};
use anemo::PeerId;
use anyhow::Result;
use fastcrypto::groups::bls12381;
use fastcrypto_tbls::{
    dkg_v1,
    nodes::PartyId,
    tbls::ThresholdBls,
    types::{ShareIndex, ThresholdBls12381MinSig},
};
use mysten_metrics::spawn_monitored_task;
use mysten_network::anemo_ext::NetworkExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::{btree_map::BTreeMap, HashMap, HashSet},
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
use tokio::sync::{
    OnceCell, {mpsc, oneshot},
};
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
    // If peer already has a full signature available for the round, it's provided here in lieu
    // of partial sigs.
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
        dkg_output: dkg_v1::Output<bls12381::G2Element, bls12381::G2Element>,
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
        result_channel: oneshot::Sender<Result<()>>,
    ) {
        self.sender
            .try_send(RandomnessMessage::AdminInjectPartialSignatures(
                authority_name,
                round,
                sigs,
                result_channel,
            ))
            .expect("RandomnessEventLoop mailbox should not overflow or be closed")
    }

    /// Admin interface handler: injects full signature for the given round at the
    /// current epoch, skipping validity checks.
    pub fn admin_inject_full_signature(
        &self,
        round: RandomnessRound,
        sig: RandomnessSignature,
        result_channel: oneshot::Sender<Result<()>>,
    ) {
        self.sender
            .try_send(RandomnessMessage::AdminInjectFullSignature(
                round,
                sig,
                result_channel,
            ))
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
        dkg_v1::Output<bls12381::G2Element, bls12381::G2Element>,
        u16,                     // aggregation_threshold
        Option<RandomnessRound>, // recovered_highest_completed_round
    ),
    SendPartialSignatures(EpochId, RandomnessRound),
    CompleteRound(EpochId, RandomnessRound),
    ReceiveSignatures(
        PeerId,
        EpochId,
        RandomnessRound,
        Vec<Vec<u8>>,
        Option<RandomnessSignature>,
    ),
    MaybeIgnoreByzantinePeer(EpochId, PeerId),
    AdminGetPartialSignatures(RandomnessRound, oneshot::Sender<Vec<u8>>),
    AdminInjectPartialSignatures(
        AuthorityName,
        RandomnessRound,
        Vec<RandomnessPartialSignature>,
        oneshot::Sender<Result<()>>,
    ),
    AdminInjectFullSignature(
        RandomnessRound,
        RandomnessSignature,
        oneshot::Sender<Result<()>>,
    ),
}

struct RandomnessEventLoop {
    name: AuthorityName,
    config: RandomnessConfig,
    mailbox: mpsc::Receiver<RandomnessMessage>,
    mailbox_sender: mpsc::WeakSender<RandomnessMessage>,
    network: anemo::Network,
    allowed_peers: AllowedPeersUpdatable,
    allowed_peers_set: HashSet<PeerId>,
    metrics: Metrics,
    randomness_tx: mpsc::Sender<(EpochId, RandomnessRound, Vec<u8>)>,

    epoch: EpochId,
    authority_info: Arc<HashMap<AuthorityName, (PeerId, PartyId)>>,
    peer_share_ids: Option<HashMap<PeerId, Vec<ShareIndex>>>,
    blocked_share_id_count: usize,
    dkg_output: Option<dkg_v1::Output<bls12381::G2Element, bls12381::G2Element>>,
    aggregation_threshold: u16,
    highest_requested_round: BTreeMap<EpochId, RandomnessRound>,
    send_tasks: BTreeMap<
        RandomnessRound,
        (
            tokio::task::JoinHandle<()>,
            Arc<OnceCell<RandomnessSignature>>,
        ),
    >,
    round_request_time: BTreeMap<(EpochId, RandomnessRound), time::Instant>,
    future_epoch_partial_sigs: BTreeMap<(EpochId, RandomnessRound, PeerId), Vec<Vec<u8>>>,
    received_partial_sigs: BTreeMap<(RandomnessRound, PeerId), Vec<RandomnessPartialSignature>>,
    completed_sigs: BTreeMap<RandomnessRound, RandomnessSignature>,
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
            RandomnessMessage::ReceiveSignatures(peer_id, epoch, round, partial_sigs, sig) => {
                if let Some(sig) = sig {
                    self.receive_full_signature(peer_id, epoch, round, sig)
                } else {
                    self.receive_partial_signatures(peer_id, epoch, round, partial_sigs)
                }
            }
            RandomnessMessage::MaybeIgnoreByzantinePeer(epoch, peer_id) => {
                self.maybe_ignore_byzantine_peer(epoch, peer_id)
            }
            RandomnessMessage::AdminGetPartialSignatures(round, tx) => {
                self.admin_get_partial_signatures(round, tx)
            }
            RandomnessMessage::AdminInjectPartialSignatures(
                authority_name,
                round,
                sigs,
                result_channel,
            ) => {
                let _ = result_channel.send(self.admin_inject_partial_signatures(
                    authority_name,
                    round,
                    sigs,
                ));
            }
            RandomnessMessage::AdminInjectFullSignature(round, sig, result_channel) => {
                let _ = result_channel.send(self.admin_inject_full_signature(round, sig));
            }
        }
    }

    #[instrument(level = "debug", skip_all, fields(?new_epoch))]
    fn update_epoch(
        &mut self,
        new_epoch: EpochId,
        authority_info: HashMap<AuthorityName, (PeerId, PartyId)>,
        dkg_output: dkg_v1::Output<bls12381::G2Element, bls12381::G2Element>,
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
        self.allowed_peers_set = authority_info
            .values()
            .map(|(peer_id, _)| *peer_id)
            .collect();
        self.allowed_peers
            .update(Arc::new(self.allowed_peers_set.clone()));
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
        for (_, (task, _)) in std::mem::take(&mut self.send_tasks) {
            task.abort();
        }
        self.metrics.set_epoch(new_epoch);

        // Throw away info from old epochs.
        self.highest_requested_round = self.highest_requested_round.split_off(&new_epoch);
        self.round_request_time = self
            .round_request_time
            .split_off(&(new_epoch, RandomnessRound(0)));
        self.received_partial_sigs.clear();
        self.completed_sigs.clear();
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
            self.completed_sigs = self.completed_sigs.split_off(&(round + 1));
            for (_, (task, _)) in self.send_tasks.iter().take_while(|(r, _)| **r <= round) {
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
        if epoch == self.epoch && self.completed_sigs.contains_key(&round) {
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
        let last_completed_signature = self.completed_sigs.last_key_value().map(|(r, _)| *r);
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

        if self.completed_sigs.contains_key(&round) {
            info!("skipping aggregation for already-completed signature");
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
                        if let Some(sender) = self.mailbox_sender.upgrade() {
                            sender.try_send(RandomnessMessage::MaybeIgnoreByzantinePeer(
                                epoch,
                                peer_id,
                            ))
                            .expect("RandomnessEventLoop mailbox should not overflow or be closed");
                        }
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
        self.process_valid_full_signature(epoch, round, sig);
    }

    #[instrument(level = "debug", skip_all, fields(?peer_id, ?epoch, ?round))]
    fn receive_full_signature(
        &mut self,
        peer_id: PeerId,
        epoch: EpochId,
        round: RandomnessRound,
        sig: RandomnessSignature,
    ) {
        let vss_pk = {
            let Some(dkg_output) = &self.dkg_output else {
                debug!("called receive_full_signature before DKG completed");
                return;
            };
            &dkg_output.vss_pk
        };

        // Basic validity checks.
        if epoch != self.epoch {
            debug!("skipping received full sig, we are on epoch {}", self.epoch);
            return;
        }
        if self.completed_sigs.contains_key(&round) {
            debug!("skipping received full sigs, we already have completed this sig");
            return;
        }
        let highest_completed_round = self.highest_completed_round.get(&epoch).copied();
        if let Some(highest_completed_round) = &highest_completed_round {
            if *highest_completed_round >= round {
                debug!("skipping received full sig, we already have completed this round");
                return;
            }
        }

        let highest_requested_round = self.highest_requested_round.get(&epoch);
        if highest_requested_round.is_none() || round > *highest_requested_round.unwrap() {
            // Wait for local consensus to catch up if necessary.
            debug!(
                "skipping received full signature, local consensus is not caught up to its round"
            );
            return;
        }

        if let Err(e) =
            ThresholdBls12381MinSig::verify(vss_pk.c0(), &round.signature_message(), &sig)
        {
            info!("received invalid full signature from peer {peer_id}: {e:?}");
            if let Some(sender) = self.mailbox_sender.upgrade() {
                sender
                    .try_send(RandomnessMessage::MaybeIgnoreByzantinePeer(epoch, peer_id))
                    .expect("RandomnessEventLoop mailbox should not overflow or be closed");
            }
            return;
        }

        debug!("received valid randomness full signature");
        self.process_valid_full_signature(epoch, round, sig);
    }

    fn process_valid_full_signature(
        &mut self,
        epoch: EpochId,
        round: RandomnessRound,
        sig: RandomnessSignature,
    ) {
        assert_eq!(epoch, self.epoch);

        if let Some((_, full_sig_cell)) = self.send_tasks.get(&round) {
            full_sig_cell
                .set(sig)
                .expect("full signature should never be processed twice");
        }
        self.completed_sigs.insert(round, sig);
        self.remove_partial_sigs_in_range((
            Bound::Included((round, PeerId([0; 32]))),
            Bound::Excluded((round + 1, PeerId([0; 32]))),
        ));
        self.metrics.record_completed_round(round);
        if let Some(start_time) = self.round_request_time.get(&(epoch, round)) {
            if let Some(metric) = self.metrics.round_generation_latency_metric() {
                metric.observe(start_time.elapsed().as_secs_f64());
            }
        }

        let sig_bytes = bcs::to_bytes(&sig).expect("signature serialization should not fail");
        self.randomness_tx
            .try_send((epoch, round, sig_bytes))
            .expect("RandomnessRoundReceiver mailbox should not overflow or be closed");
    }

    fn maybe_ignore_byzantine_peer(&mut self, epoch: EpochId, peer_id: PeerId) {
        if epoch != self.epoch {
            return; // make sure we're still on the same epoch
        }
        let Some(dkg_output) = &self.dkg_output else {
            return; // can't ignore a peer if we haven't finished DKG
        };
        if !self.allowed_peers_set.contains(&peer_id) {
            return; // peer is already disallowed
        }
        let Some(peer_share_ids) = &self.peer_share_ids else {
            return; // can't ignore a peer if we haven't finished DKG
        };
        let Some(peer_shares) = peer_share_ids.get(&peer_id) else {
            warn!("can't ignore unknown byzantine peer {peer_id:?}");
            return;
        };
        let max_ignored_shares = (self.config.max_ignored_peer_weight_factor()
            * (dkg_output.nodes.total_weight() as f64)) as usize;
        if self.blocked_share_id_count + peer_shares.len() > max_ignored_shares {
            warn!("ignoring byzantine peer {peer_id:?} with {} shares would exceed max ignored peer weight {max_ignored_shares}", peer_shares.len());
            return;
        }

        warn!(
            "ignoring byzantine peer {peer_id:?} with {} shares",
            peer_shares.len()
        );
        self.blocked_share_id_count += peer_shares.len();
        self.allowed_peers_set.remove(&peer_id);
        self.allowed_peers
            .update(Arc::new(self.allowed_peers_set.clone()));
        self.metrics.inc_num_ignored_byzantine_peers();
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

            let full_sig_cell = Arc::new(OnceCell::new());
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
                let full_sig_cell_clone = full_sig_cell.clone();

                // Record own partial sigs.
                if !self.completed_sigs.contains_key(&round) {
                    self.received_partial_sigs
                        .insert((round, self.network.peer_id()), partial_sigs.clone());
                    rounds_to_aggregate.push((epoch, round));
                }

                debug!("sending partial sigs for epoch {epoch}, round {round}");
                (
                    spawn_monitored_task!(RandomnessEventLoop::send_signatures_task(
                        name,
                        network,
                        retry_interval,
                        metrics,
                        authority_info,
                        epoch,
                        round,
                        partial_sigs,
                        full_sig_cell_clone,
                    )),
                    full_sig_cell,
                )
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

    async fn send_signatures_task(
        name: AuthorityName,
        network: anemo::Network,
        retry_interval: Duration,
        metrics: Metrics,
        authority_info: Arc<HashMap<AuthorityName, (PeerId, PartyId)>>,
        epoch: EpochId,
        round: RandomnessRound,
        partial_sigs: Vec<RandomnessPartialSignature>,
        full_sig: Arc<OnceCell<RandomnessSignature>>,
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
                let full_sig = full_sig.get().cloned();
                let request = anemo::Request::new(SendSignaturesRequest {
                    epoch,
                    round,
                    partial_sigs: if full_sig.is_none() {
                        partial_sigs.clone()
                    } else {
                        Vec::new()
                    },
                    sig: full_sig,
                })
                .with_timeout(SEND_PARTIAL_SIGNATURES_TIMEOUT);
                requests.push(async move {
                    let result = client.send_signatures(request).await;
                    if let Err(_error) = result {
                        // TODO: add Display impl to anemo::rpc::Status, log it here
                        debug!("failed to send partial signatures to {peer_name}");
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
    ) -> Result<()> {
        let peer_id = self
            .authority_info
            .get(&authority_name)
            .map(|(peer_id, _)| *peer_id)
            .ok_or(anyhow::anyhow!("unknown AuthorityName {authority_name:?}"))?;
        self.received_partial_sigs.insert((round, peer_id), sigs);
        self.maybe_aggregate_partial_signatures(self.epoch, round);
        Ok(())
    }

    fn admin_inject_full_signature(
        &mut self,
        round: RandomnessRound,
        sig: RandomnessSignature,
    ) -> Result<()> {
        let vss_pk = {
            let Some(dkg_output) = &self.dkg_output else {
                return Err(anyhow::anyhow!(
                    "called admin_inject_full_signature before DKG completed"
                ));
            };
            &dkg_output.vss_pk
        };

        ThresholdBls12381MinSig::verify(vss_pk.c0(), &round.signature_message(), &sig)
            .map_err(|e| anyhow::anyhow!("invalid full signature: {e:?}"))?;

        self.process_valid_full_signature(self.epoch, round, sig);
        Ok(())
    }
}
