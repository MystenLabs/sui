// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for the consensus-shared, threshold-gated `TransactionDenyConfig`
//! feature on a 4-validator cluster (equal stake, 2500 each, 10000 total).
//!
//! Operators pre-define named rulesets, each gated on a stake threshold among an
//! eligible set of validators; a "default" bucket threshold-gates each individual
//! proposed rule element. Validators broadcast their proposed rules via consensus; a
//! receiver activates a ruleset only when eligible voting stake exceeds its threshold.
//!
//! Every test drives the system through public interfaces — `ConsensusAdapter::submit`
//! for broadcasts and observable effects on `effective_config` / `evaluate_status` /
//! transaction signing — so a future refactor that breaks the wire path fails loudly.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sui_config::transaction_deny_config::{
    DefaultDenyBucket, DenyElementKind, PeerDenySyncConfig, SharedDenyRuleThreshold,
    SharedDenyRuleset, ValidatorEligibility,
};
use sui_macros::sim_test;
use sui_node::SuiNodeHandle;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{AuthorityName, FullObjectRef, ObjectID};
use sui_types::messages_consensus::{
    ConsensusTransaction, SharedTransactionDenyConfig, SharedTransactionDenyConfigV1,
};
use sui_types::transaction_deny_rules::TransactionDenyRules;
use test_cluster::{TestCluster, TestClusterBuilder};

const PROPAGATION_TIMEOUT: Duration = Duration::from_secs(60);

fn enable_protocol_flags() -> sui_protocol_config::OverrideGuard {
    sui_protocol_config::ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
        c.set_share_transaction_deny_config_in_consensus_for_testing(true);
        c
    })
}

// ===== Config-construction helpers =====

/// A `TransactionDenyRules` denying the objects identified by `bytes`.
fn rules_objs(bytes: &[u8]) -> TransactionDenyRules {
    TransactionDenyRules {
        object_deny_list: bytes
            .iter()
            .map(|b| ObjectID::from_single_byte(*b))
            .collect(),
        ..Default::default()
    }
}

/// Eligibility that includes every committee member (empty denylist).
fn all_eligible() -> ValidatorEligibility {
    ValidatorEligibility::Denylist(BTreeSet::new())
}

fn prelisted(
    name: &str,
    rules: TransactionDenyRules,
    eligibility: ValidatorEligibility,
    threshold_percent: u16,
) -> SharedDenyRuleset {
    SharedDenyRuleset {
        name: name.to_string(),
        rules,
        threshold: SharedDenyRuleThreshold {
            eligibility,
            stake_threshold_percent: threshold_percent,
        },
    }
}

fn default_bucket(
    name: &str,
    kinds: &[DenyElementKind],
    eligibility: ValidatorEligibility,
    threshold_percent: u16,
) -> DefaultDenyBucket {
    DefaultDenyBucket {
        name: name.to_string(),
        element_kinds: kinds.iter().copied().collect(),
        threshold: SharedDenyRuleThreshold {
            eligibility,
            stake_threshold_percent: threshold_percent,
        },
    }
}

// ===== Cluster builders =====

/// Build a 4-validator cluster; each validator's `peer_deny_sync_config` is computed by
/// `sync_picker` against the full set of committee authority names.
async fn build_cluster<S>(sync_picker: S) -> TestCluster
where
    S: Fn(AuthorityName, &[AuthorityName]) -> PeerDenySyncConfig + Send + Sync + 'static,
{
    TestClusterBuilder::new()
        .with_num_validators(4)
        .with_epoch_duration_ms(60_000)
        .with_peer_deny_sync_config_per_validator(Arc::new(sync_picker))
        .build()
        .await
}

// ===== Broadcast / propagation helpers =====

/// Submit an `UpdateTransactionDenyConfig` from `sender_handle` through the same path
/// the admin endpoint uses (`TransactionDenyConfigManager::submit_broadcast`).
/// Returns the generation.
async fn broadcast_via_consensus(
    sender_handle: &SuiNodeHandle,
    rules: Option<TransactionDenyRules>,
) -> u64 {
    sender_handle
        .with_async(|node| async move {
            let manager = node.state().transaction_deny_config_manager().clone();
            let epoch_store = node.state().load_epoch_store_one_call_per_task();
            let consensus_adapter = node
                .consensus_adapter()
                .await
                .expect("validator components must be running");
            manager
                .submit_broadcast(rules, &consensus_adapter, &epoch_store)
                .expect("submit_broadcast")
        })
        .await
}

