// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use consensus_config::AuthorityIndex;
use parking_lot::RwLock;

use crate::{
    block::{BlockAPI, Slot, TestBlock, Transaction, VerifiedBlock},
    commit::DecidedLeader,
    context::Context,
    dag_state::DagState,
    leader_schedule::{LeaderSchedule, LeaderSwapTable},
    storage::mem_store::MemStore,
    test_dag::{build_dag, build_dag_layer},
    test_dag_builder::DagBuilder,
    test_dag_parser::parse_dag,
    universal_committer::universal_committer_builder::UniversalCommitterBuilder,
};

/// Commit one leader.
#[tokio::test]
async fn direct_commit() {
    let mut test_setup = basic_dag_builder_test_setup();

    // Build fully connected dag with empty blocks adding up to voting round of
    // wave 2 to the dag so that we have 2 completed waves and one incomplete wave.
    // note: waves & rounds are zero-indexed.
    let leader_round_wave_1 = test_setup.committer.committers[0].leader_round(1);
    let voting_round_wave_2 = test_setup.committer.committers[0].leader_round(2) + 1;
    test_setup
        .dag_builder
        .layers(1..=voting_round_wave_2)
        .build()
        .persist_layers(test_setup.dag_state);

    test_setup.dag_builder.print();

    // Genesis cert will not be included in commit sequence, marking it as last decided
    let last_decided = Slot::new_for_test(0, 0);

    // The universal committer should mark the potential leaders in leader round 6 as
    // undecided because there is no way to get enough certificates for leaders of
    // leader round 6 without completing wave 2.
    let sequence = test_setup.committer.try_decide(last_decided);
    tracing::info!("Commit sequence: {sequence:#?}");

    assert_eq!(sequence.len(), 1);
    if let DecidedLeader::Commit(ref block) = sequence[0] {
        assert_eq!(
            block.author(),
            test_setup.committer.get_leaders(leader_round_wave_1)[0]
        )
    } else {
        panic!("Expected a committed leader")
    };
}

/// Ensure idempotent replies.
#[tokio::test]
async fn idempotence() {
    let (context, dag_state, committer) = basic_test_setup();

    // note: waves & rounds are zero-indexed.
    let leader_round_wave_1 = committer.committers[0].leader_round(1);
    let decision_round_wave_1 = committer.committers[0].decision_round(1);
    let references_decision_round_wave_1 = build_dag(
        context.clone(),
        dag_state.clone(),
        None,
        decision_round_wave_1,
    );

    // Commit one leader.
    let last_decided = Slot::new_for_test(0, 0);
    let first_sequence = committer.try_decide(last_decided);
    assert_eq!(first_sequence.len(), 1);

    if let DecidedLeader::Commit(ref block) = first_sequence[0] {
        assert_eq!(first_sequence[0].round(), leader_round_wave_1);
        assert_eq!(
            block.author(),
            committer.get_leaders(leader_round_wave_1)[0]
        )
    } else {
        panic!("Expected a committed leader")
    };

    // Ensure that if try_commit is called again with the same last decided leader
    // input the commit decision will be the same.
    let first_sequence = committer.try_decide(last_decided);

    assert_eq!(first_sequence.len(), 1);
    if let DecidedLeader::Commit(ref block) = first_sequence[0] {
        assert_eq!(first_sequence[0].round(), leader_round_wave_1);
        assert_eq!(
            block.author(),
            committer.get_leaders(leader_round_wave_1)[0]
        )
    } else {
        panic!("Expected a committed leader")
    };

    // Add more rounds so we have something to commit after the leader of wave 1
    let decision_round_wave_2 = committer.committers[0].decision_round(2);
    build_dag(
        context.clone(),
        dag_state.clone(),
        Some(references_decision_round_wave_1),
        decision_round_wave_2,
    );

    // Ensure we don't commit the leader of wave 1 again if we mark it as the
    // last decided.
    let leader_status_wave_1 = first_sequence.last().unwrap();
    let last_decided = Slot::new(
        leader_status_wave_1.round(),
        leader_status_wave_1.authority(),
    );
    let leader_round_wave_2 = committer.committers[0].leader_round(2);
    let second_sequence = committer.try_decide(last_decided);
    tracing::info!("Commit sequence: {second_sequence:#?}");

    assert_eq!(second_sequence.len(), 1);
    if let DecidedLeader::Commit(ref block) = second_sequence[0] {
        assert_eq!(second_sequence[0].round(), leader_round_wave_2);
        assert_eq!(
            block.author(),
            committer.get_leaders(leader_round_wave_2)[0]
        );
    } else {
        panic!("Expected a committed leader")
    };
}

