// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for resubmitting a transaction that has already been executed and
//! checkpointed, exercising the validator's `handle_submit_transaction` response across a
//! cold (post-restart) execution cache and an epoch boundary.
//!
//! A real validator restart rebuilds the in-memory `WritebackCache` empty over the same
//! on-disk RocksDB, so both resubmits below read through to the perpetual tables. The only thing
//! that differs between the two resubmits is whether the transaction's effects still exist in the
//! perpetual store. With them present, `get_executed_effects` hits and the validator returns
//! `SubmitTxResult::Executed` with the effects. With them removed, that check misses and
//! `transaction_executed_in_last_epoch` returns `Rejected(TransactionAlreadyExecuted)`.
//!
//! Sui never re-executes an already-executed checkpoint (the checkpoint executor resumes
//! strictly above the highest-executed watermark), so the effects removal in step 5 survives
//! the restart.

use std::time::Duration;

use sui_macros::sim_test;
use sui_types::base_types::{AuthorityName, SuiAddress, TransactionDigest};
use sui_types::crypto::get_account_key_pair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::messages_grpc::{SubmitTxRequest, SubmitTxResult};
use sui_types::transaction::Transaction;
use test_cluster::{TestCluster, TestClusterBuilder};

/// Stop a validator and start it again, returning once its gRPC is serving. The fresh node is
/// constructed over the same on-disk RocksDB, so its `WritebackCache` starts empty (cold)
/// while the perpetual tables are intact.
async fn restart_validator(cluster: &TestCluster, name: &AuthorityName) {
    cluster.stop_node(name);
    // Under simulation, `stop` only schedules teardown; the RocksDB file lock is released
    // asynchronously, so restarting immediately would race the previous instance for the same
    // db path and panic. Give the simulator time to finish tearing down. A real runtime joins
    // the node thread on stop, so the lock is already released there.
    tokio::time::sleep(Duration::from_secs(if cfg!(msim) { 5 } else { 1 })).await;
    cluster.start_node(name).await;

    let node = cluster.swarm.node(name).unwrap();
    let mut waited = Duration::ZERO;
    while node.health_check(true).await.is_err() {
        assert!(
            waited < Duration::from_secs(60),
            "validator did not become healthy after restart"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
        waited += Duration::from_millis(500);
    }
}

/// Submit `tx` directly to the (single) validator, bypassing the fullnode quorum driver, and
/// return that validator's `SubmitTxResult`. Retries transport errors that can briefly occur
/// while a just-restarted validator's connection is re-established.
async fn submit_to_validator(cluster: &TestCluster, tx: &Transaction) -> SubmitTxResult {
    let client = cluster
        .authority_aggregator()
        .authority_clients
        .iter()
        .next()
        .unwrap()
        .1
        .clone();

    let mut attempts = 0;
    loop {
        match client
            .submit_transaction(SubmitTxRequest::new_transaction(tx.clone()), None)
            .await
        {
            Ok(mut resp) => {
                assert_eq!(resp.results.len(), 1);
                return resp.results.pop().unwrap();
            }
            Err(e) => {
                attempts += 1;
                assert!(attempts < 20, "submit_transaction kept failing: {e}");
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

#[sim_test]
async fn test_resubmit_executed_transaction_after_restart_and_epoch_change() {
    telemetry_subscribers::init_for_testing();

    // Single validator: the node we restart is the whole committee, so there is no peer to
    // re-sync from and nothing advances the chain while it is down (no catch-up race). A long
    // epoch duration ensures only our explicit reconfiguration advances the epoch.
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_epoch_duration_ms(600_000)
        .build()
        .await;

    let validator = cluster.get_validator_pubkeys()[0];

    // 1. Submit and execute a transaction.
    let recipient: SuiAddress = get_account_key_pair().0;
    let tx_data = cluster
        .test_transaction_builder()
        .await
        .transfer_sui(Some(1), recipient)
        .build();
    let tx = cluster.sign_transaction(&tx_data).await;
    let tx_digest: TransactionDigest = *tx.digest();
    cluster.execute_transaction(tx.clone()).await;

    // 2. Wait until the transaction is in a checkpoint that the validator has *executed* — this
    //    is when its outputs are committed to the perpetual tables (so they survive a restart)
    //    and when its checkpoint drops below the highest-executed watermark (so a restart will
    //    not re-execute it).
    cluster.wait_for_tx_settlement_all_nodes(&[tx_digest]).await;

    let epoch_before = {
        let handle = cluster
            .swarm
            .node(&validator)
            .unwrap()
            .get_node_handle()
            .unwrap();
        handle.with(|node| {
            // Effects must be in the perpetual store, not merely the in-memory cache, for the
            // post-restart cold-cache read to find them.
            assert!(
                node.state()
                    .database_for_testing()
                    .get_executed_effects(&tx_digest)
                    .unwrap()
                    .is_some(),
                "executed effects should be persisted to the perpetual store before restart"
            );
            node.state().epoch_store_for_testing().epoch()
        })
    };

    // 3. Restart the validator (cold cache) and resubmit in the SAME epoch. The cache miss
    //    falls through to the perpetual store, which still has the effects -> Executed.
    restart_validator(&cluster, &validator).await;
    match submit_to_validator(&cluster, &tx).await {
        SubmitTxResult::Executed { details, .. } => {
            let details = details.expect("executed result should carry effects details");
            assert_eq!(details.effects.transaction_digest(), &tx_digest);
        }
        other => panic!("expected Executed for checkpointed tx after restart, got {other:?}"),
    }

    // 4. Force exactly one epoch change. Exactly one matters: `transaction_executed_in_last_epoch`
    //    only matches the immediately-preceding epoch.
    cluster.trigger_reconfiguration().await;
    let epoch_after = {
        let handle = cluster
            .swarm
            .node(&validator)
            .unwrap()
            .get_node_handle()
            .unwrap();
        handle.with(|node| node.state().epoch_store_for_testing().epoch())
    };
    assert_eq!(
        epoch_after,
        epoch_before + 1,
        "expected exactly one epoch advance"
    );

    // 5. Remove the effects from the perpetual store (simulating checkpoint pruning of a past
    //    epoch), restart again for a cold cache, and resubmit. Now the first `get_executed_effects`
    //    check misses, and `transaction_executed_in_last_epoch` (the transaction executed in the
    //    now-previous epoch) rejects it permanently as already executed.
    {
        let handle = cluster
            .swarm
            .node(&validator)
            .unwrap()
            .get_node_handle()
            .unwrap();
        handle.with(|node| {
            node.state()
                .database_for_testing()
                .remove_executed_effects_for_testing(&tx_digest)
                .unwrap();
        });
    }
    restart_validator(&cluster, &validator).await;
    match submit_to_validator(&cluster, &tx).await {
        SubmitTxResult::Rejected { error } => {
            let msg = error.to_string();
            assert!(
                msg.contains("was already executed"),
                "expected TransactionAlreadyExecuted, got: {msg}"
            );
        }
        other => panic!("expected Rejected(TransactionAlreadyExecuted), got {other:?}"),
    }
}
