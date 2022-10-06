// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use arc_swap::ArcSwap;
use fastcrypto::Hash;
use std::time::Duration;
use test_utils::{batch, batches, open_batch_store, test_network, CommitteeFixture};
use tokio::time::timeout;

#[tokio::test]
async fn test_successful_request_batch() {
    let (tx_message, rx_message) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);

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

    let network = test_network(myself.keypair(), &myself.info().worker_address);
    // Spawn a `Synchronizer` instance.
    let _synchronizer_handle = Synchronizer::spawn(
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache,
        store.clone(),
        rx_message,
        tx_reconfiguration,
        tx_primary,
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

    let network = test_network(myself.keypair(), &myself.info().worker_address);
    // Spawn a `Synchronizer` instance.
    let _synchronizer_handle = Synchronizer::spawn(
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache,
        store.clone(),
        rx_message,
        tx_reconfiguration,
        tx_primary,
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

    let network = test_network(myself.keypair(), &myself.info().worker_address);
    // Spawn a `Synchronizer` instance.
    let _synchronizer_handle = Synchronizer::spawn(
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache,
        store.clone(),
        rx_message,
        tx_reconfiguration,
        tx_primary,
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
