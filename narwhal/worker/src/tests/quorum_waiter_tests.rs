// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::metrics::WorkerMetrics;
use crate::NUM_SHUTDOWN_RECEIVERS;
use prometheus::Registry;
use test_utils::{
    batch, latest_protocol_version, test_network, CommitteeFixture, WorkerToWorkerMockServer,
};
use types::PreSubscribedBroadcastSender;

#[tokio::test]
async fn wait_for_quorum() {
    let (tx_quorum_waiter, rx_quorum_waiter) = test_utils::test_channel!(1);
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let my_primary = fixture.authorities().next().unwrap();
    let myself = fixture.authorities().next().unwrap().worker(0);

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let node_metrics = Arc::new(WorkerMetrics::new(&Registry::new()));

    // setup network
    let network = test_network(myself.keypair(), &myself.info().worker_address);
    // Spawn a `QuorumWaiter` instance.
    let _quorum_waiter_handler = QuorumWaiter::spawn(
        my_primary.authority().clone(),
        /* worker_id */ 0,
        committee.clone(),
        worker_cache.clone(),
        tx_shutdown.subscribe(),
        rx_quorum_waiter,
        network.clone(),
        node_metrics,
    );

    // Make a batch.
    let batch = batch(&latest_protocol_version());
    let message = WorkerBatchMessage {
        batch: batch.clone(),
    };

    // Spawn enough listeners to acknowledge our batches.
    let mut listener_handles = Vec::new();
    for worker in fixture.authorities().skip(1).map(|a| a.worker(0)) {
        let handle =
            WorkerToWorkerMockServer::spawn(worker.keypair(), worker.info().worker_address.clone());
        listener_handles.push(handle);

        // ensure that the networks are connected
        network
            .connect(worker.info().worker_address.to_anemo_address().unwrap())
            .await
            .unwrap();
    }

    // Forward the batch along with the handlers to the `QuorumWaiter`.
    let (s, r) = tokio::sync::oneshot::channel();
    tx_quorum_waiter.send((batch.clone(), s)).await.unwrap();

    // Wait for the `QuorumWaiter` to gather enough acknowledgements and output the batch.
    r.await.unwrap();

    // Ensure the other listeners correctly received the batch.
    for (mut handle, _network) in listener_handles {
        assert_eq!(handle.recv().await.unwrap(), message);
    }
}

#[tokio::test]
async fn pipeline_for_quorum() {
    let (tx_quorum_waiter, rx_quorum_waiter) = test_utils::test_channel!(1);
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let my_primary = fixture.authorities().next().unwrap();
    let myself = fixture.authorities().next().unwrap().worker(0);

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let node_metrics = Arc::new(WorkerMetrics::new(&Registry::new()));

    // setup network
    let network = test_network(myself.keypair(), &myself.info().worker_address);
    // Spawn a `QuorumWaiter` instance.
    let _quorum_waiter_handler = QuorumWaiter::spawn(
        my_primary.authority().clone(),
        /* worker_id */ 0,
        committee.clone(),
        worker_cache.clone(),
        tx_shutdown.subscribe(),
        rx_quorum_waiter,
        network.clone(),
        node_metrics,
    );

    // Make a batch.
    let batch = batch(&latest_protocol_version());
    let message = WorkerBatchMessage {
        batch: batch.clone(),
    };

    // Spawn enough listeners to acknowledge our batches.
    let mut listener_handles = Vec::new();
    for worker in fixture.authorities().skip(1).map(|a| a.worker(0)) {
        let handle =
            WorkerToWorkerMockServer::spawn(worker.keypair(), worker.info().worker_address.clone());
        listener_handles.push(handle);

        // ensure that the networks are connected
        network
            .connect(worker.info().worker_address.to_anemo_address().unwrap())
            .await
            .unwrap();
    }

    // Forward the batch along with the handlers to the `QuorumWaiter`.
    let (s0, r0) = tokio::sync::oneshot::channel();
    tx_quorum_waiter.send((batch.clone(), s0)).await.unwrap();

    // Forward the batch along with the handlers to the `QuorumWaiter`.
    let (s1, r1) = tokio::sync::oneshot::channel();
    tx_quorum_waiter.send((batch.clone(), s1)).await.unwrap();

    // Wait for the `QuorumWaiter` to gather enough acknowledgements and output the batch.
    r0.await.unwrap();

    // Ensure the other listeners correctly received the batch.
    for (mut handle, _network) in listener_handles {
        assert_eq!(handle.recv().await.unwrap(), message);
        assert_eq!(handle.recv().await.unwrap(), message);
    }

    r1.await.unwrap();
}
