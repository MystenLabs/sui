// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    common::{batch, temp_dir},
    worker::WorkerMessage,
};
use blake2::digest::Update;
use crypto::ed25519::Ed25519PublicKey;
use store::rocks;
use tokio::sync::mpsc::channel;

#[tokio::test]
async fn hash_and_store() {
    let (tx_batch, rx_batch) = channel(1);
    let (tx_digest, mut rx_digest) = channel(1);

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
        rx_batch,
        tx_digest,
        /* own_batch */ true,
    );

    // Send a batch to the `Processor`.
    let message = WorkerMessage::<Ed25519PublicKey>::Batch(batch());
    let serialized = bincode::serialize(&message).unwrap();
    tx_batch.send(serialized.clone()).await.unwrap();

    // Ensure the `Processor` outputs the batch's digest.
    let output = rx_digest.recv().await.unwrap();
    let digest = BatchDigest::new(crypto::blake2b_256(|hasher| hasher.update(&serialized)));
    let expected = bincode::serialize(&WorkerPrimaryMessage::OurBatch(digest, id)).unwrap();
    assert_eq!(output, expected);

    // Ensure the `Processor` correctly stored the batch.
    let stored_batch = store.read(digest).await.unwrap();
    assert!(stored_batch.is_some(), "The batch is not in the store");
    assert_eq!(stored_batch.unwrap(), serialized);
}
