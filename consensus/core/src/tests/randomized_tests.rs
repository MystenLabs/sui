// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{env, sync::Arc};

use consensus_types::block::{Round, TransactionIndex};
use rand::{Rng, SeedableRng, prelude::SliceRandom, rngs::StdRng};

use crate::{
    block::{BlockAPI, Slot, VerifiedBlock},
    commit::CommittedSubDag,
    commit_test_fixture::CommitTestFixture,
    context::Context,
    test_dag::create_random_dag,
    test_dag_builder::DagBuilder,
};

const NUM_RUNS: u32 = 100;
const NUM_ROUNDS: u32 = 1000;

/// Test builds a randomized dag with the following conditions:
/// - Links to 2f+1 minimum ancestors
/// - Links to leader of previous round.
///
/// Should result in a direct commit for every round.
#[tokio::test]
async fn test_randomized_dag_all_direct_commit() {
    let mut random_test_setup = random_test_setup();
    let seed = random_test_setup.seeded_rng.gen_range(0..10000);
    let num_authorities = random_test_setup.seeded_rng.gen_range(4..10);
    let include_leader_percentage = 100;

    let context = CommitTestFixture::context_with_options(num_authorities, 0, Some(5));
    let dag_builder =
        create_random_dag(seed, include_leader_percentage, NUM_ROUNDS, context.clone());
    let all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();

    // Collect finalized commit sequences from each run
    let mut commit_sequences = vec![];

    for _ in 0..NUM_RUNS {
        tracing::info!(
            "Running test with committee size {num_authorities} & {NUM_ROUNDS} rounds in the DAG..."
        );

        let mut fixture = CommitTestFixture::new(context.clone());
        fixture.add_blocks(all_blocks.clone());

        let last_decided = Slot::new_for_test(0, 0);
        let sequence = fixture.committer.try_decide(last_decided);
        tracing::debug!("Commit sequence: {sequence:#?}");

        // Process commits through linearizer and commit finalizer
        let finalized_commits = fixture.process_commits(sequence).await;
        commit_sequences.push(finalized_commits);
    }

    let last_commit_sequence = assert_commit_sequences_match(commit_sequences);
    assert_eq!(last_commit_sequence.len(), (NUM_ROUNDS - 2) as usize);
}

/// Test builds a randomized dag with the following conditions:
/// - Links to 2f+1 minimum ancestors
/// - Links to leader of previous round 50% of the time.
///
/// Blocks will randomly be fed through BlockManager and after each accepted
/// block we will try_decide() and if there is a committed sequence we will update
/// last_decided and continue. We do this from the perspective of different
/// authorities who receive the blocks in different orders and ensure the resulting
/// sequence is the same for all authorities. The resulting sequence will include
/// Commit & Skip decisions and potentially will stop before coming to a decision
/// on all waves as we may have an Undecided leader somewhere early in the sequence.
///
/// Additionally, this test processes commits through the Linearizer and CommitFinalizer
/// incrementally after each try_decide() call, similar to the production flow.
#[tokio::test]
async fn test_randomized_dag_and_decision_sequence() {
    let mut random_test_setup = random_test_setup();
    let seed = random_test_setup.seeded_rng.gen_range(0..10000);
    let num_authorities = random_test_setup.seeded_rng.gen_range(4..10);
    let include_leader_percentage = 50;

    let context = CommitTestFixture::context_with_options(num_authorities, 1, Some(5));
    let dag_builder =
        create_random_dag(seed, include_leader_percentage, NUM_ROUNDS, context.clone());
    let all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();

    // Collect finalized commit sequences from each run
    let mut commit_sequences = vec![];

    for _ in 0..NUM_RUNS {
        tracing::info!(
            "Running test with committee size {num_authorities} & {NUM_ROUNDS} rounds in the DAG..."
        );

        let mut fixture = CommitTestFixture::new(context.clone());

        // Shuffle blocks for this iteration
        let mut shuffled_blocks = all_blocks.clone();
        shuffled_blocks.shuffle(&mut random_test_setup.seeded_rng);

        let mut finalized_commits = vec![];
        let mut last_decided = Slot::new_for_test(0, 0);
        let mut i = 0;
        while i < shuffled_blocks.len() {
            let chunk_size = random_test_setup
                .seeded_rng
                .gen_range(1..=(shuffled_blocks.len() - i).min(5 * num_authorities as usize));
            let chunk = &shuffled_blocks[i..i + chunk_size];

            fixture.try_accept_blocks(chunk.to_vec());

            let sequence = fixture.committer.try_decide(last_decided);

            if !sequence.is_empty() {
                let finalized = fixture.process_commits(sequence.clone()).await;
                finalized_commits.extend(finalized);
                let leader_status = sequence.last().unwrap();
                last_decided = Slot::new(leader_status.round(), leader_status.authority());
            }

            i += chunk_size;
        }

        assert!(fixture.has_no_suspended_blocks());
        commit_sequences.push(finalized_commits);
    }

    assert_commit_sequences_match(commit_sequences);
}

