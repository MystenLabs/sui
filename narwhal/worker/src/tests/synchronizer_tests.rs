// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use arc_swap::ArcSwap;
use fastcrypto::Hash;
use prometheus::Registry;
use test_utils::{
    batch, batches, open_batch_store, test_network, CommitteeFixture, WorkerToWorkerMockServer,
};
use tokio::time::timeout;

#[tokio::test]
async fn synchronize() {
    let (tx_message, rx_message) = test_utils::test_channel!(1);
    let (tx_primary, _) = test_utils::test_channel!(1);
    let (tx_batch_processor, _) = test_utils::test_channel!(1);

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let id = 0;
    let my_primary = fixture.authorities().next().unwrap();
    let myself = my_primary.worker(id);

    let (tx_reconfiguration, _rx_reconfiguration) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));

    // Create a new test store.
    let store = open_batch_store();

    let metrics = Arc::new(WorkerMetrics::new(&Registry::new()));

    let network = test_network(myself.keypair(), &myself.info().worker_address);
    // Spawn a `Synchronizer` instance.
    let _synchronizer_handle = Synchronizer::spawn(
        my_primary.public_key(),
        id,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache.clone(),
        store.clone(),
        /* gc_depth */ 50, // Not used in this test.
        /* sync_retry_delay */
        Duration::from_millis(1_000_000), // Ensure it is not triggered.
        /* sync_retry_nodes */ 3, // Not used in this test.
        rx_message,
        tx_reconfiguration,
        tx_primary,
        tx_batch_processor,
        metrics,
        P2pNetwork::new(network.clone()),
    );

    // Spawn a listener to receive our batch requests.
    let target_primary = fixture.authorities().nth(1).unwrap();
    let target_worker = target_primary.worker(id);
    let target = target_primary.public_key();
    let missing = vec![batch().digest()];
    let expected = WorkerBatchRequest {
        digests: missing.clone(),
    };
    let (_, mut rx_worker_batch_request, _network) = WorkerToWorkerMockServer::spawn(
        target_worker.keypair(),
        target_worker.info().worker_address.clone(),
    );

    // ensure that the networks are connected
    network
        .connect(network::multiaddr_to_address(&target_worker.info().worker_address).unwrap())
        .await
        .unwrap();

    // Send a sync request.
    let message = PrimaryWorkerMessage::Synchronize(missing, target);
    tx_message.send(message).await.unwrap();

    // Ensure the target receives the sync request.
    assert_eq!(rx_worker_batch_request.recv().await.unwrap(), expected);
}

#[tokio::test]
async fn synchronize_when_batch_exists() {
    let (tx_message, rx_message) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_batch_processor, _) = test_utils::test_channel!(1);

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let id = 0;
    let my_primary = fixture.authorities().next().unwrap();
    let myself = my_primary.worker(id);

    let (tx_reconfiguration, _rx_reconfiguration) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));

    // Create a new test store.
    let store = open_batch_store();

    let metrics = Arc::new(WorkerMetrics::new(&Registry::new()));

    let network = test_network(myself.keypair(), &myself.info().worker_address);
    // Spawn a `Synchronizer` instance.
    let _synchronizer_handle = Synchronizer::spawn(
        my_primary.public_key(),
        id,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache.clone(),
        store.clone(),
        /* gc_depth */ 50, // Not used in this test.
        /* sync_retry_delay */
        Duration::from_millis(1_000_000), // Ensure it is not triggered.
        /* sync_retry_nodes */ 3, // Not used in this test.
        rx_message,
        tx_reconfiguration,
        tx_primary,
        tx_batch_processor,
        metrics,
        P2pNetwork::new(network.clone()),
    );

    // Spawn a listener to receive our batch requests.
    let target_primary = fixture.authorities().nth(1).unwrap();
    let target_worker = target_primary.worker(id);
    let target = target_primary.public_key();
    let (mut handle, _, _network) = WorkerToWorkerMockServer::spawn(
        target_worker.keypair(),
        target_worker.info().worker_address.clone(),
    );

    // ensure that the networks are connected
    network
        .connect(network::multiaddr_to_address(&target_worker.info().worker_address).unwrap())
        .await
        .unwrap();

    let batch = batch();
    let batch_id = batch.digest();
    let missing = vec![batch_id];

    // now store the batch
    store.write(batch_id, batch).await;

    // Send a sync request.
    let message = PrimaryWorkerMessage::Synchronize(missing, target);
    tx_message.send(message).await.unwrap();

    // Ensure the target does NOT receive the sync request - we practically timeout waiting.
    let result = timeout(Duration::from_secs(1), handle.recv()).await;
    assert!(result.is_err());

    // Now ensure that the batch is forwarded directly to primary
    let result_batch_message: WorkerPrimaryMessage = rx_primary.recv().await.unwrap();

    match result_batch_message {
        WorkerPrimaryMessage::OthersBatch(result_digest, worker_id) => {
            assert_eq!(result_digest, batch_id, "Batch id mismatch");
            assert_eq!(worker_id, id, "Worker id mismatch");
        }
        _ => panic!("Unexpected message received!"),
    }
}

