// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for the consensus-shared `TransactionDenyConfig` feature on a
//! 4-validator cluster. Every test drives the system through public interfaces —
//! `ConsensusAdapter::submit` for broadcasts and observable effects on transaction
//! signing — so a future refactor that breaks the wire path will fail loudly here.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sui_config::transaction_deny_config::PeerDenySyncConfig;
use sui_macros::sim_test;
use sui_node::SuiNodeHandle;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{AuthorityName, FullObjectRef};
use sui_types::messages_consensus::{
    ConsensusTransaction, SharedTransactionDenyConfig, SharedTransactionDenyConfigV1,
};
use sui_types::transaction_deny_rules::TransactionDenyRules;
use test_cluster::{TestCluster, TestClusterBuilder};

fn enable_protocol_flags() -> sui_protocol_config::OverrideGuard {
    sui_protocol_config::ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
        c.set_share_transaction_deny_config_in_consensus_for_testing(true);
        c
    })
}

/// Build a 4-validator cluster where each validator's allowlist is computed by
/// `picker` against the full set of committee authority names.
async fn build_cluster_with_allowlist<F>(picker: F) -> TestCluster
where
    F: Fn(AuthorityName, &[AuthorityName]) -> BTreeSet<AuthorityName> + Send + Sync + 'static,
{
    TestClusterBuilder::new()
        .with_num_validators(4)
        .with_epoch_duration_ms(60_000)
        .with_peer_deny_sync_config_per_validator(Arc::new(move |me, all| PeerDenySyncConfig {
            peer_allowlist: picker(me, all),
            broadcast_on_startup: false,
            broadcast_on_epoch_change: false,
        }))
        .build()
        .await
}

/// Trust every committee member.
fn trust_all(_me: AuthorityName, all: &[AuthorityName]) -> BTreeSet<AuthorityName> {
    all.iter().copied().collect()
}

/// Submit an `UpdateTransactionDenyConfig` from `sender_handle` through the same
/// path the admin endpoint uses (manager-allocated monotonic generation +
/// `ConsensusAdapter::submit`). Returns the generation.
async fn broadcast_via_consensus(
    sender_handle: &SuiNodeHandle,
    rules: Option<TransactionDenyRules>,
) -> u64 {
    sender_handle
        .with_async(|node| async move {
            let manager = node.state().transaction_deny_config_manager().clone();
            let (consensus_tx, generation) = manager
                .build_share_consensus_tx(node.state().name, rules)
                .expect("build_share_consensus_tx");
            let epoch_store = node.state().load_epoch_store_one_call_per_task();
            let consensus_adapter = node
                .consensus_adapter()
                .await
                .expect("validator components must be running");
            consensus_adapter
                .submit(consensus_tx, None, &epoch_store, None, None)
                .expect("consensus submit");
            generation
        })
        .await
}

/// Like `broadcast_via_consensus` but bypasses the manager's monotonic generation
/// guard so a test can deliberately submit an out-of-order generation. Still uses
/// the public `ConsensusAdapter::submit` path; only the message construction
/// differs from production.
async fn broadcast_via_consensus_with_explicit_generation(
    sender_handle: &SuiNodeHandle,
    generation: u64,
    rules: Option<TransactionDenyRules>,
) {
    sender_handle
        .with_async(|node| async move {
            let msg = SharedTransactionDenyConfig::V1(SharedTransactionDenyConfigV1 {
                authority: node.state().name,
                generation,
                rules,
            });
            let consensus_tx = ConsensusTransaction::new_update_transaction_deny_config(msg);
            let epoch_store = node.state().load_epoch_store_one_call_per_task();
            let consensus_adapter = node
                .consensus_adapter()
                .await
                .expect("validator components must be running");
            consensus_adapter
                .submit(consensus_tx, None, &epoch_store, None, None)
                .expect("consensus submit");
        })
        .await
}

