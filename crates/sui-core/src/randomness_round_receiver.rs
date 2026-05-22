// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

use fastcrypto::groups::bls12381;
use fastcrypto_tbls::{tbls::ThresholdBls, types::ThresholdBls12381MinSig};
use mysten_common::debug_fatal_no_invariant;
use mysten_common::fatal;
use mysten_metrics::spawn_monitored_task;
use serde::{Deserialize, Serialize};
use sui_macros::fail_point_async;
use sui_types::committee::EpochId;
use sui_types::crypto::RandomnessRound;
use sui_types::crypto::RandomnessSignature;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::execution_status::ExecutionStatus;
use sui_types::transaction::{TransactionKey, VerifiedTransaction};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{debug, info, instrument, warn};

use crate::authority::AuthorityState;
use crate::authority::epoch_start_configuration::EpochStartConfigTrait;

/// A randomness signature for a specific epoch and round, broadcast to observer peers.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RandomnessSignatureMessage {
    pub epoch: EpochId,
    pub round: RandomnessRound,
    pub signature_bytes: Vec<u8>,
}

const SIGNATURES_BROADCAST_CAPACITY: usize = 1000;
const AUXILIARY_DATA_CHANNEL_SIZE: usize = 1000;
const EXECUTED_ROUNDS_CACHE_CAPACITY: usize = 1000;

pub struct RandomnessRoundReceiverHandle {
    consensus_signatures_tx: mysten_metrics::monitored_mpsc::Sender<bytes::Bytes>,
    vss_pk_tx: watch::Sender<Option<bls12381::G2Element>>,
    signatures_broadcast: broadcast::Sender<bytes::Bytes>,
    #[cfg(test)]
    executed_consensus_rounds: Arc<Mutex<BTreeSet<(EpochId, RandomnessRound)>>>,
    _task_handle: JoinHandle<()>,
}

impl RandomnessRoundReceiverHandle {
    /// Sets the VSS public key for signature verification. Called by
    /// `RandomnessManager::advance_dkg` when DKG completes.
    pub fn set_public_key(&self, vss_pk: bls12381::G2Element) {
        self.vss_pk_tx.send(Some(vss_pk)).ok();
    }

    /// Clears the VSS public key. Called at epoch start before the new DKG
    /// completes. While the key is `None`, incoming randomness round signatues from consensus
    /// accumulate in the bounded channel giving the opportunity to buffer them until DKG completes.
    pub fn clear_public_key(&self) {
        self.vss_pk_tx.send(None).ok();
    }
}

impl RandomnessRoundReceiverHandle {
    pub fn new_for_testing() -> Arc<Self> {
        let (consensus_signatures_tx, _) =
            mysten_metrics::monitored_mpsc::channel("test_auxiliary", 1);
        let (vss_pk_tx, vss_pk_rx) = watch::channel(None);
        let (signatures_broadcast, _) = broadcast::channel(1);
        Arc::new(Self {
            consensus_signatures_tx,
            vss_pk_tx,
            signatures_broadcast,
            #[cfg(test)]
            executed_consensus_rounds: Arc::new(Mutex::new(BTreeSet::new())),
            _task_handle: tokio::spawn(async move {
                let _vss_pk_rx = vss_pk_rx;
                futures::future::pending::<()>().await;
            }),
        })
    }

    #[cfg(test)]
    pub(crate) fn public_key_for_testing(&self) -> Option<bls12381::G2Element> {
        let rx = self.vss_pk_tx.subscribe();
        *rx.borrow()
    }

    #[cfg(test)]
    pub(crate) fn mark_round_executed(&self, epoch: EpochId, round: RandomnessRound) {
        self.executed_consensus_rounds.lock().insert((epoch, round));
    }
}

impl consensus_core::RandomnessSignatureHandler for RandomnessRoundReceiverHandle {
    fn handle_randomness_signature(&self, data: bytes::Bytes) {
        if let Err(e) = self.consensus_signatures_tx.try_send(data) {
            warn!(
                "RandomnessRoundReceiverHandle: failed to forward randomness round signature: {e}"
            );
        }
    }

