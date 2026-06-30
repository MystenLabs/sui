// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod consensus_tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::{sync::Arc, time::Duration};

    use consensus_config::{
        Authority, AuthorityIndex, AuthorityName, Committee, ConsensusProtocolConfig, Epoch,
        NetworkKeyPair, Parameters, ProtocolKeyPair, Stake,
    };
    use consensus_core::NoopTransactionVerifier;
    use consensus_core::{BlockAPI, BlockStatus, TransactionVerifier, ValidationError};
    use consensus_simtests::node::{AuthorityNode, Config};
    use consensus_types::block::{BlockRef, TransactionIndex};
    use fastcrypto::traits::{KeyPair as _, ToFromBytes as _};
    use mysten_metrics::RegistryService;
    use mysten_network::{Multiaddr, multiaddr::Protocol};
    use prometheus::Registry;
    use rand::{Rng, SeedableRng as _, rngs::StdRng};
    use sui_config::local_ip_utils;
    use sui_macros::sim_test;
    use sui_simulator::{
        SimConfig,
        configs::{bimodal_latency_ms, env_config, uniform_latency_ms},
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
        const NUM_OF_AUTHORITIES: usize = 10;
        let (committee, keypairs) = local_committee_and_keys(0, [1; NUM_OF_AUTHORITIES].to_vec());
        let mut protocol_config = ConsensusProtocolConfig::for_testing();
        protocol_config.set_gc_depth_for_testing(3);

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
            let db_dir = Arc::new(TempDir::new().unwrap());
            let mut params = default_parameters();
            params.db_path = db_dir.path().to_path_buf();

            let config = Config {
                authority_index,
                db_dir,
                committee: committee.clone(),
                keypairs: keypairs.clone(),
                boot_counter: boot_counters[authority_index],
                protocol_config: protocol_config.clone(),
                clock_drift: clock_drifts[authority_index.value() as usize],
                transaction_verifier: Arc::new(NoopTransactionVerifier {}),
                parameters: params,
                observer_network_keypair: None,
                observer_ip: None,
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
        const NUM_OF_AUTHORITIES: usize = 10;
        const REJECTION_PROBABILITY: f64 = 0.1;
        const NUM_TRANSACTIONS: u16 = 10000;
        const MAX_TRANSACTIONS_BATCH_SIZE: u16 = 8;

        let (committee, keypairs) = local_committee_and_keys(0, [1; NUM_OF_AUTHORITIES].to_vec());
        let mut protocol_config = ConsensusProtocolConfig::for_testing();
        protocol_config.set_gc_depth_for_testing(3);

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
            let db_dir = Arc::new(TempDir::new().unwrap());
            let mut params = default_parameters();
            params.db_path = db_dir.path().to_path_buf();

            let config = Config {
                authority_index,
                db_dir,
                committee: committee.clone(),
                keypairs: keypairs.clone(),
                boot_counter: boot_counters[authority_index],
                protocol_config: protocol_config.clone(),
                clock_drift: clock_drifts[authority_index.value() as usize],
                transaction_verifier: Arc::new(RandomizedTransactionVerifier::new(
                    REJECTION_PROBABILITY,
                )),
                parameters: params,
                observer_network_keypair: None,
                observer_ip: None,
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

    /// Multi-node test for the `enforce_transaction_vote_dependencies` feature with the flag ON.
    ///
    /// When the flag is ON, a block's transaction-vote targets become additional acceptance
    /// dependencies in `BlockManager`. In honest operation a block's vote targets are always within
    /// its causal history (votes are derived from `link_causal_history` of the block's ancestors),
    /// so the targets are present by the time the voting block is accepted and the enforcement is a
    /// no-op for liveness. This test stands up a live, all-honest committee and asserts:
    ///   (a) consensus stays LIVE: commits advance well past genesis on every authority;
    ///   (b) blocks carrying transaction-votes on present targets are accepted, and the enforcement
    ///       processes real vote targets (not a trivially-empty set): the deterministic verifier
    ///       rejects half the transactions, so finalized commits carry reject votes -- the V2
    ///       transaction-votes whose targets BlockManager treats as acceptance dependencies;
    ///   (d) parity: the flag-ON run commits the SAME set of transactions as an identical flag-OFF
    ///       control run (the enforcement neither drops nor stalls any transaction).
    ///
    /// The deterministic suspend -> arrive -> unsuspend path (c) is covered by the
    /// `enforce_transaction_vote_dependencies_suspends_then_unsuspends` unit test in
    /// `consensus/core/src/block_manager.rs`, which can withhold a specific vote target; in a live
    /// honest committee a vote target is never absent (see above), so that path cannot be forced
    /// here without injecting Byzantine behavior.
    #[sim_test(config = "test_config()")]
    async fn test_enforce_transaction_vote_dependencies_liveness_and_parity() {
        telemetry_subscribers::init_for_testing();
        let db_registry = Registry::new();
        DBMetrics::init(RegistryService::new(db_registry));

        // A fixed, deterministic workload so the committed transaction set is well-defined and can
        // be compared across the flag-OFF and flag-ON runs.
        const NUM_OF_AUTHORITIES: usize = 4;
        const NUM_TRANSACTIONS: u16 = 400;

        // Control run with the flag OFF, then the run under test with the flag ON.
        let (off_committed, off_reject_voted_blocks) =
            run_vote_dependency_workload(NUM_OF_AUTHORITIES, NUM_TRANSACTIONS, false).await;
        let (on_committed, on_reject_voted_blocks) =
            run_vote_dependency_workload(NUM_OF_AUTHORITIES, NUM_TRANSACTIONS, true).await;

        // (d) Parity: both runs commit the full submitted transaction set, and the two committed
        // sets are identical. Enforcement neither drops nor stalls any transaction.
        let expected: std::collections::BTreeSet<Vec<u8>> = (0..NUM_TRANSACTIONS)
            .map(vote_dependency_transaction)
            .collect();
        assert_eq!(
            off_committed, expected,
            "flag-OFF control should commit every submitted transaction"
        );
        assert_eq!(
            on_committed, expected,
            "flag-ON run should commit every submitted transaction"
        );
        assert_eq!(
            on_committed, off_committed,
            "flag-ON and flag-OFF runs must commit the same transaction set (parity)"
        );

        // (b) The enforcement path is exercised on real targets, not trivially bypassed: the
        // deterministic verifier rejects half the transactions, so finalized commits carry reject
        // votes. Reject votes are the V2 transaction-votes whose targets BlockManager treats as
        // acceptance dependencies under the flag; a positive count proves the flag-ON run processed
        // real, present vote targets end-to-end rather than an empty vote set.
        assert!(
            on_reject_voted_blocks > 0,
            "flag-ON run should commit blocks with reject votes so the enforcement path processes \
             real vote targets"
        );
        // Sanity: the control run also produced reject votes, confirming the workload exercises V2
        // transaction-voting in both configurations.
        assert!(
            off_reject_voted_blocks > 0,
            "flag-OFF control should also produce reject votes"
        );
    }

    /// The deterministic payload for transaction `i` in the vote-dependency workload.
    fn vote_dependency_transaction(i: u16) -> Vec<u8> {
        let mut txn = vec![0u8; 16];
        txn[0..2].copy_from_slice(&i.to_le_bytes());
        txn
    }

    /// Runs a live committee with `enforce_transaction_vote_dependencies` set to `enforce`, submits
    /// a fixed deterministic set of `num_transactions`, and drains finalized commits until all of
    /// them are committed.
    ///
    /// Returns the set of committed transaction payloads and the number of committed blocks whose
    /// transactions received reject votes. (a) Liveness is asserted internally: every authority must
    /// advance commits, and the drain must complete within the timeout.
    async fn run_vote_dependency_workload(
        num_of_authorities: usize,
        num_transactions: u16,
        enforce: bool,
    ) -> (std::collections::BTreeSet<Vec<u8>>, usize) {
        // Each transaction whose first byte is odd is deterministically rejected, so V2 blocks carry
        // reject votes on real, present targets in both the flag-ON and flag-OFF runs.
        let (committee, keypairs) = local_committee_and_keys(0, vec![1; num_of_authorities]);
        let mut protocol_config = ConsensusProtocolConfig::for_testing();
        // Use a generous GC depth and pace submissions (below) so that, in this all-honest
        // committee, every submitted transaction is committed on a finalized subdag rather than
        // garbage-collected. This makes the "commit the full submitted set" parity check reliable.
        protocol_config.set_gc_depth_for_testing(30);
        // transaction_voting_enabled is already true in for_testing(), so V2 transaction-votes are
        // produced. Opt this run into the enforcement under test.
        protocol_config.set_enforce_transaction_vote_dependencies_for_testing(enforce);
        assert_eq!(
            protocol_config.enforce_transaction_vote_dependencies(),
            enforce
        );
        assert!(
            protocol_config.transaction_voting_enabled(),
            "V2 transaction-votes must be enabled for this test"
        );

        let mut authorities = Vec::with_capacity(committee.size());
        let mut transaction_clients = Vec::with_capacity(committee.size());

        for (authority_index, _authority_info) in committee.authorities() {
            let db_dir = Arc::new(TempDir::new().unwrap());
            let mut params = default_parameters();
            params.db_path = db_dir.path().to_path_buf();

            let config = Config {
                authority_index,
                db_dir,
                committee: committee.clone(),
                keypairs: keypairs.clone(),
                boot_counter: 0,
                protocol_config: protocol_config.clone(),
                clock_drift: 0,
                transaction_verifier: Arc::new(DeterministicRejectVerifier {}),
                parameters: params,
                observer_network_keypair: None,
                observer_ip: None,
            };
            let node = AuthorityNode::new(config);
            node.start().await.unwrap();
            node.spawn_committed_subdag_consumer().unwrap();
            transaction_clients.push(node.transaction_client());
            authorities.push(node);
        }

        let mut commit_consumer_receivers = vec![];
        for authority in &authorities {
            commit_consumer_receivers.push(authority.commit_consumer_receiver());
        }

        // Submit the fixed deterministic workload, round-robin across authorities.
        let transaction_clients_clone = transaction_clients.clone();
        let submit_handle = tokio::spawn(async move {
            for i in 0..num_transactions {
                let txn = vote_dependency_transaction(i);
                transaction_clients_clone[i as usize % transaction_clients_clone.len()]
                    .submit(vec![txn])
                    .await
                    .unwrap();
                // Pace submissions so blocks are committed before they age out of the GC window.
                if i % 8 == 0 {
                    sleep(Duration::from_millis(50)).await;
                }
            }
        });
        submit_handle.await.unwrap();

        // Drain finalized commits from every authority until all submitted transactions are
        // committed, cross-checking that authorities agree on the commit sequence (safety) as we go.
        let mut committed: std::collections::BTreeSet<Vec<u8>> = std::collections::BTreeSet::new();
        // Count blocks whose transactions received reject votes. Reject votes are exactly the V2
        // transaction-votes whose targets the enforcement path treats as acceptance dependencies, so
        // a positive count proves the enforcement processed real, present vote targets end-to-end
        // (and was not trivially bypassed). We read the public `rejected_transactions_by_block`
        // rather than the (crate-private) `transaction_votes()` slice type.
        let mut reject_voted_blocks = 0usize;
        let mut highest_commit_index = 0u32;
        while committed.len() < num_transactions as usize {
            let mut last_sub_dag_commit_ref = None;
            for receiver in commit_consumer_receivers.iter_mut() {
                let sub_dag = timeout(Duration::from_secs(90), receiver.recv())
                    .await
                    .expect("Timeout waiting for subdag draining commits")
                    .expect("Commit consumer closed unexpectedly");

                // Safety: all authorities must output the same commit at each index.
                if let Some(expected_ref) = last_sub_dag_commit_ref {
                    assert_eq!(expected_ref, sub_dag.commit_ref);
                } else {
                    last_sub_dag_commit_ref = Some(sub_dag.commit_ref);
                    highest_commit_index = sub_dag.commit_ref.index;
                    reject_voted_blocks += sub_dag
                        .rejected_transactions_by_block
                        .values()
                        .filter(|rejected| !rejected.is_empty())
                        .count();
                    for block in &sub_dag.blocks {
                        for txn in block.transactions() {
                            committed.insert(txn.data().to_vec());
                        }
                    }
                }
            }
        }

        // (a) Liveness: commits advanced well past genesis.
        assert!(
            highest_commit_index > 1,
            "consensus should advance commits beyond genesis (highest index {highest_commit_index})"
        );

        for authority in authorities {
            authority.stop();
        }

        (committed, reject_voted_blocks)
    }

    // Test with multiple Observer nodes in a chain
    #[sim_test(config = "test_config()")]
    async fn test_observer_chain_connectivity() {
        telemetry_subscribers::init_for_testing();
        let db_registry = Registry::new();
        DBMetrics::init(RegistryService::new(db_registry));

        const NUM_OF_AUTHORITIES: usize = 4;
        const NUM_TRANSACTIONS: u16 = 300;

        // Create committee and validators
        let (committee, keypairs) = local_committee_and_keys(0, [1; NUM_OF_AUTHORITIES].to_vec());
        let mut protocol_config = ConsensusProtocolConfig::for_testing();
        protocol_config.set_gc_depth_for_testing(5);

        let mut authorities = Vec::with_capacity(committee.size());
        let mut transaction_clients = Vec::with_capacity(committee.size());
        let mut boot_counters = [0; NUM_OF_AUTHORITIES];

        // Start validators with observer server support enabled
        // Note: In production, validators would need observer server ports configured
        for (authority_index, _) in committee.authorities() {
            let db_dir = Arc::new(TempDir::new().unwrap());
            let mut parameters = default_parameters();
            parameters.db_path = db_dir.path().to_path_buf();
            parameters.observer.server_port = Some(9600 + authority_index.value() as u16);

            let config = Config {
                authority_index,
                db_dir,
                committee: committee.clone(),
                keypairs: keypairs.clone(),
                boot_counter: boot_counters[authority_index],
                protocol_config: protocol_config.clone(),
                clock_drift: 0,
                transaction_verifier: Arc::new(NoopTransactionVerifier {}),
                parameters,
                observer_network_keypair: None,
                observer_ip: None,
            };
            let node = AuthorityNode::new(config);
            node.start().await.unwrap();
            node.spawn_committed_subdag_consumer().unwrap();
            transaction_clients.push(node.transaction_client());
            boot_counters[authority_index] += 1;
            authorities.push(node);
        }

        // Pre-allocate IPs for Observer nodes so we can configure them properly
        let observer1_ip = local_ip_utils::get_new_ip();
        let observer2_ip = local_ip_utils::get_new_ip();

        // Create Observer 1 - connects to validator
        let observer1_dir = Arc::new(TempDir::new().unwrap());
        let observer1_keypair =
            NetworkKeyPair::generate(&mut rand::rngs::StdRng::from_seed([42; 32]));

        // Configure Observer 1 to connect to validator 0's observer server port
        let validator_index = AuthorityIndex::new_for_test(0);
        let validator_info = committee.authority(validator_index);

        // Extract IP from validator's address and swap port to observer port
        let validator_observer_port = 9600 + validator_index.value() as u16;
        let validator_observer_address =
            replace_port_in_multiaddr(&validator_info.address, validator_observer_port)
                .expect("Failed to create observer address");

        let mut observer1_params = default_parameters();
        observer1_params.db_path = observer1_dir.path().to_path_buf();
        observer1_params.observer = consensus_config::ObserverParameters {
            // Enable observer server on Observer 1 (port 9610) so Observer 2 can connect
            server_port: Some(9610),
            // Connect to validator 0's observer server port
            peers: vec![consensus_config::PeerRecord {
                public_key: validator_info.network_key.clone(),
                address: validator_observer_address,
            }],
            ..Default::default()
        };

        tracing::info!(
            "Starting Observer 1 (connects to validator) with IP {}",
            observer1_ip
        );

        // Create Observer 1 using AuthorityNode with pre-allocated IP
        let observer1_config = Config {
            authority_index: AuthorityIndex::new_for_test(100), // Use a high index for Observer
            db_dir: observer1_dir,
            committee: committee.clone(),
            keypairs: keypairs.clone(), // Observer won't use these
            boot_counter: 0,
            protocol_config: protocol_config.clone(),
            clock_drift: 0,
            transaction_verifier: Arc::new(NoopTransactionVerifier {}),
            parameters: observer1_params,
            observer_network_keypair: Some(observer1_keypair.clone()),
            observer_ip: Some(observer1_ip.clone()), // Pass the pre-allocated IP
        };

        let observer1 = AuthorityNode::new(observer1_config);
        observer1.start().await.unwrap();
        observer1.spawn_committed_subdag_consumer().unwrap();
        let observer1_monitor = observer1.commit_consumer_monitor();

        // Create Observer 2 - connects to Observer 1
        let observer2_dir = Arc::new(TempDir::new().unwrap());
        let observer2_keypair =
            NetworkKeyPair::generate(&mut rand::rngs::StdRng::from_seed([99; 32]));

        // Create an address for Observer 1's observer server using the pre-allocated IP
        let observer1_address = format!("/ip4/{}/udp/9610", observer1_ip).parse().unwrap();

        let mut observer2_params = default_parameters();
        observer2_params.db_path = observer2_dir.path().to_path_buf();
        observer2_params.observer = consensus_config::ObserverParameters {
            // Configure to connect to Observer 1's observer server at port 9610
            peers: vec![consensus_config::PeerRecord {
                public_key: observer1_keypair.public().clone(),
                address: observer1_address,
            }],
            ..Default::default()
        };

        tracing::info!(
            "Starting Observer 2 (connects to Observer 1) with IP {}",
            observer2_ip
        );

        // Create Observer 2 using AuthorityNode with pre-allocated IP
        let observer2_config = Config {
            authority_index: AuthorityIndex::new_for_test(101), // Use a different high index for Observer 2
            db_dir: observer2_dir,
            committee: committee.clone(),
            keypairs: keypairs.clone(), // Observer won't use these
            boot_counter: 0,
            protocol_config: protocol_config.clone(),
            clock_drift: 0,
            transaction_verifier: Arc::new(NoopTransactionVerifier {}),
            parameters: observer2_params,
            observer_network_keypair: Some(observer2_keypair.clone()),
            observer_ip: Some(observer2_ip.clone()), // Pass the pre-allocated IP
        };

        let observer2 = AuthorityNode::new(observer2_config);
        observer2.start().await.unwrap();
        observer2.spawn_committed_subdag_consumer().unwrap();
        let observer2_monitor = observer2.commit_consumer_monitor();

        // Wait for all nodes to establish connections
        tracing::info!("Waiting for nodes to establish connections...");
        sleep(Duration::from_secs(5)).await;

        // Submit transactions from validators
        tracing::info!("Submitting {} transactions", NUM_TRANSACTIONS);
        for i in 0..NUM_TRANSACTIONS {
            let txn = vec![i as u8; 16];
            transaction_clients[i as usize % transaction_clients.len()]
                .submit(vec![txn])
                .await
                .unwrap();
            if i % 50 == 0 {
                sleep(Duration::from_millis(200)).await;
            }
        }

        // Wait for processing and syncing
        tracing::info!("Waiting for processing and syncing...");
        sleep(Duration::from_secs(15)).await;

        // Check progress of all nodes
        let validator_commits = authorities[0]
            .commit_consumer_monitor()
            .highest_handled_commit();
        let observer1_commits = observer1_monitor.highest_handled_commit();
        let observer2_commits = observer2_monitor.highest_handled_commit();

        // Validators should definitely make progress
        assert!(
            validator_commits > 10,
            "Validators should make significant progress"
        );

        // Give observers a chance to sync by checking if they're making progress
        // Note: Observers may take time to sync, so we check for any progress
        const MAX_COMMIT_DIFFERENCE: u32 = 10;
        assert!(
            validator_commits - observer1_commits <= MAX_COMMIT_DIFFERENCE,
            "Observer 1 should make progress"
        );
        assert!(
            validator_commits - observer2_commits <= MAX_COMMIT_DIFFERENCE,
            "Observer 2 should make progress"
        );

        // Clean up
        observer1.stop();
        observer2.stop();
        for authority in authorities {
            authority.stop();
        }
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
            let authority_keypair =
                fastcrypto::bls12381::min_sig::BLS12381KeyPair::generate(&mut rng);
            let protocol_keypair = ProtocolKeyPair::generate(&mut rng);
            let network_keypair = NetworkKeyPair::generate(&mut rng);
            authorities.push(Authority {
                stake,
                address: get_available_local_address(),
                hostname: format!("test_host_{i}").to_string(),
                authority_name: AuthorityName::from_bytes(authority_keypair.public().as_bytes()),
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

    // Helper function to create default parameters with custom db_path
    fn default_parameters() -> Parameters {
        consensus_simtests::node::default_parameters()
    }

    // Helper function to replace the port in a Multiaddr
    fn replace_port_in_multiaddr(addr: &Multiaddr, new_port: u16) -> Result<Multiaddr, String> {
        let mut iter = addr.iter();
        match (iter.next(), iter.next()) {
            (Some(Protocol::Ip4(ipaddr)), Some(Protocol::Udp(_))) => {
                Ok(format!("/ip4/{}/udp/{}", ipaddr, new_port).parse().unwrap())
            }
            (Some(Protocol::Ip6(ipaddr)), Some(Protocol::Udp(_))) => {
                Ok(format!("/ip6/{}/udp/{}", ipaddr, new_port).parse().unwrap())
            }
            (Some(Protocol::Dns(hostname)), Some(Protocol::Udp(_))) => {
                Ok(format!("/dns/{}/udp/{}", hostname, new_port)
                    .parse()
                    .unwrap())
            }
            _ => Err(format!("Unsupported multiaddr format: {}", addr)),
        }
    }

    // Transaction verifier that votes deterministically by transaction content: a transaction is
    // rejected iff its first byte is odd. Determinism keeps the produced reject votes identical
    // across the flag-OFF and flag-ON runs, which is required for the parity assertion.
    struct DeterministicRejectVerifier {}

    impl TransactionVerifier for DeterministicRejectVerifier {
        fn verify_batch(&self, _transactions: &[&[u8]]) -> Result<(), ValidationError> {
            Ok(())
        }

        fn verify_and_vote_batch(
            &self,
            _block_ref: &BlockRef,
            batch: &[&[u8]],
        ) -> Result<Vec<TransactionIndex>, ValidationError> {
            let mut rejected_indices = vec![];
            for (index, transaction) in batch.iter().enumerate() {
                if transaction.first().is_some_and(|b| b % 2 == 1) {
                    rejected_indices.push(index as TransactionIndex);
                }
            }
            Ok(rejected_indices)
        }
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
            _block_ref: &BlockRef,
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
