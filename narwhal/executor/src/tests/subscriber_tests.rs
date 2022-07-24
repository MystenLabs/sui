// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    fixtures::{test_store, test_u64_certificates},
    sequencer::MockSequencer,
};
use crypto::ed25519::Ed25519PublicKey;
use test_utils::committee;
use tokio::sync::mpsc::{channel, Sender};
use types::Certificate;

/// Spawn a mock consensus core and a test subscriber.
async fn spawn_consensus_and_subscriber(
    rx_sequence: Receiver<Certificate<Ed25519PublicKey>>,
    tx_batch_loader: Sender<ConsensusOutput<Ed25519PublicKey>>,
    tx_executor: Sender<ConsensusOutput<Ed25519PublicKey>>,
) -> (
    Store<BatchDigest, SerializedBatchMessage>,
    watch::Sender<ReconfigureNotification<Ed25519PublicKey>>,
) {
    let (tx_consensus_to_client, rx_consensus_to_client) = channel(10);
    let (tx_client_to_consensus, rx_client_to_consensus) = channel(10);

    let committee = committee(None);
    let message = ReconfigureNotification::NewCommittee(committee);
    let (tx_reconfigure, rx_reconfigure) = watch::channel(message);

    // Spawn a mock consensus core.
    MockSequencer::spawn(rx_sequence, rx_client_to_consensus, tx_consensus_to_client);

    // Spawn a test subscriber.
    let store = test_store();
    let next_consensus_index = SequenceNumber::default();
    Subscriber::<Ed25519PublicKey>::spawn(
        store.clone(),
        rx_reconfigure,
        rx_consensus_to_client,
        tx_client_to_consensus,
        tx_batch_loader,
        tx_executor,
        next_consensus_index,
    );

    (store, tx_reconfigure)
}

#[tokio::test]
async fn handle_certificate_with_downloaded_batch() {
    let (tx_sequence, rx_sequence) = channel(10);
    let (tx_batch_loader, mut rx_batch_loader) = channel(10);
    let (tx_executor, mut rx_executor) = channel(10);

    // Spawn a subscriber.
    let (store, _tx_reconfigure) =
        spawn_consensus_and_subscriber(rx_sequence, tx_batch_loader, tx_executor).await;

    // Feed certificates to the mock sequencer and ensure the batch loader receive the command to
    // download the corresponding transaction data.
    let total_certificates = 2;
    let certificates = test_u64_certificates(
        total_certificates,
        /* batches_per_certificate */ 2,
        /* transactions_per_batch */ 2,
    );
    for (certificate, batches) in certificates {
        for (digest, batch) in batches {
            store.write(digest, batch).await;
        }
        tx_sequence.send(certificate).await.unwrap();
    }

    for i in 0..total_certificates {
        let output = rx_batch_loader.recv().await.unwrap();
        assert_eq!(output.consensus_index, i as SequenceNumber);

        let output = rx_executor.recv().await.unwrap();
        assert_eq!(output.consensus_index, i as SequenceNumber);
    }
}

#[tokio::test]
async fn handle_empty_certificate() {
    let (tx_sequence, rx_sequence) = channel(10);
    let (tx_batch_loader, mut rx_batch_loader) = channel(10);
    let (tx_executor, mut rx_executor) = channel(10);

    // Spawn a subscriber.
    let _do_not_drop =
        spawn_consensus_and_subscriber(rx_sequence, tx_batch_loader, tx_executor).await;

    // Feed certificates to the mock sequencer and ensure the batch loader receive the command to
    // download the corresponding transaction data.
    for _ in 0..2 {
        tx_sequence.send(Certificate::default()).await.unwrap();
    }
    for i in 0..2 {
        let output = rx_batch_loader.recv().await.unwrap();
        assert_eq!(output.consensus_index, i);

        let output = rx_executor.recv().await.unwrap();
        assert_eq!(output.consensus_index, i);
    }
}

#[tokio::test]
async fn synchronize() {
    let (tx_sequence, rx_sequence) = channel(10);
    let (tx_batch_loader, mut rx_batch_loader) = channel(10);
    let (tx_executor, mut rx_executor) = channel(10);
    let (tx_consensus_to_client, rx_consensus_to_client) = channel(10);
    let (tx_client_to_consensus, rx_client_to_consensus) = channel(10);

    let committee = committee(None);
    let message = ReconfigureNotification::NewCommittee(committee);
    let (_tx_reconfigure, rx_reconfigure) = watch::channel(message);

    // Spawn a mock consensus core.
    MockSequencer::spawn(rx_sequence, rx_client_to_consensus, tx_consensus_to_client);

    // Send two certificates.
    for _ in 0..2 {
        tx_sequence.send(Certificate::default()).await.unwrap();
    }
    tokio::task::yield_now().await;

    // Spawn a subscriber.
    let store = test_store();
    let next_consensus_index = SequenceNumber::default();
    Subscriber::<Ed25519PublicKey>::spawn(
        store.clone(),
        rx_reconfigure,
        rx_consensus_to_client,
        tx_client_to_consensus,
        tx_batch_loader,
        tx_executor,
        next_consensus_index,
    );

    // Send two extra certificates. The client needs to sync for the first two certificates.
    for _ in 0..2 {
        tx_sequence.send(Certificate::default()).await.unwrap();
    }

    // Ensure the client synchronizes the first twi certificates.
    for i in 0..4 {
        let output = rx_batch_loader.recv().await.unwrap();
        assert_eq!(output.consensus_index, i);

        let output = rx_executor.recv().await.unwrap();
        assert_eq!(output.consensus_index, i);
    }
}
