// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;

use crate::NUM_SHUTDOWN_RECEIVERS;
use store::rocks;
use store::rocks::ReadWriteOptions;
use test_utils::{temp_dir, transaction};
use types::PreSubscribedBroadcastSender;

fn create_batches_store() -> Store<BatchDigest, Batch> {
    let db = rocks::DBMap::<BatchDigest, Batch>::open(
        temp_dir(),
        None,
        Some("batches"),
        &ReadWriteOptions::default(),
    )
    .unwrap();
    Store::new(db)
}

#[tokio::test]
async fn make_batch() {
    let client = create_network_client();
    let store = create_batch_store();
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_batch_maker, rx_batch_maker) = test_utils::test_channel!(1);
    let (tx_message, mut rx_message) = test_utils::test_channel!(1);
    let (tx_digest, mut rx_digest) = test_utils::test_channel!(1);

    // Mock the primary client to always succeed.
    let mut mock_server = MockWorkerToPrimary::new();
    mock_server
        .expect_report_our_batch()
        .returning(|_| Ok(anemo::Response::new(())));
    client.set_worker_to_primary_local_handler(Arc::new(mock_server));

    // Spawn a `BatchMaker` instance.
    let id = 0;
    let _batch_maker_handle = BatchMaker::spawn(
        id,
        /* max_batch_size */ 200,
        /* max_batch_delay */
        Duration::from_millis(1_000_000), // Ensure the timer is not triggered.
        tx_shutdown.subscribe(),
        rx_batch_maker,
        tx_message,
        store.clone(),
    );

    // Send enough transactions to seal a batch.
    let tx = transaction();
    let (s0, r0) = tokio::sync::oneshot::channel();
    let (s1, r1) = tokio::sync::oneshot::channel();
    tx_batch_maker.send((tx.clone(), s0)).await.unwrap();
    tx_batch_maker.send((tx.clone(), s1)).await.unwrap();

    // Ensure the batch is as expected.
    let expected_batch = Batch::new(vec![tx.clone(), tx.clone()]);
    let (batch, resp) = rx_quorum_waiter.recv().await.unwrap();

    assert_eq!(batch.transactions(), expected_batch.transactions());

    // Eventually deliver message
    assert!(resp.send(()).is_ok());

    // Batch maker should finish creating the batch.
    assert!(r0.await.is_ok());
    assert!(r1.await.is_ok());

    // Ensure the batch is stored
    assert!(store.get(&expected_batch.digest()).unwrap().is_some());
}

#[tokio::test]
async fn batch_timeout() {
    let client = create_network_client();
    let store = create_batch_store();
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_batch_maker, rx_batch_maker) = test_utils::test_channel!(1);
    let (tx_message, mut rx_message) = test_utils::test_channel!(1);
    let (tx_digest, mut rx_digest) = test_utils::test_channel!(1);

    // Spawn a `BatchMaker` instance.
    let id = 0;
    let _batch_maker_handle = BatchMaker::spawn(
        id,
        /* max_batch_size */ 200,
        /* max_batch_delay */
        Duration::from_millis(50), // Ensure the timer is triggered.
        tx_shutdown.subscribe(),
        rx_batch_maker,
        tx_message,
        store.clone(),
    );

    // Do not send enough transactions to seal a batch.
    let tx = transaction();
    let (s0, r0) = tokio::sync::oneshot::channel();
    tx_batch_maker.send((tx.clone(), s0)).await.unwrap();

    // Ensure the batch is as expected.
    let (batch, resp) = rx_quorum_waiter.recv().await.unwrap();
    let expected_batch = Batch::new(vec![tx.clone()]);
    assert_eq!(batch.transactions(), expected_batch.transactions());

    // Eventually deliver message
    assert!(resp.send(()).is_ok());

    // Batch maker should finish creating the batch.
    assert!(r0.await.is_ok());

    // Ensure the batch is stored
    assert!(store.get(&batch.digest()).unwrap().is_some());
}
