// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod consensus_tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::{sync::Arc, time::Duration};

    use consensus_config::{
        Authority, AuthorityIndex, AuthorityKeyPair, Committee, Epoch, NetworkKeyPair,
        ProtocolKeyPair, Stake,
    };
    use consensus_core::NoopTransactionVerifier;
    use consensus_core::{BlockAPI, BlockStatus, TransactionVerifier, ValidationError};
    use consensus_simtests::node::{AuthorityNode, Config};
    use consensus_types::block::TransactionIndex;
    use mysten_metrics::RegistryService;
    use mysten_network::Multiaddr;
    use prometheus::Registry;
    use rand::{rngs::StdRng, Rng, SeedableRng as _};
    use sui_config::local_ip_utils;
    use sui_macros::sim_test;
    use sui_protocol_config::ProtocolConfig;
    use sui_simulator::{
        configs::{bimodal_latency_ms, env_config, uniform_latency_ms},
        SimConfig,
    };
    use tempfile::TempDir;
    use tokio::task::JoinSet;
    use tokio::time::{sleep, timeout};
    use typed_store::DBMetrics;

    fn test_config() -> SimConfig {
        env_config(
            uniform_latency_ms(10..20),
            [
                (
                    "regional_high_variance",
                    bimodal_latency_ms(30..40, 300..800, 0.01),
                ),
                (
                    "global_high_variance",
                    bimodal_latency_ms(60..80, 500..1500, 0.01),
                ),
            ],
        )
    }

    #[sim_test(config = "test_config()")]
    async fn test_committee_start_simple() {
        telemetry_subscribers::init_for_testing();
        let db_registry = Registry::new();
        DBMetrics::init(RegistryService::new(db_registry));
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_consensus_gc_depth_for_testing(3);
            config
        });

        const NUM_OF_AUTHORITIES: usize = 10;
        let (committee, keypairs) = local_committee_and_keys(0, [1; NUM_OF_AUTHORITIES].to_vec());
        let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();

        let mut authorities = Vec::with_capacity(committee.size());
        let mut transaction_clients = Vec::with_capacity(committee.size());
        let mut boot_counters = [0; NUM_OF_AUTHORITIES];
        let mut clock_drifts = [0; NUM_OF_AUTHORITIES];
        clock_drifts[0] = 50;
        clock_drifts[1] = 100;
        clock_drifts[2] = 120;

        for (authority_index, _authority_info) in committee.authorities() {
            // Introduce a non-trivial clock drift for the first node (it's time will be ahead of the others). This will provide extra reassurance
            // around the block timestamp checks.
            let config = Config {
                authority_index,
                db_dir: Arc::new(TempDir::new().unwrap()),
                committee: committee.clone(),
                keypairs: keypairs.clone(),
                network_type: sui_protocol_config::ConsensusNetwork::Tonic,
                boot_counter: boot_counters[authority_index],
                protocol_config: protocol_config.clone(),
                clock_drift: clock_drifts[authority_index.value() as usize],
                transaction_verifier: Arc::new(NoopTransactionVerifier {}),
            };
            let node = AuthorityNode::new(config);

            if authority_index != AuthorityIndex::new_for_test(NUM_OF_AUTHORITIES as u32 - 1) {
                node.start().await.unwrap();
                node.spawn_committed_subdag_consumer().unwrap();

                let client = node.transaction_client();
                transaction_clients.push(client);
            }

            boot_counters[authority_index] += 1;
            authorities.push(node);
        }

        let transaction_clients_clone = transaction_clients.clone();
        let _handle = tokio::spawn(async move {
            const NUM_TRANSACTIONS: u16 = 1000;

            for i in 0..NUM_TRANSACTIONS {
                let txn = vec![i as u8; 16];
                transaction_clients_clone[i as usize % transaction_clients_clone.len()]
                    .submit(vec![txn])
                    .await
                    .unwrap();
            }
        });

        // wait for authorities
        sleep(Duration::from_secs(60)).await;

        // Now start the last authority.
        tracing::info!(authority =% NUM_OF_AUTHORITIES - 1, "Starting authority and waiting for it to catch up");
        authorities[NUM_OF_AUTHORITIES - 1].start().await.unwrap();
        authorities[NUM_OF_AUTHORITIES - 1]
            .spawn_committed_subdag_consumer()
            .unwrap();

        // Wait for it to catch up
        sleep(Duration::from_secs(230)).await;
        let commit_consumer_monitor = authorities[NUM_OF_AUTHORITIES - 1].commit_consumer_monitor();
        let highest_committed_index = commit_consumer_monitor.highest_handled_commit();
        assert!(
            highest_committed_index >= 80,
            "Highest handled commit {highest_committed_index} < 80"
        );
    }

    // Tests the fastpath transactions with randomized votes. The test creates a fixed number of transactions,
    // sends them to random authorities, and randomizes votes on them (accept or reject). The output is verified
    // by comparing commits across validators and ensuring they are consistent.
    #[sim_test(config = "test_config()")]
    async fn test_committee_fast_path() {
        telemetry_subscribers::init_for_testing();
        let db_registry = Registry::new();
        DBMetrics::init(RegistryService::new(db_registry));
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_consensus_gc_depth_for_testing(3);
            config
        });

        const NUM_OF_AUTHORITIES: usize = 10;
        const REJECTION_PROBABILITY: f64 = 0.1;
        const NUM_TRANSACTIONS: u16 = 10000;
        const MAX_TRANSACTIONS_BATCH_SIZE: u16 = 8;

        let (committee, keypairs) = local_committee_and_keys(0, [1; NUM_OF_AUTHORITIES].to_vec());
        let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();

        let mut authorities = Vec::with_capacity(committee.size());
        let mut transaction_clients = Vec::with_capacity(committee.size());
        let mut boot_counters = [0; NUM_OF_AUTHORITIES];
        let mut clock_drifts = [0; NUM_OF_AUTHORITIES];
        clock_drifts[0] = 50;
        clock_drifts[1] = 100;
        clock_drifts[2] = 120;

        // Initialize consensus authorities and transaction clients.
        for (authority_index, _authority_info) in committee.authorities() {
            // Introduce clock drifts for the first three nodes (their time will be ahead of the others).
            // This will provide extra reassurance around the block timestamp checks.
            let config = Config {
                authority_index,
                db_dir: Arc::new(TempDir::new().unwrap()),
                committee: committee.clone(),
                keypairs: keypairs.clone(),
                network_type: sui_protocol_config::ConsensusNetwork::Tonic,
                boot_counter: boot_counters[authority_index],
                protocol_config: protocol_config.clone(),
                clock_drift: clock_drifts[authority_index.value() as usize],
                transaction_verifier: Arc::new(RandomizedTransactionVerifier::new(
                    REJECTION_PROBABILITY,
                )),
            };
            let node = AuthorityNode::new(config);
            node.start().await.unwrap();
            node.spawn_committed_subdag_consumer().unwrap();

            let client = node.transaction_client();
            transaction_clients.push(client);

            boot_counters[authority_index] += 1;
            authorities.push(node);
        }

        // Initialize commit consumers.
        let mut commit_consumer_receivers = vec![];
        for authority in &authorities {
            commit_consumer_receivers.push(authority.commit_consumer_receiver());
        }

        let mut join_set = JoinSet::new();
        let mut transaction_index = 0;
        let total_sequenced_transactions = Arc::new(AtomicU64::new(0));
        let total_garbage_collected_transactions = Arc::new(AtomicU64::new(0));

        loop {
            // randomly decide the number of transactions to submit.
            // We are taking advantage of the batching capabilities of transaction client to
            // make sure that we submit more than one transaction per block.
            let num_of_transactions = rand::thread_rng().gen_range(0..=MAX_TRANSACTIONS_BATCH_SIZE);
            let transaction_indexes_range =
                transaction_index..transaction_index + num_of_transactions;
            let mut transactions = vec![];

            for i in transaction_indexes_range.clone() {
                transactions.push(vec![i as u8; 16]);
                transaction_index += 1;
            }

            let index =
                (transaction_index - num_of_transactions) as usize % transaction_clients.len();
            let (_block_ref, _indexes, status_waiter) = transaction_clients[index]
                .submit(transactions)
                .await
                .unwrap();

            join_set.spawn({
                let total_sequenced_transactions = total_sequenced_transactions.clone();
                let total_garbage_collected_transactions =
                    total_garbage_collected_transactions.clone();
                async move {
                    match status_waiter.await {
                        Ok(BlockStatus::Sequenced(_)) => {
                            // Just increment the transaction count
                            total_sequenced_transactions
                                .fetch_add(num_of_transactions as u64, Ordering::SeqCst);
                        }
                        Ok(BlockStatus::GarbageCollected(_)) => {
                            total_garbage_collected_transactions
                                .fetch_add(num_of_transactions as u64, Ordering::SeqCst);
                        }
                        Err(e) => {
                            panic!(
                                "Transactions in range {:?} failed with error: {e}",
                                transaction_indexes_range
                            );
                        }
                    }
                }
            });

            let sleep_duration = Duration::from_millis(rand::thread_rng().gen_range(1..100));
            sleep(sleep_duration).await;

            // Exit when we have submitted the defined number of transactions.
            if transaction_index as u16 >= NUM_TRANSACTIONS {
                break;
            }
        }

        // Wait for submission transactions to finish.
        while let Some(result) = join_set.join_next().await {
            result.unwrap();
        }

        // Iterate over the committed sub dags until all the transactions have been committed on finalized sub dags.
        let mut transaction_count = 0;
        let mut total_rejected_transactions = 0;
        loop {
            let mut sub_dags = vec![];
            let mut last_sub_dag_commit_ref = None;

            // We attempt to gather all the committed sub das per authority one at a time. A correctly working authority should output the same sequence. Since the
            // underlying used channel is unbounded we won't have issues we dropped sub dags.
            for authority_index in 0..NUM_OF_AUTHORITIES {
                tracing::trace!("Waiting for sub dag from authority {authority_index}");
                if let Some(sub_dag) = timeout(
                    Duration::from_secs(90),
                    commit_consumer_receivers[authority_index].recv(),
                )
                .await
                .expect("Timeout waiting for subdag")
                {
                    if let Some(last_sub_dag_commit_ref) = last_sub_dag_commit_ref {
                        assert_eq!(last_sub_dag_commit_ref, sub_dag.commit_ref);
                    } else {
                        last_sub_dag_commit_ref = Some(sub_dag.commit_ref);
                    }

                    sub_dags.push(sub_dag);
                } else {
                    panic!("Commit consumer for authority {authority_index} closed.");
                }
            }

            tracing::info!(
                "Received {} sub dags for commit {:?}",
                sub_dags.len(),
                last_sub_dag_commit_ref.unwrap()
            );

            // Ensure the rejected transactions match across the sub dags.
            let first_sub_dag = sub_dags[0].clone();
            total_rejected_transactions += first_sub_dag
                .rejected_transactions_by_block
                .iter()
                .map(|(_, rejected_transactions)| rejected_transactions.len())
                .sum::<usize>();

            for sub_dag in sub_dags.iter().skip(1) {
                assert_eq!(
                    first_sub_dag.rejected_transactions_by_block,
                    sub_dag.rejected_transactions_by_block
                );
            }

            // Now pick the first sub dag and count all the transactions included in it.
            transaction_count += first_sub_dag
                .blocks
                .iter()
                .map(|block| block.transactions().len())
                .sum::<usize>();

            // Exit when we have confirmed that all the sequenced transactions have been processed.
            if transaction_count as u64 >= total_sequenced_transactions.load(Ordering::SeqCst) {
                break;
            }
        }

        tracing::info!("Total committed transactions: {}", transaction_count);
        tracing::info!(
            "Total sequenced transactions: {}",
            total_sequenced_transactions.load(Ordering::SeqCst)
        );
        tracing::info!(
            "Total garbage collected transactions: {}",
            total_garbage_collected_transactions.load(Ordering::SeqCst)
        );
        tracing::info!(
            "Total rejected transactions: {}",
            total_rejected_transactions
        );
    }
    /// Creates a committee for local testing, and the corresponding key pairs for the authorities.
    pub fn local_committee_and_keys(
        epoch: Epoch,
        authorities_stake: Vec<Stake>,
    ) -> (Committee, Vec<(NetworkKeyPair, ProtocolKeyPair)>) {
        let mut authorities = vec![];
        let mut key_pairs = vec![];
        let mut rng = StdRng::from_seed([0; 32]);
        for (i, stake) in authorities_stake.into_iter().enumerate() {
            let authority_keypair = AuthorityKeyPair::generate(&mut rng);
            let protocol_keypair = ProtocolKeyPair::generate(&mut rng);
            let network_keypair = NetworkKeyPair::generate(&mut rng);
            authorities.push(Authority {
                stake,
                address: get_available_local_address(),
                hostname: format!("test_host_{i}").to_string(),
                authority_key: authority_keypair.public(),
                protocol_key: protocol_keypair.public(),
                network_key: network_keypair.public(),
            });
            key_pairs.push((network_keypair, protocol_keypair));
        }

        let committee = Committee::new(epoch, authorities);
        (committee, key_pairs)
    }

    fn get_available_local_address() -> Multiaddr {
        let ip = local_ip_utils::get_new_ip();

        local_ip_utils::new_udp_address_for_testing(&ip)
    }

    // Transaction verifier with randomized voting.
    struct RandomizedTransactionVerifier {
        rejection_probability: f64,
    }

    impl RandomizedTransactionVerifier {
        fn new(rejection_probability: f64) -> Self {
            Self {
                rejection_probability,
            }
        }
    }

    impl TransactionVerifier for RandomizedTransactionVerifier {
        fn verify_batch(&self, _transactions: &[&[u8]]) -> Result<(), ValidationError> {
            Ok(())
        }

        fn verify_and_vote_batch(
            &self,
            batch: &[&[u8]],
        ) -> Result<Vec<TransactionIndex>, ValidationError> {
            let mut rejected_indices = vec![];

            // Randomly decide which transactions to reject according to the rejection probability.
            for (index, _transaction) in batch.iter().enumerate() {
                let rejected = rand::thread_rng().gen_bool(self.rejection_probability);
                if rejected {
                    tracing::trace!("Rejecting transaction {index}");
                    rejected_indices.push(index as TransactionIndex);
                }
            }

            Ok(rejected_indices)
        }
    }
}
