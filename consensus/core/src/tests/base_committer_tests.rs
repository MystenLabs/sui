// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashSet, sync::Arc};

use consensus_config::AuthorityIndex;
use parking_lot::RwLock;

use crate::{
    base_committer::base_committer_builder::BaseCommitterBuilder,
    block::{BlockAPI, TestBlock, Transaction, VerifiedBlock},
    commit::LeaderStatus,
    context::Context,
    dag_state::DagState,
    storage::mem_store::MemStore,
    test_dag::{build_dag, build_dag_layer},
};

/// Commit one leader.  
#[tokio::test]
async fn try_direct_commit() {
    telemetry_subscribers::init_for_testing();
    // Commitee of 4 with even stake
    let context = Arc::new(Context::new_for_test(4).0);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));
    let committer = BaseCommitterBuilder::new(context.clone(), dag_state.clone()).build();

    // Build fully connected dag with empty blocks. Adding 8 rounds to the dag
    // so that we have 2 completed waves and one incomplete wave.
    // note: rounds & waves are zero indexed.
    let num_rounds_in_dag = 8;
    let voting_round_wave_2 = committer.leader_round(2) + 1;
    let incomplete_wave_leader_round = 6;
    build_dag(context, dag_state, None, voting_round_wave_2);

    // Leader rounds are the first rounds of each wave. In this case rounds 3 & 6.
    let mut leader_rounds: Vec<u32> = (1..num_rounds_in_dag)
        .map(|r| committer.leader_round(r))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    // Iterate from highest leader round first
    leader_rounds.sort_by(|a, b| b.cmp(a));
    for round in leader_rounds.into_iter() {
        let leader = committer
            .elect_leader(round)
            .expect("should have elected leader");
        tracing::info!("Try direct commit for leader {leader}",);
        let leader_status = committer.try_direct_decide(leader);
        tracing::info!("Leader commit status: {leader_status}");

        if round < incomplete_wave_leader_round {
            if let LeaderStatus::Commit(ref committed_block) = leader_status {
                assert_eq!(committed_block.author(), leader.authority)
            } else {
                panic!("Expected a committed leader at round {}", round)
            };
        } else {
            // The base committer should mark the potential leader in r6 as undecided
            // as there is no way to get enough certificates because we did not build
            // the dag layer for the decision round of wave 3.
            if let LeaderStatus::Undecided(undecided_slot) = leader_status {
                assert_eq!(undecided_slot, leader)
            } else {
                panic!("Expected an undecided leader")
            };
        }
    }
}

/// Ensure idempotent replies.
#[tokio::test]
async fn idempotence() {
    telemetry_subscribers::init_for_testing();
    // Commitee of 4 with even stake
    let context = Arc::new(Context::new_for_test(4).0);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));
    let committer = BaseCommitterBuilder::new(context.clone(), dag_state.clone()).build();

    // Build fully connected dag with empty blocks. Adding 5 rounds to the dag
    // aka thte decision round of wave 1.
    let decision_round_wave_1 = committer.decision_round(1);
    build_dag(context, dag_state, None, decision_round_wave_1);

    // Commit one leader.
    let leader_round_wave_1 = committer.leader_round(1);
    let leader = committer
        .elect_leader(leader_round_wave_1)
        .expect("should have elected leader");
    tracing::info!("Try direct commit for leader {leader}",);
    let leader_status = committer.try_direct_decide(leader);
    tracing::info!("Leader commit status: {leader_status}");

    if let LeaderStatus::Commit(ref block) = leader_status {
        assert_eq!(block.author(), leader.authority)
    } else {
        panic!("Expected a committed leader")
    };

    // Commit the same leader again on the same dag state and get the same result
    tracing::info!("Try direct commit for leader {leader} again",);
    let leader_status = committer.try_direct_decide(leader);
    tracing::info!("Leader commit status: {leader_status}");

    if let LeaderStatus::Commit(ref committed_block) = leader_status {
        assert_eq!(committed_block.author(), leader.authority)
    } else {
        panic!("Expected a committed leader")
    };
}

