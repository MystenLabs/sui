// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use crate::authority::test_authority_builder::TestAuthorityBuilder;
use sui_types::base_types::TransactionDigest;
use sui_types::transaction::TransactionKey;
use tokio::time::timeout;

#[tokio::test]
async fn test_notify_read_executed_transactions_to_checkpoint() {
    let authority_state = TestAuthorityBuilder::new().build().await;
    let store = authority_state.epoch_store_for_testing();
    let checkpoint_sequence_1 = 10;
    let checkpoint_sequence_2 = 12;

    let txes_to_be_notified = vec![
        TransactionDigest::random(),
        TransactionDigest::random(),
        TransactionDigest::random(),
    ];

    // Insert only the first transaction already
    store
        .insert_finalized_transactions(
            vec![txes_to_be_notified[0]].as_slice(),
            checkpoint_sequence_1,
        )
        .expect("Should not fail");

    // Now register to get notified for the addition of some of the above transactions
    let txes_to_be_notified_cloned = txes_to_be_notified.clone();
    let handle = tokio::spawn(async move {
        let notify = store.transactions_executed_in_checkpoint_notify(txes_to_be_notified_cloned);
        notify.await
    });

    // Now insert the rest of the transactions
    let store = authority_state.epoch_store_for_testing();
    store
        .insert_finalized_transactions(&txes_to_be_notified[1..], checkpoint_sequence_2)
        .expect("Should not fail");

    // We should get notified about all the transactions having been executed via checkpoints
    let _ = timeout(Duration::from_secs(5), handle)
        .await
        .expect("Should not timeout")
        .expect("Should not fail");

    // And the transactions should be found into the table
    let result = store
        .multi_get_transaction_checkpoint(txes_to_be_notified.as_slice())
        .expect("Should not fail");
    assert_eq!(result.len(), txes_to_be_notified.len());

    assert_eq!(result[0].unwrap(), checkpoint_sequence_1);
    assert_eq!(result[1].unwrap(), checkpoint_sequence_2);
    assert_eq!(result[2].unwrap(), checkpoint_sequence_2);
}

/// Verifies that calling `notify_barrier_executed` with an `AccumulatorSettlement`
/// key resolves `notify_read_tx_key_to_digest`, which is the mechanism used by the
/// scheduler to detect that the barrier transaction has already been executed
/// (e.g. by the checkpoint executor) and skip the settlement wait.
///
/// Unlike `insert_tx_key`, `notify_barrier_executed` is in-memory-only (no DB
/// persistence), so it won't leave stale entries that survive a crash while
/// the effects may not.
#[tokio::test]
async fn test_notify_barrier_executed_resolves_settlement_wait() {
    let authority_state = TestAuthorityBuilder::new().build().await;
    let store = authority_state.epoch_store_for_testing();

    let epoch = store.epoch();
    let checkpoint_height = 42u64;
    let key = TransactionKey::AccumulatorSettlement(epoch, checkpoint_height);
    let barrier_digest = TransactionDigest::random();

    // Spawn a task that races wait_for_settlement_transactions against
    // notify_read_tx_key_to_digest, simulating the scheduler's select! pattern.
    let store_clone = authority_state.epoch_store_for_testing();
    let handle = tokio::spawn(async move {
        let keys = [key];
        tokio::select! {
            _txns = store_clone.wait_for_settlement_transactions(key) => {
                false // resolved via checkpoint builder notification
            }
            result = store_clone.notify_read_tx_key_to_digest(&keys) => {
                result.is_ok() // resolved via barrier tx execution
            }
        }
    });

    // Give the spawned task time to register with notify_read_tx_key_to_digest.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Simulate what commit_certificate does after writing effects:
    // it fires an in-memory notify for barrier transactions.
    store.notify_barrier_executed(key, barrier_digest);

    let resolved_via_tx_key = timeout(Duration::from_secs(5), handle)
        .await
        .expect("should not timeout")
        .expect("task should not panic");

    assert!(
        resolved_via_tx_key,
        "select! should resolve on notify_read_tx_key_to_digest, not wait_for_settlement_transactions"
    );
}
