// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use consensus_config::{AuthorityIndex, Committee};
use parking_lot::RwLock;

use crate::{
    block::{BlockAPI, Slot, TestBlock, Transaction, VerifiedBlock, genesis_blocks},
    commit::LeaderStatus,
    context::Context,
    dag_state::DagState,
    leader_slot_decider::LeaderSlotDecider,
    storage::mem_store::MemStore,
    test_dag::{build_dag, build_dag_layer},
};

/// Setup with a v3 Committee.
fn setup(
    committee_size: usize,
    malicious_stake: u64,
    crash_stake: u64,
) -> (Arc<Context>, Arc<RwLock<DagState>>, LeaderSlotDecider) {
    let (mut context, _) = Context::new_for_test(committee_size);
    let v3_committee = Committee::new_v3(
        context.committee.epoch(),
        context.committee.authorities_slice().to_vec(),
        malicious_stake,
        crash_stake,
    );
    context = context.with_committee(v3_committee);
    context.protocol_config.set_enable_v3_for_testing(true);
    let context = Arc::new(context);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));
    let decider = LeaderSlotDecider::new(context.clone(), dag_state.clone());
    (context, dag_state, decider)
}

fn expect_commit(status: LeaderStatus, slot: Slot) -> VerifiedBlock {
    match status {
        LeaderStatus::Commit(block) => {
            assert_eq!(block.author(), slot.authority);
            assert_eq!(block.round(), slot.round);
            block
        }
        other => panic!("Expected Commit, got {other:?}"),
    }
}

fn expect_skip(status: LeaderStatus, slot: Slot) {
    match status {
        LeaderStatus::Skip(s) => assert_eq!(s, slot),
        other => panic!("Expected Skip, got {other:?}"),
    }
}

fn expect_undecided(status: LeaderStatus, slot: Slot) {
    match status {
        LeaderStatus::Undecided(s) => assert_eq!(s, slot),
        other => panic!("Expected Undecided, got {other:?}"),
    }
}

/// A fully connected round 2 commits the round-1 leader directly.
#[tokio::test]
async fn try_direct_decide_commit() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, decider) = setup(6, 1, 0);

    build_dag(context, dag_state, None, 2);

    let slot = Slot::new_for_test(1, 0);
    let status = decider.try_direct_decide(slot);

    expect_commit(status, slot);
}

/// When every round-2 block omits the round-1 leader, it is directly skipped.
#[tokio::test]
async fn try_direct_decide_skip() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, decider) = setup(6, 1, 0);

    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    let refs_without_leader: Vec<_> = refs_round_1
        .into_iter()
        .filter(|r| r.author != AuthorityIndex::new_for_test(0))
        .collect();
    build_dag(context, dag_state, Some(refs_without_leader), 2);

    let slot = Slot::new_for_test(1, 0);
    let status = decider.try_direct_decide(slot);

    expect_skip(status, slot);
}

/// Without next-round quorum, no decision is possible — Undecided.
#[tokio::test]
async fn try_direct_decide_undecided_no_next_round_quorum() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, decider) = setup(6, 1, 0);

    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);

    let slot = Slot::new_for_test(1, 0);
    expect_undecided(decider.try_direct_decide(slot), slot);

    let connections = context
        .committee
        .authorities()
        .take((context.committee.quorum_threshold() - 1) as usize)
        .map(|authority| (authority.0, refs_round_1.clone()))
        .collect();
    build_dag_layer(connections, dag_state);

    expect_undecided(decider.try_direct_decide(slot), slot);
}

/// Round 2 has neither quorum-many votes for nor quorum-many blames against
/// the leader — direct decision must be Undecided.
#[tokio::test]
async fn try_direct_decide_undecided_split() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, decider) = setup(9, 1, 1);

    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    let refs_without_leader: Vec<_> = refs_round_1
        .iter()
        .cloned()
        .filter(|r| r.author != AuthorityIndex::new_for_test(0))
        .collect();

    // 4 round-2 blocks reference all of round 1 (votes for leader); 5 omit
    // the leader. Total votes for leader = 4, against = 5 — neither side has
    // quorum (7) of stake.
    let mut authorities = context.committee.authorities();
    let mut connections = vec![];
    for _ in 0..4 {
        connections.push((authorities.next().unwrap().0, refs_round_1.clone()));
    }
    for _ in 0..5 {
        connections.push((authorities.next().unwrap().0, refs_without_leader.clone()));
    }
    build_dag_layer(connections, dag_state.clone());

    let slot = Slot::new_for_test(1, 0);
    let status = decider.try_direct_decide(slot);

    expect_undecided(status, slot);
}

