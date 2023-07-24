// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::hash::Hash;
use std::vec;
use test_utils::{get_protocol_config, latest_protocol_version, CommitteeFixture};
use types::{MockWorkerToWorker, WorkerToWorkerServer};

use super::*;
use crate::TrivialTransactionValidator;

#[tokio::test]
async fn synchronize() {
    telemetry_subscribers::init_for_testing();

    let latest_protocol_config = latest_protocol_version();
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let authority_id = fixture.authorities().next().unwrap().id();
    let id = 0;

    // Create a new test store.
    let store = test_utils::create_batch_store();

    // Create network with mock behavior to respond to RequestBatches request.
    let target_primary = fixture.authorities().nth(1).unwrap();
    let batch = test_utils::batch(&latest_protocol_config);
    let digest = batch.digest();
    let message = WorkerSynchronizeMessage {
        digests: vec![digest],
        target: target_primary.id(),
        is_certified: false,
    };

    let mut mock_server = MockWorkerToWorker::new();
    let mock_batch_response = batch.clone();
    mock_server
        .expect_request_batches()
        .withf(move |request| request.body().batch_digests == vec![digest])
        .return_once(move |_| {
            Ok(anemo::Response::new(RequestBatchesResponse {
                batches: vec![mock_batch_response],
                is_size_limit_reached: false,
            }))
        });
    let routes = anemo::Router::new().add_rpc_service(WorkerToWorkerServer::new(mock_server));
    let target_worker = target_primary.worker(id);
    let _recv_network = target_worker.new_network(routes);
    let send_network = test_utils::random_network();
    send_network
        .connect_with_peer_id(
            target_worker
                .info()
                .worker_address
                .to_anemo_address()
                .unwrap(),
            anemo::PeerId(target_worker.info().name.0.to_bytes()),
        )
        .await
        .unwrap();

    let handler = PrimaryReceiverHandler {
        authority_id,
        id,
        committee,
        protocol_config: latest_protocol_config.clone(),
        worker_cache,
        store: store.clone(),
        request_batches_timeout: Duration::from_secs(999),
        request_batches_retry_nodes: 3, // Not used in this test.
        network: Some(send_network),
        batch_fetcher: None,
        validator: TrivialTransactionValidator,
    };

    // Verify the batch is not in store
    assert!(store.get(&digest).unwrap().is_none());

    // Send a sync request.
    let request = anemo::Request::new(message);
    handler.synchronize(request).await.unwrap();

    // Verify it is now stored
    assert!(store.get(&digest).unwrap().is_some());
}

