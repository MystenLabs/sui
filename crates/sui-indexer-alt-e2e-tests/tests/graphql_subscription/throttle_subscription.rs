// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end coverage for the per-subscriber delivery throttle. The throttle lives in the SSE
//! handler, so it wraps every subscription identically; these tests drive it through the checkpoint
//! and transaction subscriptions.
//!
//! Timing is made robust by measuring a backlog: the validator produces several checkpoints, then a
//! subscription resumes from a past point so the backfill has every payload ready up front. The gaps
//! between arrivals then reflect the throttle's enforced sleep (`cost / rate`), not the rate at
//! which the validator produces checkpoints. All timing assertions are one-sided lower or upper
//! bounds with generous margins, since delivery can lag the sleep (scheduling) but never beat it.

use std::collections::BTreeSet;
use std::time::Duration;
use std::time::Instant;

use serde_json::Value;
use serde_json::json;

use super::testing::SubscriptionTestCluster;
use super::testing::collect_next_n_arrivals;
use super::testing::transfer_coins;

/// A lean checkpoint subscription selecting only the sequence number, resumed from genesis so the
/// backfill has a ready backlog.
const LEAN_QUERY: &str = "subscription($after: UInt53) { \
    checkpoints(afterCheckpoint: $after) { node { sequenceNumber } } }";

/// Wait until the validator has produced at least `target` checkpoints. A backfill from checkpoint 0
/// then has every payload ready up front, so the arrival gaps measure the throttle rather than the
/// rate at which the validator produces checkpoints.
async fn wait_for_tip(cluster: &SubscriptionTestCluster, target: u64) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while cluster.validator_checkpoint_tip() < target {
        assert!(
            Instant::now() < deadline,
            "validator did not reach checkpoint {target}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The sequence numbers of the checkpoint payloads collected, in arrival order.
fn checkpoint_seqs(arrivals: &[(Duration, Value)]) -> Vec<u64> {
    arrivals
        .iter()
        .map(|(_, v)| {
            v["data"]["checkpoints"]["node"]["sequenceNumber"]
                .as_u64()
                .expect("missing sequenceNumber")
        })
        .collect()
}

/// A lean checkpoint payload costs 10 (4 nodes + depth 3), so at 20 nodes/sec each gap is 0.5s. The
/// first is delivered immediately, and delivery stays in order.
#[tokio::test]
async fn throttle_paces_backfilled_checkpoints() {
    let cluster = SubscriptionTestCluster::new_with_throttle_budget(20).await;
    wait_for_tip(&cluster, 5).await;

    let mut stream = cluster
        .subscribe_with_variables(LEAN_QUERY, Some(json!({ "after": 0 })))
        .await;
    let arrivals = collect_next_n_arrivals(&mut stream, 4).await;
    let times: Vec<Duration> = arrivals.iter().map(|(t, _)| *t).collect();

    // First immediate, then three 0.5s gaps put the fourth ~1.5s in; assert a conservative >= 1s.
    assert!(
        *times.last().unwrap() >= Duration::from_secs(1),
        "throttled backlog drained too fast: {times:?}"
    );

    // Each consecutive gap is a real ~0.5s pacing delay.
    for pair in times.windows(2) {
        assert!(
            pair[1] - pair[0] >= Duration::from_millis(300),
            "throttle gap too small: {times:?}"
        );
    }

    // Throttling must not reorder or drop: sequential checkpoints starting at 1.
    assert_eq!(
        checkpoint_seqs(&arrivals),
        vec![1, 2, 3, 4],
        "unexpected order under throttle"
    );
}

/// Rate 0 disables pacing, so the same backlog drains at backfill speed (< 2s, not paced).
#[tokio::test]
async fn unthrottled_backfill_is_fast() {
    let cluster = SubscriptionTestCluster::new_with_throttle_budget(0).await;
    wait_for_tip(&cluster, 5).await;

    let mut stream = cluster
        .subscribe_with_variables(LEAN_QUERY, Some(json!({ "after": 0 })))
        .await;
    let total = collect_next_n_arrivals(&mut stream, 4)
        .await
        .last()
        .unwrap()
        .0;

    assert!(
        total < Duration::from_secs(2),
        "unthrottled backlog should drain fast, took {total:?}"
    );
}

/// Cost tracks the payload: a rich checkpoint (cost 18) paces slower than a lean one (cost 10) at the
/// same rate, 0.9s vs 0.5s per gap at 20 nodes/sec.
#[tokio::test]
async fn richer_payload_paces_slower() {
    const RICH_QUERY: &str = "subscription($after: UInt53) { \
        checkpoints(afterCheckpoint: $after) { node { \
            sequenceNumber digest timestamp networkTotalTransactions \
            epoch { epochId referenceGasPrice } } } }";

    let cluster = SubscriptionTestCluster::new_with_throttle_budget(20).await;
    wait_for_tip(&cluster, 5).await;

    let mut lean = cluster
        .subscribe_with_variables(LEAN_QUERY, Some(json!({ "after": 0 })))
        .await;
    let lean_total = collect_next_n_arrivals(&mut lean, 4)
        .await
        .last()
        .unwrap()
        .0;
    drop(lean);

    let mut rich = cluster
        .subscribe_with_variables(RICH_QUERY, Some(json!({ "after": 0 })))
        .await;
    let rich_total = collect_next_n_arrivals(&mut rich, 4)
        .await
        .last()
        .unwrap()
        .0;

    assert!(
        rich_total > lean_total + Duration::from_secs(1),
        "richer payload should pace slower: lean={lean_total:?} rich={rich_total:?}"
    );
}

/// The throttle wraps the transaction subscription too: each 4-node match costs 10, so at 20 nodes/sec
/// the backfilled run is paced 0.5s per edge, past the sub-second scan, with no matches dropped.
#[tokio::test]
async fn throttle_paces_transaction_backfill() {
    let mut cluster =
        SubscriptionTestCluster::new_with_ledger_history_and_throttle_budget(20).await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    // Capture the tip first so these matches land strictly after it and arrive via the backfill scan.
    let resume_from = cluster.validator_checkpoint_tip();
    let expected: BTreeSet<String> = transfer_coins(&mut cluster.validator, &[100, 200, 300, 400])
        .await
        .into_iter()
        .collect();
    // Let the matches settle into checkpoints and the ledger-history index before subscribing.
    tokio::time::sleep(Duration::from_secs(5)).await;

    let query = format!(
        "subscription($sender: SuiAddress!) {{ \
            transactions(filter: {{ afterCheckpoint: {resume_from}, sentAddress: $sender }}) {{ \
                node {{ digest }} }} }}"
    );
    let mut stream = cluster
        .subscribe_with_variables(&query, Some(json!({ "sender": sender.to_string() })))
        .await;

    // Four matches: first immediate, then three 0.5s gaps (cost 10 at 20 nodes/sec) put the run
    // ~1.5s in, past the sub-second scan; assert a conservative >= 1s.
    let arrivals = collect_next_n_arrivals(&mut stream, 4).await;
    assert!(
        arrivals.last().unwrap().0 >= Duration::from_secs(1),
        "throttled transaction backfill drained too fast: {:?}",
        arrivals.last().unwrap().0
    );

    // No drops: every expected digest arrived.
    let digests: BTreeSet<String> = arrivals
        .iter()
        .map(|(_, v)| {
            v["data"]["transactions"]["node"]["digest"]
                .as_str()
                .expect("edge missing digest")
                .to_string()
        })
        .collect();
    assert!(
        digests.is_superset(&expected),
        "throttled backfill dropped matches: got {digests:?}, expected {expected:?}"
    );
}