/// If no block exists at the leader slot, a quorum of next-round blocks
/// directly skips the empty slot.
#[tokio::test]
async fn try_direct_decide_skip_empty_slot() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, decider) = setup(6, 1, 0);

    let empty_slot = Slot::new_for_test(1, 0);
    let genesis_refs: Vec<_> = genesis_blocks(context.as_ref())
        .into_iter()
        .map(|block| block.reference())
        .collect();
    let refs_round_1 = context
        .committee
        .authorities()
        .filter(|authority| authority.0 != empty_slot.authority)
        .map(|authority| (authority.0, genesis_refs.clone()))
        .collect();
    let refs_round_1 = build_dag_layer(refs_round_1, dag_state.clone());
    let connections = context
        .committee
        .authorities()
        .take(context.committee.quorum_threshold() as usize)
        .map(|authority| (authority.0, refs_round_1.clone()))
        .collect();
    build_dag_layer(connections, dag_state);

    expect_skip(decider.try_direct_decide(empty_slot), empty_slot);
}

/// Equivocating voters in the voting round must not be double-counted as
/// separate blames for direct skip.
#[tokio::test]
async fn try_direct_decide_undecided_duplicate_reject_voter() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, decider) = setup(6, 1, 0);

    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    let slot = Slot::new_for_test(1, 0);
    let refs_without_leader: Vec<_> = refs_round_1
        .iter()
        .cloned()
        .filter(|r| r.author != slot.authority)
        .collect();

    let author_0 = AuthorityIndex::new_for_test(0);
    let author_1 = AuthorityIndex::new_for_test(1);
    let author_2 = AuthorityIndex::new_for_test(2);
    let connections = vec![
        (author_0, refs_round_1.clone()),
        (author_1, refs_without_leader.clone()),
        (author_2, refs_without_leader.clone()),
        (AuthorityIndex::new_for_test(3), refs_without_leader.clone()),
        (AuthorityIndex::new_for_test(4), refs_without_leader.clone()),
    ];
    build_dag_layer(connections, dag_state.clone());

    let duplicate_non_vote = VerifiedBlock::new_for_test(
        TestBlock::new(2, author_1.value() as u32)
            .set_ancestors(refs_without_leader)
            .set_transactions(vec![Transaction::new(vec![1])])
            .build(),
    );
    dag_state.write().accept_block(duplicate_non_vote);

    // The unique rejecting authorities are only 1..4, despite author 1's
    // equivocation. A buggy block-counted reject tally would count author 1
    // twice and incorrectly skip.
    expect_undecided(decider.try_direct_decide(slot), slot);
}

/// A Byzantine voter with one voting block and one non-voting block counts on
/// both sides of the direct rule.
#[tokio::test]
async fn try_direct_decide_skip_counts_mixed_byzantine_voter() {
    telemetry_subscribers::init_for_testing();
    // v3 committee with quorum=7. Six honest reject voters are not enough to
    // skip by themselves, so this test depends on the mixed Byzantine voter
    // being counted as a reject voter as well.
    let (context, dag_state, decider) = setup(9, 1, 1);
    assert_eq!(context.committee.certification_threshold(), 4);
    assert_eq!(context.committee.quorum_threshold(), 7);

    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    let slot = Slot::new_for_test(1, 0);
    let refs_without_leader: Vec<_> = refs_round_1
        .iter()
        .cloned()
        .filter(|r| r.author != slot.authority)
        .collect();

    let author_0 = AuthorityIndex::new_for_test(0);
    let byzantine_author = AuthorityIndex::new_for_test(1);
    let connections = vec![
        (author_0, refs_round_1.clone()),
        (byzantine_author, refs_round_1.clone()),
        (AuthorityIndex::new_for_test(2), refs_without_leader.clone()),
        (AuthorityIndex::new_for_test(3), refs_without_leader.clone()),
        (AuthorityIndex::new_for_test(4), refs_without_leader.clone()),
        (AuthorityIndex::new_for_test(5), refs_without_leader.clone()),
        (AuthorityIndex::new_for_test(6), refs_without_leader.clone()),
        (AuthorityIndex::new_for_test(7), refs_without_leader.clone()),
    ];
    build_dag_layer(connections, dag_state.clone());

    let byzantine_non_vote = VerifiedBlock::new_for_test(
        TestBlock::new(2, byzantine_author.value() as u32)
            .set_ancestors(refs_without_leader)
            .set_transactions(vec![Transaction::new(vec![1])])
            .build(),
    );
    dag_state.write().accept_block(byzantine_non_vote);

    expect_skip(decider.try_direct_decide(slot), slot);
}

