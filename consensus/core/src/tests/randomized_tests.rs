// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::env;

use rand::{Rng, SeedableRng, prelude::SliceRandom, rngs::StdRng};

use crate::{
    block::{BlockAPI, Slot},
    commit::DecidedLeader,
    commit_test_fixture::CommitTestFixture,
    test_dag::create_random_dag,
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
        let mut fixture = CommitTestFixture::with_options(num_authorities, 0, None);

        let include_leader_percentage = 100;
        let dag_builder = create_random_dag(
            seed,
            include_leader_percentage,
            NUM_ROUNDS,
            fixture.context.clone(),
        );

        // Add blocks to local state including TransactionCertifier and DagState.
        fixture.add_blocks(dag_builder.blocks.values().cloned().collect());

        tracing::info!(
            "Running test with committee size {num_authorities} & {NUM_ROUNDS} rounds in the DAG..."
        );

        let last_decided = Slot::new_for_test(0, 0);
        let sequence = fixture.committer.try_decide(last_decided);
        tracing::debug!("Commit sequence: {sequence:#?}");

        assert_eq!(sequence.len(), (NUM_ROUNDS - 2) as usize);
        for (i, leader_block) in sequence.iter().enumerate() {
            // First sequenced leader should be in round 1.
            let leader_round = i as u32 + 1;
            if let DecidedLeader::Commit(block, _direct) = leader_block {
                assert_eq!(block.round(), leader_round);
                assert_eq!(
                    block.author(),
                    fixture.committer.get_leaders(leader_round)[0]
                );
            } else {
                panic!("Expected a committed leader")
            };
        }

        // Process commits through linearizer and commit finalizer
        let finalized_commits = fixture.process_commits(sequence.clone()).await;
        let expected_commit_count = sequence
            .iter()
            .filter(|s| matches!(s, DecidedLeader::Commit(_, _)))
            .count();
        assert_eq!(finalized_commits.len(), expected_commit_count);
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
///
/// Additionally, this test processes commits through the Linearizer and CommitFinalizer
/// incrementally after each try_decide() call, similar to the production flow.
#[tokio::test]
async fn test_randomized_dag_and_decision_sequence() {
    let mut random_test_setup = random_test_setup();

    for _ in 0..NUM_RUNS {
        let seed = random_test_setup.seeded_rng.gen_range(0..10000);
        let num_authorities = random_test_setup.seeded_rng.gen_range(4..10);

        // Setup for Authority 1
        let mut fixture_1 = CommitTestFixture::with_options(num_authorities, 1, None);

        let include_leader_percentage = 50;
        let dag_builder = create_random_dag(
            seed,
            include_leader_percentage,
            NUM_ROUNDS,
            fixture_1.context.clone(),
        );

        tracing::info!(
            "Running test with committee size {num_authorities} & {NUM_ROUNDS} rounds in the DAG..."
        );

        let mut all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();
        all_blocks.shuffle(&mut random_test_setup.seeded_rng);

        let mut sequenced_leaders_1 = vec![];
        let mut finalized_commits_1 = vec![];
        let mut last_decided = Slot::new_for_test(0, 0);
        let mut i = 0;
        while i < all_blocks.len() {
            let chunk_size = random_test_setup
                .seeded_rng
                .gen_range(1..=(all_blocks.len() - i));
            let chunk = &all_blocks[i..i + chunk_size];

            // Try accept the blocks into DagState via BlockManager. Also votes for the blocks via TransactionCertifier.
            fixture_1.try_accept_blocks(chunk.to_vec());

            let sequence = fixture_1.committer.try_decide(last_decided);

            if !sequence.is_empty() {
                sequenced_leaders_1.extend(sequence.clone());

                // Process commits incrementally after each try_decide()
                let finalized = fixture_1.process_commits(sequence.clone()).await;
                finalized_commits_1.extend(finalized);

                let leader_status = sequence.last().unwrap();
                last_decided = Slot::new(leader_status.round(), leader_status.authority());
            }

            i += chunk_size;
        }

        assert!(fixture_1.has_no_suspended_blocks());

        // Setup for Authority 2
        let mut fixture_2 = CommitTestFixture::with_options(num_authorities, 2, None);

        let mut all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();
        all_blocks.shuffle(&mut random_test_setup.seeded_rng);

        let mut sequenced_leaders_2 = vec![];
        let mut finalized_commits_2 = vec![];
        let mut last_decided = Slot::new_for_test(0, 0);
        let mut i = 0;
        while i < all_blocks.len() {
            let chunk_size = random_test_setup
                .seeded_rng
                .gen_range(1..=(all_blocks.len() - i));
            let chunk = &all_blocks[i..i + chunk_size];

            // Try accept the blocks into DagState via BlockManager. Also votes for the blocks via TransactionCertifier.
            fixture_2.try_accept_blocks(chunk.to_vec());

            let sequence = fixture_2.committer.try_decide(last_decided);

            if !sequence.is_empty() {
                sequenced_leaders_2.extend(sequence.clone());

                // Process commits incrementally after each try_decide()
                let finalized = fixture_2.process_commits(sequence.clone()).await;
                finalized_commits_2.extend(finalized);

                let leader_status = sequence.last().unwrap();
                last_decided = Slot::new(leader_status.round(), leader_status.authority());
            }

            i += chunk_size;
        }

        assert!(fixture_2.has_no_suspended_blocks());

        // Ensure despite the difference in when blocks were received eventually after
        // receiving all blocks both authorities should return the same sequence of blocks.
        assert_eq!(sequenced_leaders_1, sequenced_leaders_2);

        // Both authorities should produce identical finalized commit sequences
        assert_eq!(finalized_commits_1.len(), finalized_commits_2.len());
        for (f1, f2) in finalized_commits_1.iter().zip(finalized_commits_2.iter()) {
            assert_eq!(f1.commit_ref, f2.commit_ref);
            assert_eq!(f1.leader, f2.leader);
        }
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
