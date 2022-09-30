// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use fastcrypto::Hash;
use test_utils::CommitteeFixture;

#[tokio::test]
async fn synchronize() {
    telemetry_subscribers::init_for_testing();

    let (tx_synchronizer, _rx_synchronizer) = test_utils::test_channel!(1);
    let (tx_primary, _rx_primary) = test_utils::test_channel!(1);
    let (tx_request_batches_rpc, mut rx_request_batches_rpc) = test_utils::test_channel!(1);
    let (tx_batch_processor, mut rx_batch_processor) = test_utils::test_channel!(1);

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let worker_cache = fixture.shared_worker_cache();
    let id = 0;

    // Create a new test store.
    let store = test_utils::open_batch_store();

    let handler = PrimaryReceiverHandler {
        id,
        worker_cache,
        store,
        request_batches_retry_nodes: 3, // Not used in this test.
        tx_synchronizer,
        tx_request_batches_rpc,
        tx_primary,
        tx_batch_processor,
    };

    // Set up mock behavior for child RequestBatches RPC.
    let target_primary = fixture.authorities().nth(1).unwrap();
    let target_worker = target_primary.worker(id);
    let target_worker_network_key = target_worker.info().name.clone();
    let target = target_primary.public_key();
    let batch = test_utils::batch();
    let missing = vec![batch.digest()];
    let message = WorkerSynchronizeMessage {
        digests: missing.clone(),
        target,
    };
    let responder_handle = tokio::spawn(async move {
        let (target, num_nodes, request, response_channel) =
            rx_request_batches_rpc.recv().await.unwrap();
        assert_eq!(target.unwrap(), target_worker_network_key);
        assert!(num_nodes.is_none());
        assert_eq!(request, WorkerBatchRequest { digests: missing });

        // Reply with batch.
        assert!(response_channel
            .send(Ok(anemo::Response::new(WorkerBatchResponse {
                batches: vec![batch.clone()]
            })))
            .is_ok());

        // Ensure the handler sends the returned batch to the processor.
        let recv_batch = rx_batch_processor.recv().await.unwrap();
        let recv_digest = &recv_batch.digest();
        debug!("Received batch {recv_digest:?} from handler for processing");
        assert_eq!(recv_batch, batch);
    });

    // Send a sync request.
    handler
        .synchronize(anemo::Request::new(message))
        .await
        .unwrap();
    responder_handle.await.unwrap();
}

#[tokio::test]
async fn synchronize_when_batch_exists() {
    telemetry_subscribers::init_for_testing();

    let (tx_synchronizer, _rx_synchronizer) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_request_batches_rpc, _rx_request_batches_rpc) = test_utils::test_channel!(1);
    let (tx_batch_processor, _rx_batch_processor) = test_utils::test_channel!(1);

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let worker_cache = fixture.shared_worker_cache();
    let id = 0;

    // Create a new test store.
    let store = test_utils::open_batch_store();

    let handler = PrimaryReceiverHandler {
        id,
        worker_cache,
        store: store.clone(),
        request_batches_retry_nodes: 3, // Not used in this test.
        tx_synchronizer,
        tx_request_batches_rpc,
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
    handler
        .synchronize(anemo::Request::new(message))
        .await
        .unwrap();
    responder_handle.await.unwrap();
}