/// Test similar to test_randomized_dag_and_decision_sequence but with random reject votes.
///
/// This test:
/// - Creates a DAG with transactions in each block
/// - Randomly generates reject votes for transactions in ancestor blocks
/// - Uses try_accept_blocks_with_own_votes() to process blocks with reject votes
/// - Verifies that both authorities produce consistent commit sequences
#[tokio::test]
async fn test_randomized_dag_with_reject_votes() {
    let mut random_test_setup = random_test_setup();
    let seed = random_test_setup.seeded_rng.gen_range(0..10000);
    let num_authorities = random_test_setup.seeded_rng.gen_range(4..10);
    let reject_percentage: u8 = random_test_setup.seeded_rng.gen_range(0..30); // 0-30% reject rate
    let num_transactions: u32 = 5; // transactions per block
    let include_leader_percentage = 70;

    tracing::info!(
        "Running test with committee size {num_authorities}, {NUM_ROUNDS} rounds, \
         {num_transactions} txns/block, {reject_percentage}% reject rate..."
    );

    let context = CommitTestFixture::context_with_options(num_authorities, 1, Some(5));
    let dag_builder = create_random_dag_with_transactions(
        seed,
        include_leader_percentage,
        NUM_ROUNDS,
        num_transactions,
        context.clone(),
    );
    let all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();

    // Generate random reject votes for this chunk
    let blocks_with_votes = generate_random_reject_votes(
        &all_blocks,
        &mut random_test_setup.seeded_rng,
        reject_percentage,
        num_transactions,
    );

    // Collect finalized commit sequences from each run.
    let mut commit_sequences = vec![];

    for _ in 0..NUM_RUNS {
        // Setup for this iteration of test Authority
        let mut fixture = CommitTestFixture::new(context.clone());

        // Shuffle the blocks with votes for this iteration of test.
        let mut blocks_with_votes = blocks_with_votes.clone();
        blocks_with_votes.shuffle(&mut random_test_setup.seeded_rng);

        let mut finalized_commits = vec![];
        let mut last_decided = Slot::new_for_test(0, 0);
        let mut i = 0;
        while i < all_blocks.len() {
            let chunk_size = random_test_setup
                .seeded_rng
                .gen_range(1..=(all_blocks.len() - i).min(5 * num_authorities as usize));
            let chunk = &blocks_with_votes[i..i + chunk_size];

            // Try accept blocks with own reject votes
            fixture.try_accept_blocks_with_own_votes(chunk.to_vec());

            let sequence = fixture.committer.try_decide(last_decided);

            if !sequence.is_empty() {
                let finalized = fixture.process_commits(sequence.clone()).await;
                finalized_commits.extend(finalized);
                let leader_status = sequence.last().unwrap();
                last_decided = Slot::new(leader_status.round(), leader_status.authority());
            }

            i += chunk_size;
        }

        commit_sequences.push(finalized_commits);
    }

    assert_commit_sequences_match(commit_sequences);
}