// TODO: Remove once we have upgraded to protocol version 12.
#[tokio::test]
async fn synchronize_versioned_batches() {
    telemetry_subscribers::init_for_testing();

    let latest_protocol_config = &latest_protocol_version();
    // protocol version 11 should only support BatchV1
    let protocol_config_v11 = &get_protocol_config(11);
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let authority_id = fixture.authorities().next().unwrap().id();
    let id = 0;

    // Create a new test store.
    let store = test_utils::create_batch_store();

    // Create network with mock behavior to respond to RequestBatches request.
    let target_primary = fixture.authorities().nth(1).unwrap();
    let batch_v1 = test_utils::batch(protocol_config_v11);
    let digest_v1 = batch_v1.digest();
    let message_v1 = WorkerSynchronizeMessage {
        digests: vec![digest_v1],
        target: target_primary.id(),
        is_certified: false,
    };

    let batch_v2 = test_utils::batch(latest_protocol_config);
    let digest_v2 = batch_v2.digest();
    let message_v2 = WorkerSynchronizeMessage {
        digests: vec![digest_v2],
        target: target_primary.id(),
        is_certified: false,
    };

    let mut mock_server = MockWorkerToWorker::new();
    let mock_batch_response_v1 = batch_v1.clone();
    mock_server
        .expect_request_batches()
        .withf(move |request| request.body().batch_digests == vec![digest_v1])
        .times(2)
        .returning(move |_| {
            Ok(anemo::Response::new(RequestBatchesResponse {
                batches: vec![mock_batch_response_v1.clone()],
                is_size_limit_reached: false,
            }))
        });
    let mock_batch_response_v2 = batch_v2.clone();
    mock_server
        .expect_request_batches()
        .withf(move |request| request.body().batch_digests == vec![digest_v2])
        .times(2)
        .returning(move |_| {
            Ok(anemo::Response::new(RequestBatchesResponse {
                batches: vec![mock_batch_response_v2.clone()],
                is_size_limit_reached: false,
            }))
        });
    let routes = anemo::Router::new().add_rpc_service(WorkerToWorkerServer::new(mock_server));
    let target_worker = target_primary.worker(id);
    let _recv_network = target_worker.new_network(routes);
    let send_network = test_utils::random_network();
    send_network
        .connect_with_peer_id(
            target_worker
                .info()
                .worker_address
                .to_anemo_address()
                .unwrap(),
            anemo::PeerId(target_worker.info().name.0.to_bytes()),
        )
        .await
        .unwrap();

    let handler_v11 = PrimaryReceiverHandler {
        authority_id,
        id,
        committee: committee.clone(),
        protocol_config: protocol_config_v11.clone(),
        worker_cache: worker_cache.clone(),
        store: store.clone(),
        request_batches_timeout: Duration::from_secs(999),
        request_batches_retry_nodes: 3, // Not used in this test.
        network: Some(send_network.clone()),
        batch_fetcher: None,
        validator: TrivialTransactionValidator,
    };

    let handler_latest_version = PrimaryReceiverHandler {
        authority_id,
        id,
        committee,
        protocol_config: latest_protocol_config.clone(),
        worker_cache,
        store: store.clone(),
        request_batches_timeout: Duration::from_secs(999),
        request_batches_retry_nodes: 3, // Not used in this test.
        network: Some(send_network),
        batch_fetcher: None,
        validator: TrivialTransactionValidator,
    };

    // Case #1: Receive BatchV1 and network has not upgraded to 12 so we are okay
    assert!(store.get(&digest_v1).unwrap().is_none());
    let request = anemo::Request::new(message_v1.clone());
    handler_v11.synchronize(request).await.unwrap();
    assert!(store.get(&digest_v1).unwrap().is_some());

    // Remove the batch from the store so we can test the other cases.
    store.remove(&digest_v1).unwrap();

    // Case #2: Receive BatchV1 but network has upgraded to 12 so we fail because we expect BatchV2
    let request = anemo::Request::new(message_v1);
    let response = handler_latest_version.synchronize(request).await;
    assert!(response.is_err());

    // Case #3: Receive BatchV2 but network is still in v11 so we fail because we expect BatchV1
    let request = anemo::Request::new(message_v2.clone());
    let response = handler_v11.synchronize(request).await;
    assert!(response.is_err());

    // Case #4: Receive BatchV2 and network is upgraded to 12 so we are okay
    assert!(store.get(&digest_v2).unwrap().is_none());
    let request = anemo::Request::new(message_v2.clone());
    handler_latest_version.synchronize(request).await.unwrap();
    assert!(store.get(&digest_v2).unwrap().is_some());
}

#[tokio::test]
async fn synchronize_when_batch_exists() {
    telemetry_subscribers::init_for_testing();

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let authority_id = fixture.authorities().next().unwrap().id();
    let id = 0;

    // Create a new test store.
    let store = test_utils::create_batch_store();

    // Create network without mock behavior since it will not be needed.
    let send_network = test_utils::random_network();

    let handler = PrimaryReceiverHandler {
        authority_id,
        id,
        committee: committee.clone(),
        protocol_config: latest_protocol_version(),
        worker_cache,
        store: store.clone(),
        request_batches_timeout: Duration::from_secs(999),
        request_batches_retry_nodes: 3, // Not used in this test.
        network: Some(send_network),
        batch_fetcher: None,
        validator: TrivialTransactionValidator,
    };

    // Store the batch.
    let batch = test_utils::batch(&latest_protocol_version());
    let batch_id = batch.digest();
    let missing = vec![batch_id];
    store.insert(&batch_id, &batch).unwrap();

    // Send a sync request.
    let target_primary = fixture.authorities().nth(1).unwrap();
    let message = WorkerSynchronizeMessage {
        digests: missing.clone(),
        target: target_primary.id(),
        is_certified: false,
    };
    // The sync request should succeed.
    handler
        .synchronize(anemo::Request::new(message))
        .await
        .unwrap();
}
