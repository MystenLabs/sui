// Copyright(C) Facebook, Inc. and its affiliates.
use super::*;
use crate::common::batch;
use crate::worker::WorkerMessage;
use std::fs;
use tokio::sync::mpsc::channel;

#[tokio::test]
async fn hash_and_store() {
    let (tx_batch, rx_batch) = channel(1);
    let (tx_digest, mut rx_digest) = channel(1);

    // Create a new test store.
    let path = ".db_test_hash_and_store";
    let _ = fs::remove_dir_all(path);
    let mut store = Store::new(path).unwrap();

    // Spawn a new `Processor` instance.
    let id = 0;
    Processor::spawn(
        id,
        store.clone(),
        rx_batch,
        tx_digest,
        /* own_batch */ true,
    );

    // Send a batch to the `Processor`.
    let message = WorkerMessage::Batch(batch());
    let serialized = bincode::serialize(&message).unwrap();
    tx_batch.send(serialized.clone()).await.unwrap();

    // Ensure the `Processor` outputs the batch's digest.
    let output = rx_digest.recv().await.unwrap();
    let digest = Digest(
        Sha512::digest(&serialized).as_slice()[..32]
            .try_into()
            .unwrap(),
    );
    let expected = bincode::serialize(&WorkerPrimaryMessage::OurBatch(digest.clone(), id)).unwrap();
    assert_eq!(output, expected);

    // Ensure the `Processor` correctly stored the batch.
    let stored_batch = store.read(digest.to_vec()).await.unwrap();
    assert!(stored_batch.is_some(), "The batch is not in the store");
    assert_eq!(stored_batch.unwrap(), serialized);
}
