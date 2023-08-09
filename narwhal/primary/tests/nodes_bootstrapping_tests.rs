// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use bytes::Bytes;
use std::time::Duration;
use test_utils::cluster::{setup_tracing, Cluster};
use types::TransactionProto;

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_response_error_after_shutdown_internal_consensus() {
    // Enabled debug tracing so we can easily observe the
    // nodes logs.
    let _guard = setup_tracing();

    let delay = Duration::from_secs(10); // 10 seconds

    // A cluster of 4 nodes will be created, with internal consensus.
    let cluster = Cluster::new(None);

    // ==== Start first authority ====
    let authority = cluster.authority(0);
    authority.start(false, Some(1)).await;

    tokio::time::sleep(delay).await;

    authority.stop_all().await;

    tokio::time::sleep(delay).await;

    let worker_id = 0;
    let mut client = authority.new_transactions_client(&worker_id).await;

    // Create a fake transaction
    let tx_str = "test transaction".to_string();
    let tx = bcs::to_bytes(&tx_str).unwrap();
    let txn = TransactionProto {
        transaction: Bytes::from(tx),
    };

    // Should fail submitting to consensus.
    let Err(e) = client.submit_transaction(txn).await else {
        panic!("Submitting transactions after Narwhal shutdown should fail!");
    };
    assert!(
        e.message()
            .contains("error trying to connect: tcp connect error:"),
        "Actual: {}",
        e
    );
}

/// Nodes will be started in a staggered fashion. This is simulating
/// a real world scenario where nodes across validators will not start
/// in the same time.
#[ignore]
#[tokio::test]
async fn test_node_staggered_starts() {
    // Enabled debug tracing so we can easily observe the
    // nodes logs.
    let _guard = setup_tracing();

    let node_staggered_delay = Duration::from_secs(60 * 2); // 2 minutes

    // A cluster of 4 nodes will be created
    let cluster = Cluster::new(None);

    // ==== Start first authority ====
    cluster.authority(0).start(false, Some(1)).await;

    tokio::time::sleep(node_staggered_delay).await;

    // No node should be able to commit, no reported round was expected
    cluster.assert_progress(0, 0).await;

    // ==== Start second authority ====
    cluster.authority(1).start(false, Some(1)).await;

    tokio::time::sleep(node_staggered_delay).await;

    // No node should be able to commit, no reported round was expected
    cluster.assert_progress(0, 0).await;

    // ==== Start third authority ====
    // Now 2f + 1 nodes are becoming available and we expect all the nodes to
    // start making progress (advance in rounds).
    cluster.authority(2).start(false, Some(1)).await;

    tokio::time::sleep(node_staggered_delay).await;

    // We have only (f) unavailable nodes, so all should have made progress and committed at least after the first round
    cluster.assert_progress(3, 2).await;

    // ==== Start fourth authority ====
    // Now 3f + 1 nodes are becoming available (the whole network) and all the nodes
    // should make progress
    cluster.authority(3).start(false, Some(1)).await;

    tokio::time::sleep(node_staggered_delay).await;

    // All nodes are available so all should have made progress and committed at least after the first round
    cluster.assert_progress(4, 2).await;
}

/// All the nodes have an outage at the same time, when they recover, the rounds begin to advance.
#[ignore]
#[tokio::test]
async fn test_full_outage_and_recovery() {
    let stop_and_start_delay = Duration::from_secs(12);
    let node_advance_delay = Duration::from_secs(60);

    // A cluster of 4 nodes will be created
    let mut cluster = Cluster::new(None);

    // ===== Start the cluster ====
    cluster.start(Some(4), Some(1), None).await;

    // Let the nodes advance a bit
    tokio::time::sleep(node_advance_delay).await;

    // Stop all the nodes
    cluster.authority(0).stop_all().await;
    tokio::time::sleep(stop_and_start_delay).await;

    cluster.authority(1).stop_all().await;
    tokio::time::sleep(stop_and_start_delay).await;

    cluster.authority(2).stop_all().await;
    tokio::time::sleep(stop_and_start_delay).await;

    cluster.authority(3).stop_all().await;
    tokio::time::sleep(stop_and_start_delay).await;

    // Start all the nodes
    cluster.authority(0).start(true, Some(1)).await;
    tokio::time::sleep(stop_and_start_delay).await;

    cluster.authority(1).start(true, Some(1)).await;
    tokio::time::sleep(stop_and_start_delay).await;

    cluster.authority(2).start(true, Some(1)).await;
    tokio::time::sleep(stop_and_start_delay).await;

    cluster.authority(3).start(true, Some(1)).await;

    // now wait a bit to give the opportunity to recover
    tokio::time::sleep(node_advance_delay).await;

    // Ensure that nodes have made progress
    cluster.assert_progress(4, 2).await;
}

