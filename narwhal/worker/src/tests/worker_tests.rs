// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use arc_swap::ArcSwap;
use bytes::Bytes;
use fastcrypto::Hash;
use prometheus::Registry;
use store::rocks;
use test_utils::{
    batch, temp_dir, CommitteeFixture, WorkerToPrimaryMockServer, WorkerToWorkerMockServer,
};
use types::{TransactionsClient, WorkerPrimaryMessage};

#[tokio::test]
async fn handle_clients_transactions() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();

    let worker_id = 0;
    let my_primary = fixture.authorities().next().unwrap();
    let myself = my_primary.worker(worker_id);
    let name = my_primary.public_key();

    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };

    // Create a new test store.
    let db = rocks::DBMap::<BatchDigest, Batch>::open(temp_dir(), None, Some("batches")).unwrap();
    let store = Store::new(db);

    let registry = Registry::new();
    let metrics = crate::metrics::initialise_metrics(&registry);

    // Spawn a `Worker` instance.
    Worker::spawn(
        name.clone(),
        myself.keypair(),
        worker_id,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        worker_cache.clone(),
        parameters,
        store,
        metrics,
    );

    // Spawn a network listener to receive our batch's digest.
    let batch = batch();
    let batch_digest = batch.digest();

    let primary_address = committee.primary(&name).unwrap();
    let expected = WorkerPrimaryMessage::OurBatch(batch_digest, worker_id);
    let (mut handle, _network) =
        WorkerToPrimaryMockServer::spawn(my_primary.network_keypair().copy(), primary_address);

    // Spawn enough workers' listeners to acknowledge our batches.
    let mut other_workers = Vec::new();
    for worker in fixture.authorities().skip(1).map(|a| a.worker(worker_id)) {
        let handle =
            WorkerToWorkerMockServer::spawn(worker.keypair(), worker.info().worker_address.clone());
        other_workers.push(handle);
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
    assert_eq!(handle.recv().await.unwrap(), expected);
}
