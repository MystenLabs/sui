// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;

use crate::NUM_SHUTDOWN_RECEIVERS;
use prometheus::Registry;
use test_utils::{create_batch_store, latest_protocol_version, transaction};
use types::MockWorkerToPrimary;
use types::PreSubscribedBroadcastSender;

fn create_network_client() -> NetworkClient {
    NetworkClient::new_with_empty_id()
}

#[tokio::test]
async fn make_batch() {
    let client = create_network_client();
    let store = create_batch_store();
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_batch_maker, rx_batch_maker) = test_utils::test_channel!(1);
    let (tx_quorum_waiter, mut rx_quorum_waiter) = test_utils::test_channel!(1);
    let node_metrics = WorkerMetrics::new(&Registry::new());

    // Mock the primary client to always succeed.
    let mut mock_server = MockWorkerToPrimary::new();
    mock_server
        .expect_report_own_batch()
        .returning(|_| Ok(anemo::Response::new(())));
    client.set_worker_to_primary_local_handler(Arc::new(mock_server));

    // Spawn a `BatchMaker` instance.
    let id = 0;
    let _batch_maker_handle = BatchMaker::spawn(
        id,
        /* max_batch_size */ 500,
        /* max_batch_delay */
        Duration::from_millis(1_000_000), // Ensure the timer is not triggered.
        tx_shutdown.subscribe(),
        rx_batch_maker,
        tx_quorum_waiter,
        Arc::new(node_metrics),
        client,
        store.clone(),
        latest_protocol_version(),
    );

    // Send enough transactions to seal a batch.
    let tx = transaction();
    let txs = vec![transaction(), transaction(), transaction()];
    let (s0, r0) = tokio::sync::oneshot::channel();
    let (s1, r1) = tokio::sync::oneshot::channel();
    let (s2, r2) = tokio::sync::oneshot::channel();
    tx_batch_maker.send((vec![tx.clone()], s0)).await.unwrap();
    tx_batch_maker.send((txs.clone(), s1)).await.unwrap();
    tx_batch_maker.send((vec![tx.clone()], s2)).await.unwrap();

    // Ensure the batch is as expected.
    let expected_batch = Batch::new(
        vec![
            tx.clone(),
            txs[0].clone(),
            txs[1].clone(),
            txs[2].clone(),
            tx.clone(),
        ],
        &latest_protocol_version(),
    );
    let (batch, resp) = rx_quorum_waiter.recv().await.unwrap();

    assert_eq!(batch.transactions(), expected_batch.transactions());

    // Eventually deliver message
    assert!(resp.send(()).is_ok());

    // Batch maker should finish creating the batch.
    assert!(r0.await.is_ok());
    assert!(r1.await.is_ok());
    assert!(r2.await.is_ok());

    // Ensure the batch is stored
    assert!(store.get(&expected_batch.digest()).unwrap().is_some());
}

#[tokio::test]
async fn batch_timeout() {
    let client = create_network_client();
    let store = create_batch_store();
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_batch_maker, rx_batch_maker) = test_utils::test_channel!(1);
    let (tx_quorum_waiter, mut rx_quorum_waiter) = test_utils::test_channel!(1);
    let node_metrics = WorkerMetrics::new(&Registry::new());

    // Mock the primary client to always succeed.
    let mut mock_server = MockWorkerToPrimary::new();
    mock_server
        .expect_report_own_batch()
        .returning(|_| Ok(anemo::Response::new(())));
    client.set_worker_to_primary_local_handler(Arc::new(mock_server));

    // Spawn a `BatchMaker` instance.
    let id = 0;
    let _batch_maker_handle = BatchMaker::spawn(
        id,
        /* max_batch_size */ 200,
        /* max_batch_delay */
        Duration::from_millis(50), // Ensure the timer is triggered.
        tx_shutdown.subscribe(),
        rx_batch_maker,
        tx_quorum_waiter,
        Arc::new(node_metrics),
        client,
        store.clone(),
        latest_protocol_version(),
    );

    // Do not send enough transactions to seal a batch.
    let tx = transaction();
    let (s0, r0) = tokio::sync::oneshot::channel();
    tx_batch_maker.send((vec![tx.clone()], s0)).await.unwrap();

    // Ensure the batch is as expected.
    let (batch, resp) = rx_quorum_waiter.recv().await.unwrap();
    let expected_batch = Batch::new(vec![tx.clone()], &latest_protocol_version());
    assert_eq!(batch.transactions(), expected_batch.transactions());

    // Eventually deliver message
    assert!(resp.send(()).is_ok());

    // Batch maker should finish creating the batch.
    assert!(r0.await.is_ok());

    // Ensure the batch is stored
    assert!(store.get(&batch.digest()).unwrap().is_some());
}
