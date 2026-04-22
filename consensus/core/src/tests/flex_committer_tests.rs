// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use consensus_config::AuthorityIndex;
use parking_lot::RwLock;

use crate::{
    block::BlockAPI,
    commit::CommitAPI,
    context::Context,
    dag_state::DagState,
    flex_committer::FlexCommitter,
    leader_schedule_v3::NextCommitLeaderSchedule,
    storage::mem_store::MemStore,
    test_dag::{build_dag, build_dag_layer},
};

fn setup(num_authorities: usize) -> (Arc<Context>, Arc<RwLock<DagState>>, FlexCommitter) {
    let (mut context, _) = Context::new_for_test(num_authorities);
    context.protocol_config.set_enable_v3_for_testing(true);
    let context = Arc::new(context);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));
    let committer = FlexCommitter::new(context.clone(), dag_state.clone());
    (context, dag_state, committer)
}

fn next_commit_leader_schedule(allowed: Vec<AuthorityIndex>) -> NextCommitLeaderSchedule {
    NextCommitLeaderSchedule {
        // > 0 to force `maybe_refresh_pending_commit_state` to install our
        // allowed_leaders. The default index is 0.
        next_commit_index: 1,
        min_next_leader_round: 1,
        allowed_leaders: allowed,
    }
}

/// One leader per round, fully connected DAG → committer emits a commit
/// rooted at that leader.
#[tokio::test]
async fn try_commit_single_leader_committed() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, mut committer) = setup(4);

    build_dag(context, dag_state, None, 2);

    let next = next_commit_leader_schedule(vec![AuthorityIndex::new_for_test(0)]);
    let (commit, subdag) = committer
        .try_commit(next)
        .expect("expected a commit when round 1 is fully voted");

    assert_eq!(commit.leader().round, 1);
    assert_eq!(commit.leader().author, AuthorityIndex::new_for_test(0));
    assert!(
        subdag
            .blocks
            .iter()
            .any(|b| b.round() == 1 && b.author() == AuthorityIndex::new_for_test(0)),
        "sub-dag must include the leader block",
    );
}

/// One leader per round, no round-2 block references it → all slots Skip,
/// no commit emitted.
#[tokio::test]
async fn try_commit_single_leader_all_skipped() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, mut committer) = setup(4);

    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    let refs_without_leader: Vec<_> = refs_round_1
        .into_iter()
        .filter(|r| r.author != AuthorityIndex::new_for_test(0))
        .collect();
    build_dag(context, dag_state, Some(refs_without_leader), 2);

    let next = next_commit_leader_schedule(vec![AuthorityIndex::new_for_test(0)]);
    assert!(committer.try_commit(next).is_none());
}

/// One leader per round, no round-2 blocks at all → slot Undecided, no
/// commit emitted (and no anchor available for indirect commit).
#[tokio::test]
async fn try_commit_single_leader_undecided() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, mut committer) = setup(4);

    build_dag(context, dag_state, None, 1);

    let next = next_commit_leader_schedule(vec![AuthorityIndex::new_for_test(0)]);
    assert!(committer.try_commit(next).is_none());
}

/// Commits must be emitted in round order: while an earlier round still has
/// Undecided slots, no later round may be committed — even if the later
/// round is fully decided and has a committed leader.
///
/// DAG shape (the leader at every round is authority 0):
///   - Round 1: full layer (4 blocks).
///   - Round 2: split layer — authorities 0, 1 reference all of round 1
///     (vote for the round-1 leader), authorities 2, 3 omit it (blame).
///     Stake for / against the round-1 leader is 2/2, neither side reaches
///     quorum — round 1 stays Undecided.
///   - Round 3: full layer on top of round 2, so the round-2 leader has 4
///     votes and would direct-commit on its own.
///
/// `try_commit` must therefore return `None`: the round-1 ambiguity blocks
/// emitting the round-2 commit until indirect-commit can resolve round 1.
#[tokio::test]
async fn try_commit_does_not_skip_undecided_prefix_round() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, mut committer) = setup(4);
    let leader = AuthorityIndex::new_for_test(0);

    // Round 1: full layer.
    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    let refs_round_1_without_leader: Vec<_> = refs_round_1
        .iter()
        .copied()
        .filter(|r| r.author != leader)
        .collect();

    // Round 2: 2 voters for the round-1 leader, 2 blames against it.
    let mut authorities = context.committee.authorities();
    let mut connections = Vec::with_capacity(4);
    for _ in 0..2 {
        connections.push((authorities.next().unwrap().0, refs_round_1.clone()));
    }
    for _ in 0..2 {
        connections.push((
            authorities.next().unwrap().0,
            refs_round_1_without_leader.clone(),
        ));
    }
    let refs_round_2 = build_dag_layer(connections, dag_state.clone());

    // Round 3: full layer — the round-2 leader has a quorum of votes.
    build_dag(context.clone(), dag_state, Some(refs_round_2), 3);

    let next = next_commit_leader_schedule(vec![leader]);
    assert!(
        committer.try_commit(next).is_none(),
        "round 2 must not be committed while round 1 remains undecided",
    );
}