/// Like `broadcast_via_consensus` but bypasses the manager's monotonic generation guard
/// so a test can deliberately submit an out-of-order generation. Still uses the public
/// `ConsensusAdapter::submit` path; only the message construction differs.
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

/// Wait until every validator — including the originator, which applies its own
/// broadcast at submission — has accepted (or surpassed) `generation` from
/// `authority`. Panics on timeout.
async fn wait_for_generation(
    handles: &[SuiNodeHandle],
    authority: AuthorityName,
    generation: u64,
    timeout: Duration,
) {
    let deadline = Instant::now() + timeout;
    loop {
        let all = handles.iter().all(|h| {
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
                "Consensus did not propagate {authority:?} generation {generation} to all \
                 validators within {timeout:?}",
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Broadcast `rules` from `sender` and wait until every validator has it.
async fn broadcast_and_wait(
    handles: &[SuiNodeHandle],
    sender: &SuiNodeHandle,
    rules: Option<TransactionDenyRules>,
) {
    let generation = broadcast_via_consensus(sender, rules).await;
    let authority = sender.with(|n| n.state().name);
    wait_for_generation(handles, authority, generation, PROPAGATION_TIMEOUT).await;
}

/// Find the validator handle whose authority name matches `name`.
fn handle_for(handles: &[SuiNodeHandle], name: AuthorityName) -> SuiNodeHandle {
    handles
        .iter()
        .find(|h| h.with(|node| node.state().name == name))
        .expect("authority name not in cluster")
        .clone()
}

// ===== Observation helpers =====

fn effective_denies_obj(handle: &SuiNodeHandle, id: ObjectID) -> bool {
    handle.with(|node| {
        node.state()
            .transaction_deny_config_manager()
            .effective_config()
            .load()
            .get_object_deny_set()
            .contains(&id)
    })
}

fn effective_user_transaction_disabled(handle: &SuiNodeHandle) -> bool {
    handle.with(|node| {
        node.state()
            .transaction_deny_config_manager()
            .effective_config()
            .load()
            .user_transaction_disabled()
    })
}

fn ruleset_is_active(handle: &SuiNodeHandle, name: &str) -> bool {
    handle.with(|node| {
        node.state()
            .transaction_deny_config_manager()
            .evaluate_status()
            .prelisted
            .iter()
            .find(|p| p.name == name)
            .expect("pre-listed ruleset name not found")
            .active
    })
}

// ===== Tests =====

/// Full lifecycle through real transaction execution: a baseline transfer succeeds; a
/// pre-listed `user_transaction_disabled` ruleset is voted past its stake threshold by
/// the whole committee and signing stops; withdrawing the votes restores signing.
#[sim_test]
async fn test_lifecycle_blocks_then_withdraws() {
    let _guard = enable_protocol_flags();
    let kill_rules = TransactionDenyRules {
        user_transaction_disabled: true,
        ..Default::default()
    };
    let cfg_rules = kill_rules.clone();
    let cluster = build_cluster(move |_me, _all| PeerDenySyncConfig {
        rulesets: vec![prelisted("kill", cfg_rules.clone(), all_eligible(), 50)],
        ..Default::default()
    })
    .await;
    let handles = cluster.all_validator_handles();

    let context = &cluster.wallet;
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let accounts = context.get_all_accounts_and_gas_objects().await.unwrap();
    let sender = accounts[0].0;
    let receiver = accounts[1].0;
    let new_transfer = || async {
        let accounts = context.get_all_accounts_and_gas_objects().await.unwrap();
        let gas_object = accounts[0].1[0];
        let object_to_send = accounts[0].1[1];
        context
            .sign_transaction(
                &TestTransactionBuilder::new(sender, gas_object, gas_price)
                    .transfer(FullObjectRef::from_fastpath_ref(object_to_send), receiver)
                    .build(),
            )
            .await
    };

    // Phase 1 — baseline succeeds before any votes.
    context
        .execute_transaction_must_succeed(new_transfer().await)
        .await;

    // Phase 2 — all 4 validators vote for the kill ruleset. Each validator then sees the
    // other 3 (7500 stake > 50% of 10000) and activates it.
    for h in &handles {
        broadcast_and_wait(&handles, h, Some(kill_rules.clone())).await;
    }
    let err = context
        .execute_transaction_may_fail(new_transfer().await)
        .await
        .expect_err("transaction must be denied while the kill ruleset is active");
    assert!(
        err.to_string().contains("temporarily disabled"),
        "unexpected error message: {err}",
    );

    // Phase 3 — all 4 withdraw; the ruleset drops below threshold everywhere.
    for h in &handles {
        broadcast_and_wait(&handles, h, None).await;
    }
    context
        .execute_transaction_must_succeed(new_transfer().await)
        .await;
}

/// A pre-listed ruleset activates only once eligible voting stake reaches its
/// threshold. With an all-eligible 60% ruleset: 2/4 validators (5000 = 50%) is not
/// enough, 3/4 (7500 = 75%) is.
#[sim_test]
async fn test_prelisted_config_activates_at_threshold() {
    let _guard = enable_protocol_flags();
    let cluster = build_cluster(|_me, _all| PeerDenySyncConfig {
        rulesets: vec![prelisted("c", rules_objs(&[1]), all_eligible(), 60)],
        ..Default::default()
    })
    .await;
    let handles = cluster.all_validator_handles();
    let observer = &handles[3];

    // Two voters: 5000 stake = 50%, below the 60% threshold.
    broadcast_and_wait(&handles, &handles[0], Some(rules_objs(&[1]))).await;
    broadcast_and_wait(&handles, &handles[1], Some(rules_objs(&[1]))).await;
    assert!(!ruleset_is_active(observer, "c"));
    assert!(!effective_denies_obj(
        observer,
        ObjectID::from_single_byte(1)
    ));

    // Third voter: 7500 stake = 75% >= 60% — ruleset activates.
    broadcast_and_wait(&handles, &handles[2], Some(rules_objs(&[1]))).await;
    assert!(ruleset_is_active(observer, "c"));
    assert!(effective_denies_obj(
        observer,
        ObjectID::from_single_byte(1)
    ));
}

/// A single proposal counts as a vote for every pre-listed ruleset whose rules it is a
/// superset of — including partially-overlapping and nested rulesets.
#[sim_test]
async fn test_superset_votes_for_overlapping_configs() {
    let _guard = enable_protocol_flags();
    let cluster = build_cluster(|_me, _all| PeerDenySyncConfig {
        rulesets: vec![
            // `x` and `y` partially overlap on object 2; `z` is a superset of `x`.
            prelisted("x", rules_objs(&[1, 2]), all_eligible(), 50),
            prelisted("y", rules_objs(&[2, 3]), all_eligible(), 50),
            prelisted("z", rules_objs(&[1, 2, 3]), all_eligible(), 50),
        ],
        ..Default::default()
    })
    .await;
    let handles = cluster.all_validator_handles();
    let observer = &handles[3];

    // 3/4 validators each propose {1,2,3} — a superset of all three rulesets.
    for h in handles.iter().take(3) {
        broadcast_and_wait(&handles, h, Some(rules_objs(&[1, 2, 3]))).await;
    }
    for name in ["x", "y", "z"] {
        assert!(
            ruleset_is_active(observer, name),
            "ruleset {name} should be active"
        );
    }
    for id in [1, 2, 3] {
        assert!(effective_denies_obj(
            observer,
            ObjectID::from_single_byte(id)
        ));
    }
}

/// `ValidatorEligibility` filters whose votes count: an `Allowlist` ruleset ignores
/// non-allowlisted proposals, a `Denylist` ruleset ignores denylisted proposals — and
/// both still activate once eligible votes reach the threshold.
#[sim_test]
async fn test_eligibility_filters_votes() {
    let _guard = enable_protocol_flags();
    // Two rulesets over the same rule: one allowlist-gated, one denylist-gated. The
    // picker and the test both sort `all`, so they agree on the lexicographic indices.
    let cluster = build_cluster(|_me, all| {
        let mut sorted = all.to_vec();
        sorted.sort();
        PeerDenySyncConfig {
            rulesets: vec![
                // Eligible: names[0], names[1] (5000 stake); 60% needs 3000.
                prelisted(
                    "allow",
                    rules_objs(&[1]),
                    ValidatorEligibility::Allowlist([sorted[0], sorted[1]].into_iter().collect()),
                    60,
                ),
                // Eligible: everyone but names[0] (7500 stake); 60% needs 4500.
                prelisted(
                    "deny",
                    rules_objs(&[1]),
                    ValidatorEligibility::Denylist([sorted[0]].into_iter().collect()),
                    60,
                ),
            ],
            ..Default::default()
        }
    })
    .await;
    let handles = cluster.all_validator_handles();
    let mut names = cluster.get_validator_pubkeys();
    names.sort();
    let observer = handle_for(&handles, names[3]);

    // names[2] votes: not allowlisted (`allow` ignores it); alone it is 2500 of `deny`'s
    // 7500 eligible stake — both rulesets stay inactive.
    broadcast_and_wait(
        &handles,
        &handle_for(&handles, names[2]),
        Some(rules_objs(&[1])),
    )
    .await;
    assert!(!ruleset_is_active(&observer, "allow"));
    assert!(!ruleset_is_active(&observer, "deny"));

    // names[0] votes: allowlisted but 2500 of 5000 is below 60%; denylisted, so `deny`
    // ignores it entirely — both still inactive.
    broadcast_and_wait(
        &handles,
        &handle_for(&handles, names[0]),
        Some(rules_objs(&[1])),
    )
    .await;
    assert!(!ruleset_is_active(&observer, "allow"));
    assert!(!ruleset_is_active(&observer, "deny"));

    // names[1] votes: `allow` reaches names[0]+names[1] = 5000 >= 3000; `deny` reaches
    // names[1]+names[2] = 5000 >= 4500. Both activate.
    broadcast_and_wait(
        &handles,
        &handle_for(&handles, names[1]),
        Some(rules_objs(&[1])),
    )
    .await;
    assert!(ruleset_is_active(&observer, "allow"));
    assert!(ruleset_is_active(&observer, "deny"));
}

/// A default bucket threshold-gates each proposed rule element of one of its
/// `element_kinds` independently: a well-supported element activates while a
/// less-proposed one in the same proposals does not.
#[sim_test]
async fn test_default_per_element_voting() {
    let _guard = enable_protocol_flags();
    let cluster = build_cluster(|_me, _all| PeerDenySyncConfig {
        default_buckets: vec![default_bucket(
            "objs",
            &[DenyElementKind::Object],
            all_eligible(),
            60,
        )],
        ..Default::default()
    })
    .await;
    let handles = cluster.all_validator_handles();
    let observer = &handles[3];

    // 3 validators propose object 1; only one of them also proposes object 2.
    broadcast_and_wait(&handles, &handles[0], Some(rules_objs(&[1, 2]))).await;
    broadcast_and_wait(&handles, &handles[1], Some(rules_objs(&[1]))).await;
    broadcast_and_wait(&handles, &handles[2], Some(rules_objs(&[1]))).await;

    // Object 1: 7500 = 75% >= 60% — applied. Object 2: 2500 = 25% — not applied.
    assert!(effective_denies_obj(
        observer,
        ObjectID::from_single_byte(1)
    ));
    assert!(!effective_denies_obj(
        observer,
        ObjectID::from_single_byte(2)
    ));
}

/// A rule element counts toward both its pre-listed ruleset and a default bucket
/// independently. Here the pre-listed ruleset's threshold is unreachable, but the
/// default bucket still applies the element.
#[sim_test]
async fn test_element_counts_for_both_prelisted_and_default() {
    let _guard = enable_protocol_flags();
    let cluster = build_cluster(|_me, _all| PeerDenySyncConfig {
        rulesets: vec![prelisted(
            "c",
            rules_objs(&[1]),
            all_eligible(),
            // 90% is unreachable with only 3/4 validators voting (7500 < 9000).
            90,
        )],
        default_buckets: vec![default_bucket(
            "objs",
            &[DenyElementKind::Object],
            all_eligible(),
            50,
        )],
        ..Default::default()
    })
    .await;
    let handles = cluster.all_validator_handles();
    let observer = &handles[3];

    for h in handles.iter().take(3) {
        broadcast_and_wait(&handles, h, Some(rules_objs(&[1]))).await;
    }
    // Pre-listed ruleset stays inactive (below 90%)...
    assert!(!ruleset_is_active(observer, "c"));
    // ...but object 1 is still denied because it crossed the default bucket's 50%.
    assert!(effective_denies_obj(
        observer,
        ObjectID::from_single_byte(1)
    ));
}

/// Two default buckets with different thresholds operate independently: the lower
/// bucket activates when stake crosses its bar; the higher bucket does not, even
/// though every validator's proposal contains an element of the higher bucket's kind.
/// In particular `UserTransactionDisabled` placed in a high-threshold bucket can be
/// kept off even with majority votes, while object deny-list entries in a
/// low-threshold bucket activate as usual.
#[sim_test]
async fn test_default_buckets_segregate_kinds() {
    let _guard = enable_protocol_flags();
    let cluster = build_cluster(|_me, _all| PeerDenySyncConfig {
        default_buckets: vec![
            default_bucket("objs", &[DenyElementKind::Object], all_eligible(), 60),
            default_bucket(
                "kill-switches",
                &[DenyElementKind::UserTransactionDisabled],
                all_eligible(),
                90,
            ),
        ],
        ..Default::default()
    })
    .await;
    let handles = cluster.all_validator_handles();
    let observer = &handles[3];

    let proposal = TransactionDenyRules {
        object_deny_list: [ObjectID::from_single_byte(1)].into_iter().collect(),
        user_transaction_disabled: true,
        ..Default::default()
    };
    // 3/4 validators broadcast — 7500 stake = 75%. Above the 60% obj bucket but
    // below the 90% kill-switch bucket.
    for h in handles.iter().take(3) {
        broadcast_and_wait(&handles, h, Some(proposal.clone())).await;
    }
    assert!(effective_denies_obj(
        observer,
        ObjectID::from_single_byte(1)
    ));
    assert!(!effective_user_transaction_disabled(observer));
}

/// A validator's own broadcast counts toward its own thresholds, exactly like a
/// peer's — there is no special "self" handling.
#[sim_test]
async fn test_own_broadcast_counts_as_a_vote() {
    let _guard = enable_protocol_flags();
    let cluster = build_cluster(|_me, _all| PeerDenySyncConfig {
        rulesets: vec![prelisted("c", rules_objs(&[1]), all_eligible(), 60)],
        ..Default::default()
    })
    .await;
    let handles = cluster.all_validator_handles();
    let broadcaster = &handles[0];

    // handles[1] and handles[2] broadcast; observed on handles[0], that is 5000 stake
    // (50%) — below the 60% threshold.
    broadcast_and_wait(&handles, &handles[1], Some(rules_objs(&[1]))).await;
    broadcast_and_wait(&handles, &handles[2], Some(rules_objs(&[1]))).await;
    assert!(!ruleset_is_active(broadcaster, "c"));

    // handles[0] broadcasts too, which also applies its own vote locally; its own
    // evaluation now sees 3 voters (7500 = 75% >= 60%) and the ruleset activates.
    broadcast_and_wait(&handles, broadcaster, Some(rules_objs(&[1]))).await;
    assert!(ruleset_is_active(broadcaster, "c"));
}

/// A stale-generation update must not overwrite the live proposal. A marker broadcast
/// from a different validator gives a sync point past the stale message.
#[sim_test]
async fn test_stale_generation_is_rejected() {
    let _guard = enable_protocol_flags();
    let cluster = build_cluster(|_me, _all| PeerDenySyncConfig {
        rulesets: vec![prelisted("c", rules_objs(&[1]), all_eligible(), 50)],
        ..Default::default()
    })
    .await;
    let handles = cluster.all_validator_handles();
    let broadcaster = &handles[0];
    let broadcaster_authority = broadcaster.with(|n| n.state().name);
    let marker = &handles[1];
    let marker_authority = marker.with(|n| n.state().name);

    // Live proposal at a manager-allocated (large) generation.
    let live_gen = broadcast_via_consensus(broadcaster, Some(rules_objs(&[1]))).await;
    wait_for_generation(
        &handles,
        broadcaster_authority,
        live_gen,
        PROPAGATION_TIMEOUT,
    )
    .await;

    // Deliberately stale generation 1 with empty rules — must be ignored.
    broadcast_via_consensus_with_explicit_generation(
        broadcaster,
        1,
        Some(TransactionDenyRules::default()),
    )
    .await;

    // Marker from a different validator: once it lands everywhere, consensus has
    // progressed past the stale message.
    let marker_gen = broadcast_via_consensus(marker, Some(TransactionDenyRules::default())).await;
    wait_for_generation(&handles, marker_authority, marker_gen, PROPAGATION_TIMEOUT).await;

    // Every validator still holds the live (non-stale) proposal.
    for h in &handles {
        h.with(|node| {
            let snapshot = node
                .state()
                .transaction_deny_config_manager()
                .peer_configs_snapshot();
            let entry = snapshot
                .get(&broadcaster_authority)
                .expect("broadcaster entry must still exist");
            assert_eq!(
                entry.generation(),
                live_gen,
                "stale update appears to have replaced the live entry",
            );
            assert!(
                entry
                    .rules()
                    .map(|r| r.object_deny_list.contains(&ObjectID::from_single_byte(1)))
                    .unwrap_or(false),
                "stale update appears to have cleared the live rules",
            );
        });
    }
}
