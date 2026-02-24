// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{borrow::BorrowMut, sync::Arc};

use parking_lot::RwLock;

use crate::{
    block::{BlockAPI, Slot},
    commit::LeaderStatus,
    context::Context,
    dag_state::DagState,
    storage::mem_store::MemStore,
    test_dag::{build_dag, build_dag_layer},
    universal_committer::universal_committer_builder::UniversalCommitterBuilder,
};

/// Commit one leader.
#[test]
fn direct_commit() {
    let (context, dag_state, committer) = basic_test_setup();

    // Build fully connected fully connected dag with empty blocks
    // adding up to voting round of wave 2 to the dag so that we have
    // 2 completed waves and one incomplete wave.
    // note: waves & rounds are zero-indexed.
    let voting_round_wave_2 = committer.committers[0].leader_round(2) + 1;
    build_dag(context, dag_state, None, voting_round_wave_2);

    // Genesis cert will not be included in commit sequence, marking it as last decided
    let last_committed = Slot::new_for_test(0, 0);

    // The universal committer should mark the potential leaders in leader round 6 as
    // undecided because there is no way to get enough certificates for leaders of
    // leader round 6 without completing wave 2.
    // Ensure 3 leaders have been committed in round 3.
    let sequence = committer.try_commit(last_committed);
    tracing::info!("Commit sequence: {sequence:#?}");
    assert_eq!(sequence.len(), 3);

    let leader_round_wave_1 = committer.committers[0].leader_round(1);
    let leaders_wave_1 = committer.get_leaders(leader_round_wave_1);
    for (idx, leader_status) in sequence.iter().enumerate() {
        if let LeaderStatus::Commit(ref block) = leader_status {
            assert_eq!(block.author(), leaders_wave_1[idx]);
        } else {
            panic!("Expected a committed leader")
        };
    }
}

/// Indirect-commit the first leader.
#[test]
fn indirect_commit() {
    let (context, dag_state, committer) = basic_test_setup();

    // Add enough blocks to reach the leader round of wave 1.
    // note: waves & rounds are zero-indexed.
    let leader_round_wave_1 = committer.committers[0].leader_round(1);
    let references_leader_wave_1 = build_dag(
        context.clone(),
        dag_state.clone(),
        None,
        leader_round_wave_1,
    );

    // Filter out one of the leaders of wave 1.
    let mut expected_sequenced_leaders = committer.get_leaders(leader_round_wave_1);
    let leader_1_wave_1 = expected_sequenced_leaders[0];
    let references_without_leader_wave_1: Vec<_> = references_leader_wave_1
        .iter()
        .cloned()
        .filter(|x| x.author != leader_1_wave_1)
        .collect();

    // Only 2f+1 validators vote for one of the leaders of wave 1.
    let connections_with_leader_wave_1 = context
        .committee
        .authorities()
        .take(context.committee.quorum_threshold() as usize)
        .map(|authority| (authority.0, references_leader_wave_1.clone()))
        .collect();
    let references_with_votes_for_leader_wave_1 =
        build_dag_layer(connections_with_leader_wave_1, dag_state.clone());

    // The validators not part of the 2f+1 above do not vote for one of the leaders
    // of wave 1
    let connections_without_leader_wave_1 = context
        .committee
        .authorities()
        .skip(context.committee.quorum_threshold() as usize)
        .map(|authority| (authority.0, references_without_leader_wave_1.clone()))
        .collect();
    let references_without_votes_for_leader_wave_1 =
        build_dag_layer(connections_without_leader_wave_1, dag_state.clone());

    // Only f+1 validators certify one of the leaders of wave 1.
    let mut references_decision_round_wave_1 = Vec::new();

    let connections_with_certs_for_leader_wave_1 = context
        .committee
        .authorities()
        .take(context.committee.validity_threshold() as usize)
        .map(|authority| (authority.0, references_with_votes_for_leader_wave_1.clone()))
        .collect();
    references_decision_round_wave_1.extend(build_dag_layer(
        connections_with_certs_for_leader_wave_1,
        dag_state.clone(),
    ));

    let references_voting_round_wave_1: Vec<_> = references_without_votes_for_leader_wave_1
        .into_iter()
        .chain(references_with_votes_for_leader_wave_1)
        .take(context.committee.quorum_threshold() as usize)
        .collect();

    // The validators not part of the f+1 above will not certify one of the leaders of wave 1.
    let connections_without_votes_for_leader_1 = context
        .committee
        .authorities()
        .skip(context.committee.validity_threshold() as usize)
        .map(|authority| (authority.0, references_voting_round_wave_1.clone()))
        .collect();
    references_decision_round_wave_1.extend(build_dag_layer(
        connections_without_votes_for_leader_1,
        dag_state.clone(),
    ));

    // Add enough blocks to decide the leaders of wave 2 connecting to the references
    // manually constructed of the decision round of wave 1.
    let leader_round_wave_2 = committer.committers[0].leader_round(2);
    let decision_round_wave_2 = committer.committers[0].decision_round(2);
    build_dag(
        context.clone(),
        dag_state.clone(),
        Some(references_decision_round_wave_1),
        decision_round_wave_2,
    );

    // Add leaders of wave 2 to the expected sequence list.
    expected_sequenced_leaders.append(committer.get_leaders(leader_round_wave_2).borrow_mut());

    // Ensure 6 leaders have been sequenced across waves 1 & 2. This includes one
    // leader that was indirectly committed.
    let last_decided = Slot::new_for_test(0, 0);
    let sequence = committer.try_commit(last_decided);
    tracing::info!("Commit sequence: {sequence:#?}");
    assert_eq!(sequence.len(), 6);

    for (idx, leader_status) in sequence.iter().enumerate() {
        if let LeaderStatus::Commit(ref block) = leader_status {
            assert_eq!(block.author(), expected_sequenced_leaders[idx]);
        } else {
            panic!("Expected a committed leader")
        };
    }
}

fn basic_test_setup() -> (
    Arc<Context>,
    Arc<RwLock<DagState>>,
    super::UniversalCommitter,
) {
    telemetry_subscribers::init_for_testing();
    // Commitee of 4 with even stake
    let context = Arc::new(Context::new_for_test(4).0);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));

    // Create committer without pipelining and and 3 leaders per round
    let committer = UniversalCommitterBuilder::new(context.clone(), dag_state.clone())
        .with_number_of_leaders(3)
        .build();

    // note: without pipelining and with multi-leader (3) enabled there should be 3 committers
    assert!(committer.committers.len() == 3);

    (context, dag_state, committer)
}
