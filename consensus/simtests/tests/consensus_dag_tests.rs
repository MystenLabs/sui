// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests commit logic with randomized DAGs containing reject votes.

#[cfg(msim)]
mod consensus_dag_tests {
    use consensus_config::AuthorityIndex;
    use consensus_core::{
        CommitTestFixture, CommittedSubDag, RandomDag, RandomDagConfig, Slot,
        assert_commit_sequences_match,
    };
    use rand::{SeedableRng as _, rngs::StdRng};
    use sui_macros::sim_test;

    #[sim_test]
    async fn test_randomized_dag_with_4_authorities() {
        let config = RandomDagConfig {
            num_authorities: 4,
            num_rounds: 6000,
            num_transactions: 5,
            reject_percentage: 10,
            equivocators: vec![],
        };
        let commits = test_randomized_dag_with_reject_votes(config, 3, 100).await;
        assert!(
            commits.len() > 1,
            "It should be very unlikely to have only {} commits!",
            commits.len()
        );
    }

    #[sim_test]
    async fn test_randomized_dag_with_7_authorities() {
        let config = RandomDagConfig {
            num_authorities: 7,
            num_rounds: 2000,
            num_transactions: 5,
            reject_percentage: 5,
            equivocators: vec![],
        };
        test_randomized_dag_with_reject_votes(config, 5, 100).await;
    }

    #[sim_test]
    async fn test_randomized_dag_with_4_authorities_1_equivocator() {
        let config = RandomDagConfig {
            num_authorities: 4,
            num_rounds: 6000,
            num_transactions: 5,
            reject_percentage: 10,
            equivocators: vec![(AuthorityIndex::new_for_test(0), 1)],
        };
        let commits = test_randomized_dag_with_reject_votes(config, 3, 100).await;
        assert!(
            commits.len() > 1,
            "It should be very unlikely to have only {} commits!",
            commits.len()
        );
    }

    #[sim_test]
    async fn test_randomized_dag_with_7_authorities_2_equivocators() {
        let config = RandomDagConfig {
            num_authorities: 7,
            num_rounds: 2000,
            num_transactions: 5,
            reject_percentage: 5,
            equivocators: vec![
                (AuthorityIndex::new_for_test(0), 1),
                (AuthorityIndex::new_for_test(1), 1),
            ],
        };
        test_randomized_dag_with_reject_votes(config, 5, 100).await;
    }

    #[sim_test]
    async fn test_randomized_dag_with_10_authorities_3_equivocators() {
        let config = RandomDagConfig {
            num_authorities: 10,
            num_rounds: 2000,
            num_transactions: 5,
            reject_percentage: 5,
            equivocators: vec![(AuthorityIndex::new_for_test(0), 3)],
        };
        test_randomized_dag_with_reject_votes(config, 5, 100).await;
    }

    async fn test_randomized_dag_with_reject_votes(
        config: RandomDagConfig,
        gc_round: u32,
        num_runs: usize,
    ) -> Vec<CommittedSubDag> {
        let mut rng = StdRng::from_entropy();

        tracing::info!(
            "Running randomized test with {} authorities, {} rounds, \
             {} txns/block, {}% reject rate...",
            config.num_authorities,
            config.num_rounds,
            config.num_transactions,
            config.reject_percentage,
        );

        let context =
            CommitTestFixture::context_with_options(config.num_authorities, 0, Some(gc_round));
        let dag = RandomDag::new(context.clone(), &mut rng, config);

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

        assert_commit_sequences_match(commit_sequences)
    }
}
