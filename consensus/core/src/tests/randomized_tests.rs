// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{env, sync::Arc};

use consensus_config::AuthorityIndex;
use parking_lot::RwLock;
use rand::{prelude::SliceRandom, rngs::StdRng, Rng, SeedableRng};

use crate::{
    block::{BlockAPI, Slot},
    block_manager::BlockManager,
    block_verifier::NoopBlockVerifier,
    commit::DecidedLeader,
    context::Context,
    dag_state::DagState,
    leader_schedule::{LeaderSchedule, LeaderSwapTable},
    storage::mem_store::MemStore,
    test_dag::create_random_dag,
    universal_committer::{
        universal_committer_builder::UniversalCommitterBuilder, UniversalCommitter,
    },
};

const NUM_RUNS: u32 = 100;
const NUM_ROUNDS: u32 = 200;

/// Test builds a randomized dag with the following conditions:
/// - Links to 2f+1 minimum ancestors
/// - Links to leader of previous round.
///
/// Should result in a direct commit for every round.
#[tokio::test]
async fn test_randomized_dag_all_direct_commit() {
    let mut random_test_setup = random_test_setup();

    for _ in 0..NUM_RUNS {
        let seed = random_test_setup.seeded_rng.gen_range(0..10000);
        let num_authorities = random_test_setup.seeded_rng.gen_range(4..10);
        let authority = authority_setup(num_authorities, 0);

        let include_leader_percentage = 100;
        let dag_builder = create_random_dag(
            seed,
            include_leader_percentage,
            NUM_ROUNDS,
            authority.context.clone(),
        );

        dag_builder.persist_all_blocks(authority.dag_state.clone());

        tracing::info!(
            "Running test with committee size {num_authorities} & {NUM_ROUNDS} rounds in the DAG..."
        );

        let last_decided = Slot::new_for_test(0, 0);
        let sequence = authority.committer.try_decide(last_decided);
        tracing::debug!("Commit sequence: {sequence:#?}");

        assert_eq!(sequence.len(), (NUM_ROUNDS - 2) as usize);
        for (i, leader_block) in sequence.iter().enumerate() {
            // First sequenced leader should be in round 1.
            let leader_round = i as u32 + 1;
            if let DecidedLeader::Commit(ref block) = leader_block {
                assert_eq!(block.round(), leader_round);
                assert_eq!(
                    block.author(),
                    authority.committer.get_leaders(leader_round)[0]
                );
            } else {
                panic!("Expected a committed leader")
            };
        }
    }
}