#[ignore]
#[tokio::test]
async fn test_second_node_restart() {
    // Enabled debug tracing so we can easily observe the
    // nodes logs.
    let _guard = setup_tracing();

    let restart_delay = Duration::from_secs(120);
    let node_advance_delay = Duration::from_secs(60);

    // A cluster of 4 nodes will be created
    let mut cluster = Cluster::new(None);

    // ===== Start the cluster ====
    cluster.start(Some(4), Some(1), None).await;

    // Let the nodes advance a bit
    tokio::time::sleep(node_advance_delay).await;

    // Now restart node 2 with some delay between
    cluster.authority(2).restart(true, restart_delay).await;

    // now wait a bit to give the opportunity to recover
    tokio::time::sleep(node_advance_delay).await;

    // Ensure that nodes have made progress
    cluster.assert_progress(4, 2).await;

    // Now restart node 3 with some delay between
    cluster.authority(3).restart(true, restart_delay).await;

    // now wait a bit to give the opportunity to recover
    tokio::time::sleep(node_advance_delay).await;

    // Ensure that nodes have made progress
    cluster.assert_progress(4, 2).await;
}

#[ignore]
#[tokio::test]
/// We are testing the loss of liveness of a healthy cluster. While 3f+1 nodes run
/// we are shutting down f+1 nodes. Then we are bringing the f+1 nodes back again
/// We expect the restarted nodes to be able to make new proposals, and all the nodes
/// should be able to propose from where they left of at last round, and the rounds should
/// all advance.
async fn test_loss_of_liveness_without_recovery() {
    // Enabled debug tracing so we can easily observe the
    // nodes logs.
    let _guard = setup_tracing();

    let node_advance_delay = Duration::from_secs(60);

    // A cluster of 4 nodes will be created
    let mut cluster = Cluster::new(None);

    // ===== Start the cluster ====
    cluster.start(Some(4), Some(1), None).await;

    // Let the nodes advance a bit
    tokio::time::sleep(node_advance_delay).await;

    // Ensure that nodes have made progress
    cluster.assert_progress(4, 2).await;

    // Now stop node 2 & 3
    cluster.authority(2).stop_all().await;
    cluster.authority(3).stop_all().await;

    // wait and fetch the latest commit round
    tokio::time::sleep(node_advance_delay).await;
    let rounds_1 = cluster.assert_progress(2, 0).await;

    // wait and fetch again the rounds
    tokio::time::sleep(node_advance_delay).await;
    let rounds_2 = cluster.assert_progress(2, 0).await;

    // We assert that nodes haven't advanced at all
    assert_eq!(rounds_1, rounds_2);

    // Now bring up nodes
    cluster.authority(2).start(true, Some(1)).await;
    cluster.authority(3).start(true, Some(1)).await;

    // wait and fetch the latest commit round. All of them should have advanced and we allow a small
    // threshold in case some node is faster than the others
    tokio::time::sleep(node_advance_delay).await;
    let rounds_3 = cluster.assert_progress(4, 2).await;

    // we test that nodes 0 & 1 have actually advanced in rounds compared to before.
    assert!(rounds_3.get(&0) > rounds_2.get(&0));
    assert!(rounds_3.get(&1) > rounds_2.get(&1));
}

#[ignore]
#[tokio::test]
/// We are testing the loss of liveness of a healthy cluster. While 3f+1 nodes run
/// we are shutting down f+1 nodes one by one with some delay between them.
/// Then we are bringing the f+1 nodes back again. We expect the cluster to
/// recover and effectively make progress.
async fn test_loss_of_liveness_with_recovery() {
    // Enabled debug tracing so we can easily observe the
    // nodes logs.
    let _guard = setup_tracing();

    let node_advance_delay = Duration::from_secs(60);

    // A cluster of 4 nodes will be created
    let mut cluster = Cluster::new(None);

    // ===== Start the cluster ====
    cluster.start(Some(4), Some(1), None).await;

    // Let the nodes advance a bit
    tokio::time::sleep(node_advance_delay).await;

    // Ensure that nodes have made progress
    cluster.assert_progress(4, 2).await;

    // Now stop node 2
    cluster.authority(2).stop_all().await;

    // allow other nodes to advance
    tokio::time::sleep(node_advance_delay).await;

    // Now stop node 3
    cluster.authority(3).stop_all().await;

    // wait and fetch the latest commit round
    tokio::time::sleep(node_advance_delay).await;
    let rounds_1 = cluster.assert_progress(2, 0).await;

    // wait and fetch again the rounds
    tokio::time::sleep(node_advance_delay).await;
    let rounds_2 = cluster.assert_progress(2, 0).await;

    // We assert that nodes haven't advanced at all
    assert_eq!(rounds_1, rounds_2);

    // Now bring up nodes
    cluster.authority(2).start(true, Some(1)).await;
    cluster.authority(3).start(true, Some(1)).await;

    // wait and fetch the latest commit round
    tokio::time::sleep(node_advance_delay).await;
    let rounds_3 = cluster.assert_progress(4, 0).await;

    let round_2_max = rounds_2.values().max().unwrap();
    assert!(
        rounds_3.values().all(|v| v > round_2_max),
        "All the nodes should have advanced more from the previous round"
    );
}
