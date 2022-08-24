// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::worker::WorkerMessage;
use fastcrypto::Hash;
use store::rocks;
use test_utils::{batch, committee, temp_dir};

#[tokio::test]
async fn hash_and_store_our_batch() {
    // GIVEN
    let (tx_batch, rx_batch) = test_utils::test_channel!(1);
    let (tx_digest, mut rx_digest) = test_utils::test_channel!(1);

    let committee = committee(None).clone();
    let (_tx_reconfiguration, rx_reconfiguration) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));

    // Create a new test store.
    let store = create_batches_store();

    // Spawn a new `Processor` instance.
    let id = 0;
    let _processor_handler = Processor::spawn(
        id,
        store.clone(),
        rx_reconfiguration,
        rx_batch,
        tx_digest,
        /* own_batch */ true,
    );

    // Send a batch to the `Processor`.
    let batch = batch();
    let message = WorkerMessage::Batch(batch.clone());
    let serialized = bincode::serialize(&message).unwrap();

    // the process should be idempotent - no matter how many times we write
    // the same batch it should be stored and output the message to the tx_digest channel
    for _ in 0..3 {
        // WHEN
        tx_batch.send(serialized.clone()).await.unwrap();

        // THEN
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
}

#[tokio::test]
async fn hash_and_store_others_batch() {
    // GIVEN
    let (tx_batch, rx_batch) = test_utils::test_channel!(1);
    let (tx_digest, mut rx_digest) = test_utils::test_channel!(1);

    let committee = committee(None).clone();
    let (_tx_reconfiguration, rx_reconfiguration) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));

    // Create a new test store.
    let store = create_batches_store();

    // Spawn a new `Processor` instance.
    let id = 0;
    let _processor_handler = Processor::spawn(
        id,
        store.clone(),
        rx_reconfiguration,
        rx_batch,
        tx_digest,
        /* own_batch */ false,
    );

    // Send a batch to the `Processor`.
    let batch = batch();
    let message = WorkerMessage::Batch(batch.clone());
    let serialized = bincode::serialize(&message).unwrap();

    for _ in 0..3 {
        // WHEN
        tx_batch.send(serialized.clone()).await.unwrap();

        // THEN
        // Ensure the `Processor` outputs the batch's digest.
        let digest = batch.digest();
        match rx_digest.recv().await.unwrap() {
            WorkerPrimaryMessage::OthersBatch(x, y) => {
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
}

fn create_batches_store() -> Store<BatchDigest, SerializedBatchMessage> {
    let db = rocks::DBMap::<BatchDigest, SerializedBatchMessage>::open(
        temp_dir(),
        None,
        Some("batches"),
    )
    .unwrap();
    Store::new(db)
}