/// Commit one by one each leader as the dag progresses in ideal conditions.
#[tokio::test]
async fn multiple_direct_commit() {
    let (context, dag_state, committer) = basic_test_setup();

    let mut ancestors = None;
    let mut last_decided = Slot::new_for_test(0, 0);
    for n in 1..=10 {
        // Build the dag up to the decision round for each wave starting with wave 1.
        // note: waves & rounds are zero-indexed.
        let decision_round = committer.committers[0].decision_round(n);
        ancestors = Some(build_dag(
            context.clone(),
            dag_state.clone(),
            ancestors,
            decision_round,
        ));

        // After each wave is complete try commit the leader of that wave.
        let leader_round = committer.committers[0].leader_round(n);
        let sequence = committer.try_decide(last_decided);
        tracing::info!("Commit sequence: {sequence:#?}");

        assert_eq!(sequence.len(), 1);
        if let DecidedLeader::Commit(ref block) = sequence[0] {
            assert_eq!(block.round(), leader_round);
            assert_eq!(block.author(), committer.get_leaders(leader_round)[0]);
        } else {
            panic!("Expected a committed leader")
        }

        // Update the last decided leader so only one new leader is committed as
        // each new wave is completed.
        let leader_status = sequence.last().unwrap();
        last_decided = Slot::new(leader_status.round(), leader_status.authority());
    }
}

/// Commit 10 leaders in a row (calling the committer after adding them).
#[tokio::test]
async fn direct_commit_late_call() {
    let (context, dag_state, committer) = basic_test_setup();

    // note: waves & rounds are zero-indexed.
    let num_waves = 11;
    let decision_round_wave_10 = committer.committers[0].decision_round(10);
    build_dag(
        context.clone(),
        dag_state.clone(),
        None,
        decision_round_wave_10,
    );

    let last_decided = Slot::new_for_test(0, 0);
    let sequence = committer.try_decide(last_decided);
    tracing::info!("Commit sequence: {sequence:#?}");

    // With 11 waves completed, excluding wave 0 with genesis round as its leader
    // round, ensure we have 10 leaders committed.
    assert_eq!(sequence.len(), num_waves - 1_usize);
    for (i, leader_block) in sequence.iter().enumerate() {
        let leader_round = committer.committers[0].leader_round(i as u32 + 1);
        if let DecidedLeader::Commit(ref block) = leader_block {
            assert_eq!(block.round(), leader_round);
            assert_eq!(block.author(), committer.get_leaders(leader_round)[0]);
        } else {
            panic!("Expected a committed leader")
        };
    }
}

/// Do not commit anything if we are still in the first wave.
#[tokio::test]
async fn no_genesis_commit() {
    let (context, dag_state, committer) = basic_test_setup();

    // note: waves & rounds are zero-indexed.
    let decision_round_wave_1 = committer.committers[0].decision_round(1);
    let mut ancestors = None;
    for r in 0..decision_round_wave_1 {
        ancestors = Some(build_dag(context.clone(), dag_state.clone(), ancestors, r));

        let last_committed = Slot::new_for_test(0, 0);
        let sequence = committer.try_decide(last_committed);
        tracing::info!("Commit sequence: {sequence:#?}");
        assert!(sequence.is_empty());
    }
}