#[tokio::test]
async fn test_successful_request_batch() {
    let (tx_message, rx_message) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_batch_processor, _) = test_utils::test_channel!(1);

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let id = 0;
    let my_primary = fixture.authorities().next().unwrap();
    let myself = my_primary.worker(id);

    let (tx_reconfiguration, _rx_reconfiguration) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));

    // Create a new test store.
    let store = open_batch_store();

    let metrics = Arc::new(WorkerMetrics::new(&Registry::new()));

    let network = test_network(myself.keypair(), &myself.info().worker_address);
    // Spawn a `Synchronizer` instance.
    let _synchronizer_handle = Synchronizer::spawn(
        my_primary.public_key(),
        id,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache,
        store.clone(),
        /* gc_depth */ 50, // Not used in this test.
        /* sync_retry_delay */
        Duration::from_millis(1_000_000), // Ensure it is not triggered.
        /* sync_retry_nodes */ 3, // Not used in this test.
        rx_message,
        tx_reconfiguration,
        tx_primary,
        tx_batch_processor,
        metrics,
        P2pNetwork::new(network),
    );

    // Create a dummy batch and store
    let expected_batch = batch();
    let expected_digest = expected_batch.digest();
    store.write(expected_digest, expected_batch.clone()).await;

    // WHEN we send a message to retrieve the batch
    let message = PrimaryWorkerMessage::RequestBatch(expected_digest);

    tx_message
        .send(message)
        .await
        .expect("Should be able to send message");

    // THEN we should receive batch the batch
    if let Ok(Some(message)) = timeout(Duration::from_secs(5), rx_primary.recv()).await {
        match message {
            WorkerPrimaryMessage::RequestedBatch(digest, batch) => {
                assert_eq!(batch, expected_batch);
                assert_eq!(digest, expected_digest)
            }
            _ => panic!("Unexpected message"),
        }
    } else {
        panic!("Expected to successfully received a request batch");
    }
}

#[tokio::test]
async fn test_request_batch_not_found() {
    let (tx_message, rx_message) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_batch_processor, _) = test_utils::test_channel!(1);

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let id = 0;
    let my_primary = fixture.authorities().next().unwrap();
    let myself = my_primary.worker(id);

    let (tx_reconfiguration, _rx_reconfiguration) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));

    // Create a new test store.
    let store = open_batch_store();

    let metrics = Arc::new(WorkerMetrics::new(&Registry::new()));

    let network = test_network(myself.keypair(), &myself.info().worker_address);
    // Spawn a `Synchronizer` instance.
    let _synchronizer_handle = Synchronizer::spawn(
        my_primary.public_key(),
        id,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache,
        store.clone(),
        /* gc_depth */ 50, // Not used in this test.
        /* sync_retry_delay */
        Duration::from_millis(1_000_000), // Ensure it is not triggered.
        /* sync_retry_nodes */ 3, // Not used in this test.
        rx_message,
        tx_reconfiguration,
        tx_primary,
        tx_batch_processor,
        metrics,
        P2pNetwork::new(network),
    );

    // The non existing batch id
    let expected_batch_id = BatchDigest::default();

    // WHEN we send a message to retrieve the batch that doesn't exist
    let message = PrimaryWorkerMessage::RequestBatch(expected_batch_id);

    tx_message
        .send(message)
        .await
        .expect("Should be able to send message");

    // THEN we should receive batch the batch
    if let Ok(Some(message)) = timeout(Duration::from_secs(5), rx_primary.recv()).await {
        match message {
            WorkerPrimaryMessage::Error(error) => {
                assert_eq!(
                    error,
                    WorkerPrimaryError::RequestedBatchNotFound(expected_batch_id)
                );
            }
            _ => panic!("Unexpected message"),
        }
    } else {
        panic!("Expected to successfully received a request batch");
    }
}

#[tokio::test]
async fn test_successful_batch_delete() {
    let (tx_message, rx_message) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_batch_processor, _) = test_utils::test_channel!(1);

    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let id = 0;
    let my_primary = fixture.authorities().next().unwrap();
    let myself = my_primary.worker(id);

    let (tx_reconfiguration, _rx_reconfiguration) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));

    // Create a new test store.
    let store = open_batch_store();

    let metrics = Arc::new(WorkerMetrics::new(&Registry::new()));

    let network = test_network(myself.keypair(), &myself.info().worker_address);
    // Spawn a `Synchronizer` instance.
    let _synchronizer_handle = Synchronizer::spawn(
        my_primary.public_key(),
        id,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache,
        store.clone(),
        /* gc_depth */ 50, // Not used in this test.
        /* sync_retry_delay */
        Duration::from_millis(1_000_000), // Ensure it is not triggered.
        /* sync_retry_nodes */ 3, // Not used in this test.
        rx_message,
        tx_reconfiguration,
        tx_primary,
        tx_batch_processor,
        metrics,
        P2pNetwork::new(network),
    );

    // Create dummy batches and store them
    let expected_batches = batches(10);
    let mut batch_digests = Vec::new();

    for batch in expected_batches.clone() {
        let digest = batch.digest();

        batch_digests.push(digest);

        store.write(digest, batch).await;
    }

    // WHEN we send a message to delete batches
    let message = PrimaryWorkerMessage::DeleteBatches(batch_digests.clone());

    tx_message
        .send(message)
        .await
        .expect("Should be able to send message");

    // THEN we should receive the acknowledgement that the batches have been deleted
    if let Ok(Some(message)) = timeout(Duration::from_secs(5), rx_primary.recv()).await {
        match message {
            WorkerPrimaryMessage::DeletedBatches(digests) => {
                assert_eq!(digests, batch_digests);
            }
            _ => panic!("Unexpected message"),
        }
    } else {
        panic!("Expected to successfully receive a deleted batches request");
    }

    // AND batches should be deleted
    for batch in expected_batches {
        let digest = batch.digest();

        let result = store.read(digest).await;
        assert!(result.as_ref().is_ok());
        assert!(result.unwrap().is_none());
    }
}
