// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

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
    telemetry_subscribers::init_for_testing();
    // Commitee of 4 with even stake
    let context = Arc::new(Context::new_for_test(4).0);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));

    // Create committer without pipelining and only 1 leader per round
    let committer = UniversalCommitterBuilder::new(context.clone(), dag_state.clone()).build();

    // Build fully connected dag with empty blocks adding up to voting round of
    // wave 2 to the dag so that we have 2 completed waves and one incomplete wave.
    // note: without pipelining or multi-leader enabled there should only be one committer.
    assert!(committer.committers.len() == 1);
    let leader_round_wave_1 = committer.committers[0].leader_round(1);
    let voting_round_wave_2 = committer.committers[0].leader_round(2) + 1;
    build_dag(context, dag_state, None, voting_round_wave_2);

    // Genesis cert will not be included in commit sequence, marking it as last decided
    let last_decided = Slot::new_for_test(0, 0);

    // The universal committer should mark the potential leaders in r6 as undecided
    // because there is no way to get enough certificates for r6 leaders without
    // completing wave 2.
    let sequence = committer.try_commit(last_decided);
    tracing::info!("Commit sequence: {sequence:?}");

    assert_eq!(sequence.len(), 1);
    if let LeaderStatus::Commit(ref block) = sequence[0] {
        assert_eq!(
            block.author(),
            committer.get_leaders(leader_round_wave_1)[0]
        )
    } else {
        panic!("Expected a committed leader")
    };
}

/// Indirect-commit the first leader.
#[test]
fn indirect_commit() {
    telemetry_subscribers::init_for_testing();
    // Commitee of 4 with even stake
    let context = Arc::new(Context::new_for_test(4).0);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));

    // Create committer without pipelining and only 1 leader per round
    let committer = UniversalCommitterBuilder::new(context.clone(), dag_state.clone()).build();

    // Add enough blocks to reach the leader round of wave 1.
    let leader_round_wave_1 = committer.committers[0].leader_round(1);
    let references_leader_wave_1 = build_dag(
        context.clone(),
        dag_state.clone(),
        None,
        leader_round_wave_1,
    );

    // Filter out that leader.
    let leader_wave_1 = committer.get_leaders(leader_round_wave_1)[0];
    let references_without_leader_wave_1: Vec<_> = references_leader_wave_1
        .iter()
        .cloned()
        .filter(|x| x.author != leader_wave_1)
        .collect();

    // Only 2f+1 validators vote for the leader of wave 1.
    let connections_with_leader_wave_1 = context
        .committee
        .authorities()
        .take(context.committee.quorum_threshold() as usize)
        .map(|authority| (authority.0, references_leader_wave_1.clone()))
        .collect();
    let references_with_votes_for_leader_wave_1 =
        build_dag_layer(connections_with_leader_wave_1, dag_state.clone());

    // The validators not part of the 2f+1 above do not vote for the leader
    // of wave 1
    let connections_without_leader_wave_1 = context
        .committee
        .authorities()
        .skip(context.committee.quorum_threshold() as usize)
        .map(|authority| (authority.0, references_without_leader_wave_1.clone()))
        .collect();
    let references_without_votes_for_leader_wave_1 =
        build_dag_layer(connections_without_leader_wave_1, dag_state.clone());

    // Only f+1 validators certify the leader of wave 1.
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

    // The validators not part of the f+1 above will not certify the leader of wave 1.
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

    // Add enough blocks to decide the leader of wave 2 connecting to the references
    // manually constructed of the decision round of wave 1.
    let leader_round_wave_3 = committer.committers[0].leader_round(3);
    build_dag(
        context.clone(),
        dag_state.clone(),
        Some(references_decision_round_wave_1),
        leader_round_wave_3,
    );

    let last_decided = Slot::new_for_test(0, 0);
    let sequence = committer.try_commit(last_decided);
    tracing::info!("Commit sequence: {sequence:?}");
    assert_eq!(sequence.len(), 2);

    for (idx, decided_leader) in sequence.iter().enumerate() {
        let leader_round = committer.committers[0].leader_round(idx as u32 + 1);
        let expected_leader = committer.get_leaders(leader_round)[0];
        if let LeaderStatus::Commit(ref block) = decided_leader {
            assert_eq!(block.author(), expected_leader);
        } else {
            panic!("Expected a committed leader")
        };
    }
}
