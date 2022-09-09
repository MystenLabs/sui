// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use fastcrypto::Hash;
use store::rocks;
use test_utils::{batch, mock_network, temp_dir, CommitteeFixture, WorkerToWorkerMockServer};
use types::BatchDigest;

#[tokio::test]
async fn worker_batch_reply() {
    let (tx_worker_request, rx_worker_request) = test_utils::test_channel!(1);
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let id = 0;
    let worker_1 = fixture.authorities().next().unwrap().worker(id);
    let worker_2 = fixture.authorities().nth(1).unwrap().worker(id);
    let worker_2_primary_name = fixture.authorities().nth(1).unwrap().public_key();
    let (_tx_reconfiguration, rx_reconfiguration) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));

    // Create a new test store.
    let db = rocks::DBMap::<BatchDigest, Batch>::open(temp_dir(), None, Some("batches")).unwrap();
    let store = Store::new(db);

    // Add a batch to the store.
    let batch = batch();
    let batch_digest = batch.digest();
    store.write(batch_digest, batch.clone()).await;

    // setup network
    let network = mock_network(worker_1.keypair(), &worker_1.info().worker_to_worker);
    // Spawn an `Helper` instance.
    let _helper_handle = Helper::spawn(
        id,
        committee.clone(),
        worker_cache.clone(),
        store,
        rx_reconfiguration,
        rx_worker_request,
        P2pNetwork::new(network.clone()),
    );

    // Spawn a listener to receive the batch reply.
    let (mut handle, _network) = WorkerToWorkerMockServer::spawn(
        worker_2.keypair(),
        worker_2.info().worker_to_worker.clone(),
    );

    // ensure that the two networks are connected
    network
        .connect(network::multiaddr_to_address(&worker_2.info().worker_to_worker).unwrap())
        .await
        .unwrap();

    // Send a batch request.
    let digests = vec![batch_digest];
    tx_worker_request
        .send((digests, worker_2_primary_name))
        .await
        .unwrap();

    // Ensure the requestor received the batch (ie. it did not panic).
    let expected = WorkerMessage::Batch(batch);
    assert_eq!(handle.recv().await.unwrap(), expected);
}
