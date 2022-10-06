// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;

use prometheus::Registry;
use store::rocks;
use test_utils::{temp_dir, transaction, CommitteeFixture};

fn create_batches_store() -> Store<BatchDigest, Batch> {
    let db = rocks::DBMap::<BatchDigest, Batch>::open(temp_dir(), None, Some("batches")).unwrap();
    Store::new(db)
}

#[tokio::test]
async fn make_batch() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let store = create_batches_store();
    let (_tx_reconfiguration, rx_reconfiguration) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_batch_maker, rx_batch_maker) = test_utils::test_channel!(1);
    let (tx_message, mut rx_message) = test_utils::test_channel!(1);
    let (tx_digest, mut rx_digest) = test_utils::test_channel!(1);
    let node_metrics = WorkerMetrics::new(&Registry::new());

    // Spawn a `BatchMaker` instance.
    let id = 0;
    let _batch_maker_handle = BatchMaker::spawn(
        id,
        committee,
        /* max_batch_size */ 200,
        /* max_batch_delay */
        Duration::from_millis(1_000_000), // Ensure the timer is not triggered.
        rx_reconfiguration,
        rx_batch_maker,
        tx_message,
        Arc::new(node_metrics),
        store,
        tx_digest,
    );

    // Send enough transactions to seal a batch.
    let tx = transaction();
    tx_batch_maker.send(tx.clone()).await.unwrap();
    tx_batch_maker.send(tx.clone()).await.unwrap();

    // Ensure the batch is as expected.
    let expected_batch = Batch::new(vec![tx.clone(), tx.clone()]);
    let (batch, overall_response) = rx_message.recv().await.unwrap();

    assert_eq!(batch.transactions, expected_batch.transactions);

    // Eventually deliver message
    if let Some(resp) = overall_response {
        assert!(resp.send(()).is_ok());
    }

    // Now we send to primary
    let _message = rx_digest.recv().await.unwrap();
}

#[tokio::test]
async fn batch_timeout() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let store = create_batches_store();
    let (_tx_reconfiguration, rx_reconfiguration) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_batch_maker, rx_batch_maker) = test_utils::test_channel!(1);
    let (tx_message, mut rx_message) = test_utils::test_channel!(1);
    let node_metrics = WorkerMetrics::new(&Registry::new());
    let (tx_digest, _rx_digest) = test_utils::test_channel!(1);

    // Spawn a `BatchMaker` instance.
    let id = 0;
    let _batch_maker_handle = BatchMaker::spawn(
        id,
        committee,
        /* max_batch_size */ 200,
        /* max_batch_delay */
        Duration::from_millis(50), // Ensure the timer is triggered.
        rx_reconfiguration,
        rx_batch_maker,
        tx_message,
        Arc::new(node_metrics),
        store,
        tx_digest,
    );

    // Do not send enough transactions to seal a batch.
    let tx = transaction();
    tx_batch_maker.send(tx.clone()).await.unwrap();

    // Ensure the batch is as expected.
    let (batch, overall_response) = rx_message.recv().await.unwrap();
    let expected_batch = Batch::new(vec![tx.clone()]);
    assert_eq!(batch.transactions, expected_batch.transactions);
}
