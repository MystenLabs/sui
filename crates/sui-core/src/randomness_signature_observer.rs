// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bytes::Bytes;
use consensus_core::AuxiliaryDataHandler;
use fastcrypto::groups::bls12381;
use fastcrypto_tbls::{tbls::ThresholdBls, types::ThresholdBls12381MinSig};
use mysten_metrics::monitored_mpsc;
use sui_types::committee::EpochId;
use sui_types::crypto::{RandomnessRound, RandomnessSignature};
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

use crate::authority::RandomnessSignatureMessage;

const CHANNEL_SIZE: usize = 1000;

/// Implements [`AuxiliaryDataHandler`] so consensus
/// can forward auxiliary data directly. Internally delegates to a bounded
/// monitored channel consumed by the background worker task.
///
/// Also exposes [`update_epoch`](Self::update_epoch) so `RandomnessManager`
/// can supply the VSS public key once observer DKG completes.
pub struct RandomnessSignatureObserver {
    data_tx: monitored_mpsc::Sender<Bytes>,
    vss_pk_tx: watch::Sender<Option<bls12381::G2Element>>,
}

impl RandomnessSignatureObserver {
    /// Sets the VSS public key for signature verification. Should only be called
    /// once per epoch, when observer DKG completes in `RandomnessManager::advance_dkg()`.
    pub fn set_public_key(&self, vss_pk: bls12381::G2Element) {
        if self.vss_pk_tx.borrow().is_some() {
            return;
        }
        self.vss_pk_tx.send(Some(vss_pk)).ok();
    }
}

impl AuxiliaryDataHandler for RandomnessSignatureObserver {
    fn handle(&self, data: Bytes) {
        if let Err(e) = self.data_tx.try_send(data) {
            warn!("RandomnessSignatureObserver: failed to forward auxiliary data: {e}");
        }
    }
}

/// Background task that receives randomness signatures, verifies them using the
/// DKG output's VSS public key, and forwards valid signatures to
/// `RandomnessRoundReceiver` for transaction creation.
struct RandomnessSignatureWorker {
    randomness_tx: mpsc::Sender<(EpochId, RandomnessRound, Vec<u8>)>,
    vss_pk_rx: watch::Receiver<Option<bls12381::G2Element>>,
}

impl RandomnessSignatureObserver {
    pub fn start(randomness_tx: mpsc::Sender<(EpochId, RandomnessRound, Vec<u8>)>) -> Self {
        let (data_tx, data_rx) =
            monitored_mpsc::channel("randomness_signature_observer", CHANNEL_SIZE);
        let (vss_pk_tx, vss_pk_rx) = watch::channel(None);

        let worker = RandomnessSignatureWorker {
            randomness_tx,
            vss_pk_rx,
        };
        mysten_metrics::spawn_monitored_task!(worker.run(data_rx));

        Self { data_tx, vss_pk_tx }
    }
}

impl RandomnessSignatureWorker {
    async fn run(mut self, mut data_rx: monitored_mpsc::Receiver<Bytes>) {
        info!("RandomnessSignatureWorker: waiting for DKG to complete");

        // Wait until vss_pk is available (DKG completion).
        loop {
            if self.vss_pk_rx.borrow_and_update().is_some() {
                break;
            }
            if self.vss_pk_rx.changed().await.is_err() {
                info!(
                    "RandomnessSignatureWorker: handle dropped before DKG completed, shutting down"
                );
                return;
            }
        }

        let vss_pk = (*self.vss_pk_rx.borrow()).expect("checked above that vss_pk is Some");

        info!("RandomnessSignatureWorker: DKG complete, processing signatures");

        while let Some(data) = data_rx.recv().await {
            self.process_signature(&vss_pk, &data);
        }

        info!("RandomnessSignatureWorker: channel closed, shutting down");
    }

