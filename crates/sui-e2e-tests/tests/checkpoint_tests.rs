// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use sui_macros::register_fail_point;
use sui_macros::register_fail_point_if;
use sui_macros::sim_test;
use sui_test_transaction_builder::make_transfer_sui_transaction;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn basic_checkpoints_integration_test() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let tx = make_transfer_sui_transaction(&test_cluster.wallet, None, None).await;
    let digest = *tx.digest();
    test_cluster.execute_transaction(tx).await;

    for _ in 0..600 {
        let all_included = test_cluster
            .swarm
            .validator_node_handles()
            .into_iter()
            .all(|handle| {
                handle.with(|node| {
                    node.state()
                        .epoch_store_for_testing()
                        .is_transaction_executed_in_checkpoint(&digest)
                        .unwrap()
                })
            });
        if all_included {
            // success
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    panic!("Did not include transaction in checkpoint in 60 seconds");
}

#[sim_test]
async fn test_checkpoint_split_brain() {
    #[cfg(msim)]
    {
        // this test intentionally halts the network by causing a fork, so we cannot panic on
        // loss of liveness
        use sui_core::authority::{init_checkpoint_timeout_config, CheckpointTimeoutConfig};
        init_checkpoint_timeout_config(CheckpointTimeoutConfig {
            warning_timeout: Duration::from_secs(2),
            panic_timeout: None,
        });
    }
    let committee_size = 9;
    // count number of nodes that have reached split brain condition
    let count_split_brain_nodes: Arc<Mutex<AtomicUsize>> = Default::default();
    let count_clone = count_split_brain_nodes.clone();
    register_fail_point("split_brain_reached", move || {
        let counter = count_clone.lock().unwrap();
        counter.fetch_add(1, Ordering::Relaxed);
    });

    register_fail_point_if("cp_execution_nondeterminism", || true);

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(committee_size)
        .build()
        .await;

    let tx = make_transfer_sui_transaction(&test_cluster.wallet, None, None).await;
    test_cluster
        .wallet
        .execute_transaction_may_fail(tx)
        .await
        .ok();

    // provide enough time for validators to detect split brain
    tokio::time::sleep(Duration::from_secs(20)).await;

    // all honest validators should eventually detect a split brain
    let final_count = count_split_brain_nodes.lock().unwrap();
    assert!(final_count.load(Ordering::Relaxed) >= 1);
}
