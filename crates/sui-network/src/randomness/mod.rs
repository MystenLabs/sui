// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::PeerId;
use anyhow::Result;
use fastcrypto::groups::bls12381;
use fastcrypto_tbls::{dkg, nodes::PartyId, tbls::ThresholdBls, types::ThresholdBls12381MinSig};
use futures::{stream::FuturesUnordered, StreamExt};
use mysten_metrics::spawn_monitored_task;
use mysten_network::anemo_ext::NetworkExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::{btree_map::BTreeMap, HashMap},
    ops::Bound,
    sync::Arc,
    time::Duration,
};
use sui_types::{
    base_types::AuthorityName,
    committee::EpochId,
    crypto::{RandomnessPartialSignature, RandomnessRound},
};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

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
pub struct SendPartialSignaturesRequest {
    epoch: EpochId,
    round: RandomnessRound,
    // BCS-serialized `RandomnessPartialSignature` values. We store raw bytes here to enable
    // defenses against too-large messages.
    sigs: Vec<Vec<u8>>,
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
    pub async fn update_epoch(
        &self,
        new_epoch: EpochId,
        authority_info: HashMap<AuthorityName, (PeerId, PartyId)>,
        dkg_output: dkg::Output<bls12381::G2Element, bls12381::G2Element>,
        aggregation_threshold: u32,
    ) {
        self.sender
            .send(RandomnessMessage::UpdateEpoch(
                new_epoch,
                authority_info,
                dkg_output,
                aggregation_threshold,
            ))
            .await
            .unwrap()
    }

    /// Begins transmitting partial signatures for the given epoch and round until canceled.
    pub async fn send_partial_signatures(&self, epoch: EpochId, round: RandomnessRound) {
        self.sender
            .send(RandomnessMessage::SendPartialSignatures(epoch, round))
            .await
            .unwrap()
    }

    /// Cancels transmitting partial signatures for the given epoch and round.
    pub async fn cancel_send_partial_signatures(&self, epoch: EpochId, round: RandomnessRound) {
        self.sender
            .send(RandomnessMessage::CancelSendPartialSignatures(epoch, round))
            .await
            .unwrap()
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
        u32, // aggregation_threshold
    ),
    SendPartialSignatures(EpochId, RandomnessRound),
    CancelSendPartialSignatures(EpochId, RandomnessRound),
    ReceivePartialSignatures(PeerId, EpochId, RandomnessRound, Vec<Vec<u8>>),
}

struct RandomnessEventLoop {
    mailbox: mpsc::Receiver<RandomnessMessage>,
    network: anemo::Network,
    randomness_tx: mpsc::Sender<(EpochId, RandomnessRound, Vec<u8>)>,

