// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::hash::Hash;
use std::vec;
use test_utils::{latest_protocol_version, CommitteeFixture};
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