/// Test builds a randomized dag with the following conditions:
/// - Links to 2f+1 minimum ancestors
/// - Links to leader of previous round 50% of the time.
///
/// Blocks will randomly be fed through BlockManager and after each accepted
/// block we will try_decide() and if there is a committed sequence we will update
/// last_decided and continue. We do this from the perspective of two different
/// authorities who receive the blocks in different orders and ensure the resulting
/// sequence is the same for both authorities. The resulting sequence will include
/// Commit & Skip decisions and potentially will stop before coming to a decision
/// on all waves as we may have an Undecided leader somewhere early in the sequence.
#[tokio::test]
async fn test_randomized_dag_and_decision_sequence() {
    let mut random_test_setup = random_test_setup();

    for _ in 0..NUM_RUNS {
        let seed = random_test_setup.seeded_rng.gen_range(0..10000);
        let num_authorities = random_test_setup.seeded_rng.gen_range(4..10);

        // Setup for Authority 1
        let mut authority_1 = authority_setup(num_authorities, 1);

        let include_leader_percentage = 50;
        let dag_builder = create_random_dag(
            seed,
            include_leader_percentage,
            NUM_ROUNDS,
            authority_1.context.clone(),
        );

        tracing::info!(
        "Running test with committee size {num_authorities} & {NUM_ROUNDS} rounds in the DAG..."
        );

        let mut all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();
        all_blocks.shuffle(&mut random_test_setup.seeded_rng);

        let mut sequenced_leaders_1 = vec![];
        let mut last_decided = Slot::new_for_test(0, 0);
        let mut i = 0;
        while i < all_blocks.len() {
            let chunk_size = random_test_setup
                .seeded_rng
                .gen_range(1..=(all_blocks.len() - i));
            let chunk = &all_blocks[i..i + chunk_size];

            let _ = authority_1.block_manager.try_accept_blocks(chunk.to_vec());
            let sequence = authority_1.committer.try_decide(last_decided);

            if !sequence.is_empty() {
                sequenced_leaders_1.extend(sequence.clone());
                let leader_status = sequence.last().unwrap();
                last_decided = Slot::new(leader_status.round(), leader_status.authority());
            }

            i += chunk_size;
        }

        assert!(authority_1.block_manager.is_empty());

        // Setup for Authority 2
        let mut authority_2 = authority_setup(num_authorities, 2);

        let mut all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();
        all_blocks.shuffle(&mut random_test_setup.seeded_rng);

        let mut sequenced_leaders_2 = vec![];
        let mut last_decided = Slot::new_for_test(0, 0);
        let mut i = 0;
        while i < all_blocks.len() {
            let chunk_size = random_test_setup
                .seeded_rng
                .gen_range(1..=(all_blocks.len() - i));
            let chunk = &all_blocks[i..i + chunk_size];

            let _ = authority_2.block_manager.try_accept_blocks(chunk.to_vec());
            let sequence = authority_2.committer.try_decide(last_decided);

            if !sequence.is_empty() {
                sequenced_leaders_2.extend(sequence.clone());
                let leader_status = sequence.last().unwrap();
                last_decided = Slot::new(leader_status.round(), leader_status.authority());
            }

            i += chunk_size;
        }

        assert!(authority_2.block_manager.is_empty());

        // Ensure despite the difference in when blocks were received eventually after
        // receiving all blocks both authorities should return the same sequence of blocks.
        assert_eq!(sequenced_leaders_1, sequenced_leaders_2);
    }
}

struct AuthorityTestFixture {
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,
    committer: UniversalCommitter,
    block_manager: BlockManager,
}

fn authority_setup(num_authorities: usize, authority_index: u32) -> AuthorityTestFixture {
    let context = Arc::new(
        Context::new_for_test(num_authorities)
            .0
            .with_authority_index(AuthorityIndex::new_for_test(authority_index)),
    );
    let leader_schedule = Arc::new(LeaderSchedule::new(
        context.clone(),
        LeaderSwapTable::default(),
    ));
    let dag_state = Arc::new(RwLock::new(DagState::new(
        context.clone(),
        Arc::new(MemStore::new()),
    )));

    // Create committer with pipelining and only 1 leader per leader round
    let committer =
        UniversalCommitterBuilder::new(context.clone(), leader_schedule, dag_state.clone())
            .with_pipeline(true)
            .build();

    let block_manager = BlockManager::new(
        context.clone(),
        dag_state.clone(),
        Arc::new(NoopBlockVerifier),
    );

    AuthorityTestFixture {
        context,
        dag_state,
        committer,
        block_manager,
    }
}

struct RandomTestFixture {
    seeded_rng: StdRng,
}

fn random_test_setup() -> RandomTestFixture {
    telemetry_subscribers::init_for_testing();
    let mut rng = StdRng::from_entropy();
    let seed = match env::var("DAG_TEST_SEED") {
        Ok(seed_str) => {
            if let Ok(seed) = seed_str.parse::<u64>() {
                seed
            } else {
                tracing::warn!("Invalid DAG_TEST_SEED format. Using random seed.");
                rng.gen_range(0..10000)
            }
        }
        Err(_) => rng.gen_range(0..10000),
    };
    tracing::warn!("Using Random Seed: {seed}");

    let seeded_rng = StdRng::seed_from_u64(seed);
    RandomTestFixture { seeded_rng }
}