    fn process_signature(&self, vss_pk: &bls12381::G2Element, data: &Bytes) {
        let msg: RandomnessSignatureMessage = match bcs::from_bytes(data) {
            Ok(msg) => msg,
            Err(e) => {
                warn!("RandomnessSignatureWorker: failed to deserialize signature message: {e}");
                return;
            }
        };

        let sig: RandomnessSignature = match bcs::from_bytes(&msg.signature_bytes) {
            Ok(sig) => sig,
            Err(e) => {
                warn!(
                    "RandomnessSignatureWorker: failed to deserialize signature \
                     for epoch {} round {}: {e}",
                    msg.epoch, msg.round
                );
                return;
            }
        };

        if let Err(e) =
            ThresholdBls12381MinSig::verify(vss_pk, &msg.round.signature_message(), &sig)
        {
            warn!(
                "RandomnessSignatureWorker: invalid signature \
                 for epoch {} round {}: {e}",
                msg.epoch, msg.round
            );
            return;
        }

        debug!(
            "RandomnessSignatureWorker: verified signature for epoch {} round {}",
            msg.epoch, msg.round
        );

        if self
            .randomness_tx
            .try_send((msg.epoch, msg.round, msg.signature_bytes))
            .is_err()
        {
            warn!(
                "RandomnessSignatureWorker: failed to forward signature \
                 for epoch {} round {} (channel full or closed)",
                msg.epoch, msg.round
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fastcrypto::groups::{GroupElement, HashToGroupElement, bls12381};

    /// Helper: generate a (private_key, public_key) pair for BLS12-381 min-sig.
    fn generate_keypair() -> (bls12381::Scalar, bls12381::G2Element) {
        let sk = bls12381::Scalar::generator();
        let pk = bls12381::G2Element::generator() * sk;
        (sk, pk)
    }

    /// Helper: sign a message with BLS min-sig. Signature = H(msg) * sk.
    fn sign(sk: &bls12381::Scalar, msg: &[u8]) -> RandomnessSignature {
        bls12381::G1Element::hash_to_group_element(msg) * sk
    }

    /// Helper: build a BCS-encoded RandomnessSignatureMessage.
    fn build_message(epoch: EpochId, round: RandomnessRound, sig: &RandomnessSignature) -> Bytes {
        let msg = RandomnessSignatureMessage {
            epoch,
            round,
            signature_bytes: bcs::to_bytes(sig).unwrap(),
        };
        Bytes::from(bcs::to_bytes(&msg).unwrap())
    }

    #[tokio::test]
    async fn test_valid_signature_forwarded() {
        let (sk, pk) = generate_keypair();
        let round = RandomnessRound(1);
        let sig = sign(&sk, &round.signature_message());
        let data = build_message(0, round, &sig);

        let (randomness_tx, mut randomness_rx) = mpsc::channel(10);
        let observer = RandomnessSignatureObserver::start(randomness_tx);
        observer.set_public_key(pk);

        // Give the worker time to start and receive the vss_pk.
        tokio::task::yield_now().await;

        observer.handle(data);

        let (epoch, received_round, sig_bytes) =
            tokio::time::timeout(std::time::Duration::from_secs(5), randomness_rx.recv())
                .await
                .expect("timed out")
                .expect("channel closed");

        assert_eq!(epoch, 0);
        assert_eq!(received_round, round);
        assert_eq!(sig_bytes, bcs::to_bytes(&sig).unwrap());
    }

    #[tokio::test]
    async fn test_invalid_signature_dropped() {
        let (_sk, pk) = generate_keypair();
        let round = RandomnessRound(1);

        // Sign with a different key so verification fails.
        let wrong_sk = bls12381::Scalar::generator() + bls12381::Scalar::generator();
        let bad_sig = sign(&wrong_sk, &round.signature_message());
        let data = build_message(0, round, &bad_sig);

        let (randomness_tx, mut randomness_rx) = mpsc::channel(10);
        let observer = RandomnessSignatureObserver::start(randomness_tx);
        observer.set_public_key(pk);

        tokio::task::yield_now().await;

        observer.handle(data);

        // Should not receive anything.
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), randomness_rx.recv()).await;
        assert!(result.is_err(), "should have timed out (no message)");
    }

    #[tokio::test]
    async fn test_malformed_data_dropped() {
        let (_sk, pk) = generate_keypair();

        let (randomness_tx, mut randomness_rx) = mpsc::channel(10);
        let observer = RandomnessSignatureObserver::start(randomness_tx);
        observer.set_public_key(pk);

        tokio::task::yield_now().await;

        // Send garbage bytes.
        observer.handle(Bytes::from(vec![0u8; 32]));

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), randomness_rx.recv()).await;
        assert!(result.is_err(), "should have timed out (no message)");
    }

    #[tokio::test]
    async fn test_data_buffered_until_dkg_completes() {
        let (sk, pk) = generate_keypair();
        let round = RandomnessRound(5);
        let sig = sign(&sk, &round.signature_message());
        let data = build_message(0, round, &sig);

        let (randomness_tx, mut randomness_rx) = mpsc::channel(10);
        let observer = RandomnessSignatureObserver::start(randomness_tx);

        // Send data BEFORE setting the public key (DKG not yet complete).
        observer.handle(data);

        // Nothing should be forwarded yet.
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), randomness_rx.recv()).await;
        assert!(result.is_err(), "should have timed out (DKG not complete)");

        // Now complete DKG.
        observer.set_public_key(pk);

        // The buffered message should now be processed.
        let (epoch, received_round, _) =
            tokio::time::timeout(std::time::Duration::from_secs(5), randomness_rx.recv())
                .await
                .expect("timed out")
                .expect("channel closed");

        assert_eq!(epoch, 0);
        assert_eq!(received_round, round);
    }

    #[tokio::test]
    async fn test_set_public_key_only_once() {
        let (sk, pk) = generate_keypair();
        let other_sk = bls12381::Scalar::generator() + bls12381::Scalar::generator();
        let other_pk = bls12381::G2Element::generator() * other_sk;

        let round = RandomnessRound(1);
        let sig = sign(&sk, &round.signature_message());
        let data = build_message(0, round, &sig);

        let (randomness_tx, mut randomness_rx) = mpsc::channel(10);
        let observer = RandomnessSignatureObserver::start(randomness_tx);

        // Set the correct key first.
        observer.set_public_key(pk);
        // Attempt to overwrite with a different key -- should be ignored.
        observer.set_public_key(other_pk);

        tokio::task::yield_now().await;

        observer.handle(data);

        // Should still verify with the original key.
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), randomness_rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        assert_eq!(result.1, round);
    }

    #[tokio::test]
    async fn test_multiple_rounds() {
        let (sk, pk) = generate_keypair();

        let (randomness_tx, mut randomness_rx) = mpsc::channel(10);
        let observer = RandomnessSignatureObserver::start(randomness_tx);
        observer.set_public_key(pk);

        tokio::task::yield_now().await;

        for i in 0..5u64 {
            let round = RandomnessRound(i);
            let sig = sign(&sk, &round.signature_message());
            observer.handle(build_message(1, round, &sig));
        }

        for i in 0..5u64 {
            let (epoch, received_round, _) =
                tokio::time::timeout(std::time::Duration::from_secs(5), randomness_rx.recv())
                    .await
                    .expect("timed out")
                    .expect("channel closed");
            assert_eq!(epoch, 1);
            assert_eq!(received_round, RandomnessRound(i));
        }
    }
}