/// We directly skip the leader if there are enough non-votes (blames).
#[tokio::test]
async fn direct_skip_no_leader_votes() {
    let mut test_setup = basic_dag_builder_test_setup();

    // Add enough blocks to reach the leader round of wave 1.
    // note: waves & rounds are zero-indexed.
    let leader_round_wave_1 = test_setup.committer.committers[0].leader_round(1);
    test_setup
        .dag_builder
        .layers(1..=leader_round_wave_1)
        .build()
        .persist_layers(test_setup.dag_state.clone());

    // Add enough blocks to reach the decision round of the first leader but without
    // votes for the leader of wave 1.
    let leader_wave_1 = test_setup.committer.get_leaders(leader_round_wave_1)[0];
    let voting_round_wave_1 = leader_round_wave_1 + 1;
    test_setup
        .dag_builder
        .layer(voting_round_wave_1)
        .no_leader_link(leader_round_wave_1, vec![])
        .persist_layers(test_setup.dag_state.clone());

    let decision_round_wave_1 = test_setup.committer.committers[0].decision_round(1);
    test_setup
        .dag_builder
        .layer(decision_round_wave_1)
        .build()
        .persist_layers(test_setup.dag_state);

    test_setup.dag_builder.print();

    // Ensure no blocks are committed because there are 2f+1 blame (non-votes) for
    // the leader of wave 1.
    let last_decided = Slot::new_for_test(0, 0);
    let sequence = test_setup.committer.try_decide(last_decided);
    tracing::info!("Commit sequence: {sequence:#?}");

    assert_eq!(sequence.len(), 1);
    if let DecidedLeader::Skip(leader) = sequence[0] {
        assert_eq!(leader.authority, leader_wave_1);
        assert_eq!(leader.round, leader_round_wave_1);
    } else {
        panic!("Expected to directly skip the leader");
    }
}

/// We directly skip the leader if it is missing.
#[tokio::test]
async fn direct_skip_missing_leader_block() {
    let mut test_setup = basic_dag_builder_test_setup();

    // Add enough blocks to reach the decision round of wave 0
    // note: waves & rounds are zero-indexed.
    let decision_round_wave_0 = test_setup.committer.committers[0].decision_round(0);
    test_setup
        .dag_builder
        .layers(1..=decision_round_wave_0)
        .build();

    // Create a leader round in the dag without the leader block.
    let leader_round_wave_1 = test_setup.committer.committers[0].leader_round(1);
    test_setup
        .dag_builder
        .layer(leader_round_wave_1)
        .no_leader_block(vec![])
        .build();

    // Add enough blocks to reach the decision round of wave 1.
    let voting_round_wave_1 = leader_round_wave_1 + 1;
    let decision_round_wave_1 = test_setup.committer.committers[0].decision_round(1);
    test_setup
        .dag_builder
        .layers(voting_round_wave_1..=decision_round_wave_1)
        .build();

    test_setup.dag_builder.print();
    test_setup
        .dag_builder
        .persist_all_blocks(test_setup.dag_state.clone());

    // Ensure the leader is skipped because the leader is missing.
    let last_committed = Slot::new_for_test(0, 0);
    let sequence = test_setup.committer.try_decide(last_committed);
    tracing::info!("Commit sequence: {sequence:#?}");

    assert_eq!(sequence.len(), 1);
    if let DecidedLeader::Skip(leader) = sequence[0] {
        assert_eq!(
            leader.authority,
            test_setup.committer.get_leaders(leader_round_wave_1)[0]
        );
        assert_eq!(leader.round, leader_round_wave_1);
    } else {
        panic!("Expected to directly skip the leader");
    }
}

