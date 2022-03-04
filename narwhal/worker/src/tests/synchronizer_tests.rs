// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::common::{
    batch, batch_digest, committee_with_base_port, keys, listener, open_batch_store,
    resolve_batch_digest, serialise_batch,
};
use crypto::ed25519::Ed25519PublicKey;
use tokio::{sync::mpsc::channel, time::timeout};

#[tokio::test]
async fn synchronize() {
    let (tx_message, rx_message) = channel(1);
    let (tx_primary, _) = channel(1);

    let mut keys = keys();
    let (name, _) = keys.pop().unwrap();
    let id = 0;
    let committee = committee_with_base_port(9_000);

    // Create a new test store.
    let store = open_batch_store();

    // Spawn a `Synchronizer` instance.
    Synchronizer::spawn(
        name.clone(),
        id,
        committee.clone(),
        store.clone(),
        /* gc_depth */ 50, // Not used in this test.
        /* sync_retry_delay */ 1_000_000, // Ensure it is not triggered.
        /* sync_retry_nodes */ 3, // Not used in this test.
        rx_message,
        tx_primary,
    );

    // Spawn a listener to receive our batch requests.
    let (target, _) = keys.pop().unwrap();
    let address = committee.worker(&target, &id).unwrap().worker_to_worker;
    let missing = vec![batch_digest()];
    let message = WorkerMessage::BatchRequest(missing.clone(), name.clone());
    let serialized = bincode::serialize(&message).unwrap();
    let handle = listener(address, Some(Bytes::from(serialized)));

    // Send a sync request.
    let message = PrimaryWorkerMessage::Synchronize(missing, target);
    tx_message.send(message).await.unwrap();

    // Ensure the target receives the sync request.
    assert!(handle.await.is_ok());
}

#[tokio::test]
async fn test_successful_request_batch() {
    let (tx_message, rx_message) = channel(1);
    let (tx_primary, mut rx_primary) = channel(1);

    let mut keys = keys();
    let (name, _) = keys.pop().unwrap();
    let id = 0;
    let committee = committee_with_base_port(9_000);

    // Create a new test store.
    let store = open_batch_store();

    // Spawn a `Synchronizer` instance.
    Synchronizer::spawn(
        name.clone(),
        id,
        committee.clone(),
        store.clone(),
        /* gc_depth */ 50, // Not used in this test.
        /* sync_retry_delay */ 1_000_000, // Ensure it is not triggered.
        /* sync_retry_nodes */ 3, // Not used in this test.
        rx_message,
        tx_primary,
    );

    // Create a dummy batch and store
    let expected_batch = batch();
    let batch_serialised = serialise_batch(expected_batch.clone());
    let expected_digest = resolve_batch_digest(batch_serialised.clone());
    store
        .write(expected_digest.clone(), batch_serialised.clone())
        .await;

    // WHEN we send a message to retrieve the batch
    let message = PrimaryWorkerMessage::<Ed25519PublicKey>::RequestBatch(expected_digest.clone());

    tx_message
        .send(message)
        .await
        .expect("Should be able to send message");

    // THEN we should receive batch the batch
    if let Ok(Some(message)) = timeout(Duration::from_secs(5), rx_primary.recv()).await {
        match bincode::deserialize(&message).unwrap() {
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
