// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use core::panic;
use parking_lot::RwLock;
use std::sync::Arc;

use crate::{
    base_committer::base_committer_builder::BaseCommitterBuilder, block::BlockAPI,
    commit::LeaderStatus, context::Context, dag_state::DagState, storage::mem_store::MemStore,
    test_dag_parser::parse_dag,
};

#[tokio::test]
async fn direct_commit() {
    telemetry_subscribers::init_for_testing();
    let context = Arc::new(Context::new_for_test(4).0);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));
    let committer = BaseCommitterBuilder::new(context.clone(), dag_state.clone()).build();

    // Round 3 is a leader round
    // D3 is an elected leader for wave 1
    // Round 4 is a voting round
    // Round 5 is a decision round (acknowledge)
    let dag_str = "DAG {
        Round 0 : { 4 },
        Round 1 : { * },
        Round 2 : { * },
        Round 3 : { * },
        Round 4 : { 
            A -> [D3],
            B -> [D3],
            C -> [D3],
            D -> [], 
        },
        Round 5 : { 
            A -> [A4, B4, C4],
            B -> [A4, B4, C4],
            C -> [A4, B4, C4],
            D -> [], 
        },
        }";

    let (_, dag_builder) = parse_dag(dag_str).expect("a DAG should be valid");
    dag_builder.persist_all_blocks(dag_state.clone());

    let leader_round = committer.leader_round(1);
    tracing::info!("Leader round at wave 1: {leader_round}");
    let leader = committer
        .elect_leader(leader_round)
        .expect("there should be a leader at wave 1");
    let leader_status = committer.try_direct_decide(leader);
    if let LeaderStatus::Commit(_) = leader_status {
        tracing::info!("Committed: {leader_status}");
    } else {
        panic!("Expected a committed leader, got {leader_status}");
    }
}

#[tokio::test]
async fn direct_skip() {
    telemetry_subscribers::init_for_testing();
    let context = Arc::new(Context::new_for_test(4).0);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));
    let committer = BaseCommitterBuilder::new(context.clone(), dag_state.clone()).build();

    // Round 3 is a leader round
    // D3 is an elected leader for wave 1
    // Round 4 is a voting round
    // Round 5 is a decision round (acknowledge)
    let dag_str = "DAG {
        Round 0 : { 4 },
        Round 1 : { * },
        Round 2 : { * },
        Round 3 : { * },
        Round 4 : { 
            A -> [D3],
            B -> [],
            C -> [],
            D -> [], 
        },
        Round 5 : { * },
        }";

    let (_, dag_builder) = parse_dag(dag_str).expect("a DAG should be valid");
    dag_builder.persist_all_blocks(dag_state.clone());

    let leader_round = committer.leader_round(1);
    tracing::info!("Leader round at wave 1: {leader_round}");
    let leader = committer
        .elect_leader(leader_round)
        .expect("there should be a leader at wave 1");
    let leader_status = committer.try_direct_decide(leader);
    if let LeaderStatus::Skip(_) = leader_status {
        tracing::info!("Skip: {leader_status}");
    } else {
        panic!("Expected a skipped leader, got {leader_status}");
    }
}

#[tokio::test]
async fn direct_undecided() {
    telemetry_subscribers::init_for_testing();
    let context = Arc::new(Context::new_for_test(4).0);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));
    let committer = BaseCommitterBuilder::new(context.clone(), dag_state.clone()).build();

    // Round 3 is a leader round
    // D3 is an elected leader for wave 1
    // Round 4 is a voting round
    // Round 5 is a decision round (acknowledge)
    let dag_str = "DAG {
        Round 0 : { 4 },
        Round 1 : { * },
        Round 2 : { * },
        Round 3 : { * },
        Round 4 : { 
            A -> [D3],
            B -> [D3],
            C -> [],
            D -> [], 
        },
        Round 5 : { * },
        }";

    let (_, dag_builder) = parse_dag(dag_str).expect("a DAG should be valid");
    dag_builder.persist_all_blocks(dag_state.clone());

    let leader_round = committer.leader_round(1);
    tracing::info!("Leader round at wave 1: {leader_round}");
    let leader = committer
        .elect_leader(leader_round)
        .expect("there should be a leader at wave 1");
    let leader_status = committer.try_direct_decide(leader);
    if let LeaderStatus::Undecided(_) = leader_status {
        tracing::info!("Undecided: {leader_status}");
    } else {
        panic!("Expected a undecided leader, got {leader_status}");
    }
}