/// Indirect-commit the first leader.
#[tokio::test]
async fn indirect_commit() {
    telemetry_subscribers::init_for_testing();
    // Dag Notes:
    // - Fully connected up to the leader round of wave 1.
    // - Only 2f+1 validators vote for the leader of wave 1.
    // - Only f+1 validators certify the leader of wave 1.
    // - The validators not part of the f+1 above will not certify the leader
    // of wave 1.
    // - Fully connected blocks to decide the leader of wave 2.
    let dag_str = "DAG { 
        Round 0 : { 4 },
        Round 1 : { * },
        Round 2 : { * },
        Round 3 : { * },
        Round 4 : {
            A -> [-D3],
            B -> [*],
            C -> [*],
            D -> [*],
        },
        Round 5 : {
            A -> [*],
            B -> [*],
            C -> [A4],
            D -> [A4],
        },
        Round 6 : { * },
        Round 7 : { * },
        Round 8 : { * },
     }";

    let (_, dag_builder) = parse_dag(dag_str).expect("Invalid dag");
    let dag_state = Arc::new(RwLock::new(DagState::new(
        dag_builder.context.clone(),
        Arc::new(MemStore::new()),
    )));
    let leader_schedule = Arc::new(LeaderSchedule::new(
        dag_builder.context.clone(),
        LeaderSwapTable::default(),
    ));

    dag_builder.print();
    dag_builder.persist_all_blocks(dag_state.clone());

    // Create committer without pipelining and only 1 leader per leader round
    let committer = UniversalCommitterBuilder::new(
        dag_builder.context.clone(),
        leader_schedule,
        dag_state.clone(),
    )
    .build();
    // note: without pipelining or multi-leader enabled there should only be one committer.
    assert!(committer.committers.len() == 1);

    // Ensure we indirectly commit the leader of wave 1 via the directly committed
    // leader of wave 2.
    let last_decided = Slot::new_for_test(0, 0);
    let sequence = committer.try_decide(last_decided);
    tracing::info!("Commit sequence: {sequence:#?}");
    assert_eq!(sequence.len(), 2);

    for (idx, decided_leader) in sequence.iter().enumerate() {
        let leader_round = committer.committers[0].leader_round(idx as u32 + 1);
        let expected_leader = committer.get_leaders(leader_round)[0];
        if let DecidedLeader::Commit(ref block) = decided_leader {
            assert_eq!(block.round(), leader_round);
            assert_eq!(block.author(), expected_leader);
        } else {
            panic!("Expected a committed leader")
        };
    }
}