/// Equivocation in the leader slot does not prevent directly committing the
/// one block with quorum children.
#[tokio::test]
async fn try_direct_decide_equivocating_commit() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, decider) = setup(6, 1, 0);

    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    let slot = Slot::new_for_test(1, 1);
    let leader_ref = refs_round_1
        .iter()
        .find(|r| r.author == slot.authority)
        .copied()
        .expect("leader block should exist");
    let leader = dag_state
        .read()
        .get_block(&leader_ref)
        .expect("leader block should be in dag");
    let equivocating_leader = VerifiedBlock::new_for_test(
        TestBlock::new(slot.round, slot.authority.value() as u32)
            .set_ancestors(leader.ancestors().to_vec())
            .set_transactions(vec![Transaction::new(vec![1])])
            .build(),
    );
    let equivocating_leader_ref = equivocating_leader.reference();
    dag_state.write().accept_block(equivocating_leader);

    let refs_without_leader: Vec<_> = refs_round_1
        .iter()
        .cloned()
        .filter(|r| *r != leader_ref)
        .chain(std::iter::once(equivocating_leader_ref))
        .collect();
    let mut authorities = context.committee.authorities();
    let mut connections = vec![];
    for _ in 0..context.committee.quorum_threshold() {
        connections.push((authorities.next().unwrap().0, refs_round_1.clone()));
    }
    connections.push((authorities.next().unwrap().0, refs_without_leader));
    build_dag_layer(connections, dag_state.clone());

    let committed = expect_commit(decider.try_direct_decide(slot), slot);
    assert_eq!(committed.reference(), leader_ref);
}

/// Equivocation in the leader slot is directly skipped only when every block in
/// the slot has quorum blame.
#[tokio::test]
async fn try_direct_decide_equivocating_skip() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, decider) = setup(6, 1, 0);

    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    let slot = Slot::new_for_test(1, 1);
    let leader = dag_state
        .read()
        .get_block(
            refs_round_1
                .iter()
                .find(|r| r.author == slot.authority)
                .expect("leader block should exist"),
        )
        .expect("leader block should be in dag");
    let equivocating_leader = VerifiedBlock::new_for_test(
        TestBlock::new(slot.round, slot.authority.value() as u32)
            .set_ancestors(leader.ancestors().to_vec())
            .set_transactions(vec![Transaction::new(vec![1])])
            .build(),
    );
    dag_state.write().accept_block(equivocating_leader);

    let refs_without_slot: Vec<_> = refs_round_1
        .into_iter()
        .filter(|r| r.author != slot.authority)
        .collect();
    let connections = context
        .committee
        .authorities()
        .map(|authority| (authority.0, refs_without_slot.clone()))
        .collect();
    build_dag_layer(connections, dag_state.clone());

    expect_skip(decider.try_direct_decide(slot), slot);
}

/// A leader simultaneously reaching the commit quorum and the skip quorum can
/// only happen if Byzantine authorities equivocate in the voting round. The
/// direct rule must detect this broken fault assumption and panic instead of
/// returning a decision.
#[tokio::test]
#[should_panic(expected = "cannot be both committed and skipped")]
async fn try_direct_decide_panics_when_committed_and_skipped() {
    telemetry_subscribers::init_for_testing();
    // v3 committee with quorum = 7 out of 9.
    let (context, dag_state, decider) = setup(9, 1, 1);
    assert_eq!(context.committee.quorum_threshold(), 7);

    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    let slot = Slot::new_for_test(1, 1);
    let refs_without_leader: Vec<_> = refs_round_1
        .iter()
        .cloned()
        .filter(|r| r.author != slot.authority)
        .collect();

    // All 9 authorities vote for the leader -> commit quorum (children_stake = 9).
    build_dag(
        context.clone(),
        dag_state.clone(),
        Some(refs_round_1.clone()),
        2,
    );

    // 7 Byzantine authorities (2..=8, skipping local author 0) also publish a
    // round-2 block that omits the leader -> skip quorum (reject_votes = 7), so
    // the leader is both committed and skipped.
    for author in 2..=8u32 {
        let non_vote = VerifiedBlock::new_for_test(
            TestBlock::new(2, author)
                .set_ancestors(refs_without_leader.clone())
                .set_transactions(vec![Transaction::new(vec![1])])
                .build(),
        );
        dag_state.write().accept_block(non_vote);
    }

    decider.try_direct_decide(slot);
}