/// Wait until every receiving validator has accepted (or surpassed) `generation`
/// from `authority`. The originator itself is excluded — self-broadcasts loop back
/// via consensus and are dropped at apply_update time on principle. Panics on
/// timeout; use a generous bound (rare-class consensus messages can take ~30s in
/// this test cluster).
async fn wait_for_generation(
    handles: &[SuiNodeHandle],
    authority: AuthorityName,
    generation: u64,
    timeout: Duration,
) {
    let deadline = Instant::now() + timeout;
    loop {
        let all = handles
            .iter()
            .filter(|h| h.with(|n| n.state().name) != authority)
            .all(|h| {
                h.with(|node| {
                    node.state()
                        .transaction_deny_config_manager()
                        .peer_configs_snapshot()
                        .get(&authority)
                        .map(|m| m.generation() >= generation)
                        .unwrap_or(false)
                })
            });
        if all {
            return;
        }
        if Instant::now() > deadline {
            panic!(
                "Consensus did not propagate {authority:?} generation {generation} to all receiving validators within {timeout:?}",
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Find the validator handle whose authority name matches `name`.
fn handle_for(handles: &[SuiNodeHandle], name: AuthorityName) -> SuiNodeHandle {
    handles
        .iter()
        .find(|h| h.with(|node| node.state().name == name))
        .expect("authority name not in cluster")
        .clone()
}

/// Iterate the validator handles that are *not* `originator`. Self-broadcasts loop
/// back via consensus and are dropped at the originator's `apply_update`, so they
/// never appear in the originator's own `peer_configs` or `effective_config`.
/// Tests verifying broadcast-side effects must skip the originator.
fn receivers<'a>(
    handles: &'a [SuiNodeHandle],
    originator: AuthorityName,
) -> impl Iterator<Item = &'a SuiNodeHandle> + 'a {
    handles
        .iter()
        .filter(move |h| h.with(|n| n.state().name) != originator)
}

const PROPAGATION_TIMEOUT: Duration = Duration::from_secs(60);

/// Full lifecycle: a baseline transaction succeeds; a peer broadcasts a deny
/// recommendation against the sender's address and the next transaction is
/// rejected at signing time; a withdrawal restores signing.
#[sim_test]
async fn test_peer_recommendation_blocks_then_withdraws() {
    let _guard = enable_protocol_flags();
    let test_cluster = build_cluster_with_allowlist(trust_all).await;
    let handles = test_cluster.all_validator_handles();

    let context = &test_cluster.wallet;
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let accounts_and_objs = context.get_all_accounts_and_gas_objects().await.unwrap();
    let sender = accounts_and_objs[0].0;
    let receiver = accounts_and_objs[1].0;

    let broadcaster = handles[0].clone();
    let broadcaster_authority = broadcaster.with(|n| n.state().name);

    // Helper: build and submit a fresh sender→receiver transfer using whatever
    // fastpath gas/object refs are currently owned by `sender`. We refresh the
    // refs every time because each must-succeed transfer consumes them.
    let new_transfer = || async {
        let accounts_and_objs = context.get_all_accounts_and_gas_objects().await.unwrap();
        let gas_object = accounts_and_objs[0].1[0];
        let object_to_send = accounts_and_objs[0].1[1];
        context
            .sign_transaction(
                &TestTransactionBuilder::new(sender, gas_object, gas_price)
                    .transfer(FullObjectRef::from_fastpath_ref(object_to_send), receiver)
                    .build(),
            )
            .await
    };

    // Phase 1 — baseline: tx succeeds before any recommendation.
    context
        .execute_transaction_must_succeed(new_transfer().await)
        .await;

    // Phase 2 — broadcast a deny rule, wait for propagation, observe denial.
    let rules = TransactionDenyRules {
        address_deny_list: BTreeSet::from([sender]),
        ..Default::default()
    };
    let gen_deny = broadcast_via_consensus(&broadcaster, Some(rules)).await;
    wait_for_generation(
        &handles,
        broadcaster_authority,
        gen_deny,
        PROPAGATION_TIMEOUT,
    )
    .await;
    let err = context
        .execute_transaction_may_fail(new_transfer().await)
        .await
        .expect_err("transaction must be denied while peer recommendation is active");
    let msg = err.to_string();
    assert!(
        msg.contains("temporarily disabled") || msg.contains("denied"),
        "unexpected error message: {msg}",
    );

    // Phase 3 — withdraw, wait for propagation, observe signing restored.
    let gen_withdraw = broadcast_via_consensus(&broadcaster, None).await;
    wait_for_generation(
        &handles,
        broadcaster_authority,
        gen_withdraw,
        PROPAGATION_TIMEOUT,
    )
    .await;
    context
        .execute_transaction_must_succeed(new_transfer().await)
        .await;
}

/// A recommendation from a non-allowlisted authority must not affect any
/// validator's effective config or transaction signing. We send a marker from a
/// trusted peer afterwards to establish a "consensus has caught up past the bad
/// message" sync point, then assert the bad message was dropped.
#[sim_test]
async fn test_recommendation_from_non_allowlisted_peer_is_ignored() {
    let _guard = enable_protocol_flags();

    // Pick the "bad" authority deterministically: the lexicographically smallest
    // committee member. The picker (running per-validator with the genesis
    // committee slice) and the test code (running afterwards with
    // `get_validator_pubkeys`) may see different slice orders, so we sort to
    // ensure they agree on which authority is excluded.
    let test_cluster = build_cluster_with_allowlist(|_me, all| {
        let mut sorted = all.to_vec();
        sorted.sort();
        let bad = sorted[0];
        all.iter().filter(|n| **n != bad).copied().collect()
    })
    .await;
    let handles = test_cluster.all_validator_handles();
    let mut names = test_cluster.get_validator_pubkeys();
    names.sort();
    let bad_authority = names[0];
    let marker_authority = names[1];
    let bad_broadcaster = handle_for(&handles, bad_authority);
    let marker_sender = handle_for(&handles, marker_authority);

    let context = &test_cluster.wallet;
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let accounts_and_objs = context.get_all_accounts_and_gas_objects().await.unwrap();
    let sender = accounts_and_objs[0].0;
    let receiver = accounts_and_objs[1].0;
    let gas_object = accounts_and_objs[0].1[0];
    let object_to_send = accounts_and_objs[0].1[1];

    // Bad broadcaster: tries to deny `sender`.
    let bad_rules = TransactionDenyRules {
        address_deny_list: BTreeSet::from([sender]),
        ..Default::default()
    };
    broadcast_via_consensus(&bad_broadcaster, Some(bad_rules)).await;

    // Trusted marker sender: a benign empty recommendation. When this lands at all
    // validators, the bad broadcast (submitted before this one) has had time to
    // be sequenced and processed.
    let marker_gen =
        broadcast_via_consensus(&marker_sender, Some(TransactionDenyRules::default())).await;
    wait_for_generation(&handles, marker_authority, marker_gen, PROPAGATION_TIMEOUT).await;

    // Assert no validator accepted the bad broadcast.
    for h in &handles {
        let snap = h.with(|node| {
            node.state()
                .transaction_deny_config_manager()
                .peer_configs_snapshot()
        });
        assert!(
            !snap.contains_key(&bad_authority),
            "non-allowlisted peer's update was accepted on at least one validator",
        );
    }

    // The transfer succeeds since no validator denies `sender`.
    let tx = context
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, gas_price)
                .transfer(FullObjectRef::from_fastpath_ref(object_to_send), receiver)
                .build(),
        )
        .await;
    context.execute_transaction_must_succeed(tx).await;
}

/// A stale-generation update must not overwrite the live recommendation. Marker
/// from a different broadcaster gives us a sync point past the stale message.
#[sim_test]
async fn test_stale_generation_is_rejected_e2e() {
    let _guard = enable_protocol_flags();
    let test_cluster = build_cluster_with_allowlist(trust_all).await;
    let handles = test_cluster.all_validator_handles();

    let broadcaster = handles[0].clone();
    let broadcaster_authority = broadcaster.with(|n| n.state().name);
    let marker_sender = handles[1].clone();
    let marker_authority = marker_sender.with(|n| n.state().name);

    let context = &test_cluster.wallet;
    let accounts_and_objs = context.get_all_accounts_and_gas_objects().await.unwrap();
    let sender = accounts_and_objs[0].0;

    // Broadcast a real recommendation (live generation set by manager).
    let live_rules = TransactionDenyRules {
        address_deny_list: BTreeSet::from([sender]),
        ..Default::default()
    };
    let live_gen = broadcast_via_consensus(&broadcaster, Some(live_rules.clone())).await;
    wait_for_generation(
        &handles,
        broadcaster_authority,
        live_gen,
        PROPAGATION_TIMEOUT,
    )
    .await;

    // Submit a deliberately stale generation (1) from the same broadcaster, with
    // empty rules. If it were accepted it would clear the deny list.
    broadcast_via_consensus_with_explicit_generation(
        &broadcaster,
        1,
        Some(TransactionDenyRules::default()),
    )
    .await;

    // Marker from a different broadcaster (different consensus key): when it
    // lands at all validators, consensus has progressed past the stale message.
    let marker_gen =
        broadcast_via_consensus(&marker_sender, Some(TransactionDenyRules::default())).await;
    wait_for_generation(&handles, marker_authority, marker_gen, PROPAGATION_TIMEOUT).await;

    // The broadcaster's entry on every receiving validator must still be the live
    // (non-stale) one. The broadcaster itself doesn't carry a self-entry.
    for h in receivers(&handles, broadcaster_authority) {
        h.with(|node| {
            let snap = node
                .state()
                .transaction_deny_config_manager()
                .peer_configs_snapshot();
            let entry = snap
                .get(&broadcaster_authority)
                .expect("entry must still exist on receiver");
            assert_eq!(
                entry.generation(),
                live_gen,
                "stale update appears to have replaced the live entry's generation",
            );
            let still_blocking = entry
                .rules()
                .map(|r| r.address_deny_list.contains(&sender))
                .unwrap_or(false);
            assert!(
                still_blocking,
                "stale update appears to have replaced the live rules",
            );
        });
    }
}

/// Boolean kill switches (e.g. `user_transaction_disabled`) carried in a
/// recommendation must OR with every validator's local config.
#[sim_test]
async fn test_peer_recommendation_or_merges_boolean_kill_switches() {
    let _guard = enable_protocol_flags();
    let test_cluster = build_cluster_with_allowlist(trust_all).await;
    let handles = test_cluster.all_validator_handles();

    let broadcaster = handles[0].clone();
    let broadcaster_authority = broadcaster.with(|n| n.state().name);

    for h in &handles {
        assert!(!h.with(|node| {
            node.state()
                .transaction_deny_config_manager()
                .effective_config()
                .load()
                .user_transaction_disabled()
        }));
    }

    let rules = TransactionDenyRules {
        user_transaction_disabled: true,
        ..Default::default()
    };
    let generation = broadcast_via_consensus(&broadcaster, Some(rules)).await;
    wait_for_generation(
        &handles,
        broadcaster_authority,
        generation,
        PROPAGATION_TIMEOUT,
    )
    .await;

    for h in receivers(&handles, broadcaster_authority) {
        assert!(h.with(|node| {
            node.state()
                .transaction_deny_config_manager()
                .effective_config()
                .load()
                .user_transaction_disabled()
        }));
    }
    // The broadcaster's effective config is unchanged: local config is the source
    // of truth for our own rules; self-broadcasts don't loop back into our own
    // `peer_configs`.
    assert!(!broadcaster.with(|node| {
        node.state()
            .transaction_deny_config_manager()
            .effective_config()
            .load()
            .user_transaction_disabled()
    }));
}
