// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests commit logic with randomized DAGs containing reject votes.

#[cfg(msim)]
mod consensus_dag_tests {
    use consensus_core::{CommitTestFixture, RandomDag, Slot, assert_commit_sequences_match};
    use consensus_types::block::Round;
    use rand::{SeedableRng as _, rngs::StdRng};
    use sui_macros::sim_test;

    const NUM_RUNS: u32 = 100;
    const MAX_STEP: u32 = 3;

    #[sim_test]
    async fn test_randomized_dag_with_4_authorities() {
        test_randomized_dag_with_reject_votes(4, 6000, 10).await;
    }

    #[sim_test]
    async fn test_randomized_dag_with_7_authorities() {
        test_randomized_dag_with_reject_votes(7, 2000, 5).await;
    }

    async fn test_randomized_dag_with_reject_votes(
        num_authorities: usize,
        num_rounds: Round,
        reject_percentage: u8,
    ) {
        let mut rng = StdRng::from_entropy();
        let num_transactions: u32 = 5;

        tracing::info!(
            "Running randomized test with {num_authorities} authorities, {num_rounds} rounds, \
             {num_transactions} txns/block, {reject_percentage}% reject rate...",
        );

        let context = CommitTestFixture::context_with_options(num_authorities, 0, Some(6));
        let dag = RandomDag::new(
            context.clone(),
            &mut rng,
            num_rounds,
            num_transactions,
            reject_percentage,
        );

        // Collect finalized commit sequences from each run.
        let mut commit_sequences = vec![];

        for _ in 0..NUM_RUNS {
            let mut fixture = CommitTestFixture::new(context.clone());
            let mut finalized_commits = vec![];
            let mut last_decided = Slot::new_for_test(0, 0);

            for block in dag.random_iter(&mut rng, MAX_STEP) {
                fixture.try_accept_blocks(vec![block]);

                let (finalized, new_last_decided) = fixture.try_commit(last_decided).await;
                finalized_commits.extend(finalized);
                last_decided = new_last_decided;
            }

            commit_sequences.push(finalized_commits);
        }

        assert_commit_sequences_match(commit_sequences);
    }
}
