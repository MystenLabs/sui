// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_common::register_debug_fatal_handler;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use sui_macros::register_fail_point_arg;
use sui_macros::sim_test;
use sui_test_transaction_builder::make_transfer_sui_transaction;
use sui_types::base_types::AuthorityName;
use test_cluster::TestClusterBuilder;
use tokio::time::sleep;
use tracing::info;

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

    register_debug_fatal_handler!(
        "Split brain detected in checkpoint signature aggregation",
        move || {
            let counter = count_clone.lock().unwrap();
            counter.fetch_add(1, Ordering::Relaxed);
        }
    );

    register_fail_point_arg("simulate_fork_during_execution", || {
        Some((
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::<
                AuthorityName,
            >::new())),
            true, // full_halt = true for checkpoint tests (expects network halt)
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::<
                String,
                String,
            >::new())), // empty effects overrides - not used in this test
        ))
    });

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

#[sim_test]
async fn test_checkpoint_timestamps_non_decreasing() {
    let epoch_duration_ms = 10_000; // 10 seconds
    let num_epochs_to_run = 3;

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(epoch_duration_ms)
        .disable_fullnode_pruning()
        .build()
        .await;

    sleep(Duration::from_millis(
        epoch_duration_ms * num_epochs_to_run + epoch_duration_ms / 2,
    ))
    .await;

    // Retrieve checkpoints and verify timestamps from the first full node.
    let full_node = test_cluster
        .swarm
        .fullnodes()
        .next()
        .expect("No full node is found");

    let checkpoint_store = full_node
        .get_node_handle()
        .unwrap()
        .state()
        .checkpoint_store
        .clone();

    let highest_executed_checkpoint = checkpoint_store
        .get_highest_executed_checkpoint()
        .expect("Failed to get highest executed checkpoint")
        .expect("No executed checkpoints found in store");

    assert!(
        highest_executed_checkpoint.epoch() > 0,
        "Test did not run long enough to cross epochs"
    );

    let mut current_seq = *highest_executed_checkpoint.sequence_number();
    let mut prev_timestamp = highest_executed_checkpoint.timestamp();
    let mut checkpoints_checked = 0;

    // Iterate backwards from the highest checkpoint
    loop {
        if current_seq == 0 {
            info!("Reached checkpoint 0.");
            break;
        }
        current_seq -= 1;

        // Fetch the previous digest to continue iteration
        let current_checkpoint = checkpoint_store
            .get_checkpoint_by_sequence_number(current_seq)
            .expect("DB error getting current checkpoint")
            .unwrap_or_else(|| panic!("checkpoint missing for seq {}", current_seq));
        let current_timestamp = current_checkpoint.timestamp();
        assert!(
            current_timestamp <= prev_timestamp,
            "Timestamp decreased! current seq {}, {:?} vs {:?}",
            current_seq,
            current_timestamp,
            prev_timestamp,
        );
        prev_timestamp = current_timestamp;
        checkpoints_checked += 1;
    }

    assert!(checkpoints_checked > 0, "Test created only 1 checkpoint",);
}