/// Commit one by one each leader as the dag progresses in ideal conditions.
#[tokio::test]
async fn multiple_direct_commit() {
    telemetry_subscribers::init_for_testing();
    // Commitee of 4 with even stake
    let context = Arc::new(Context::new_for_test(4).0);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));
    let committer = BaseCommitterBuilder::new(context.clone(), dag_state.clone()).build();

    let mut ancestors = None;
    for n in 1..=10 {
        // note: rounds are zero indexed.
        let decision_round = committer.decision_round(n);
        ancestors = Some(build_dag(
            context.clone(),
            dag_state.clone(),
            ancestors,
            decision_round,
        ));

        // Leader round is the first round of each wave.
        // note: rounds are zero indexed.
        let leader_round = committer.leader_round(n);
        let leader = committer
            .elect_leader(leader_round)
            .expect("should have elected leader");
        tracing::info!("Try direct commit for leader {leader}",);
        let leader_status = committer.try_direct_decide(leader);
        tracing::info!("Leader commit status: {leader_status}");

        if let LeaderStatus::Commit(ref committed_block) = leader_status {
            assert_eq!(committed_block.author(), leader.authority)
        } else {
            panic!("Expected a committed leader")
        };
    }
}

/// We directly skip the leader if it has enough blame.
#[tokio::test]
async fn direct_skip() {
    telemetry_subscribers::init_for_testing();
    // Commitee of 4 with even stake
    let context = Arc::new(Context::new_for_test(4).0);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));
    let committer = BaseCommitterBuilder::new(context.clone(), dag_state.clone()).build();

    // Add enough blocks to reach the leader round of wave 1.
    let leader_round_wave_1 = committer.leader_round(1);
    let references_leader_round_wave_1 = build_dag(
        context.clone(),
        dag_state.clone(),
        None,
        leader_round_wave_1,
    );

    // Votes in round 4 will not include the leader of wave 1.
    // Filter out that leader.
    let leader_wave_1 = committer
        .elect_leader(leader_round_wave_1)
        .expect("should have elected leader");
    let references_without_leader_wave_1: Vec<_> = references_leader_round_wave_1
        .into_iter()
        .filter(|x| x.author != leader_wave_1.authority)
        .collect();

    // Add enough blocks to reach the decision round of wave 1.
    let decision_round_wave_1 = committer.decision_round(1);
    build_dag(
        context.clone(),
        dag_state.clone(),
        Some(references_without_leader_wave_1),
        decision_round_wave_1,
    );

    // Ensure no blocks are committed.
    tracing::info!("Try direct commit for leader {leader_wave_1}",);
    let leader_status = committer.try_direct_decide(leader_wave_1);
    tracing::info!("Leader commit status: {leader_status}");

    if let LeaderStatus::Skip(skipped_leader) = leader_status {
        assert_eq!(skipped_leader, leader_wave_1);
    } else {
        panic!("Expected to directly skip the leader");
    }
}