/// Generate random reject votes for blocks.
/// For each block, randomly select some transactions from its ancestors to reject.
fn generate_random_reject_votes(
    blocks: &[VerifiedBlock],
    rng: &mut StdRng,
    reject_percentage: u8,
    num_transactions_per_block: u32,
) -> Vec<(VerifiedBlock, Vec<TransactionIndex>)> {
    blocks
        .iter()
        .map(|block| {
            let mut reject_votes = vec![];
            // For each ancestor, randomly decide if we reject some of its transactions
            for ancestor in block.ancestors() {
                if ancestor.round == 0 {
                    continue; // Skip genesis blocks
                }
                for txn_idx in 0..num_transactions_per_block {
                    if rng.gen_range(0..100) < reject_percentage {
                        reject_votes.push(txn_idx as TransactionIndex);
                    }
                }
            }
            (block.clone(), reject_votes)
        })
        .collect()
}

/// Create a random DAG with transactions in each block.
fn create_random_dag_with_transactions(
    seed: u64,
    include_leader_percentage: u64,
    num_rounds: Round,
    num_transactions: u32,
    context: Arc<Context>,
) -> DagBuilder {
    assert!(
        (0..=100).contains(&include_leader_percentage),
        "include_leader_percentage must be in the range 0..100"
    );

    let mut rng = StdRng::seed_from_u64(seed);
    let mut dag_builder = DagBuilder::new(context);

    for r in 1..=num_rounds {
        let random_num = rng.gen_range(0..100);
        let include_leader = random_num <= include_leader_percentage;
        dag_builder
            .layer(r)
            .num_transactions(num_transactions)
            .rejected_transactions_pct(30, None)
            .min_ancestor_links(include_leader, None); // terminal - must be last
    }

    dag_builder
}

/// Compare commit sequences across all runs, asserting they are identical.
/// Returns the last commit sequence for additional assertions if needed.
fn assert_commit_sequences_match(
    mut commit_sequences: Vec<Vec<CommittedSubDag>>,
) -> Vec<CommittedSubDag> {
    let last_commit_sequence = commit_sequences.pop().unwrap();

    for (run, commit_sequence) in commit_sequences.into_iter().enumerate() {
        assert_eq!(
            commit_sequence.len(),
            last_commit_sequence.len(),
            "Commit sequence length mismatch at run {run}"
        );
        for (commit_index, (c1, c2)) in commit_sequence
            .iter()
            .zip(last_commit_sequence.iter())
            .enumerate()
        {
            assert_eq!(
                c1.leader, c2.leader,
                "Leader mismatch at commit {commit_index}"
            );
            assert_eq!(
                c1.commit_ref, c2.commit_ref,
                "Commit sequence mismatch at commit {commit_index}"
            );
            assert_eq!(
                c1.rejected_transactions_by_block, c2.rejected_transactions_by_block,
                "Rejected transactions mismatch at commit {commit_index}"
            );
        }
    }

    let mut total_transactions = 0;
    let mut rejected_transactions = 0;
    let mut reject_votes = 0;
    let mut blocks = 4;
    for commit in last_commit_sequence.iter() {
        total_transactions += commit
            .blocks
            .iter()
            .map(|block| block.transactions().len())
            .sum::<usize>();
        rejected_transactions += commit
            .rejected_transactions_by_block
            .values()
            .map(|transactions| transactions.len())
            .sum::<usize>();
        reject_votes += commit
            .blocks
            .iter()
            .map(|block| block.transaction_votes().len())
            .sum::<usize>();
        blocks += commit.blocks.len();
    }
    tracing::info!(
        "Finished comparing commit sequences. Commits: {}, Blocks: {}, Total transactions: {}, Rejected transactions: {}, Reject votes: {}",
        last_commit_sequence.len(),
        blocks,
        total_transactions,
        rejected_transactions,
        reject_votes
    );

    last_commit_sequence
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