    fn subscribe_randomness_signatures(&self) -> broadcast::Receiver<bytes::Bytes> {
        self.signatures_broadcast.subscribe()
    }
}

pub struct RandomnessRoundReceiver {
    authority_state: Arc<AuthorityState>,
    randomness_rx: mpsc::Receiver<(EpochId, RandomnessRound, Vec<u8>)>,
    consensus_signatures_rx: mysten_metrics::monitored_mpsc::Receiver<bytes::Bytes>,
    vss_pk_rx: watch::Receiver<Option<bls12381::G2Element>>,
    /// Best-effort broadcast of verified signatures. Primarily used for propagating the
    /// signatures via consensus to non-committee peers (observers) syncing their state via consensus
    /// from a read-only capacity.
    signatures_broadcast: broadcast::Sender<bytes::Bytes>,
    /// Tracks rounds whose randomness transaction executed successfully via the consensus relay
    /// path. Prevents rebroadcast loops in observer-to-observer topologies (although in practice is not anticipated having such)
    /// while still allowing retries for rounds that failed execution.
    executed_consensus_rounds: Arc<Mutex<BTreeSet<(EpochId, RandomnessRound)>>>,
}

impl RandomnessRoundReceiver {
    /// Spawns the receiver loop and returns the shared handle.
    pub fn spawn(
        authority_state: Arc<AuthorityState>,
        randomness_rx: mpsc::Receiver<(EpochId, RandomnessRound, Vec<u8>)>,
    ) -> Arc<RandomnessRoundReceiverHandle> {
        let (signatures_broadcast, _) =
            broadcast::channel::<bytes::Bytes>(SIGNATURES_BROADCAST_CAPACITY);

        let (consensus_signatures_tx, consensus_signatures_rx) =
            mysten_metrics::monitored_mpsc::channel(
                "consensus_randomness_round_signatures",
                AUXILIARY_DATA_CHANNEL_SIZE,
            );
        let (vss_pk_tx, vss_pk_rx) = watch::channel(None);
        let executed_consensus_rounds = Arc::new(Mutex::new(BTreeSet::new()));

        let rrr = RandomnessRoundReceiver {
            authority_state,
            randomness_rx,
            consensus_signatures_rx,
            vss_pk_rx,
            signatures_broadcast: signatures_broadcast.clone(),
            executed_consensus_rounds: executed_consensus_rounds.clone(),
        };
        let task_handle = spawn_monitored_task!(rrr.run());

        Arc::new(RandomnessRoundReceiverHandle {
            consensus_signatures_tx,
            vss_pk_tx,
            signatures_broadcast,
            #[cfg(test)]
            executed_consensus_rounds,
            _task_handle: task_handle,
        })
    }

    async fn run(mut self) {
        info!("RandomnessRoundReceiver event loop started");

        loop {
            let vss_pk = *self.vss_pk_rx.borrow_and_update();
            tokio::select! {
                maybe_recv = self.randomness_rx.recv() => {
                    if let Some((epoch, round, bytes)) = maybe_recv {
                        self.handle_new_randomness(epoch, round, bytes).await;
                    } else {
                        break;
                    }
                },
                Some(data) = self.consensus_signatures_rx.recv(), if vss_pk.is_some() => {
                    self.handle_consensus_randomness_signature(vss_pk.unwrap(), &data).await;
                },
                // Wake up when the key changes so we re-evaluate the select guard.
                _ = self.vss_pk_rx.changed() => {},
            }
        }

        info!("RandomnessRoundReceiver event loop ended");
    }

