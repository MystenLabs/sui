// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests commit logic with randomized DAGs containing reject votes.

#[cfg(msim)]
mod consensus_dag_tests {
    use consensus_core::{CommitTestFixture, RandomDag, Slot, assert_commit_sequences_match};
    use consensus_types::block::Round;
    use rand::{SeedableRng as _, rngs::StdRng};
    use sui_macros::sim_test;

    #[sim_test]
    async fn test_randomized_dag_with_4_authorities() {
        test_randomized_dag_with_reject_votes(RandomizedDagTestConfig {
            num_runs: 100,
            num_authorities: 4,
            num_rounds: 6000,
            reject_percentage: 10,
        })
        .await;
    }

    #[sim_test]
    async fn test_randomized_dag_with_7_authorities() {
        test_randomized_dag_with_reject_votes(RandomizedDagTestConfig {
            num_runs: 100,
            num_authorities: 7,
            num_rounds: 2000,
            reject_percentage: 5,
        })
        .await;
    }

    struct RandomizedDagTestConfig {
        num_runs: usize,
        num_authorities: usize,
        num_rounds: Round,
        reject_percentage: u8,
    }

    async fn test_randomized_dag_with_reject_votes(config: RandomizedDagTestConfig) {
        let RandomizedDagTestConfig {
            num_runs,
            num_authorities,
            num_rounds,
            reject_percentage,
        } = config;

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

        for i in 0..num_runs {
            tracing::info!("Run {i} of randomized test...");
            let mut fixture = CommitTestFixture::new(context.clone());
            let mut finalized_commits = vec![];
            let mut last_decided = Slot::new_for_test(0, 0);

            for block in dag.random_iter(&mut rng) {
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
