// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::worker::WorkerMessage;
use crypto::{ed25519::Ed25519PublicKey, Hash};
use store::rocks;
use test_utils::{batch, committee, temp_dir};
use tokio::sync::mpsc::channel;

#[tokio::test]
async fn hash_and_store() {
    let (tx_batch, rx_batch) = channel(1);
    let (tx_digest, mut rx_digest) = channel(1);

    let committee = committee(None).clone();
    let (_tx_reconfiguration, rx_reconfiguration) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));

    // Create a new test store.
    let db = rocks::DBMap::<BatchDigest, SerializedBatchMessage>::open(
        temp_dir(),
        None,
        Some("batches"),
    )
    .unwrap();
    let store = Store::new(db);

    // Spawn a new `Processor` instance.
    let id = 0;
    Processor::spawn(
        id,
        store.clone(),
        rx_reconfiguration,
        rx_batch,
        tx_digest,
        /* own_batch */ true,
    );

    // Send a batch to the `Processor`.
    let batch = batch();
    let message = WorkerMessage::<Ed25519PublicKey>::Batch(batch.clone());
    let serialized = bincode::serialize(&message).unwrap();
    tx_batch.send(serialized.clone()).await.unwrap();

    // Ensure the `Processor` outputs the batch's digest.
    let digest = batch.digest();
    match rx_digest.recv().await.unwrap() {
        WorkerPrimaryMessage::OurBatch(x, y) => {
            assert_eq!(x, digest);
            assert_eq!(y, id);
        }
        _ => panic!("Unexpected protocol message"),
    }

    // Ensure the `Processor` correctly stored the batch.
    let stored_batch = store.read(digest).await.unwrap();
    assert!(stored_batch.is_some(), "The batch is not in the store");
    assert_eq!(stored_batch.unwrap(), serialized);
}
