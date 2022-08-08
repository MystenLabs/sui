// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::fixtures::{test_store, test_u64_certificates};
use test_utils::{committee, test_channel};
use types::{Certificate, SequenceNumber};

/// Spawn a mock consensus core and a test subscriber.
async fn spawn_subscriber(
    rx_sequence: metered_channel::Receiver<ConsensusOutput>,
    tx_batch_loader: metered_channel::Sender<ConsensusOutput>,
    tx_executor: metered_channel::Sender<ConsensusOutput>,
) -> (
    Store<BatchDigest, SerializedBatchMessage>,
    watch::Sender<ReconfigureNotification>,
) {
    let committee = committee(None);
    let message = ReconfigureNotification::NewEpoch(committee);
    let (tx_reconfigure, rx_reconfigure) = watch::channel(message);

    // Spawn a test subscriber.
    let store = test_store();
    let _subscriber_handle = Subscriber::spawn(
        store.clone(),
        rx_reconfigure,
        rx_sequence,
        tx_batch_loader,
        tx_executor,
    );

    (store, tx_reconfigure)
}

#[tokio::test]
async fn handle_certificate_with_downloaded_batch() {
    let (tx_sequence, rx_sequence) = test_channel!(10);
    let (tx_batch_loader, mut rx_batch_loader) = test_channel!(10);
    let (tx_executor, mut rx_executor) = test_channel!(10);

    // Spawn a subscriber.
    let (store, _tx_reconfigure) =
        spawn_subscriber(rx_sequence, tx_batch_loader, tx_executor).await;

    // Feed certificates to the mock sequencer and ensure the batch loader receive the command to
    // download the corresponding transaction data.
    let total_certificates = 2;
    let certificates = test_u64_certificates(
        total_certificates,
        /* batches_per_certificate */ 2,
        /* transactions_per_batch */ 2,
    );
    for (i, (certificate, batches)) in certificates.into_iter().enumerate() {
        for (digest, batch) in batches {
            store.write(digest, batch).await;
        }
        let message = ConsensusOutput {
            certificate,
            consensus_index: i as SequenceNumber,
        };
        tx_sequence.send(message).await.unwrap();
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
    let (tx_sequence, rx_sequence) = test_channel!(10);
    let (tx_batch_loader, mut rx_batch_loader) = test_channel!(10);
    let (tx_executor, mut rx_executor) = test_channel!(10);

    // Spawn a subscriber.
    let _do_not_drop = spawn_subscriber(rx_sequence, tx_batch_loader, tx_executor).await;

    // Feed certificates to the mock sequencer and ensure the batch loader receive the command to
    // download the corresponding transaction data.
    for i in 0..2 {
        let message = ConsensusOutput {
            certificate: Certificate::default(),
            consensus_index: i as SequenceNumber,
        };
        tx_sequence.send(message).await.unwrap();
    }
    for i in 0..2 {
        let output = rx_batch_loader.recv().await.unwrap();
        assert_eq!(output.consensus_index, i);

        let output = rx_executor.recv().await.unwrap();
        assert_eq!(output.consensus_index, i);
    }
}
