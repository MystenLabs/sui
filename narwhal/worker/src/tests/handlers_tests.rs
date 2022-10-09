// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use fastcrypto::Hash;
use test_utils::CommitteeFixture;
use types::WorkerToWorkerServer;

#[tokio::test]
async fn synchronize() {
    telemetry_subscribers::init_for_testing();

    let (tx_synchronizer, _rx_synchronizer) = test_utils::test_channel!(1);
    let (tx_primary, _rx_primary) = test_utils::test_channel!(1);
    let (tx_batch_processor, mut rx_batch_processor) = test_utils::test_channel!(1);

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let worker_cache = fixture.shared_worker_cache();
    let name = fixture.authorities().next().unwrap().public_key();
    let id = 0;

    // Create a new test store.
    let store = test_utils::open_batch_store();

    let handler = PrimaryReceiverHandler {
        name,
        id,
        worker_cache,
        store,
        request_batches_timeout: Duration::from_secs(999),
        request_batches_retry_nodes: 3, // Not used in this test.
        tx_synchronizer,
        tx_primary,
        tx_batch_processor,
    };

    // Set up mock behavior for child RequestBatches RPC.
    let target_primary = fixture.authorities().nth(1).unwrap();
    let target = target_primary.public_key();
    let batch = test_utils::batch();
    let missing = vec![batch.digest()];
    let message = WorkerSynchronizeMessage {
        digests: missing.clone(),
        target,
    };

    struct MockWorkerToWorker {
        expected_request: WorkerBatchRequest,
        response: WorkerBatchResponse,
    }
    #[async_trait]
    impl WorkerToWorker for MockWorkerToWorker {
        async fn send_message(
            &self,
            _request: anemo::Request<WorkerMessage>,
        ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
            unimplemented!();
        }
        async fn request_batches(
            &self,
            request: anemo::Request<WorkerBatchRequest>,
        ) -> Result<anemo::Response<WorkerBatchResponse>, anemo::rpc::Status> {
            assert_eq!(*request.body(), self.expected_request);
            Ok(anemo::Response::new(self.response.clone()))
        }
    }

    let routes =
        anemo::Router::new().add_rpc_service(WorkerToWorkerServer::new(MockWorkerToWorker {
            expected_request: WorkerBatchRequest { digests: missing },
            response: WorkerBatchResponse {
                batches: vec![batch.clone()],
            },
        }));
    let target_worker = target_primary.worker(id);
    let _recv_network = target_worker.new_network(routes);

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
    let recv_batch = rx_batch_processor.recv().await.unwrap();
    assert_eq!(recv_batch, batch);
}

#[tokio::test]
async fn synchronize_when_batch_exists() {
    telemetry_subscribers::init_for_testing();

    let (tx_synchronizer, _rx_synchronizer) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_batch_processor, _rx_batch_processor) = test_utils::test_channel!(1);

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let worker_cache = fixture.shared_worker_cache();
    let name = fixture.authorities().next().unwrap().public_key();
    let id = 0;

    // Create a new test store.
    let store = test_utils::open_batch_store();

    let handler = PrimaryReceiverHandler {
        name,
        id,
        worker_cache,
        store: store.clone(),
        request_batches_timeout: Duration::from_secs(999),
        request_batches_retry_nodes: 3, // Not used in this test.
        tx_synchronizer,
        tx_primary,
        tx_batch_processor,
    };

    // Store the batch.
    let batch = test_utils::batch();
    let batch_id = batch.digest();
    let missing = vec![batch_id];
    store.write(batch_id, batch).await;

    // Set up mock behavior for child RequestBatches RPC.
    let target_primary = fixture.authorities().nth(1).unwrap();
    let target = target_primary.public_key();
    let message = WorkerSynchronizeMessage {
        digests: missing.clone(),
        target,
    };
    let responder_handle = tokio::spawn(async move {
        if let WorkerPrimaryMessage::OthersBatch(recv_digest, recv_id) =
            rx_primary.recv().await.unwrap()
        {
            assert_eq!(recv_digest, batch_id);
            assert_eq!(recv_id, id);
        } else {
            panic!("received unexpected WorkerPrimaryMessage");
        }
    });

    // Send a sync request.
    // Don't bother to inject a fake network because handler shouldn't need it.
    handler
        .synchronize(anemo::Request::new(message))
        .await
        .unwrap();
    responder_handle.await.unwrap();
}

#[tokio::test]
async fn delete_batches() {
    telemetry_subscribers::init_for_testing();

    let (tx_synchronizer, _rx_synchronizer) = test_utils::test_channel!(1);
    let (tx_primary, _rx_primary) = test_utils::test_channel!(1);
    let (tx_batch_processor, _rx_batch_processor) = test_utils::test_channel!(1);

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let worker_cache = fixture.shared_worker_cache();
    let name = fixture.authorities().next().unwrap().public_key();
    let id = 0;

    // Create a new test store.
    let store = test_utils::open_batch_store();
    let batch = test_utils::batch();
    let digest = batch.digest();
    store.write(digest, batch.clone()).await;

    // Send a delete request.
    let handler = PrimaryReceiverHandler {
        name,
        id,
        worker_cache,
        store: store.clone(),
        request_batches_timeout: Duration::from_secs(999),
        request_batches_retry_nodes: 3, // Not used in this test.
        tx_synchronizer,
        tx_primary,
        tx_batch_processor,
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