/// An anchor whose causal history includes the round-1 leader certifies it
/// indirectly — we must commit.
#[tokio::test]
async fn try_indirect_decide_commit() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, decider) = setup(6, 1, 0);

    // Fully connected DAG up to round 4. Round 2 has all 4 round-1 blocks as
    // ancestors, so the round-1 leader is certified. Anchor is any round-4
    // block.
    let refs_round_4 = build_dag(context.clone(), dag_state.clone(), None, 4);
    let anchor = dag_state.read().get_block(&refs_round_4[0]).unwrap();

    let slot = Slot::new_for_test(1, 0);
    let statuses = decider.try_indirect_decide(&anchor, &[slot]);

    assert_eq!(statuses.len(), 1);
    expect_commit(statuses.into_iter().next().unwrap(), slot);
}

/// No round-2 block references the round-1 leader, so the anchor's causal
/// history does not certify it — Skip.
#[tokio::test]
async fn try_indirect_decide_skip() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, decider) = setup(6, 1, 0);

    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    let refs_without_leader: Vec<_> = refs_round_1
        .into_iter()
        .filter(|r| r.author != AuthorityIndex::new_for_test(0))
        .collect();
    let refs_round_2 = build_dag(
        context.clone(),
        dag_state.clone(),
        Some(refs_without_leader),
        2,
    );
    let refs_round_3 = build_dag(context.clone(), dag_state.clone(), Some(refs_round_2), 3);

    let anchor = dag_state.read().get_block(&refs_round_3[0]).unwrap();

    let slot = Slot::new_for_test(1, 0);
    let statuses = decider.try_indirect_decide(&anchor, &[slot]);

    assert_eq!(statuses.len(), 1);
    expect_skip(statuses.into_iter().next().unwrap(), slot);
}

/// One call covers multiple slots at the same decision round; each slot is
/// decided independently against the same BFS-collected vote map.
#[tokio::test]
async fn try_indirect_decide_multi_slot() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, decider) = setup(6, 1, 0);

    // Round 1 fully. Round 2 omits authority-0's round-1 block but includes
    // the other authorities. Round 3 fully on top.
    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    let refs_without_leader_0: Vec<_> = refs_round_1
        .iter()
        .cloned()
        .filter(|r| r.author != AuthorityIndex::new_for_test(0))
        .collect();
    let refs_round_2 = build_dag(
        context.clone(),
        dag_state.clone(),
        Some(refs_without_leader_0),
        2,
    );
    let refs_round_3 = build_dag(context.clone(), dag_state.clone(), Some(refs_round_2), 3);

    let anchor = dag_state.read().get_block(&refs_round_3[0]).unwrap();

    let slots = vec![Slot::new_for_test(1, 0), Slot::new_for_test(1, 1)];
    let statuses = decider.try_indirect_decide(&anchor, &slots);

    assert_eq!(statuses.len(), 2);
    let mut iter = statuses.into_iter();
    // (1, 0) — never referenced → Skip.
    expect_skip(iter.next().unwrap(), slots[0]);
    // (1, 1) — referenced by all round-2 blocks → Commit.
    expect_commit(iter.next().unwrap(), slots[1]);
}

/// A decision block can have both cert-threshold commit votes and at least
/// cert-threshold blame votes. Blame votes are not tracked by the indirect
/// rule, and a single commit certificate still commits the slot.
#[tokio::test]
async fn try_indirect_decide_commit_with_cert_threshold_blame_votes() {
    telemetry_subscribers::init_for_testing();
    // v3 committee with cert=4, quorum=7 — lets 4 commit votes and 5 blame
    // votes coexist against a single block.
    let (context, dag_state, decider) = setup(9, 1, 1);
    assert_eq!(context.committee.certification_threshold(), 4);
    assert_eq!(context.committee.quorum_threshold(), 7);

    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    let leader_author = AuthorityIndex::new_for_test(0);
    let refs_without_leader: Vec<_> = refs_round_1
        .iter()
        .cloned()
        .filter(|r| r.author != leader_author)
        .collect();

    // 4 round-2 blocks reference the leader (commit votes = cert);
    // 5 omit it (blame votes are also at least cert).
    let mut authorities = context.committee.authorities();
    let mut connections = vec![];
    for _ in 0..4 {
        connections.push((authorities.next().unwrap().0, refs_round_1.clone()));
    }
    for _ in 0..5 {
        connections.push((authorities.next().unwrap().0, refs_without_leader.clone()));
    }
    let refs_round_2 = build_dag_layer(connections, dag_state.clone());
    let refs_round_3 = build_dag(context.clone(), dag_state.clone(), Some(refs_round_2), 3);

    let anchor = dag_state.read().get_block(&refs_round_3[0]).unwrap();

    let slot = Slot::new_for_test(1, leader_author.value() as u32);
    let statuses = decider.try_indirect_decide(&anchor, &[slot]);

    assert_eq!(statuses.len(), 1);
    expect_commit(statuses.into_iter().next().unwrap(), slot);
}