/// Commit the first leader, skip the 2nd, and commit the 3rd leader.
#[tokio::test]
async fn indirect_skip() {
    let (context, dag_state, committer) = basic_test_setup();

    // Add enough blocks to reach the leader of wave 2
    // note: waves & rounds are zero-indexed.
    let leader_round_wave_2 = committer.committers[0].leader_round(2);
    let references_leader_round_wave_2 = build_dag(
        context.clone(),
        dag_state.clone(),
        None,
        leader_round_wave_2,
    );

    // Filter out the leader of wave 2.
    let leader_wave_2 = committer.get_leaders(leader_round_wave_2)[0];
    let references_without_leader_wave_2: Vec<_> = references_leader_round_wave_2
        .iter()
        .cloned()
        .filter(|x| x.author != leader_wave_2)
        .collect();

    // Only f+1 validators connect to the leader of wave 2. This is setting up the
    // scenario where we have <2f+1 blame & <2f+1 certificates for the leader of wave 2
    // which will mean we mark it as Undecided. Note there are not enough votes
    // to form a certified link to the leader of wave 2 as well.
    let mut references = Vec::new();

    let connections_with_leader_wave_2 = context
        .committee
        .authorities()
        .take(context.committee.validity_threshold() as usize)
        .map(|authority| (authority.0, references_leader_round_wave_2.clone()))
        .collect::<Vec<_>>();

    references.extend(build_dag_layer(
        connections_with_leader_wave_2,
        dag_state.clone(),
    ));

    let connections_without_leader_wave_2 = context
        .committee
        .authorities()
        .skip(context.committee.validity_threshold() as usize)
        .map(|authority| (authority.0, references_without_leader_wave_2.clone()))
        .collect();

    references.extend(build_dag_layer(
        connections_without_leader_wave_2,
        dag_state.clone(),
    ));

    // Add enough blocks to reach the decision round of the leader of wave 3.
    let decision_round_wave_3 = committer.committers[0].decision_round(3);
    build_dag(
        context.clone(),
        dag_state.clone(),
        Some(references),
        decision_round_wave_3,
    );

    // Ensure we make a commit decision for the leaders of wave 1 ~ 3
    let last_committed = Slot::new_for_test(0, 0);
    let sequence = committer.try_decide(last_committed);
    tracing::info!("Commit sequence: {sequence:#?}");
    assert_eq!(sequence.len(), 3);

    // Ensure we commit the leader of wave 1 directly.
    let leader_round_wave_1 = committer.committers[0].leader_round(1);
    let leader_wave_1 = committer.get_leaders(leader_round_wave_1)[0];
    if let DecidedLeader::Commit(ref block) = sequence[0] {
        assert_eq!(block.round(), leader_round_wave_1);
        assert_eq!(block.author(), leader_wave_1);
    } else {
        panic!("Expected a committed leader")
    };

    // Ensure we skip the leader of wave 2 after it had been marked undecided directly.
    // This happens because we do not have enough votes in voting round of wave 2
    // for the certificates of decision round wave 2 to form a certified link to
    // the leader of wave 2.
    if let DecidedLeader::Skip(leader) = sequence[1] {
        assert_eq!(leader.authority, leader_wave_2);
        assert_eq!(leader.round, leader_round_wave_2);
    } else {
        panic!("Expected a skipped leader")
    }

    // Ensure we commit the 3rd leader directly.
    let leader_round_wave_3 = committer.committers[0].leader_round(3);
    let leader_wave_3 = committer.get_leaders(leader_round_wave_3)[0];
    if let DecidedLeader::Commit(ref block) = sequence[2] {
        assert_eq!(block.round(), leader_round_wave_3);
        assert_eq!(block.author(), leader_wave_3);
    } else {
        panic!("Expected a committed leader")
    }
}

/// If there is no leader with enough support nor blame, we commit nothing.
#[tokio::test]
async fn undecided() {
    let (context, dag_state, committer) = basic_test_setup();

    // Add enough blocks to reach the leader of wave 1.
    // note: waves & rounds are zero-indexed.
    let leader_round_wave_1 = committer.committers[0].leader_round(1);
    let references_leader_round_wave_1 = build_dag(
        context.clone(),
        dag_state.clone(),
        None,
        leader_round_wave_1,
    );

    // Filter out the leader of wave 1.
    let references_without_leader_1: Vec<_> = references_leader_round_wave_1
        .iter()
        .cloned()
        .filter(|x| x.author != committer.get_leaders(leader_round_wave_1)[0])
        .collect();

    // Create a dag layer where only one authority votes for the leader of wave 1.
    let mut authorities = context.committee.authorities();
    let leader_wave_1_connection = vec![(
        authorities.next().unwrap().0,
        references_leader_round_wave_1,
    )];
    let non_leader_wave_1_connections: Vec<_> = authorities
        .take((context.committee.quorum_threshold() - 1) as usize)
        .map(|authority| (authority.0, references_without_leader_1.clone()))
        .collect();

    let connections_voting_round_wave_1 = leader_wave_1_connection
        .into_iter()
        .chain(non_leader_wave_1_connections)
        .collect::<Vec<_>>();
    let references_voting_round_wave_1 =
        build_dag_layer(connections_voting_round_wave_1, dag_state.clone());

    // Add enough blocks to reach the decision round of the leader of wave 1.
    let decision_round_wave_1 = committer.committers[0].decision_round(1);
    build_dag(
        context.clone(),
        dag_state.clone(),
        Some(references_voting_round_wave_1),
        decision_round_wave_1,
    );

    // Ensure outcome of direct & indirect rule is undecided. So not commit decisions
    // should be returned.
    let last_committed = Slot::new_for_test(0, 0);
    let sequence = committer.try_decide(last_committed);
    tracing::info!("Commit sequence: {sequence:#?}");
    assert!(sequence.is_empty());
}

