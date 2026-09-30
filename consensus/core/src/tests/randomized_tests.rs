// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
};

use rand::{Rng as _, SeedableRng as _, rngs::StdRng};

use crate::{
    block::Slot,
    commit_test_fixture::{CommitTestFixture, RandomDag, assert_commit_sequences_match},
    test_dag::{RandomDagEquivocationConfig, create_random_dag},
};

const NUM_RUNS: u32 = 100;
const MAX_STEP: u32 = 3;

/// Test builds a randomized dag with the following conditions:
/// - Links to 2f+1 minimum ancestors
/// - Links to leader of previous round.
///
/// Should result in a direct commit for every round.
#[tokio::test]
async fn test_randomized_dag_all_direct_commit() {
    let seed = random_test_seed();
    let num_authorities = 7;
    let num_rounds = 1000;
    let include_leader_percentage = 100;

    let context = CommitTestFixture::context_with_options(num_authorities, 0, Some(6));
    let dag_builder = create_random_dag(
        seed,
        include_leader_percentage,
        num_rounds,
        context.clone(),
        RandomDagEquivocationConfig::default(),
    );
    let all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();

    // Collect finalized commit sequences from each run
    let mut commit_sequences = vec![];

    for _ in 0..NUM_RUNS {
        tracing::info!(
            "Running test with {num_authorities} authorities & {num_rounds} rounds in the DAG with seed {seed}..."
        );

        let mut fixture = CommitTestFixture::new(context.clone());
        fixture.add_blocks(all_blocks.clone());

        let last_decided = Slot::new_for_test(0, 0);
        let (finalized_commits, _) = fixture.try_commit(last_decided).await;
        commit_sequences.push(finalized_commits);
    }

    let last_commit_sequence = assert_commit_sequences_match(commit_sequences);
    assert_eq!(last_commit_sequence.len(), (num_rounds - 2) as usize);
}

/// Test builds a randomized dag with the following conditions:
/// - Links to 2f+1 minimum ancestors
/// - Links to leader of previous round 50% of the time.
///
/// Blocks are delivered via RandomDagIterator in a constrained random order based on
/// quorum rounds, simulating realistic block arrival patterns. After each accepted
/// block we will try_commit() and if there is a committed sequence we will update
/// last_decided and continue. We do this from the perspective of different
/// authorities who receive the blocks in different orders and ensure the resulting
/// sequence is the same for all authorities. The resulting sequence will include
/// Commit & Skip decisions and potentially will stop before coming to a decision
/// on all waves as we may have an Undecided leader somewhere early in the sequence.
///
/// Additionally, this test processes commits through the Linearizer and CommitFinalizer
/// incrementally after each try_commit() call, similar to the production flow.
#[tokio::test]
async fn test_randomized_dag_and_decision_sequence() {
    let seed = random_test_seed();
    let mut rng = StdRng::seed_from_u64(seed);
    let num_authorities = 7;
    let num_rounds = 1000;
    let include_leader_percentage = 50;

    let context = CommitTestFixture::context_with_options(num_authorities, 0, Some(6));
    let dag_builder = create_random_dag(
        seed,
        include_leader_percentage,
        num_rounds,
        context.clone(),
        RandomDagEquivocationConfig::default(),
    );
    let all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();

    // Create RandomDag from existing blocks for using RandomDagIterator
    let dag = RandomDag::from_blocks(context.clone(), all_blocks);

    // Collect finalized commit sequences from each run
    let mut commit_sequences = vec![];

    for _ in 0..NUM_RUNS {
        tracing::info!(
            "Running test with {num_authorities} authorities & {num_rounds} rounds in the DAG with seed {seed}..."
        );

        let mut fixture = CommitTestFixture::new(context.clone());
        let mut finalized_commits = vec![];
        let mut last_decided = Slot::new_for_test(0, 0);

        // Use RandomDagIterator to deliver blocks in constrained random order
        for block in dag.random_iter(&mut rng, MAX_STEP) {
            fixture.try_accept_blocks(vec![block]);

            let (finalized, new_last_decided) = fixture.try_commit(last_decided).await;
            finalized_commits.extend(finalized);
            last_decided = new_last_decided;
        }

        assert!(fixture.has_no_suspended_blocks());
        commit_sequences.push(finalized_commits);
    }

    assert_commit_sequences_match(commit_sequences);
}

