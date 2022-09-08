// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;

use arc_swap::ArcSwap;
use fastcrypto::traits::KeyPair;
use futures::StreamExt;
use prometheus::Registry;
use std::time::Duration;
use store::rocks;
use test_utils::{
    batch, digest_batch, serialize_batch_message, temp_dir, CommitteeFixture,
    WorkerToPrimaryMockServer, WorkerToWorkerMockServer,
};
use types::{
    serialized_batch_digest, TransactionsClient, WorkerPrimaryMessage, WorkerToWorkerClient,
};

#[tokio::test]
async fn handle_clients_transactions() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();

    let author = fixture.authorities().last().unwrap();
    let name = author.public_key();

    let worker_id = 0;
    let worker_keypair = author.worker(worker_id).keypair().copy();

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

    let registry = Registry::new();
    let metrics = Metrics {
        worker_metrics: Some(WorkerMetrics::new(&registry)),
        channel_metrics: Some(WorkerChannelMetrics::new(&registry)),
        endpoint_metrics: Some(WorkerEndpointMetrics::new(&registry)),
        network_metrics: Some(WorkerNetworkMetrics::new(&registry)),
    };

    // Spawn a `Worker` instance.
    Worker::spawn(
        name.clone(),
        worker_keypair,
        worker_id,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache.clone(),
        parameters,
        store,
        metrics,
    );

    // Spawn a network listener to receive our batch's digest.
    let batch = batch();
    let serialized_batch = serialize_batch_message(batch.clone());
    let batch_digest = serialized_batch_digest(&serialized_batch).unwrap();

    let primary_address = committee.primary(&name).unwrap().worker_to_primary;
    let expected =
        bincode::serialize(&WorkerPrimaryMessage::OurBatch(batch_digest, worker_id)).unwrap();
    let mut handle = WorkerToPrimaryMockServer::spawn(primary_address);

    // Spawn enough workers' listeners to acknowledge our batches.
    let mut other_workers = Vec::new();
    for (_, addresses) in worker_cache.load().others_workers(&name, &worker_id) {
        let address = addresses.worker_to_worker;
        other_workers.push(WorkerToWorkerMockServer::spawn(address));
    }

    // Wait till other services have been able to start up
    tokio::task::yield_now().await;
    // Send enough transactions to create a batch.
    let address = worker_cache
        .load()
        .worker(&name, &worker_id)
        .unwrap()
        .transactions;
    let config = mysten_network::config::Config::new();
    let channel = config.connect_lazy(&address).unwrap();
    let mut client = TransactionsClient::new(channel);
    for tx in batch.0 {
        let txn = TransactionProto {
            transaction: Bytes::from(tx.clone()),
        };
        client.submit_transaction(txn).await.unwrap();
    }

    // Ensure the primary received the batch's digest (ie. it did not panic).
    assert_eq!(handle.recv().await.unwrap().payload, expected);
}

#[tokio::test]
async fn handle_client_batch_request() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();

    let author = fixture.authorities().last().unwrap();
    let name = author.public_key();

    let worker_id = 0;
    let worker_keypair = author.worker(worker_id).keypair().copy();

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

    let registry = Registry::new();
    let metrics = Metrics {
        worker_metrics: Some(WorkerMetrics::new(&registry)),
        channel_metrics: Some(WorkerChannelMetrics::new(&registry)),
        endpoint_metrics: Some(WorkerEndpointMetrics::new(&registry)),
        network_metrics: Some(WorkerNetworkMetrics::new(&registry)),
    };

    // Spawn a `Worker` instance.
    Worker::spawn(
        name.clone(),
        worker_keypair,
        worker_id,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache.clone(),
        parameters,
        store,
        metrics,
    );

    // Spawn a client to ask for batches and receive the reply.
    tokio::task::yield_now().await;
    let address = worker_cache
        .load()
        .worker(&name, &worker_id)
        .unwrap()
        .worker_to_worker;
    let config = mysten_network::config::Config::new();
    let channel = config.connect_lazy(&address).unwrap();
    let mut client = WorkerToWorkerClient::new(channel);

    // Send batch request.
    let digests = vec![digest_batch(batch.clone())];
    let message = ClientBatchRequest(digests);
    let mut stream = client
        .client_batch_request(BincodeEncodedPayload::try_from(&message).unwrap())
        .await
        .unwrap()
        .into_inner();

    // Wait for the reply and ensure it is as expected.
    let bytes = stream.next().await.unwrap().unwrap().payload;
    let expected = Bytes::from(serialize_batch_message(batch));
    assert_eq!(bytes, expected);
}
