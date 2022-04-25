// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::worker::WorkerMessage;
use blake2::digest::Update;
use crypto::{ed25519::Ed25519PublicKey, traits::KeyPair};
use futures::{SinkExt, StreamExt};
use network::SimpleSender;
use primary::WorkerPrimaryMessage;
use std::time::Duration;
use store::rocks;
use test_utils::{
    batch, committee_with_base_port, digest_batch, expecting_listener, keys,
    serialize_batch_message, temp_dir, WorkerToPrimaryMockServer,
};
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[tokio::test]
async fn handle_clients_transactions() {
    let name = keys().pop().unwrap().public().clone();
    let id = 0;
    let committee = committee_with_base_port(11_000);
    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };

    // Create a new test store.
    let db = rocks::DBMap::<BatchDigest, SerializedBatchMessage>::open(
        temp_dir(),
        None,
        Some("batches"),
    )
    .unwrap();
    let store = Store::new(db);

    // Spawn a `Worker` instance.
    Worker::spawn(name.clone(), id, committee.clone(), parameters, store);

    // Spawn a network listener to receive our batch's digest.
    let batch = batch();
    let serialized_batch = serialize_batch_message(batch.clone());
    let batch_digest = BatchDigest::new(crypto::blake2b_256(|hasher| {
        hasher.update(&serialized_batch)
    }));

    let primary_address = committee.primary(&name).unwrap().worker_to_primary;
    let expected = bincode::serialize(&WorkerPrimaryMessage::OurBatch(batch_digest, id)).unwrap();
    let mut handle = WorkerToPrimaryMockServer::spawn(primary_address);

    // Spawn enough workers' listeners to acknowledge our batches.
    for (_, addresses) in committee.others_workers(&name, &id) {
        let address = addresses.worker_to_worker;
        let _ = expecting_listener(address, /* expected */ None);
    }

    // Send enough transactions to create a batch.
    let mut network = SimpleSender::new();
    let address = committee.worker(&name, &id).unwrap().transactions;

    for tx in batch.0 {
        network.send(address, Bytes::from(tx.clone())).await;
    }

    // Ensure the primary received the batch's digest (ie. it did not panic).
    assert_eq!(handle.recv().await.unwrap().payload, expected);
}

#[tokio::test]
async fn handle_client_batch_request() {
    let name = keys().pop().unwrap().public().clone();
    let id = 0;
    let committee = committee_with_base_port(11_001);
    let parameters = Parameters {
        max_header_delay: Duration::from_millis(100_000), // Ensure no batches are created.
        ..Parameters::default()
    };

    // Create a new test store.
    let db = rocks::DBMap::<BatchDigest, SerializedBatchMessage>::open(
        temp_dir(),
        None,
        Some("batches"),
    )
    .unwrap();
    let store = Store::new(db);

    // Add a batch to the store.
    let batch = batch();
    store
        .write(
            digest_batch(batch.clone()),
            serialize_batch_message(batch.clone()),
        )
        .await;

    // Spawn a `Worker` instance.
    Worker::spawn(name.clone(), id, committee.clone(), parameters, store);

    // Spawn a client to ask for batches and receive the reply.
    tokio::task::yield_now().await;
    let address = committee.worker(&name, &id).unwrap().worker_to_worker;
    let socket = TcpStream::connect(address).await.unwrap();
    let (mut writer, mut reader) = Framed::new(socket, LengthDelimitedCodec::new()).split();

    // Send batch request.
    let digests = vec![digest_batch(batch.clone())];
    let message = WorkerMessage::<Ed25519PublicKey>::ClientBatchRequest(digests);
    let serialized = bincode::serialize(&message).unwrap();
    writer.send(Bytes::from(serialized)).await.unwrap();

    // Wait for the reply and ensure it is as expected.
    let bytes = reader.next().await.unwrap().unwrap();
    let expected = Bytes::from(serialize_batch_message(batch));
    assert_eq!(bytes, expected);
}