/// When two equivocating blocks at the same slot both meet the certification
/// threshold, there cannot have been a direct commit on either; the indirect
/// rule must Skip the slot.
#[tokio::test]
async fn try_indirect_decide_skip_multiple_certified_blocks() {
    telemetry_subscribers::init_for_testing();
    // v3 committee with cert=4, quorum=7 — lets two equivocating blocks each
    // accumulate cert-threshold commit votes from disjoint voter sets.
    let (context, dag_state, decider) = setup(9, 1, 1);
    assert_eq!(context.committee.certification_threshold(), 4);
    assert_eq!(context.committee.quorum_threshold(), 7);

    let refs_round_1 = build_dag(context.clone(), dag_state.clone(), None, 1);
    // Use a non-own author for the equivocation slot — accept_block rejects
    // equivocations from the local authority (which is author 0).
    let slot = Slot::new_for_test(1, 1);
    let leader_a_ref = refs_round_1
        .iter()
        .find(|r| r.author == slot.authority)
        .copied()
        .expect("leader block should exist");
    let leader_a = dag_state
        .read()
        .get_block(&leader_a_ref)
        .expect("leader block should be in dag");
    let leader_b = VerifiedBlock::new_for_test(
        TestBlock::new(slot.round, slot.authority.value() as u32)
            .set_ancestors(leader_a.ancestors().to_vec())
            .set_transactions(vec![Transaction::new(vec![1])])
            .build(),
    );
    let leader_b_ref = leader_b.reference();
    dag_state.write().accept_block(leader_b);

    // 4 round-2 voters reference leader_a; 5 reference leader_b. Both meet
    // cert (4), so both are certified.
    let refs_with_b: Vec<_> = refs_round_1
        .iter()
        .cloned()
        .filter(|r| *r != leader_a_ref)
        .chain(std::iter::once(leader_b_ref))
        .collect();
    let mut authorities = context.committee.authorities();
    let mut connections = vec![];
    for _ in 0..4 {
        connections.push((authorities.next().unwrap().0, refs_round_1.clone()));
    }
    for _ in 0..5 {
        connections.push((authorities.next().unwrap().0, refs_with_b.clone()));
    }
    let refs_round_2 = build_dag_layer(connections, dag_state.clone());
    let refs_round_3 = build_dag(context.clone(), dag_state.clone(), Some(refs_round_2), 3);

    let anchor = dag_state.read().get_block(&refs_round_3[0]).unwrap();

    let statuses = decider.try_indirect_decide(&anchor, &[slot]);

    assert_eq!(statuses.len(), 1);
    expect_skip(statuses.into_iter().next().unwrap(), slot);
}

/// `try_indirect_decide` requires at least one decision slot.
#[tokio::test]
#[should_panic(expected = "assertion failed: !decision_slots.is_empty()")]
async fn try_indirect_decide_panics_on_empty_decision_slots() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, decider) = setup(4, 1, 0);

    let refs_round_2 = build_dag(context.clone(), dag_state.clone(), None, 2);
    let anchor = dag_state.read().get_block(&refs_round_2[0]).unwrap();

    decider.try_indirect_decide(&anchor, &[]);
}

/// All decision slots passed to `try_indirect_decide` must share the same round.
#[tokio::test]
#[should_panic(expected = "decision_slots.iter().all")]
async fn try_indirect_decide_panics_on_mismatched_decision_rounds() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, decider) = setup(4, 1, 0);

    let refs_round_2 = build_dag(context.clone(), dag_state.clone(), None, 2);
    let anchor = dag_state.read().get_block(&refs_round_2[0]).unwrap();

    decider.try_indirect_decide(
        &anchor,
        &[Slot::new_for_test(1, 0), Slot::new_for_test(2, 0)],
    );
}

/// The anchor block must be at least `INDIRECT_COMMIT_DEPTH` rounds above the
/// decision round.
#[tokio::test]
#[should_panic(expected = "is too close to decision round")]
async fn try_indirect_decide_panics_when_anchor_too_close() {
    telemetry_subscribers::init_for_testing();
    let (context, dag_state, decider) = setup(4, 1, 0);

    // Anchor at round 2; decision round 1 requires anchor.round() >= 1 + 2 = 3.
    let refs_round_2 = build_dag(context.clone(), dag_state.clone(), None, 2);
    let anchor = dag_state.read().get_block(&refs_round_2[0]).unwrap();

    decider.try_indirect_decide(&anchor, &[Slot::new_for_test(1, 0)]);
}