/// Indirect-commit the first leader.
#[tokio::test]
async fn indirect_commit() {
    telemetry_subscribers::init_for_testing();
    // Commitee of 4 with even stake
    let context = Arc::new(Context::new_for_test(4).0);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));
    let committer = BaseCommitterBuilder::new(context.clone(), dag_state.clone()).build();

    // Add enough blocks to reach the leader round of wave 1.
    let leader_round_wave_1 = committer.leader_round(1);
    let references_leader_round_wave_1 = build_dag(
        context.clone(),
        dag_state.clone(),
        None,
        leader_round_wave_1,
    );

    // Filter out that leader.
    let leader_wave_1 = committer
        .elect_leader(leader_round_wave_1)
        .expect("should have elected leader");
    let references_without_leader_wave_1: Vec<_> = references_leader_round_wave_1
        .iter()
        .cloned()
        .filter(|x| x.author != leader_wave_1.authority)
        .collect();

    // Only 2f+1 validators vote for the leader of wave 1.
    let connections_with_leader_wave_1 = context
        .committee
        .authorities()
        .take(context.committee.quorum_threshold() as usize)
        .map(|authority| (authority.0, references_leader_round_wave_1.clone()))
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
    let decision_round_wave_2 = committer.decision_round(2);
    build_dag(
        context.clone(),
        dag_state.clone(),
        Some(references_decision_round_wave_1),
        decision_round_wave_2,
    );

    // Try direct commit leader from wave 2 which should result in Commit
    let leader_wave_2 = committer
        .elect_leader(committer.leader_round(2))
        .expect("should have elected leader");
    tracing::info!("Try direct commit for leader {leader_wave_2}");
    let leader_status = committer.try_direct_decide(leader_wave_2);
    tracing::info!("Leader commit status: {leader_status}");

    let mut decided_leaders = vec![];
    if let LeaderStatus::Commit(ref committed_block) = leader_status {
        assert_eq!(committed_block.author(), leader_wave_2.authority);
        decided_leaders.push(leader_status);
    } else {
        panic!("Expected a committed leader")
    };

    // Try direct commit leader from wave 1 which should result in Undecided
    tracing::info!("Try direct commit for leader {leader_wave_1}");
    let leader_status = committer.try_direct_decide(leader_wave_1);
    tracing::info!("Leader commit status: {leader_status}");

    if let LeaderStatus::Undecided(undecided_slot) = leader_status {
        assert_eq!(undecided_slot, leader_wave_1)
    } else {
        panic!("Expected an undecided leader")
    };

    // Quick Summary:
    // Leader of wave 2 or C6 has the necessary votes/certs to be directly commited.
    // Then, when we get to the leader of wave 1 or D3, we see that we cannot direct commit
    // and it is marked as undecided. But this time we have a committed anchor so we
    // check if there is a certified link from the anchor (c6) to the undecided leader
    // (d3). There is a certified link through A5 with votes A4,B4,C4. So we can mark
    // this leader as committed indirectly.

    // Ensure we commit the leader of wave 1 indirectly with the committed leader
    // of wave 2 as the anchor.
    tracing::info!("Try indirect commit for leader {leader_wave_1}",);
    let leader_status = committer.try_indirect_decide(leader_wave_1, decided_leaders.iter());
    tracing::info!("Leader commit status: {leader_status}");

    if let LeaderStatus::Commit(ref committed_block) = leader_status {
        assert_eq!(committed_block.author(), leader_wave_1.authority)
    } else {
        panic!("Expected a committed leader")
    };
}