    epoch: EpochId,
    authority_info: Arc<HashMap<AuthorityName, (PeerId, PartyId)>>,
    peer_share_counts: Option<HashMap<PeerId, u16>>,
    dkg_output: Option<dkg::Output<bls12381::G2Element, bls12381::G2Element>>,
    aggregation_threshold: u32,
    pending_tasks: BTreeMap<(EpochId, RandomnessRound), ()>,
    send_tasks: BTreeMap<(EpochId, RandomnessRound), tokio::task::JoinHandle<()>>,
    received_partial_sigs:
        BTreeMap<(EpochId, RandomnessRound, PeerId), Vec<RandomnessPartialSignature>>,
    completed_sigs: BTreeMap<(EpochId, RandomnessRound), ()>,
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
                        self.handle_message(message).await;
                    } else {
                        break;
                    }
                },
            }
        }

        info!("Randomness network event loop ended");
    }

    async fn handle_message(&mut self, message: RandomnessMessage) {
        match message {
            RandomnessMessage::UpdateEpoch(
                epoch,
                authority_info,
                dkg_output,
                aggregation_threshold,
            ) => {
                if let Err(e) = self
                    .update_epoch(epoch, authority_info, dkg_output, aggregation_threshold)
                    .await
                {
                    error!("BUG: failed to update epoch in RandomnessEventLoop: {e:?}");
                }
            }
            RandomnessMessage::SendPartialSignatures(epoch, round) => {
                self.send_partial_signatures(epoch, round)
            }
            RandomnessMessage::CancelSendPartialSignatures(epoch, round) => {
                self.cancel_send_partial_signatures(epoch, round)
            }
            RandomnessMessage::ReceivePartialSignatures(peer_id, epoch, round, sigs) => {
                self.receive_partial_signatures(peer_id, epoch, round, sigs)
                    .await
            }
        }
    }

    async fn update_epoch(
        &mut self,
        new_epoch: EpochId,
        authority_info: HashMap<AuthorityName, (PeerId, PartyId)>,
        dkg_output: dkg::Output<bls12381::G2Element, bls12381::G2Element>,
        aggregation_threshold: u32,
    ) -> Result<()> {
        assert!(self.dkg_output.is_none() || new_epoch > self.epoch);

        self.peer_share_counts = Some(authority_info.iter().try_fold(
            HashMap::new(),
            |mut acc, (_name, (peer_id, party_id))| -> Result<_> {
                let weight = dkg_output.nodes.node_id_to_node(*party_id)?.weight;
                acc.insert(*peer_id, weight);
                Ok(acc)
            },
        )?);
        self.epoch = new_epoch;
        self.authority_info = Arc::new(authority_info);
        self.dkg_output = Some(dkg_output);
        self.aggregation_threshold = aggregation_threshold;
        for (_, task) in std::mem::take(&mut self.send_tasks) {
            task.abort();
        }

        // Throw away any signatures from old epochs.
        self.received_partial_sigs = self.received_partial_sigs.split_off(&(
            new_epoch + 1,
            RandomnessRound(0),
            PeerId([0; 32]),
        ));
        self.completed_sigs = self
            .completed_sigs
            .split_off(&(new_epoch, RandomnessRound(0)));

        // Start any pending tasks for the new epoch.
        self.maybe_start_pending_tasks();

        // Aggregate any sigs received early from the new epoch.
        let mut aggregate_rounds = Vec::new(); // can't call aggregate directly while iterating because it takes &mut self
        let mut next_eligible_round = 0;
        for (epoch, round, _) in self.received_partial_sigs.keys() {
            if *epoch != new_epoch {
                break;
            }
            if round.0 >= next_eligible_round {
                aggregate_rounds.push(*round);
                next_eligible_round = round.0 + 1;
            }
        }
        for round in aggregate_rounds {
            self.maybe_aggregate_partial_signatures(new_epoch, round)
                .await;
        }

        Ok(())
    }

    fn send_partial_signatures(&mut self, epoch: EpochId, round: RandomnessRound) {
        if epoch < self.epoch {
            info!(
                    "skipping sending partial sigs for epoch {epoch} round {round}, we are already up to epoch {}",
                    self.epoch
                );
            return;
        }

        self.pending_tasks.insert((epoch, round), ());
        self.maybe_start_pending_tasks();
    }

    fn cancel_send_partial_signatures(&mut self, epoch: EpochId, round: RandomnessRound) {
        debug!("canceling sending partial sigs for epoch {epoch}, round {round}");
        self.pending_tasks.remove(&(epoch, round));
        if let Some(task) = self.send_tasks.remove(&(epoch, round)) {
            task.abort();
            self.maybe_start_pending_tasks();
        }
    }

    async fn receive_partial_signatures(
        &mut self,
        peer_id: PeerId,
        epoch: EpochId,
        round: RandomnessRound,
        sig_bytes: Vec<Vec<u8>>,
    ) {
        // Big slate of validity checks on received partial signatures.
        let peer_share_counts = if let Some(peer_share_counts) = &self.peer_share_counts {
            peer_share_counts
        } else {
            debug!("can't accept partial signatures until DKG has completed");
            return;
        };
        if epoch < self.epoch {
            debug!(
                "skipping received partial sigs for epoch {epoch} round {round}, we are already up to epoch {}",
                self.epoch
            );
            return;
        }
        if epoch > self.epoch + 1 {
            debug!(
                "skipping received partial sigs for epoch {epoch}, we are still on epoch {epoch}"
            );
            return;
        }
        if self.completed_sigs.contains_key(&(epoch, round)) {
            debug!(
                "skipping received partial sigs for epoch {epoch} round {round}, we already have completed this sig"
            );
            return;
        }
        let expected_share_count = if let Some(count) = peer_share_counts.get(&peer_id) {
            count
        } else {
            debug!("received partial sigs from unknown peer {peer_id}");
            return;
        };
        if sig_bytes.len() != *expected_share_count as usize {
            debug!(
                "received partial sigs from {peer_id} with wrong share count: expected {expected_share_count}, got {}",
                sig_bytes.len(),
            );
            return;
        }
        if let Some(((last_completed_epoch, last_completed_round), _)) =
            self.completed_sigs.last_key_value()
        {
            const MAX_PARTIAL_SIGS_ROUNDS_AHEAD: u64 = 5;
            if epoch == *last_completed_epoch
                && round.0
                    >= last_completed_round
                        .0
                        .saturating_add(MAX_PARTIAL_SIGS_ROUNDS_AHEAD)
            {
                debug!(
                    "skipping received partial sigs for epoch {epoch} round {round}, most recent round we completed was only {last_completed_round}",
                );
                return;
            } else if round.0 >= MAX_PARTIAL_SIGS_ROUNDS_AHEAD {
                debug!(
                    "skipping received partial sigs for epoch {epoch} round {round}, most recent epoch we completed was only {last_completed_epoch}",
                );
                return;
            }
        }

        // We passed all the checks, deserialize and save the partial sigs.
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
                    debug!("failed to deserialize partial sigs from {peer_id}: {e:?}");
                    return;
                }
            };
        debug!(
            "recording partial signatures for epoch {epoch}, round {round}, received from {peer_id}"
        );
        self.received_partial_sigs
            .insert((epoch, round, peer_id), partial_sigs);

        self.maybe_aggregate_partial_signatures(epoch, round).await;
    }

    async fn maybe_aggregate_partial_signatures(&mut self, epoch: EpochId, round: RandomnessRound) {
        let vss_pk = {
            let Some(dkg_output) = &self.dkg_output else {
                debug!(
                    "random beacon: called maybe_aggregate_partial_signatures before DKG completed"
                );
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
            .map(|(_, sigs)| sigs)
            .flatten();
        let mut sig =
            match ThresholdBls12381MinSig::aggregate(self.aggregation_threshold, sig_range) {
                Ok(sig) => sig,
                Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => return, // wait for more input
                Err(e) => {
                    error!("Error while aggregating randomness partial signatures: {e:?}");
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
                            "Received invalid partial signatures from possibly-Byzantine peer {peer_id}"
                        );
                        return false;
                    }
                    true
                });
            let sig_range = self
                .received_partial_sigs
                .range(sig_bounds)
                .map(|(_, sigs)| sigs)
                .flatten();
            sig = match ThresholdBls12381MinSig::aggregate(self.aggregation_threshold, sig_range) {
                Ok(sig) => sig,
                Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => return, // wait for more input
                Err(e) => {
                    error!("Error while aggregating randomness partial signatures: {e:?}");
                    return;
                }
            };
            if let Err(e) =
                ThresholdBls12381MinSig::verify(vss_pk.c0(), &round.signature_message(), &sig)
            {
                error!("Error while verifying randomness partial signatures after removing invalid partials: {e:?}");
                return;
            }
        }

        debug!("generated randomness full signature for epoch {epoch}, round {round}");
        self.completed_sigs.insert((epoch, round), ());

        // TODO-DNS is there a way to do this by range, instead of saving all the keys and
        // individually removing each one? From Googling I found some incomplete feature
        // requests from 2022 but no clear answer. Same question applies to the above
        // use of `retain` which is also not ideal.
        let keys_to_remove: Vec<_> = self
            .received_partial_sigs
            .range(sig_bounds)
            .map(|(key, _)| key.clone())
            .collect();
        for key in keys_to_remove {
            self.received_partial_sigs.remove(&key);
        }

        let bytes = bcs::to_bytes(&sig).expect("signature serialization should not fail");
        self.randomness_tx
            .send((epoch, round, bytes))
            .await
            .expect("randomness_tx should never be closed");
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
        for ((epoch, round), _) in &self.pending_tasks {
            if epoch > &self.epoch {
                break; // wait for DKG in new epoch
            }

            const MAX_CONCURRENT_SEND_PARTIAL_SIGNATURES: usize = 5;
            if self.send_tasks.len() >= MAX_CONCURRENT_SEND_PARTIAL_SIGNATURES {
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

            self.send_tasks.entry((*epoch, *round)).or_insert_with(|| {
                let network = self.network.clone();
                let authority_info = self.authority_info.clone();
                let epoch = *epoch;
                let round = *round;
                let sigs = ThresholdBls12381MinSig::partial_sign_batch(
                    shares.iter(),
                    &round.signature_message(),
                );
                debug!("sending partial sigs for epoch {epoch}, round {round}");
                spawn_monitored_task!(RandomnessEventLoop::send_partial_signatures_task(
                    network,
                    authority_info,
                    epoch,
                    round,
                    sigs
                ))
            });
        }

        if let Some(last_handled_key) = last_handled_key {
            // Remove stuff from the pending_tasks map that we've handled.
            let split_point = self
                .pending_tasks
                .range((Bound::Excluded(last_handled_key), Bound::Unbounded))
                .next()
                .map(|(k, _)| *k);
            if let Some(key) = split_point {
                let mut keep_tasks = self.pending_tasks.split_off(&key);
                std::mem::swap(&mut self.pending_tasks, &mut keep_tasks);
            } else {
                self.pending_tasks.clear();
            }
        }
    }

    async fn send_partial_signatures_task(
        network: anemo::Network,
        authority_info: Arc<HashMap<AuthorityName, (PeerId, PartyId)>>,
        epoch: EpochId,
        round: RandomnessRound,
        sigs: Vec<RandomnessPartialSignature>,
    ) {
        let peers: HashMap<_, _> = authority_info
            .iter()
            .map(|(name, (peer_id, _party_id))| (name, network.waiting_peer(peer_id.clone())))
            .collect();
        let sigs: Vec<_> = sigs
            .iter()
            .map(|sig| bcs::to_bytes(sig).expect("message serialization should not fail"))
            .collect();

        loop {
            let mut requests = FuturesUnordered::new();
            for (name, peer) in &peers {
                let mut client = RandomnessClient::new(peer.clone());
                const SEND_PARTIAL_SIGNATURES_TIMEOUT: Duration = Duration::from_secs(10);
                let request = anemo::Request::new(SendPartialSignaturesRequest {
                    epoch,
                    round,
                    sigs: sigs.clone(),
                })
                .with_timeout(SEND_PARTIAL_SIGNATURES_TIMEOUT);
                requests.push(async move {
                    let result = client.send_partial_signatures(request).await;
                    if let Err(e) = result {
                        debug!("failed to send partial signatures to {name}: {e:?}");
                    }
                });
            }

            while let Some(_) = requests.next().await {
                // Process all requests.
            }

            // Keep retrying send to all peers until task is aborted via external message.
            const SEND_PARTIAL_SIGNATURES_RETRY_TIME: Duration = Duration::from_secs(5);
            tokio::time::sleep(SEND_PARTIAL_SIGNATURES_RETRY_TIME).await;
        }
    }
}