// This test scenario has one authority that is acting in a byzantine manner. It
// will be sending multiple different blocks to different validators for a round.
// The commit rule should handle this and correctly commit the expected blocks.
#[tokio::test]
async fn test_byzantine_direct_commit() {
    let (context, dag_state, committer) = basic_test_setup();

    // Add enough blocks to reach leader round of wave 4
    // note: waves & rounds are zero-indexed.
    let leader_round_wave_4 = committer.committers[0].leader_round(4);
    let references_leader_round_wave_4 = build_dag(
        context.clone(),
        dag_state.clone(),
        None,
        leader_round_wave_4,
    );

    // Add blocks to reach voting round of wave 4
    let voting_round_wave_4 = committer.committers[0].leader_round(4) + 1;
    // This includes a "good vote" from validator C which is acting as a byzantine validator
    let good_references_voting_round_wave_4 = build_dag(
        context.clone(),
        dag_state.clone(),
        Some(references_leader_round_wave_4.clone()),
        voting_round_wave_4,
    );

    // DagState Update:
    // - 'A' got a good vote from 'C' above
    // - 'A' will then get a bad vote from 'C' indirectly through the ancenstors of
    //   the wave 4 decision blocks of B C D

    // Add block layer for wave 4 decision round with no votes for leader A12
    // from a byzantine validator C that sent different blocks to all validators.

    // Filter out leader from wave 4 { A12 }.
    let leader_wave_4 = committer.get_leaders(leader_round_wave_4)[0];

    // References to blocks from leader round wave 4 { B12 C12 D12 }
    let references_without_leader_round_wave_4: Vec<_> = references_leader_round_wave_4
        .into_iter()
        .filter(|x| x.author != leader_wave_4)
        .collect();

    // Accept these references/blocks as ancestors from decision round blocks in dag state
    let byzantine_block_c13_1 = VerifiedBlock::new_for_test(
        TestBlock::new(13, 2)
            .set_ancestors(references_without_leader_round_wave_4.clone())
            .set_transactions(vec![Transaction::new(vec![1])])
            .build(),
    );
    dag_state
        .write()
        .accept_block(byzantine_block_c13_1.clone());

    let byzantine_block_c13_2 = VerifiedBlock::new_for_test(
        TestBlock::new(13, 2)
            .set_ancestors(references_without_leader_round_wave_4.clone())
            .set_transactions(vec![Transaction::new(vec![2])])
            .build(),
    );
    dag_state
        .write()
        .accept_block(byzantine_block_c13_2.clone());

    let byzantine_block_c13_3 = VerifiedBlock::new_for_test(
        TestBlock::new(13, 2)
            .set_ancestors(references_without_leader_round_wave_4)
            .set_transactions(vec![Transaction::new(vec![3])])
            .build(),
    );
    dag_state
        .write()
        .accept_block(byzantine_block_c13_3.clone());

    // Ancestors of decision blocks in round 14 should include multiple byzantine non-votes C13
    // but there are enough good votes to prevent a skip. Additionally only one of the non-votes
    // per authority should be counted so we should not skip leader A12.
    let decison_block_a14 = VerifiedBlock::new_for_test(
        TestBlock::new(14, 0)
            .set_ancestors(good_references_voting_round_wave_4.clone())
            .build(),
    );
    dag_state.write().accept_block(decison_block_a14.clone());

    let good_references_voting_round_wave_4_without_c13 = good_references_voting_round_wave_4
        .into_iter()
        .filter(|r| r.author != AuthorityIndex::new_for_test(2))
        .collect::<Vec<_>>();

    let decison_block_b14 = VerifiedBlock::new_for_test(
        TestBlock::new(14, 1)
            .set_ancestors(
                good_references_voting_round_wave_4_without_c13
                    .iter()
                    .cloned()
                    .chain(std::iter::once(byzantine_block_c13_1.reference()))
                    .collect(),
            )
            .build(),
    );
    dag_state.write().accept_block(decison_block_b14.clone());

    let decison_block_c14 = VerifiedBlock::new_for_test(
        TestBlock::new(14, 2)
            .set_ancestors(
                good_references_voting_round_wave_4_without_c13
                    .iter()
                    .cloned()
                    .chain(std::iter::once(byzantine_block_c13_2.reference()))
                    .collect(),
            )
            .build(),
    );
    dag_state.write().accept_block(decison_block_c14.clone());

    let decison_block_d14 = VerifiedBlock::new_for_test(
        TestBlock::new(14, 3)
            .set_ancestors(
                good_references_voting_round_wave_4_without_c13
                    .iter()
                    .cloned()
                    .chain(std::iter::once(byzantine_block_c13_3.reference()))
                    .collect(),
            )
            .build(),
    );
    dag_state.write().accept_block(decison_block_d14.clone());

    // DagState Update:
    // - We have A13, B13, D13 & C13 as good votes in the voting round of wave 4
    // - We have 3 byzantine C13 nonvotes that we received as ancestors from decision
    //   round blocks from B, C, & D.
    // - We have B14, C14 & D14 that include this byzantine nonvote from C13 but
    // all of these blocks also have good votes for leader A12 through A, B, D.

    // Expect a successful direct commit of A12 and leaders at rounds 9, 6 & 3.
    let last_decided = Slot::new_for_test(0, 0);
    let sequence = committer.try_decide(last_decided);
    tracing::info!("Commit sequence: {sequence:#?}");

    assert_eq!(sequence.len(), 4);
    if let DecidedLeader::Commit(ref block) = sequence[3] {
        assert_eq!(
            block.author(),
            committer.get_leaders(leader_round_wave_4)[0]
        )
    } else {
        panic!("Expected a committed leader")
    };
}

