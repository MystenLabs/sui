// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;

use crate::TrivialTransactionValidator;
use fastcrypto::hash::Hash;
use test_utils::CommitteeFixture;
use types::{MockWorkerToWorker, WorkerToWorkerServer};

#[tokio::test]
async fn synchronize() {
    telemetry_subscribers::init_for_testing();

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let name = fixture.authorities().next().unwrap().public_key();
    let id = 0;
    let (tx_reconfigure, _rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));

    // Create a new test store.
    let store = test_utils::open_batch_store();

    let handler = PrimaryReceiverHandler {
        name,
        id,
        committee: committee.into(),
        worker_cache,
        store: store.clone(),
        request_batch_timeout: Duration::from_secs(999),
        request_batch_retry_nodes: 3, // Not used in this test.
        tx_reconfigure,
        validator: TrivialTransactionValidator,
    };

    // Set up mock behavior for child RequestBatches RPC.
    let target_primary = fixture.authorities().nth(1).unwrap();
    let batch = test_utils::batch();
    let digest = batch.digest();
    let message = WorkerSynchronizeMessage {
        digests: vec![digest],
        target: target_primary.public_key(),
    };

    let mut mock_server = MockWorkerToWorker::new();
    let mock_batch_response = batch.clone();
    mock_server
        .expect_request_batch()
        .withf(move |request| request.body().batch == digest)
        .return_once(move |_| {
            Ok(anemo::Response::new(RequestBatchResponse {
                batch: Some(mock_batch_response),
            }))
        });
    let routes = anemo::Router::new().add_rpc_service(WorkerToWorkerServer::new(mock_server));
    let target_worker = target_primary.worker(id);
    let _recv_network = target_worker.new_network(routes);

    // Check not in store
    assert!(store.read(digest).await.unwrap().is_none());

    // Send a sync request.
    let mut request = anemo::Request::new(message);
    let send_network = test_utils::random_network();
    send_network
        .connect_with_peer_id(
            network::multiaddr_to_address(&target_worker.info().worker_address).unwrap(),
            anemo::PeerId(target_worker.info().name.0.to_bytes()),
        )
        .await
        .unwrap();
    assert!(request
        .extensions_mut()
        .insert(send_network.downgrade())
        .is_none());
    handler.synchronize(request).await.unwrap();

    // Check its now stored
    assert!(store.notify_read(digest).await.unwrap().is_some())
}

#[tokio::test]
async fn synchronize_when_batch_exists() {
    telemetry_subscribers::init_for_testing();

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let name = fixture.authorities().next().unwrap().public_key();
    let id = 0;
    let (tx_reconfigure, _rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));

    // Create a new test store.
    let store = test_utils::open_batch_store();

    let handler = PrimaryReceiverHandler {
        name,
        id,
        committee: committee.into(),
        worker_cache,
        store: store.clone(),
        request_batch_timeout: Duration::from_secs(999),
        request_batch_retry_nodes: 3, // Not used in this test.
        tx_reconfigure,
        validator: TrivialTransactionValidator,
    };

    // Store the batch.
    let batch = test_utils::batch();
    let batch_id = batch.digest();
    let missing = vec![batch_id];
    store.async_write(batch_id, batch).await;

    // Set up mock behavior for child RequestBatches RPC.
    let target_primary = fixture.authorities().nth(1).unwrap();
    let message = WorkerSynchronizeMessage {
        digests: missing.clone(),
        target: target_primary.public_key(),
    };

    // Send a sync request.
    // Don't bother to inject a fake network because handler shouldn't need it.
    handler
        .synchronize(anemo::Request::new(message))
        .await
        .unwrap();
}

#[tokio::test]
async fn delete_batches() {
    telemetry_subscribers::init_for_testing();

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let name = fixture.authorities().next().unwrap().public_key();
    let id = 0;
    let (tx_reconfigure, _rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));

    // Create a new test store.
    let store = test_utils::open_batch_store();
    let batch = test_utils::batch();
    let digest = batch.digest();
    store.async_write(digest, batch.clone()).await;

    // Send a delete request.
    let handler = PrimaryReceiverHandler {
        name,
        id,
        committee: committee.into(),
        worker_cache,
        store: store.clone(),
        request_batch_timeout: Duration::from_secs(999),
        request_batch_retry_nodes: 3, // Not used in this test.
        tx_reconfigure,
        validator: TrivialTransactionValidator,
    };
    let message = WorkerDeleteBatchesMessage {
        digests: vec![digest],
    };
    handler
        .delete_batches(anemo::Request::new(message))
        .await
        .unwrap();

    assert!(store.read(digest).await.unwrap().is_none());
}