/// Randomized delivery must not make honest authorities commit divergent sequences
/// when up to F Byzantine authorities equivocate in a round.
#[tokio::test]
async fn test_random_dag_with_equivocations() {
    let num_authorities = 7;
    let num_rounds = 100;
    let include_leader_percentage = 100;
    let equivocation_config = RandomDagEquivocationConfig {
        equivocation_rate: 100,
        // Deliberately above F: the generator must cap this at F.
        max_equivocators: usize::MAX,
        equivocations_per_authority: 1,
    };

    // Fixed seeds make failures reproducible while exercising distinct choices of
    // equivocating authorities and minimal-parent links.
    for seed in [7, 42, 2026] {
        let context = CommitTestFixture::context_with_options(num_authorities, 0, Some(6));
        let dag_builder = create_random_dag(
            seed,
            include_leader_percentage,
            num_rounds,
            context.clone(),
            equivocation_config,
        );
        let max_safe_equivocators = (num_authorities - 1) / 3;
        let mut saw_max_equivocators = false;
        let mut all_equivocators = BTreeSet::new();
        for round in 1..=num_rounds {
            let blocks_per_author = dag_builder
                .blocks
                .keys()
                .filter(|block_ref| block_ref.round == round)
                .fold(BTreeMap::new(), |mut counts, block_ref| {
                    *counts.entry(block_ref.author).or_insert(0) += 1;
                    counts
                });
            let equivocators = blocks_per_author
                .values()
                .filter(|&&blocks| blocks > 1)
                .count();
            all_equivocators.extend(
                blocks_per_author
                    .iter()
                    .filter_map(|(&author, &blocks)| (blocks > 1).then_some(author)),
            );
            assert!(
                (1..=max_safe_equivocators).contains(&equivocators),
                "round {round} has {equivocators} equivocating authorities"
            );
            saw_max_equivocators |= equivocators == max_safe_equivocators;
        }
        assert!(
            saw_max_equivocators,
            "seed {seed} did not generate a round with {max_safe_equivocators} equivocators"
        );
        assert!(
            all_equivocators.len() <= max_safe_equivocators,
            "seed {seed} has {} distinct equivocating authorities",
            all_equivocators.len()
        );
        let all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();
        let dag = RandomDag::from_blocks(context.clone(), all_blocks);
        let mut delivery_rng = StdRng::seed_from_u64(seed);
        let mut commit_sequences = vec![];

        for _ in 0..NUM_RUNS {
            let mut fixture = CommitTestFixture::new(context.clone());
            let mut finalized_commits = vec![];
            let mut last_decided = Slot::new_for_test(0, 0);

            for block in dag.random_iter(&mut delivery_rng, MAX_STEP) {
                fixture.try_accept_blocks(vec![block]);
                let (finalized, new_last_decided) = fixture.try_commit(last_decided).await;
                finalized_commits.extend(finalized);
                last_decided = new_last_decided;
            }

            assert!(fixture.has_no_suspended_blocks());
            commit_sequences.push(finalized_commits);
        }

        let commits = assert_commit_sequences_match(commit_sequences);
        let last_commit_round = commits
            .last()
            .expect("honest quorum did not make progress")
            .leader
            .round;
        assert!(
            last_commit_round >= num_rounds * 3 / 4,
            "honest quorum stopped making progress at round {last_commit_round} for seed {seed}"
        );
    }
}

fn random_test_seed() -> u64 {
    telemetry_subscribers::init_for_testing();
    let mut seed: u64 = rand::thread_rng().r#gen();
    if let Ok(seed_str) = env::var("DAG_TEST_SEED")
        && let Ok(s) = seed_str.parse::<u64>()
    {
        seed = s;
        tracing::info!("Using DAG_TEST_SEED to override random seed: {seed}");
    }
    seed
}