/// Commit the first leader, indirectly skip the 2nd, and commit the 3rd leader.
#[tokio::test]
async fn indirect_skip() {
    telemetry_subscribers::init_for_testing();
    // Commitee of 4 with even stake
    let context = Arc::new(Context::new_for_test(4).0);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));
    let committer = BaseCommitterBuilder::new(context.clone(), dag_state.clone()).build();

    // Add enough blocks to reach the leader round of wave 2.
    let leader_round_wave_2 = committer.leader_round(2);
    let references_leader_round_wave_2 = build_dag(
        context.clone(),
        dag_state.clone(),
        None,
        leader_round_wave_2,
    );

    // Filter out that leader.
    let leader_wave_2 = committer
        .elect_leader(leader_round_wave_2)
        .expect("should have elected leader");
    let references_without_leader_wave_2: Vec<_> = references_leader_round_wave_2
        .iter()
        .cloned()
        .filter(|x| x.author != leader_wave_2.authority)
        .collect();

    // Only f+1 validators connect to the leader of wave 2.
    let mut references_voting_round_wave_2 = Vec::new();

    let connections_with_vote_leader_wave_2 = context
        .committee
        .authorities()
        .take(context.committee.validity_threshold() as usize)
        .map(|authority| (authority.0, references_leader_round_wave_2.clone()))
        .collect();

    references_voting_round_wave_2.extend(build_dag_layer(
        connections_with_vote_leader_wave_2,
        dag_state.clone(),
    ));

    let connections_without_vote_leader_wave_2 = context
        .committee
        .authorities()
        .skip(context.committee.validity_threshold() as usize)
        .map(|authority| (authority.0, references_without_leader_wave_2.clone()))
        .collect();

    references_voting_round_wave_2.extend(build_dag_layer(
        connections_without_vote_leader_wave_2,
        dag_state.clone(),
    ));

    // Add enough blocks to reach the decison round of wave 3.
    let decision_round_wave_3 = committer.decision_round(3);
    build_dag(
        context.clone(),
        dag_state.clone(),
        Some(references_voting_round_wave_2),
        decision_round_wave_3,
    );

    // Ensure we commit the leaders of wave 1 and 3 and skip the leader of wave 2

    // 1. Ensure we commit the leader of wave 3.
    let leader_round_wave_3 = committer.leader_round(3);
    let leader_wave_3 = committer
        .elect_leader(leader_round_wave_3)
        .expect("should have elected leader");
    tracing::info!("Try direct commit for leader {leader_wave_3}");
    let leader_status = committer.try_direct_decide(leader_wave_3);
    tracing::info!("Leader commit status: {leader_status}");

    let mut decided_leaders = vec![];
    if let LeaderStatus::Commit(ref committed_block) = leader_status {
        assert_eq!(committed_block.author(), leader_wave_3.authority);
        decided_leaders.push(leader_status);
    } else {
        panic!("Expected a committed leader")
    };

    // Leader of wave 2 is undecided directly and then skipped indirectly because
    // of lack of certified links.

    // 2. Ensure we directly mark leader of wave 2 undecided.
    let leader_wave_2 = committer
        .elect_leader(leader_round_wave_2)
        .expect("should have elected leader");
    tracing::info!("Try direct commit for leader {leader_wave_2}");
    let leader_status = committer.try_direct_decide(leader_wave_2);
    tracing::info!("Leader commit status: {leader_status}");

    if let LeaderStatus::Undecided(undecided_slot) = leader_status {
        assert_eq!(undecided_slot, leader_wave_2)
    } else {
        panic!("Expected an undecided leader")
    };

    // 3. Ensure we skip leader of wave 2 indirectly.
    tracing::info!("Try indirect commit for leader {leader_wave_2}",);
    let leader_status = committer.try_indirect_decide(leader_wave_2, decided_leaders.iter());
    tracing::info!("Leader commit status: {leader_status}");

    if let LeaderStatus::Skip(skipped_slot) = leader_status {
        assert_eq!(skipped_slot, leader_wave_2)
    } else {
        panic!("Expected a skipped leader")
    };

    // Ensure we directly commit the leader of wave 1.
    let leader_round_wave_1 = committer.leader_round(1);
    let leader_wave_1 = committer
        .elect_leader(leader_round_wave_1)
        .expect("should have elected leader");
    tracing::info!("Try direct commit for leader {leader_wave_1}");
    let leader_status = committer.try_direct_decide(leader_wave_1);
    tracing::info!("Leader commit status: {leader_status}");

    if let LeaderStatus::Commit(ref committed_block) = leader_status {
        assert_eq!(committed_block.author(), leader_wave_1.authority);
    } else {
        panic!("Expected a committed leader")
    };
}