/// All four authorities are leaders at round 1, fully connected DAG → one
/// commit emitted whose sub-dag bundles every committed leader block.
#[tokio::test]
async fn try_commit_multi_leader_all_committed() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, mut committer) = setup(4);

    build_dag(context, dag_state, None, 2);

    let allowed: Vec<_> = (0..4).map(AuthorityIndex::new_for_test).collect();
    let next = next_commit_leader_schedule(allowed);
    let (commit, subdag) = committer
        .try_commit(next)
        .expect("expected a commit when all leaders fully voted");

    assert_eq!(commit.leader().round, 1);
    let round_1_authors: BTreeSet<AuthorityIndex> = subdag
        .blocks
        .iter()
        .filter(|b| b.round() == 1)
        .map(|b| b.author())
        .collect();
    let expected: BTreeSet<AuthorityIndex> = (0..4).map(AuthorityIndex::new_for_test).collect();
    assert_eq!(round_1_authors, expected);
}

/// All four authorities are leaders at round 1; round 2 omits authority-0's
/// block. Three slots Commit + one slot Skip — the round is still
/// committable, and the sub-dag excludes the skipped leader.
#[tokio::test]
async fn try_commit_multi_leader_some_skipped() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, mut committer) = setup(4);

    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    let refs_without_leader_0: Vec<_> = refs_round_1
        .into_iter()
        .filter(|r| r.author != AuthorityIndex::new_for_test(0))
        .collect();
    build_dag(context, dag_state, Some(refs_without_leader_0), 2);

    let allowed: Vec<_> = (0..4).map(AuthorityIndex::new_for_test).collect();
    let next = next_commit_leader_schedule(allowed);
    let (commit, subdag) = committer
        .try_commit(next)
        .expect("expected a commit — three leaders committed, one skipped");

    assert_eq!(commit.leader().round, 1);
    assert_ne!(
        commit.leader().author,
        AuthorityIndex::new_for_test(0),
        "skipped leader must not be the named commit leader",
    );
    let round_1_authors: BTreeSet<AuthorityIndex> = subdag
        .blocks
        .iter()
        .filter(|b| b.round() == 1)
        .map(|b| b.author())
        .collect();
    let expected: BTreeSet<AuthorityIndex> = (1..4).map(AuthorityIndex::new_for_test).collect();
    assert_eq!(
        round_1_authors, expected,
        "skipped leader's block is not in the sub-dag",
    );
}

/// All four authorities are leaders, but round 2 is a "diagonal" — each
/// authority's round-2 block references only its own round-1 block. Every
/// leader gets exactly one vote and three blames → all slots Skip → no
/// commit.
#[tokio::test]
async fn try_commit_multi_leader_all_skipped() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, mut committer) = setup(4);

    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);

    let connections: Vec<_> = context
        .committee
        .authorities()
        .map(|(idx, _)| {
            let own = refs_round_1
                .iter()
                .find(|r| r.author == idx)
                .copied()
                .expect("each authority has a round-1 block");
            (idx, vec![own])
        })
        .collect();
    build_dag_layer(connections, dag_state.clone());

    let allowed: Vec<_> = (0..4).map(AuthorityIndex::new_for_test).collect();
    let next = next_commit_leader_schedule(allowed);
    assert!(committer.try_commit(next).is_none());
}