    /// Signatures that are propagated via consensus state sync. This is meant to be used from nodes that are not part of the committee
    /// and are using consensus sync additionally to sync state.
    async fn handle_consensus_randomness_signature(
        &mut self,
        vss_pk: bls12381::G2Element,
        data: &bytes::Bytes,
    ) {
        let msg: RandomnessSignatureMessage = match bcs::from_bytes(data) {
            Ok(msg) => msg,
            Err(e) => {
                warn!("RandomnessRoundReceiver: failed to deserialize round signature: {e}");
                return;
            }
        };

        let sig: RandomnessSignature = match bcs::from_bytes(&msg.signature_bytes) {
            Ok(sig) => sig,
            Err(e) => {
                warn!(
                    "RandomnessRoundReceiver: failed to deserialize signature \
                     for epoch {} round {}: {e}",
                    msg.epoch, msg.round
                );
                return;
            }
        };

        if let Err(e) =
            ThresholdBls12381MinSig::verify(&vss_pk, &msg.round.signature_message(), &sig)
        {
            warn!(
                "RandomnessRoundReceiver: invalid auxiliary signature \
                 for epoch {} round {}: {e}",
                msg.epoch, msg.round
            );
            return;
        }

        if self
            .executed_consensus_rounds
            .lock()
            .contains(&(msg.epoch, msg.round))
        {
            info!(
                "RandomnessRoundReceiver: dropping already-executed auxiliary signature for epoch {} round {}",
                msg.epoch, msg.round
            );
            return;
        }

        debug!(
            "RandomnessRoundReceiver: verified auxiliary signature for epoch {} round {}",
            msg.epoch, msg.round
        );

        self.handle_new_randomness(msg.epoch, msg.round, msg.signature_bytes)
            .await;
    }