/// If there is no leader with enough support nor blame, we commit nothing.
#[tokio::test]
async fn undecided() {
    telemetry_subscribers::init_for_testing();
    // Commitee of 4 with even stake
    let context = Arc::new(Context::new_for_test(4).0);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));
    let committer = BaseCommitterBuilder::new(context.clone(), dag_state.clone()).build();

    // Add enough blocks to reach the leader round of wave 1.
    let leader_round_wave_1 = committer.leader_round(1);
    let references_leader_round_wave_1 = build_dag(
        context.clone(),
        dag_state.clone(),
        None,
        leader_round_wave_1,
    );

    // Filter out that leader.
    let leader_wave_1 = committer
        .elect_leader(leader_round_wave_1)
        .expect("should have elected leader");
    let references_without_leader_wave_1: Vec<_> = references_leader_round_wave_1
        .iter()
        .cloned()
        .filter(|x| x.author != leader_wave_1.authority)
        .collect();

    // Create a dag layer where only one authority votes for the leader of wave 1.
    let mut authorities = context.committee.authorities();
    let connections_leader_wave_1 = vec![(
        authorities.next().unwrap().0,
        references_leader_round_wave_1,
    )];

    // Also to ensure we have < 2f+1 blames, we take less then that for connections (votes)
    // without the leader of wave 1.
    let connections_without_leader_wave_1: Vec<_> = authorities
        .take((context.committee.quorum_threshold() - 1) as usize)
        .map(|authority| (authority.0, references_without_leader_wave_1.clone()))
        .collect();

    let connections_voting_round_wave_1 = connections_leader_wave_1
        .into_iter()
        .chain(connections_without_leader_wave_1)
        .collect();
    let references_voting_round_wave_1 =
        build_dag_layer(connections_voting_round_wave_1, dag_state.clone());

    // Add enough blocks to reach the decision round of wave 1.
    let decision_round_wave_1 = committer.decision_round(1);
    build_dag(
        context.clone(),
        dag_state.clone(),
        Some(references_voting_round_wave_1),
        decision_round_wave_1,
    );

    // Ensure we directly mark leader of wave 1 undecided as there are less than
    // 2f+1 blames and 2f+1 certs
    tracing::info!("Try direct commit for leader {leader_wave_1}");
    let leader_status = committer.try_direct_decide(leader_wave_1);
    tracing::info!("Leader commit status: {leader_status}");

    if let LeaderStatus::Undecided(undecided_slot) = leader_status {
        assert_eq!(undecided_slot, leader_wave_1)
    } else {
        panic!("Expected an undecided leader")
    };

    // Ensure we indirectly mark leader of wave 1 undecided as there is no anchor
    // to make an indirect decision.
    tracing::info!("Try indirect commit for leader {leader_wave_1}");
    let leader_status = committer.try_indirect_decide(leader_wave_1, [].iter());
    tracing::info!("Leader commit status: {leader_status}");

    if let LeaderStatus::Undecided(undecided_slot) = leader_status {
        assert_eq!(undecided_slot, leader_wave_1)
    } else {
        panic!("Expected an undecided leader")
    };
}

// This test scenario has one authority that is acting in a byzantine manner. It
// will be sending multiple different blocks to different validators for a round.
// The commit rule should handle this and correctly commit the expected blocks.
#[tokio::test]
async fn test_byzantine_direct_commit() {
    telemetry_subscribers::init_for_testing();
    // Commitee of 4 with even stake
    let context = Arc::new(Context::new_for_test(4).0);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));
    let committer = BaseCommitterBuilder::new(context.clone(), dag_state.clone()).build();

    // Add enough blocks to reach leader round of wave 4
    let leader_round_wave_4 = committer.leader_round(4);
    let references_leader_round_wave_4 = build_dag(
        context.clone(),
        dag_state.clone(),
        None,
        leader_round_wave_4,
    );

    // Add blocks to reach voting round of wave 4
    let voting_round_wave_4 = leader_round_wave_4 + 1;
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

    // Filter out leader from wave 4.
    let leader_wave_4 = committer
        .elect_leader(leader_round_wave_4)
        .expect("should have elected leader");

    // B12 C12 D12
    let references_without_leader_round_wave_4: Vec<_> = references_leader_round_wave_4
        .into_iter()
        .filter(|x| x.author != leader_wave_4.authority)
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
    // - We have  B14, C14 & D14 that include this byzantine nonvote and A14 from the
    //   decision round. But all of these blocks also have good votes from A, B, C & D.
    // Expect a successful direct commit.

    tracing::info!("Try direct commit for leader {leader_wave_4}");
    let leader_status = committer.try_direct_decide(leader_wave_4);
    tracing::info!("Leader commit status: {leader_status}");

    if let LeaderStatus::Commit(ref committed_block) = leader_status {
        assert_eq!(committed_block.author(), leader_wave_4.authority);
    } else {
        panic!("Expected a committed leader")
    };
}

// TODO: Add test for indirect commit with a certified link through a byzantine validator.

// TODO: add basic tests for multi leader & pipeline. More tests will be added to
// throughly test pipelining and multileader once universal committer lands so
// these tests may not be necessary here.