// TODO: Add byzantine variant of tests for indirect/direct commit/skip/undecided decisions

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
    let leader_schedule = Arc::new(LeaderSchedule::new(
        context.clone(),
        LeaderSwapTable::default(),
    ));

    // Create committer without pipelining and only 1 leader per leader round
    let committer =
        UniversalCommitterBuilder::new(context.clone(), leader_schedule, dag_state.clone()).build();

    // note: without pipelining or multi-leader enabled there should only be one committer.
    assert!(committer.committers.len() == 1);

    (context, dag_state, committer)
}

struct TestSetup {
    dag_builder: DagBuilder,
    dag_state: Arc<RwLock<DagState>>,
    committer: super::UniversalCommitter,
}

// TODO: Make this the basic_test_setup()
fn basic_dag_builder_test_setup() -> TestSetup {
    telemetry_subscribers::init_for_testing();
    let context = Arc::new(Context::new_for_test(4).0);
    let dag_builder = DagBuilder::new(context);

    let dag_state = Arc::new(RwLock::new(DagState::new(
        dag_builder.context.clone(),
        Arc::new(MemStore::new()),
    )));
    let leader_schedule = Arc::new(LeaderSchedule::new(
        dag_builder.context.clone(),
        LeaderSwapTable::default(),
    ));

    // Create committer without pipelining and only 1 leader per leader round
    let committer = UniversalCommitterBuilder::new(
        dag_builder.context.clone(),
        leader_schedule,
        dag_state.clone(),
    )
    .build();
    // note: without pipelining or multi-leader enabled there should only be one committer.
    assert!(committer.committers.len() == 1);

    TestSetup {
        dag_builder,
        dag_state,
        committer,
    }
}