    #[instrument(level = "debug", skip_all, fields(?epoch, ?round))]
    async fn handle_new_randomness(&self, epoch: EpochId, round: RandomnessRound, bytes: Vec<u8>) {
        fail_point_async!("randomness-delay");

        let epoch_store = self.authority_state.load_epoch_store_one_call_per_task();
        if epoch_store.epoch() != epoch {
            warn!(
                "dropping randomness for epoch {epoch}, round {round}, because we are in epoch {}",
                epoch_store.epoch()
            );
            return;
        }
        // Broadcast signature to connected observer peers so they can create the same
        // transaction. This enables observer-to-observer relay chains.
        let msg = RandomnessSignatureMessage {
            epoch,
            round,
            signature_bytes: bytes.clone(),
        };
        match bcs::to_bytes(&msg) {
            Ok(encoded) => {
                let _ = self.signatures_broadcast.send(bytes::Bytes::from(encoded));
            }
            Err(err) => {
                warn!("serialisation of RandomnessSignatureMessage failed: {err}");
            }
        }

        let key = TransactionKey::RandomnessRound(epoch, round);
        let transaction = VerifiedTransaction::new_randomness_state_update(
            epoch,
            round,
            bytes,
            epoch_store
                .epoch_start_config()
                .randomness_obj_initial_shared_version()
                .expect("randomness state obj must exist"),
        );
        debug!(
            "created randomness state update transaction with digest: {:?}",
            transaction.digest()
        );
        let transaction = VerifiedExecutableTransaction::new_system(transaction, epoch);
        let digest = *transaction.digest();

        // Randomness state updates contain the full bls signature for the random round,
        // which cannot necessarily be reconstructed again later. Therefore we must immediately
        // persist this transaction. If we crash before its outputs are committed, this
        // ensures we will be able to re-execute it.
        self.authority_state
            .get_cache_commit()
            .persist_transaction(&transaction);

        // Notify the scheduler that the transaction key now has a known digest
        if epoch_store.insert_tx_key(key, digest).is_err() {
            warn!("epoch ended while handling new randomness");
        }

        let authority_state = self.authority_state.clone();
        let executed_consensus_rounds = self.executed_consensus_rounds.clone();
        spawn_monitored_task!(async move {
            // Wait for transaction execution in a separate task, to avoid deadlock in case of
            // out-of-order randomness generation. (Each RandomnessStateUpdate depends on the
            // output of the RandomnessStateUpdate from the previous round.)
            //
            // We set a very long timeout so that in case this gets stuck for some reason, the
            // validator will eventually crash rather than continuing in a zombie mode.
            const RANDOMNESS_STATE_UPDATE_EXECUTION_TIMEOUT: Duration = Duration::from_secs(300);
            let result = tokio::time::timeout(
                RANDOMNESS_STATE_UPDATE_EXECUTION_TIMEOUT,
                authority_state
                    .get_transaction_cache_reader()
                    .notify_read_executed_effects(
                        "RandomnessRoundReceiver::notify_read_executed_effects_first",
                        &[digest],
                    ),
            )
            .await;
            let mut effects = match result {
                Ok(result) => result,
                Err(_) => {
                    // Crash on randomness update execution timeout in debug builds.
                    debug_fatal_no_invariant!(
                        "randomness state update transaction execution timed out at epoch {epoch}, round {round}"
                    );
                    // Continue waiting as long as necessary in non-debug builds.
                    authority_state
                        .get_transaction_cache_reader()
                        .notify_read_executed_effects(
                            "RandomnessRoundReceiver::notify_read_executed_effects_second",
                            &[digest],
                        )
                        .await
                }
            };

            let effects = effects.pop().expect("should return effects");
            if *effects.status() != ExecutionStatus::Success {
                fatal!(
                    "failed to execute randomness state update transaction at epoch {epoch}, round {round}: {effects:?}"
                );
            }
            debug!(
                "successfully executed randomness state update transaction at epoch {epoch}, round {round}"
            );

            let cache = &mut *executed_consensus_rounds.lock();
            cache.insert((epoch, round));
            while cache.len() > EXECUTED_ROUNDS_CACHE_CAPACITY {
                cache.pop_first();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::test_authority_builder::TestAuthorityBuilder;
    use consensus_core::RandomnessSignatureHandler;
    use fastcrypto::groups::{GroupElement, HashToGroupElement, bls12381};

    fn generate_keypair() -> (bls12381::Scalar, bls12381::G2Element) {
        let sk = bls12381::Scalar::generator();
        let pk = bls12381::G2Element::generator() * sk;
        (sk, pk)
    }

    fn sign(sk: &bls12381::Scalar, msg: &[u8]) -> RandomnessSignature {
        bls12381::G1Element::hash_to_group_element(msg) * sk
    }

    fn build_message(
        epoch: EpochId,
        round: RandomnessRound,
        sig: &RandomnessSignature,
    ) -> bytes::Bytes {
        let msg = RandomnessSignatureMessage {
            epoch,
            round,
            signature_bytes: bcs::to_bytes(sig).unwrap(),
        };
        bytes::Bytes::from(bcs::to_bytes(&msg).unwrap())
    }

    #[tokio::test]
    async fn test_invalid_signature_not_broadcast() {
        let (_sk, pk) = generate_keypair();
        let state = TestAuthorityBuilder::new().build().await;
        let epoch = state.epoch_store_for_testing().epoch();

        let (_randomness_tx, randomness_rx) = mpsc::channel(1);
        let handle = RandomnessRoundReceiver::spawn(state, randomness_rx);
        let mut sig_rx = handle.subscribe_randomness_signatures();

        handle.set_public_key(pk);
        tokio::task::yield_now().await;

        let wrong_sk = bls12381::Scalar::generator() + bls12381::Scalar::generator();
        let round = RandomnessRound(1);
        let bad_sig = sign(&wrong_sk, &round.signature_message());
        handle.handle_randomness_signature(build_message(epoch, round, &bad_sig));

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(200), sig_rx.recv()).await;
        assert!(result.is_err(), "should have timed out (no broadcast)");
    }

    #[tokio::test]
    async fn test_malformed_data_not_broadcast() {
        let (_sk, pk) = generate_keypair();
        let state = TestAuthorityBuilder::new().build().await;

        let (_randomness_tx, randomness_rx) = mpsc::channel(1);
        let handle = RandomnessRoundReceiver::spawn(state, randomness_rx);
        let mut sig_rx = handle.subscribe_randomness_signatures();

        handle.set_public_key(pk);
        tokio::task::yield_now().await;

        handle.handle_randomness_signature(bytes::Bytes::from(vec![0u8; 32]));

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(200), sig_rx.recv()).await;
        assert!(result.is_err(), "should have timed out (no broadcast)");
    }

    #[tokio::test]
    async fn test_signatures_buffer_until_dkg_completes() {
        let (sk, pk) = generate_keypair();
        let state = TestAuthorityBuilder::new().build().await;
        let epoch = state.epoch_store_for_testing().epoch();

        let (_randomness_tx, randomness_rx) = mpsc::channel(1);
        let handle = RandomnessRoundReceiver::spawn(state, randomness_rx);
        let mut sig_rx = handle.subscribe_randomness_signatures();

        // Send before DKG completes — should buffer.
        let round = RandomnessRound(1);
        let sig = sign(&sk, &round.signature_message());
        handle.handle_randomness_signature(build_message(epoch, round, &sig));

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(200), sig_rx.recv()).await;
        assert!(result.is_err(), "should have timed out (DKG not complete)");

        // Now complete DKG — buffered message should be processed.
        handle.set_public_key(pk);

        let received = tokio::time::timeout(std::time::Duration::from_secs(5), sig_rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        let decoded: RandomnessSignatureMessage = bcs::from_bytes(&received).unwrap();
        assert_eq!(decoded.epoch, epoch);
        assert_eq!(decoded.round, round);
    }

    #[tokio::test]
    async fn test_executed_consensus_signature_not_rebroadcast() {
        let (sk, pk) = generate_keypair();
        let state = TestAuthorityBuilder::new().build().await;
        let epoch = state.epoch_store_for_testing().epoch();

        let (_randomness_tx, randomness_rx) = mpsc::channel(1);
        let handle = RandomnessRoundReceiver::spawn(state, randomness_rx);
        let mut sig_rx = handle.subscribe_randomness_signatures();

        handle.set_public_key(pk);
        tokio::task::yield_now().await;

        let round = RandomnessRound(1);
        let sig = sign(&sk, &round.signature_message());
        let msg = build_message(epoch, round, &sig);

        // Simulate that round 1 has already been executed successfully.
        handle.mark_round_executed(epoch, round);

        handle.handle_randomness_signature(msg);

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(200), sig_rx.recv()).await;
        assert!(
            result.is_err(),
            "already-executed round should not be rebroadcast"
        );
    }

    #[tokio::test]
    async fn test_clear_public_key_pauses_processing() {
        let (sk, pk) = generate_keypair();
        let state = TestAuthorityBuilder::new().build().await;
        let epoch = state.epoch_store_for_testing().epoch();

        let (_randomness_tx, randomness_rx) = mpsc::channel(1);
        let handle = RandomnessRoundReceiver::spawn(state, randomness_rx);
        let mut sig_rx = handle.subscribe_randomness_signatures();

        // Set key, verify a signature flows through.
        handle.set_public_key(pk);
        tokio::task::yield_now().await;

        let round1 = RandomnessRound(1);
        let sig1 = sign(&sk, &round1.signature_message());
        handle.handle_randomness_signature(build_message(epoch, round1, &sig1));

        tokio::time::timeout(std::time::Duration::from_secs(5), sig_rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        // Clear key — should pause.
        handle.clear_public_key();
        // Yield a few times so the receiver loop picks up the cleared key.
        for _ in 0..5 {
            tokio::task::yield_now().await;
        }

        let round2 = RandomnessRound(2);
        let sig2 = sign(&sk, &round2.signature_message());
        handle.handle_randomness_signature(build_message(epoch, round2, &sig2));

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(200), sig_rx.recv()).await;
        assert!(result.is_err(), "should have timed out (key cleared)");

        // Re-set key — buffered message should be processed.
        handle.set_public_key(pk);

        let received = tokio::time::timeout(std::time::Duration::from_secs(5), sig_rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        let decoded: RandomnessSignatureMessage = bcs::from_bytes(&received).unwrap();
        assert_eq!(decoded.round, round2);
    }
}