#[tokio::test]
async fn indirect_commit() {
    telemetry_subscribers::init_for_testing();
    let context = Arc::new(Context::new_for_test(4).0);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));
    let committer = BaseCommitterBuilder::new(context.clone(), dag_state.clone()).build();

    // Wave 1
    // Round 3 is a leader round
    // D3 is an elected leader for wave 1
    // Round 4 is a voting round
    // Round 5 is a decision round (acknowledge)
    //
    // Wave 2
    // Round 6 is a leader round
    // C6 is an elected leader for wave 2
    // Round 7 is a voting round
    // Round 8 is a decision round (acknowledge)
    let dag_str = "DAG {
        Round 0 : { 4 },
        Round 1 : { * },
        Round 2 : { * },
        Round 3 : { * },
        Round 4 : { 
            A -> [D3],
            B -> [D3],
            C -> [D3],
            D -> [], 
        },
        Round 5 : { 
            A -> [A4, B4, C4],
            B -> [],
            C -> [],
            D -> [], 
        },
        Round 6 : { 
            A -> [],
            B -> [],
            C -> [A5],
            D -> [], 
        },
        Round 7 : { 
            A -> [C6],
            B -> [C6],
            C -> [],
            D -> [C6], 
        },
        Round 8 : { 
            A -> [A7, B7, D7],
            B -> [A7, B7, D7],
            C -> [],
            D -> [A7, B7, D7], 
        },
    }";

    let (_, dag_builder) = parse_dag(dag_str).expect("a DAG should be valid");
    dag_builder.persist_all_blocks(dag_state.clone());

    let leader_round = committer.leader_round(1);
    tracing::info!("Leader round wave 1: {leader_round}");
    let leader = committer
        .elect_leader(leader_round)
        .expect("there should be a leader for wave 1");
    let leader_index = leader.authority;
    tracing::info!("Leader index wave 1: {leader_index}");

    let leader_status_wave1 = committer.try_direct_decide(leader);
    if let LeaderStatus::Undecided(direct_undecided) = leader_status_wave1 {
        tracing::info!("Direct undecided leader at wave 1: {direct_undecided}");
    } else {
        panic!("Expected LeaderStatus::Undecided for a leader in wave 1, applying a direct decicion rule, got {leader_status_wave1}");
    }

    let leader_round_wave2 = committer.leader_round(2);
    tracing::info!("Leader round wave 2: {leader_round_wave2}");
    let leader_wave2 = committer
        .elect_leader(leader_round_wave2)
        .expect("there should be a leader for wave 2");
    let leader_index_wave2 = leader_wave2.authority;
    tracing::info!("Leader index wave 2: {leader_index_wave2}");

    let leader_status_wave_2 = committer.try_direct_decide(leader_wave2);
    if let LeaderStatus::Commit(committed) = leader_status_wave_2.clone() {
        tracing::info!("Direct committed leader at wave 2: {committed}");
    } else {
        panic!(
            "Expected LeaderStatus::Commit for a leader in wave 2, applying a direct decicion rule, got {leader_status_wave_2}"
        );
    };

    let leader_status_wave1_indirect =
        committer.try_indirect_decide(leader, [leader_status_wave_2].iter());

    if let LeaderStatus::Commit(committed) = leader_status_wave1_indirect {
        tracing::info!("Indirect committed leader at wave 1: {committed}");
    } else {
        panic!(
            "Expected LeaderStatus::Commit for a leader in wave 1, applying an indirect decicion rule, got {leader_status_wave1_indirect}"
        );
    };
}

/// Commit the first leader, indirectly skip the 2nd, and commit the 3rd leader.
#[tokio::test]
async fn indirect_skip() {
    telemetry_subscribers::init_for_testing();
    let context = Arc::new(Context::new_for_test(4).0);
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));
    let committer = BaseCommitterBuilder::new(context.clone(), dag_state.clone()).build();

    // There are 3 rounds. Every block is connected exept
    // that only f+1 validators connect to the leader of wave 2
    let dag_str = "DAG {
        Round 0 : { 4 },
        Round 1 : { * },
        Round 2 : { * },
        Round 3 : { * },
        Round 4 : { * },
        Round 5 : { * },
        Round 6 : { * },
        Round 7 : { 
            A -> [*],
            B -> [*],
            C -> [-C6],
            D -> [-C6], 
        },
        Round 8 : { * },
        Round 9 : { * },
        Round 10 : { * },
        Round 11 : { * },
    }";

    let (_, dag_builder) = parse_dag(dag_str).expect("a DAG should be valid");
    dag_builder.persist_all_blocks(dag_state.clone());

    let leader_round = committer.leader_round(1);
    tracing::info!("Leader round wave 1: {leader_round}");
    let leader = committer
        .elect_leader(leader_round)
        .expect("there should be a leader for wave 1");
    let leader_index = leader.authority;
    tracing::info!("Leader index wave 1: {leader_index}");

    let leader_status_wave1 = committer.try_direct_decide(leader);
    if let LeaderStatus::Commit(commited) = leader_status_wave1 {
        tracing::info!("Direct undecided leader at wave 1: {commited}");
    } else {
        panic!("Expected LeaderStatus::Commit for a leader in wave 1, applying a direct decicion rule, got {leader_status_wave1}");
    }

    let leader_round_wave_2 = committer.leader_round(2);
    tracing::info!("Leader round wave 2: {leader_round_wave_2}");
    let leader_wave2 = committer
        .elect_leader(leader_round_wave_2)
        .expect("there should be a leader for wave 2");
    let leader_index_wave_2 = leader_wave2.authority;
    tracing::info!("Leader index wave 2: {leader_index_wave_2}");

    let leader_status_wave_2 = committer.try_direct_decide(leader_wave2);
    if let LeaderStatus::Undecided(undecided) = leader_status_wave_2.clone() {
        tracing::info!("Direct committed leader at wave 2: {undecided}");
    } else {
        panic!(
            "Expected LeaderStatus::Undecided for a leader in wave 2, applying a direct decicion rule, got {leader_status_wave_2}"
        );
    };

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
}
