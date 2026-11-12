// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod consensus_tests {
    use std::{
        collections::{BTreeMap, BTreeSet, HashMap},
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicU64, Ordering},
        },
        time::Duration,
    };

    use consensus_config::{
        Authority, AuthorityIndex, AuthorityName, Committee, ConsensusProtocolConfig, Epoch,
        NetworkKeyPair, Parameters, ProtocolKeyPair, Stake,
    };
    use consensus_core::NoopTransactionVerifier;
    use consensus_core::{
        BlockAPI, BlockStatus, CommitConsumerArgs, CommitIndex, CommitRef, CommittedSubDag,
        Priority, TransactionVerifier, ValidationError, storage::Store,
    };
    use consensus_simtests::node::{AuthorityNode, Config};
    use consensus_types::block::{BlockRef, BlockTimestampMs, Round, TransactionIndex};
    use fastcrypto::traits::{KeyPair as _, ToFromBytes as _};
    use mysten_metrics::RegistryService;
    use mysten_metrics::monitored_mpsc::UnboundedReceiver;
    use mysten_network::{Multiaddr, multiaddr::Protocol};
    use parking_lot::Mutex;
    use prometheus::Registry;
    use rand::{Rng, SeedableRng as _, rngs::StdRng, seq::SliceRandom as _};
    use sui_config::local_ip_utils;
    use sui_macros::{clear_fail_point, register_fail_points, sim_test};
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
    async fn test_consensus_tonic_equivocation() {
        consensus_tonic_equivocation(false).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_consensus_tonic_equivocation_v3() {
        consensus_tonic_equivocation(true).await;
    }

    async fn consensus_tonic_equivocation(v3: bool) {
        telemetry_subscribers::init_for_testing();
        DBMetrics::init(RegistryService::new(Registry::new()));
        // V3 needs six equal-stake authorities to tolerate one Byzantine authority.
        let num_authorities = if v3 { 6 } else { 4 };
        let (committee, keypairs) = local_committee_and_keys(0, vec![1; num_authorities]);
        let faulty = AuthorityIndex::new_for_test(num_authorities as u32 - 1);
        let mut protocol_config = ConsensusProtocolConfig::for_testing();
        protocol_config.set_enable_v3_for_testing(v3);
        let committee = if v3 {
            Committee::new_v3(
                committee.epoch(),
                committee.authorities_slice().to_vec(),
                1,
                0,
            )
        } else {
            protocol_config.set_gc_depth_for_testing(3);
            committee
        };
        assert!(committee.stake(faulty) < committee.validity_threshold());
        assert!(committee.total_stake() - committee.stake(faulty) >= committee.quorum_threshold());

        let addresses = [
            committee.authority(faulty).address.clone(),
            get_available_local_address(),
        ];
        let mut routes = (0..num_authorities - 1)
            .map(|index| usize::from(index >= num_authorities / 2))
            .collect::<Vec<_>>();
        routes.shuffle(&mut rand::thread_rng());
        tracing::info!(v3, %faulty, ?routes, ?addresses, "Starting two instances of one authority");

        let mut honest = Vec::new();
        let mut receivers = Vec::new();
        // Start honest nodes first, so their address overrides are in place before
        // either faulty instance can accept a subscription.
        for (index, instance) in routes.iter().copied().enumerate() {
            let node = build_node(
                &committee,
                &keypairs,
                &protocol_config,
                AuthorityIndex::new_for_test(index as u32),
                0,
                Arc::new(ReplayTransactionVerifier),
                |_| {},
            )
            .with_peer_address(faulty, addresses[instance].clone());
            node.start().await.unwrap();
            node.spawn_committed_subdag_consumer().unwrap();
            receivers.push(node.commit_consumer_receiver());
            honest.push(node);
        }

        let mut twins = Vec::new();
        for address in &addresses {
            let node = build_node(
                &committee,
                &keypairs,
                &protocol_config,
                faulty,
                0,
                Arc::new(ReplayTransactionVerifier),
                |parameters| {
                    parameters.listen_address_override = Some(address.clone());
                    parameters.internal.skip_equivocation_validation = true;
                    // Each instance starts its own history. Normal recovery would
                    // first retrieve the other instance's last proposed block.
                    parameters.sync_last_known_own_block_timeout = Duration::ZERO;
                },
            );
            node.start().await.unwrap();
            node.spawn_committed_subdag_consumer().unwrap();
            twins.push(node);
        }
        assert_ne!(twins[0].sim_node_id(), twins[1].sim_node_id());

        let proposals = Arc::new(Mutex::new([BTreeSet::new(), BTreeSet::new()]));
        let mut twin_load = Vec::new();
        for (instance, node) in twins.iter().enumerate() {
            let client = node.transaction_client();
            let proposals = proposals.clone();
            twin_load.push(tokio::spawn(async move {
                for sequence in 0u64..=u64::MAX {
                    let mut transaction = vec![instance as u8 + 3];
                    transaction.extend_from_slice(&sequence.to_le_bytes());
                    let (block_ref, _, _) = client
                        .submit(vec![transaction], Priority::Normal)
                        .await
                        .unwrap();
                    assert_eq!(block_ref.author, faulty);
                    proposals.lock()[instance].insert(block_ref);
                    sleep(Duration::from_millis(50)).await;
                }
            }));
        }
        let clients = honest
            .iter()
            .map(AuthorityNode::transaction_client)
            .collect::<Vec<_>>();
        let honest_load = tokio::spawn(async move {
            for sequence in 0u64..=u64::MAX {
                clients[sequence as usize % clients.len()]
                    .submit(vec![sequence.to_le_bytes().to_vec()], Priority::Normal)
                    .await
                    .unwrap();
                sleep(Duration::from_millis(50)).await;
            }
        });

        sleep(Duration::from_secs(30)).await;
        let recipients = [
            routes.iter().position(|instance| *instance == 0).unwrap(),
            routes.iter().position(|instance| *instance == 1).unwrap(),
        ];
        timeout(Duration::from_secs(90), async {
            loop {
                let proposals = proposals.lock().clone();
                let mut observed_rounds = BTreeSet::new();
                for first in &proposals[0] {
                    for second in &proposals[1] {
                        if first.round == second.round
                            && first != second
                            && honest[recipients[0]]
                                .store()
                                .contains_blocks(&[*first])
                                .unwrap()[0]
                            && honest[recipients[1]]
                                .store()
                                .contains_blocks(&[*second])
                                .unwrap()[0]
                        {
                            observed_rounds.insert(first.round);
                        }
                    }
                }
                if observed_rounds.len() >= 3
                    && honest
                        .iter()
                        .all(|node| node.commit_consumer_monitor().highest_handled_commit() >= 20)
                {
                    tracing::info!(?observed_rounds, "Honest nodes stored both forks");
                    break;
                }
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect(
            "honest nodes must receive equivocations in three rounds and commit during the attack",
        );

        // The honest authorities form a quorum without the faulty authority.
        // Require new commits after both faulty instances stop, as well as
        // agreement on the full prefix produced during the attack.
        for task in twin_load {
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
        }
        for node in twins {
            node.stop().await;
        }
        let target = honest
            .iter()
            .map(|node| node.commit_consumer_monitor().highest_handled_commit())
            .max()
            .unwrap()
            + 20;
        let mut rejected_transactions = 0;
        timeout(Duration::from_secs(180), async {
            for index in 1..=target {
                let expected = receivers[0].recv().await.unwrap();
                assert_eq!(expected.commit_ref.index, index);
                rejected_transactions += expected
                    .rejected_transactions_by_block
                    .values()
                    .map(Vec::len)
                    .sum::<usize>();
                for receiver in &mut receivers[1..] {
                    let actual = receiver.recv().await.unwrap();
                    assert_eq!(actual.commit_ref, expected.commit_ref);
                    assert_eq!(actual.leader, expected.leader);
                    assert_eq!(actual.timestamp_ms, expected.timestamp_ms);
                    assert_eq!(
                        actual.rejected_transactions_by_block,
                        expected.rejected_transactions_by_block,
                    );
                    assert_eq!(
                        actual
                            .blocks
                            .iter()
                            .map(|block| block.reference())
                            .collect::<Vec<_>>(),
                        expected
                            .blocks
                            .iter()
                            .map(|block| block.reference())
                            .collect::<Vec<_>>(),
                    );
                }
            }
        })
        .await
        .expect("honest authorities must agree and keep committing after equivocation");
        assert!(rejected_transactions > 0);

        honest_load.abort();
        assert!(honest_load.await.unwrap_err().is_cancelled());
        for node in honest {
            node.stop().await;
        }
    }

    #[sim_test(config = "test_config()")]
    async fn test_committee_start_simple() {
        committee_start_simple(false).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_committee_start_simple_v3() {
        committee_start_simple(true).await;
    }

    async fn committee_start_simple(v3: bool) {
        telemetry_subscribers::init_for_testing();
        let db_registry = Registry::new();
        DBMetrics::init(RegistryService::new(db_registry));
        const NUM_OF_AUTHORITIES: usize = 10;
        let (committee, keypairs) = local_committee_and_keys(0, [1; NUM_OF_AUTHORITIES].to_vec());
        let (committee, protocol_config) = crash_fault_test_setup(committee, v3);

        let mut authorities = Vec::with_capacity(committee.size());
        let mut transaction_clients = Vec::with_capacity(committee.size());
        let clock_drifts = test_clock_drifts::<NUM_OF_AUTHORITIES>();

        // Start all authorities except the last one, which is started later to catch up.
        for (authority_index, _authority_info) in committee.authorities() {
            let node = build_node(
                &committee,
                &keypairs,
                &protocol_config,
                authority_index,
                clock_drifts[authority_index],
                Arc::new(NoopTransactionVerifier {}),
                |_| {},
            );

            if authority_index != AuthorityIndex::new_for_test(NUM_OF_AUTHORITIES as u32 - 1) {
                node.start().await.unwrap();
                node.spawn_committed_subdag_consumer().unwrap();

                let client = node.transaction_client();
                transaction_clients.push(client);
            }

            authorities.push(node);
        }

        let transaction_clients_clone = transaction_clients.clone();
        let _handle = tokio::spawn(async move {
            const NUM_TRANSACTIONS: u16 = 1000;

            for i in 0..NUM_TRANSACTIONS {
                let txn = vec![i as u8; 16];
                transaction_clients_clone[i as usize % transaction_clients_clone.len()]
                    .submit(vec![txn], Priority::Normal)
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

    // Like test_committee_start_simple, but injects probabilistic crashes while the committee is
    // under load. At key consensus fail points each node has a small chance to kill itself; the
    // test harness recreates it after a short delay, so the node must recover from its persisted
    // on-disk state. Verifies that every authority recovers and keeps making commit progress.
    #[sim_test(config = "test_config()")]
    async fn test_consensus_crash_and_restart() {
        consensus_crash_and_restart(false).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_consensus_crash_and_restart_v3() {
        consensus_crash_and_restart(true).await;
    }

    async fn consensus_crash_and_restart(v3: bool) {
        telemetry_subscribers::init_for_testing();
        let db_registry = Registry::new();
        DBMetrics::init(RegistryService::new(db_registry));
        const NUM_OF_AUTHORITIES: usize = 10;
        let (committee, keypairs) = local_committee_and_keys(0, [1; NUM_OF_AUTHORITIES].to_vec());
        let (committee, protocol_config) = crash_fault_test_setup(committee, v3);

        // Start the whole committee so crashes hit a fully running network.
        let clock_drifts = test_clock_drifts::<NUM_OF_AUTHORITIES>();
        let authorities = start_committee(
            &committee,
            &keypairs,
            &protocol_config,
            &clock_drifts,
            Arc::new(NoopTransactionVerifier {}),
            |_, _| {},
        )
        .await;

        let crashes = inject_random_crashes(&authorities, Duration::ZERO);

        // Continuously submit transactions while nodes crash and restart, and keep the load
        // running until the final assertions, so commits contain real traffic rather than only
        // empty block proposals.
        let authorities_clone = authorities.clone();
        let load_handle = tokio::spawn(async move {
            let mut transaction_id = 0u64;
            loop {
                let authority =
                    &authorities_clone[transaction_id as usize % authorities_clone.len()];
                if let Some(client) = authority.transaction_client_if_running() {
                    let _ = client
                        .submit(
                            vec![transaction_id.to_le_bytes().to_vec()],
                            Priority::Normal,
                        )
                        .await;
                }
                transaction_id += 1;
                sleep(Duration::from_millis(20)).await;
            }
        });

        // Run the network across many crashes and restarts.
        sleep(Duration::from_secs(120)).await;

        // Ensure the network is still live after crashes stop.
        crashes.stop().await;

        // Because there is no persistence, commit consumers replay from the start of the commit
        // sequence on every restart, so right after a restart an authority's handled commit index
        // climbs from local state alone. 30s is a very generous window to finish replaying before
        // snapshotting commit indexes, so that progress measured afterwards can be attributed to
        // the network.
        sleep(Duration::from_secs(30)).await;

        // 1st snapshot
        let snapshotted_commit_indexes = commit_indexes(&authorities);
        tracing::info!(
            ?snapshotted_commit_indexes,
            "1st snapshot of commit indexes"
        );

        // Run the network for longer.
        sleep(Duration::from_secs(30)).await;

        // Every authority should commit past the 1st snapshot.
        for i in 0..NUM_OF_AUTHORITIES {
            let current_committed_index = authorities[i]
                .commit_consumer_monitor()
                .highest_handled_commit();
            let snapshotted_commit_index = snapshotted_commit_indexes[i];
            assert!(
                current_committed_index > snapshotted_commit_index,
                "Authority {i} made no commit progress beyond its snapshotted state: \
                 {current_committed_index} <= {snapshotted_commit_index}"
            );
        }

        load_handle.abort();
    }

    // Repeatedly restart every validator while the committee is under load. Each pass uses a
    // different random order and leaves each validator down for either no time or a few seconds.
    // Verifies that the final instances all reconnect and make new commit progress.
    #[sim_test(config = "test_config()")]
    async fn test_consensus_rolling_restarts_all_validators() {
        consensus_rolling_restarts_all_validators(false).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_consensus_rolling_restarts_all_validators_v3() {
        consensus_rolling_restarts_all_validators(true).await;
    }

    async fn consensus_rolling_restarts_all_validators(v3: bool) {
        telemetry_subscribers::init_for_testing();
        let db_registry = Registry::new();
        DBMetrics::init(RegistryService::new(db_registry));
        const NUM_OF_AUTHORITIES: usize = 4;
        const NUM_RESTART_ROUNDS: usize = 4;
        let (committee, keypairs) = local_committee_and_keys(0, [1; NUM_OF_AUTHORITIES].to_vec());
        let (committee, protocol_config) = crash_fault_test_setup(committee, v3);

        let authorities = start_committee(
            &committee,
            &keypairs,
            &protocol_config,
            &[0; NUM_OF_AUTHORITIES],
            Arc::new(NoopTransactionVerifier {}),
            |_, _| {},
        )
        .await;

        let load_authorities = authorities.clone();
        let load_handle = tokio::spawn(async move {
            let mut transaction_id = 0u64;
            loop {
                let authority = &load_authorities[transaction_id as usize % load_authorities.len()];
                if let Some(client) = authority.transaction_client_if_running() {
                    let _ = client
                        .submit(
                            vec![transaction_id.to_le_bytes().to_vec()],
                            Priority::Normal,
                        )
                        .await;
                }
                transaction_id += 1;
                sleep(Duration::from_millis(20)).await;
            }
        });

        // Let the initial instances connect and begin committing before rolling the committee.
        sleep(Duration::from_secs(10)).await;
        let mut restart_orders = Vec::with_capacity(NUM_RESTART_ROUNDS);
        for round in 0..NUM_RESTART_ROUNDS {
            let restart_order = {
                let mut rng = rand::thread_rng();
                loop {
                    let mut order = (0..NUM_OF_AUTHORITIES).collect::<Vec<_>>();
                    order.shuffle(&mut rng);
                    if !restart_orders.contains(&order) {
                        break order;
                    }
                }
            };
            tracing::info!(round, ?restart_order, "Starting validator restart round");

            for (position, authority_index) in restart_order.iter().copied().enumerate() {
                let authority = &authorities[authority_index];
                tracing::info!(round, authority_index, "Restarting validator");
                authority.stop().await;

                let downtime_secs = if (round + position) % 2 == 0 {
                    0
                } else {
                    rand::thread_rng().gen_range(1..=3)
                };
                tracing::info!(round, authority_index, downtime_secs, "Validator stopped");
                sleep(Duration::from_secs(downtime_secs)).await;

                authority.start().await.unwrap();
                authority.spawn_committed_subdag_consumer().unwrap();
            }
            restart_orders.push(restart_order);
        }

        // Every validator has now been replaced for the final time, and each final instance starts
        // a fresh commit consumer. Because there is no persistence, commit consumers replay from
        // the start of the commit sequence on every restart, so right after a restart a
        // validator's handled commit index climbs from local state alone. 30s is a very generous
        // window to finish replaying before snapshotting commit indexes, so that progress measured
        // afterwards can be attributed to the network.
        sleep(Duration::from_secs(30)).await;

        // 1st snapshot
        let snapshotted_commit_indexes = commit_indexes(&authorities);
        tracing::info!(
            ?snapshotted_commit_indexes,
            "1st snapshot of commit indexes"
        );

        // Run the network for longer.
        sleep(Duration::from_secs(30)).await;

        // Every validator should commit past the 1st snapshot, proving the fully restarted
        // committee reconnected.
        for (authority_index, authority) in authorities.iter().enumerate() {
            let current_committed_index =
                authority.commit_consumer_monitor().highest_handled_commit();
            let snapshotted_commit_index = snapshotted_commit_indexes[authority_index];
            assert!(
                current_committed_index > snapshotted_commit_index,
                "Authority {authority_index} made no commit progress beyond its snapshotted \
                 state: {current_committed_index} <= {snapshotted_commit_index}"
            );
        }

        load_handle.abort();
    }

    #[sim_test(config = "test_config()")]
    async fn test_full_replay_restart_with_transaction_voting() {
        full_replay_restart(true, false).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_full_replay_restart_without_transaction_voting() {
        full_replay_restart(false, false).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_full_replay_restart_v3() {
        full_replay_restart(true, true).await;
    }

    async fn full_replay_restart(voting: bool, v3: bool) {
        telemetry_subscribers::init_for_testing();
        DBMetrics::init(RegistryService::new(Registry::new()));
        let (committee, keypairs) = local_committee_and_keys(0, vec![1; 4]);
        // This scenario crashes one validator and does not inject equivocation.
        // V3 requires its own certification threshold even when the quorum size
        // matches v2; using v2 thresholds invalidates the v3 commit rules.
        let committee = if v3 {
            let committee = Committee::new_v3(
                committee.epoch(),
                committee.authorities_slice().to_vec(),
                0,
                1,
            );
            assert_eq!(committee.quorum_threshold(), 3);
            assert_eq!(committee.certification_threshold(), 2);
            committee
        } else {
            committee
        };
        let mut protocol_config = if voting {
            ConsensusProtocolConfig::for_testing()
        } else {
            ConsensusProtocolConfig::default()
        };
        assert_eq!(protocol_config.transaction_voting_enabled(), voting);
        protocol_config.set_enable_v3_for_testing(v3);
        if !v3 {
            protocol_config.set_gc_depth_for_testing(3);
        }
        let authorities = start_committee(
            &committee,
            &keypairs,
            &protocol_config,
            &test_clock_drifts::<4>(),
            Arc::new(ReplayTransactionVerifier),
            |_, _| {},
        )
        .await;
        let mut receiver = authorities[0].commit_consumer_receiver();
        let mut peer_receiver = authorities[1].commit_consumer_receiver();
        let clients: Vec<_> = authorities[1..]
            .iter()
            .map(|authority| authority.transaction_client())
            .collect();
        let load = tokio::spawn(async move {
            for id in 0u64..=u64::MAX {
                clients[id as usize % clients.len()]
                    .submit(vec![id.to_le_bytes().to_vec()], Priority::Normal)
                    .await
                    .unwrap();
                sleep(Duration::from_millis(100)).await;
            }
        });

        // Integration tests use the production recovery batch size. Persist
        // more than one batch, and retain actual output as the recovery oracle.
        const BATCH_SIZE: CommitIndex = 250;
        const HISTORY: CommitIndex = BATCH_SIZE + 50;
        let mut original = Vec::new();
        timeout(Duration::from_secs(180), async {
            for index in 1..=HISTORY {
                let commit = receiver.recv().await.unwrap();
                assert_eq!(commit.commit_ref.index, index);
                original.push(commit);
            }
        })
        .await
        .expect("committee must build enough persisted history to span replay batches");
        assert!(original.iter().any(|commit| {
            commit
                .blocks
                .iter()
                .any(|block| !block.transactions().is_empty())
        }));
        if voting {
            assert!(original.iter().any(|commit| {
                commit
                    .rejected_transactions_by_block
                    .values()
                    .any(|rejected| !rejected.is_empty())
            }));
        }
        authorities[0].stop().await;
        drop(receiver);

        let (args, mut receiver) = CommitConsumerArgs::new_with_full_replay();
        let monitor = args.monitor();
        assert_eq!(monitor.replay_target(), None);
        let restarting = authorities[0].clone();
        let startup =
            tokio::spawn(async move { restarting.start_with_commit_consumer(args).await });
        for index in 1..=BATCH_SIZE {
            let commit = timeout(Duration::from_secs(60), receiver.recv())
                .await
                .unwrap()
                .unwrap();
            assert_replayed_commit(&commit, &original[index as usize - 1], voting);
            if index < BATCH_SIZE {
                monitor.set_highest_handled_commit(index);
            }
        }
        let target = monitor.replay_target().unwrap();
        let stored_head = monitor.progress().highest_committed_index;
        assert!(target >= HISTORY);
        assert!(stored_head >= target);
        assert_eq!(monitor.highest_handled_commit(), BATCH_SIZE - 1);
        assert!(
            timeout(Duration::from_secs(1), receiver.recv())
                .await
                .is_err(),
            "the next replay batch must wait for application, even after delivery"
        );
        assert!(
            timeout(
                Duration::from_secs(1),
                monitor.replay_to_consumer_last_processed_commit_complete(),
            )
            .await
            .is_err()
        );
        assert!(!startup.is_finished());
        let peer_monitor = authorities[1].commit_consumer_monitor();
        let peer_head = peer_monitor.highest_handled_commit();
        timeout(Duration::from_secs(30), async {
            while peer_monitor.highest_handled_commit() <= peer_head {
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("the other validators must keep committing while replay is stalled");

        // Kill the simulated process during paced recovery, without graceful
        // ConsensusAuthority::stop, then rebuild from empty application state.
        startup.abort();
        assert!(startup.await.unwrap_err().is_cancelled());
        // Cancellation is observed before the simulator drops the task future.
        // Keep the consumer alive until the node guard has killed its producers.
        assert!(
            timeout(Duration::from_secs(60), receiver.recv())
                .await
                .unwrap()
                .is_none()
        );
        drop(receiver);
        let (args, mut receiver) = CommitConsumerArgs::new_with_full_replay();
        let monitor = args.monitor();
        assert_eq!(monitor.highest_handled_commit(), 0);
        assert_eq!(monitor.replay_target(), None);
        let restarting = authorities[0].clone();
        let startup =
            tokio::spawn(async move { restarting.start_with_commit_consumer(args).await });
        for index in 1..=target {
            let commit = timeout(Duration::from_secs(60), receiver.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(commit.commit_ref.index, index);
            if let Some(expected) = original.get(index as usize - 1) {
                assert_replayed_commit(&commit, expected, voting);
            }
            assert_eq!(monitor.replay_target(), Some(target));
            if index == target {
                assert!(
                    timeout(
                        Duration::from_secs(1),
                        monitor.replay_to_consumer_last_processed_commit_complete(),
                    )
                    .await
                    .is_err(),
                    "readiness must wait for the last application acknowledgement"
                );
                assert!(!startup.is_finished());
            }
            monitor.set_highest_handled_commit(index);
        }
        timeout(
            Duration::from_secs(60),
            monitor.replay_to_consumer_last_processed_commit_complete(),
        )
        .await
        .unwrap();
        timeout(Duration::from_secs(60), startup)
            .await
            .expect("acknowledged replay must reach live consensus")
            .unwrap()
            .unwrap();

        // Submit through the restarted validator only after startup completes.
        // Initial catch-up proposals may be garbage collected; retry those as
        // required by the submission API until a new proposal is sequenced.
        let marker = b"full-replay-live".to_vec();
        let client = authorities[0].transaction_client();
        let submitted_marker = marker.clone();
        let submission = tokio::spawn(async move {
            loop {
                let (_, _, status) = client
                    .submit(vec![submitted_marker.clone()], Priority::Normal)
                    .await
                    .unwrap();
                match status.await.unwrap() {
                    BlockStatus::Sequenced(block) => break block,
                    BlockStatus::GarbageCollected(_) => continue,
                }
            }
        });
        let live_commit = timeout(Duration::from_secs(120), async {
            let mut next_index = target + 1;
            loop {
                let commit = receiver.recv().await.unwrap();
                assert_eq!(commit.commit_ref.index, next_index);
                next_index += 1;
                monitor.set_highest_handled_commit(commit.commit_ref.index);
                if let Some(block) = commit.blocks.iter().find(|block| {
                    commit.commit_ref.index > stored_head
                        && block.author() == authorities[0].index()
                        && block
                            .transactions()
                            .iter()
                            .any(|transaction| transaction.data() == marker.as_slice())
                }) {
                    assert_eq!(submission.await.unwrap(), block.reference());
                    break commit;
                }
            }
        })
        .await
        .expect("restarted validator must propose and commit new blocks");
        let peer_commit = timeout(Duration::from_secs(60), async {
            loop {
                let commit = peer_receiver.recv().await.unwrap();
                if commit.commit_ref.index == live_commit.commit_ref.index {
                    break commit;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(live_commit.commit_ref, peer_commit.commit_ref);
        assert_eq!(
            live_commit.rejected_transactions_by_block,
            peer_commit.rejected_transactions_by_block
        );
        assert_eq!(monitor.replay_target(), Some(target));
        assert!(monitor.progress().highest_committed_index >= live_commit.commit_ref.index);
        load.abort();
        for authority in authorities {
            authority.stop().await;
        }
    }

    fn assert_replayed_commit(
        replayed: &CommittedSubDag,
        original: &CommittedSubDag,
        voting: bool,
    ) {
        assert_eq!(replayed.commit_ref, original.commit_ref);
        assert_eq!(
            replayed
                .blocks
                .iter()
                .map(|block| block.reference())
                .collect::<Vec<_>>(),
            original
                .blocks
                .iter()
                .map(|block| block.reference())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            replayed.rejected_transactions_by_block,
            original.rejected_transactions_by_block
        );
        if voting {
            assert!(replayed.recovered_rejected_transactions);
        }
    }

    struct ReplayTransactionVerifier;

    impl TransactionVerifier for ReplayTransactionVerifier {
        fn verify_batch(
            &self,
            _block_ref: &BlockRef,
            _transactions: &[&[u8]],
        ) -> Result<(), ValidationError> {
            Ok(())
        }

        fn verify_and_vote_batch(
            &self,
            _block_ref: &BlockRef,
            transactions: &[&[u8]],
        ) -> Result<Vec<TransactionIndex>, ValidationError> {
            Ok(transactions
                .iter()
                .enumerate()
                .filter(|(_, transaction)| transaction.first().is_some_and(|byte| byte % 3 == 0))
                .map(|(index, _)| index as TransactionIndex)
                .collect())
        }
    }

    // Restart one validator as a new process without its local store. The new process must
    // recover its last own block from peers before it proposes another block.
    #[sim_test(config = "test_config()")]
    async fn test_consensus_amnesia_recovery() {
        consensus_amnesia_recovery(false).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_consensus_amnesia_recovery_v3() {
        consensus_amnesia_recovery(true).await;
    }

    async fn consensus_amnesia_recovery(v3: bool) {
        telemetry_subscribers::init_for_testing();
        let db_registry = Registry::new();
        DBMetrics::init(RegistryService::new(db_registry));
        const NUM_OF_AUTHORITIES: usize = 4;
        const RESTARTED_AUTHORITY: usize = 0;
        let (committee, keypairs) = local_committee_and_keys(0, [1; NUM_OF_AUTHORITIES].to_vec());
        let (committee, protocol_config) = crash_fault_test_setup(committee, v3);

        let authorities = start_committee(
            &committee,
            &keypairs,
            &protocol_config,
            &[0; NUM_OF_AUTHORITIES],
            Arc::new(NoopTransactionVerifier {}),
            |_, _| {},
        )
        .await;

        sleep(Duration::from_secs(10)).await;
        let authority = &authorities[RESTARTED_AUTHORITY];
        let authority_index = authority.index();
        let store_before_restart = authority.store();

        authority.stop().await;
        let own_round_before_restart = store_before_restart
            .scan_last_blocks_by_author(authority_index, 1, None)
            .unwrap()
            .last()
            .expect("Restarted authority did not propose a block before restart")
            .round();
        assert!(
            own_round_before_restart > 0,
            "Authority {RESTARTED_AUTHORITY} proposed no non-genesis block before restart"
        );

        authority.start_with_empty_store().await.unwrap();
        authority.spawn_committed_subdag_consumer().unwrap();
        let mut commit_receiver = authority.commit_consumer_receiver();

        // The empty-store node first replays old commits. Wait for a committed own block above
        // its highest pre-restart proposal to prove that recovery finished and proposing resumed.
        let committed_own_round_after_restart = timeout(Duration::from_secs(120), async {
            while let Some(subdag) = commit_receiver.recv().await {
                if let Some(round) = subdag.blocks.iter().find_map(|block| {
                    (block.author() == authority_index && block.round() > own_round_before_restart)
                        .then_some(block.round())
                }) {
                    return Some(round);
                }
            }
            None
        })
        .await
        .ok()
        .flatten();
        assert!(
            committed_own_round_after_restart.is_some(),
            "Authority {RESTARTED_AUTHORITY} did not commit an own block above pre-restart round \
             {own_round_before_restart} within 120s after an empty-store restart"
        );
    }

    // Tests consensus transaction voting with randomized votes and random crashes. The test
    // creates a fixed number of transactions, sends them to random authorities, and randomizes
    // votes on them (accept or reject), while authorities randomly crash and restart under the
    // load. The output is verified by comparing commits and transactions across validators and
    // ensuring they are consistent, including commits replayed after crash recovery.
    #[sim_test(config = "test_config()")]
    async fn test_consensus_transaction_votes() {
        consensus_transaction_votes(false).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_consensus_transaction_votes_v3() {
        consensus_transaction_votes(true).await;
    }

    async fn consensus_transaction_votes(v3: bool) {
        telemetry_subscribers::init_for_testing();
        let db_registry = Registry::new();
        DBMetrics::init(RegistryService::new(db_registry));
        const NUM_OF_AUTHORITIES: usize = 10;
        const REJECTION_PROBABILITY: f64 = 0.1;
        const NUM_TRANSACTIONS: u16 = 10000;
        const MAX_TRANSACTIONS_BATCH_SIZE: u16 = 8;

        let (committee, keypairs) = local_committee_and_keys(0, [1; NUM_OF_AUTHORITIES].to_vec());
        let (committee, protocol_config) = crash_fault_test_setup(committee, v3);

        // Initialize consensus authorities and transaction clients.
        let clock_drifts = test_clock_drifts::<NUM_OF_AUTHORITIES>();
        let authorities = start_committee(
            &committee,
            &keypairs,
            &protocol_config,
            &clock_drifts,
            Arc::new(RandomizedTransactionVerifier::new(REJECTION_PROBABILITY)),
            |_, _| {},
        )
        .await;
        // Initialize commit consumers.
        let mut commit_consumer_receivers = vec![];
        for authority in &authorities {
            commit_consumer_receivers.push(authority.commit_consumer_receiver());
        }

        // Space out crashes, so the test finishes within its total time budget even though
        // sequencing all transactions has to run through crash recoveries.
        let crashes = inject_random_crashes(&authorities, Duration::from_secs(30));

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

            let index = (transaction_index - num_of_transactions) as usize % authorities.len();
            // Submit from a separate task: submitting to an authority that is down or
            // recovering can fail or block until the authority can propose again, and should
            // not stall the submission loop. Failed submissions are dropped, and the load
            // keeps flowing through the rest of the committee.
            if let Some(client) = authorities[index].transaction_client_if_running() {
                join_set.spawn({
                    let total_sequenced_transactions = total_sequenced_transactions.clone();
                    let total_garbage_collected_transactions =
                        total_garbage_collected_transactions.clone();
                    async move {
                        let Ok((_block_ref, _indexes, status_waiter)) =
                            client.submit(transactions, Priority::Normal).await
                        else {
                            // The authority crashed while the transactions were being submitted.
                            return;
                        };
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
                                // The authority crashed before the block was sequenced. The fate
                                // of these transactions is unknown, so count them as neither
                                // sequenced nor garbage collected.
                                tracing::info!(
                                    "Status of transactions in range {:?} is unknown: {e}",
                                    transaction_indexes_range
                                );
                            }
                        }
                    }
                });
            }

            let sleep_duration = Duration::from_millis(rand::thread_rng().gen_range(1..100));
            sleep(sleep_duration).await;

            // Exit when we have submitted the defined number of transactions.
            if transaction_index >= NUM_TRANSACTIONS {
                break;
            }
        }

        tracing::info!("Test phase: waiting for transaction statuses");

        // Wait for submission transactions to finish.
        while let Some(result) = join_set.join_next().await {
            result.unwrap();
        }

        tracing::info!(
            "Test phase: stopping crashes. Sequenced {}, garbage collected {}",
            total_sequenced_transactions.load(Ordering::SeqCst),
            total_garbage_collected_transactions.load(Ordering::SeqCst)
        );

        // Stop crashing authorities before comparing commits, so every authority can finish
        // recovering and make progress.
        let completed_crashes = crashes.stop().await;

        tracing::info!("Test phase: comparing commits after {completed_crashes} crashes");

        // Iterate over the committed sub dags until all the transactions have been committed on finalized sub dags.
        let mut transaction_count = 0;
        let mut total_rejected_transactions = 0;
        let mut last_seen_commit_indexes: [CommitIndex; NUM_OF_AUTHORITIES] =
            [0; NUM_OF_AUTHORITIES];
        let mut observed_commits = vec![ObservedCommits::new(); NUM_OF_AUTHORITIES];
        loop {
            let mut sub_dags = vec![];
            let mut last_sub_dag_commit_ref = None;

            // We attempt to gather all the committed sub das per authority one at a time. A correctly working authority should output the same sequence. Since the
            // underlying used channel is unbounded we won't have issues we dropped sub dags.
            for authority_index in 0..NUM_OF_AUTHORITIES {
                tracing::trace!("Waiting for sub dag from authority {authority_index}");
                let sub_dag = timeout(
                    Duration::from_secs(90),
                    next_sub_dag(
                        &authorities[authority_index],
                        &mut commit_consumer_receivers[authority_index],
                        last_seen_commit_indexes[authority_index],
                        &mut observed_commits[authority_index],
                    ),
                )
                .await
                .expect("Timeout waiting for subdag");
                last_seen_commit_indexes[authority_index] = sub_dag.commit_ref.index;

                if let Some(last_sub_dag_commit_ref) = last_sub_dag_commit_ref {
                    assert_eq!(last_sub_dag_commit_ref, sub_dag.commit_ref);
                } else {
                    last_sub_dag_commit_ref = Some(sub_dag.commit_ref);
                }

                sub_dags.push(sub_dag);
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
                .values()
                .map(|rejected_transactions| rejected_transactions.len())
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

    // One validator equivocates in its blocks and in its transaction votes. The v3 thresholds
    // tolerate its stake as Byzantine stake, and tolerate no crashed stake.
    #[sim_test(config = "test_config()")]
    async fn test_consensus_v3_equivocating_validator() {
        consensus_v3_equivocation(EquivocationTestConfig {
            num_authorities: 6,
            malicious_stake: 1,
            crash_stake: 0,
            num_equivocators: 1,
            crash_interval: None,
        })
        .await;
    }

    // Two validators equivocate, which uses the whole Byzantine fault budget.
    #[sim_test(config = "test_config()")]
    async fn test_consensus_v3_two_equivocating_validators() {
        consensus_v3_equivocation(EquivocationTestConfig {
            num_authorities: 11,
            malicious_stake: 2,
            crash_stake: 0,
            num_equivocators: 2,
            crash_interval: None,
        })
        .await;
    }

    // One validator equivocates while simulator nodes randomly crash and recover, one at a time.
    // Crashes hit honest validators and the equivocator's twins.
    #[sim_test(config = "test_config()")]
    async fn test_consensus_v3_equivocating_validator_with_crashes() {
        consensus_v3_equivocation(EquivocationTestConfig {
            num_authorities: 9,
            malicious_stake: 1,
            crash_stake: 1,
            num_equivocators: 1,
            crash_interval: Some(Duration::from_secs(20)),
        })
        .await;
    }

    struct EquivocationTestConfig {
        num_authorities: usize,
        malicious_stake: Stake,
        crash_stake: Stake,
        // The validators with the highest indexes equivocate.
        num_equivocators: usize,
        // When set, injects random crashes with this interval between crashes.
        crash_interval: Option<Duration>,
    }

    // Runs a v3 committee with equivocating validators under transaction load, and checks that
    // honest validators output the same commits and the same rejected transactions.
    //
    // Each honest validator votes with `AcceptMaskTransactionVerifier`, and the twins vote with
    // `twin_accepts()`. So the test knows the largest stake that can accept each transaction. A
    // transaction needs at least a certificate of accept votes to be accepted, and each
    // equivocator counts once. So when the largest accept stake is below the certification
    // threshold, the transaction must be rejected, even when an equivocator accepts it on one or
    // both of its chains.
    async fn consensus_v3_equivocation(config: EquivocationTestConfig) {
        telemetry_subscribers::init_for_testing();
        DBMetrics::init(RegistryService::new(Registry::new()));
        const NUM_HONEST_SUBMISSIONS: u64 = 1500;
        const MAX_TRANSACTIONS_PER_SUBMISSION: u64 = 4;

        let (committee, keypairs) = local_committee_and_keys(0, vec![1; config.num_authorities]);
        let equivocators: Vec<_> = (config.num_authorities - config.num_equivocators
            ..config.num_authorities)
            .map(|index| AuthorityIndex::new_for_test(index as u32))
            .collect();
        let mut protocol_config = ConsensusProtocolConfig::for_testing();
        protocol_config.set_enable_v3_for_testing(true);
        let network = EquivocatingCommittee::start(
            &committee,
            &keypairs,
            &protocol_config,
            equivocators.clone(),
            config.malicious_stake,
            config.crash_stake,
        )
        .await;
        let committee = network.committee.clone();
        let equivocator_stake: Stake = equivocators
            .iter()
            .map(|equivocator| committee.stake(*equivocator))
            .sum();
        // The validity threshold is f + 1, so the equivocators use at most the Byzantine budget.
        assert!(
            equivocator_stake < committee.validity_threshold(),
            "Equivocator stake {equivocator_stake} exceeds the Byzantine fault budget"
        );
        tracing::info!(
            ?equivocators,
            quorum = committee.quorum_threshold(),
            certification = committee.certification_threshold(),
            validity = committee.validity_threshold(),
            "Started v3 committee with equivocating validators"
        );

        // Take the commit receivers before any commit or crash happens.
        let mut commit_receivers: Vec<_> = network
            .honest
            .iter()
            .map(|authority| authority.commit_consumer_receiver())
            .collect();
        let crashes = config
            .crash_interval
            .map(|crash_interval| inject_random_crashes(&network.all_nodes(), crash_interval));

        let honest_mask = network.honest.iter().fold(0u64, |mask, authority| {
            mask | (1 << authority.index().value())
        });
        let total_sequenced_transactions = Arc::new(AtomicU64::new(0));
        let mut honest_submissions = JoinSet::new();
        let mut twin_submissions = JoinSet::new();
        let mut transaction_id = 0u64;
        for submission in 0..NUM_HONEST_SUBMISSIONS {
            let proposer = &network.honest[rand::thread_rng().gen_range(0..network.honest.len())];
            let num_transactions =
                rand::thread_rng().gen_range(1..=MAX_TRANSACTIONS_PER_SUBMISSION);
            let transactions: Vec<_> = (0..num_transactions)
                .map(|_| {
                    transaction_id += 1;
                    let mask = random_accept_mask(honest_mask, proposer.index());
                    mask_transaction(mask, transaction_id)
                })
                .collect();
            if let Some(client) = proposer.transaction_client_if_running() {
                let total_sequenced_transactions = total_sequenced_transactions.clone();
                honest_submissions.spawn(async move {
                    let Ok((_, _, status_waiter)) =
                        client.submit(transactions, Priority::Normal).await
                    else {
                        // The validator crashed during the submission.
                        return;
                    };
                    if let Ok(BlockStatus::Sequenced(_)) = status_waiter.await {
                        total_sequenced_transactions.fetch_add(num_transactions, Ordering::SeqCst);
                    }
                });
            }

            // Each twin includes only its own transactions in its blocks, which identifies the
            // twin of a committed block.
            for (equivocator_position, twins) in network.twins.iter().enumerate() {
                for (twin, node) in twins.iter().enumerate() {
                    let Some(client) = node.transaction_client_if_running() else {
                        continue;
                    };
                    let transaction = mask_transaction(
                        u64::MAX,
                        twin_transaction_id(equivocator_position, twin, submission),
                    );
                    // A twin can stop proposing, so its submissions are never awaited.
                    twin_submissions.spawn(async move {
                        let _ = client.submit(vec![transaction], Priority::Normal).await;
                    });
                }
            }
            let delay = Duration::from_millis(rand::thread_rng().gen_range(1..40));
            sleep(delay).await;
        }

        tracing::info!("Test phase: waiting for transaction statuses");
        while let Some(result) = honest_submissions.join_next().await {
            result.unwrap();
        }
        twin_submissions.abort_all();
        if let Some(crashes) = crashes {
            let completed_crashes = crashes.stop().await;
            tracing::info!("Test phase: stopped crashes after {completed_crashes} crashes");
        }

        let total_sequenced_transactions = total_sequenced_transactions.load(Ordering::SeqCst);
        tracing::info!(
            total_sequenced_transactions,
            "Test phase: comparing commits across honest validators"
        );
        let mut stats = EquivocationStats::default();
        let mut last_seen_commit_indexes = vec![0; network.honest.len()];
        let mut observed_commits = vec![ObservedCommits::new(); network.honest.len()];
        while stats.committed_honest_transactions < total_sequenced_transactions {
            let mut sub_dags = Vec::with_capacity(network.honest.len());
            for (position, authority) in network.honest.iter().enumerate() {
                let sub_dag = timeout(
                    Duration::from_secs(90),
                    next_sub_dag(
                        authority,
                        &mut commit_receivers[position],
                        last_seen_commit_indexes[position],
                        &mut observed_commits[position],
                    ),
                )
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "Timed out waiting for commit {} from authority {}",
                        last_seen_commit_indexes[position] + 1,
                        authority.index()
                    )
                });
                last_seen_commit_indexes[position] = sub_dag.commit_ref.index;
                sub_dags.push(sub_dag);
            }
            for sub_dag in &sub_dags[1..] {
                assert_eq!(sub_dags[0].commit_ref, sub_dag.commit_ref);
                assert_eq!(
                    sub_dags[0].rejected_transactions_by_block,
                    sub_dag.rejected_transactions_by_block,
                    "Rejected transactions diverged in commit {:?}",
                    sub_dag.commit_ref
                );
            }
            stats.record_commit(&sub_dags[0], &committee, &equivocators, equivocator_stake);
        }

        stats.log_and_check(&equivocators);

        // Twins run unmodified commit logic, so their persisted commits must match too.
        let last_commit_index = *stats.commit_refs.keys().last().unwrap();
        for twin in network.twins.iter().flatten() {
            let stored_commits = twin
                .store()
                .scan_commits((1..=last_commit_index).into())
                .unwrap();
            for commit in &stored_commits {
                let commit_ref = commit.reference();
                assert_eq!(
                    commit_ref,
                    stats.commit_refs[&commit_ref.index],
                    "A twin of equivocator {} diverged from the honest commits",
                    twin.index()
                );
            }
            tracing::info!(
                equivocator = %twin.index(),
                stored_commits = stored_commits.len(),
                "Twin commits match the honest commits"
            );
        }

        for node in network.all_nodes() {
            node.stop().await;
        }
    }

    // Returns a random accept mask over the honest validators, for a transaction proposed by
    // `proposer`. The proposer does not vote on its own transactions with its verifier, so it
    // accepts them regardless of the mask.
    fn random_accept_mask(honest_mask: u64, proposer: AuthorityIndex) -> u64 {
        let mut rng = rand::thread_rng();
        let voters: Vec<u32> = (0..u64::BITS)
            .filter(|index| (honest_mask >> index) & 1 == 1 && *index != proposer.value() as u32)
            .collect();
        match rng.gen_range(0..4) {
            // Every honest validator accepts.
            0 => honest_mask,
            // Only the proposer accepts.
            1 => 0,
            // One honest validator rejects, so an equivocator's accept vote can complete a quorum.
            2 => honest_mask & !(1 << voters[rng.gen_range(0..voters.len())]),
            // A random set of honest validators accepts.
            _ => rng.r#gen::<u64>() & honest_mask,
        }
    }

    // A v3 committee where each equivocator runs as two simulator nodes, called twins, with the
    // same keys.
    //
    // Each twin has its own network address. Each honest validator connects to one twin of every
    // equivocator, so honest validators first receive different blocks for the same slot. Block
    // synchronization then fetches the other twin's blocks, so every validator sees both chains.
    // A twin accepts the other twin's blocks for its own slots, and proposes after the highest of
    // them, so the two chains can also cross.
    //
    // The twins vote with `twin_accepts()`. So on the same transaction, the two chains of an
    // equivocator can both accept, both reject, or cast conflicting votes.
    struct EquivocatingCommittee {
        // The committee as seen by validators that connect to the first twins.
        committee: Committee,
        honest: Vec<Arc<AuthorityNode>>,
        // Both twins of each equivocator, in the order of the equivocators.
        twins: Vec<[Arc<AuthorityNode>; 2]>,
    }

    impl EquivocatingCommittee {
        async fn start(
            committee: &Committee,
            keypairs: &[(NetworkKeyPair, ProtocolKeyPair)],
            protocol_config: &ConsensusProtocolConfig,
            equivocators: Vec<AuthorityIndex>,
            malicious_stake: Stake,
            crash_stake: Stake,
        ) -> Self {
            let second_twin_addresses: Vec<_> = equivocators
                .iter()
                .map(|_| get_available_local_address())
                .collect();
            // Bit `k` of a view selects the twin of equivocator `k` that a node connects to.
            // Only the network addresses differ between views, so the thresholds are the same.
            let committee_view = |view: usize| {
                let mut authorities = committee.authorities_slice().to_vec();
                for (position, equivocator) in equivocators.iter().enumerate() {
                    if (view >> position) & 1 == 1 {
                        authorities[equivocator.value()].address =
                            second_twin_addresses[position].clone();
                    }
                }
                Committee::new_v3(committee.epoch(), authorities, malicious_stake, crash_stake)
            };
            let num_views = 1usize << equivocators.len();

            let mut honest = Vec::new();
            for (authority_index, _) in committee.authorities() {
                if equivocators.contains(&authority_index) {
                    continue;
                }
                let node = build_node(
                    &committee_view(honest.len() % num_views),
                    keypairs,
                    protocol_config,
                    authority_index,
                    test_clock_drift(authority_index),
                    Arc::new(AcceptMaskTransactionVerifier { authority_index }),
                    set_fast_commit_sync,
                );
                node.start().await.unwrap();
                node.spawn_committed_subdag_consumer().unwrap();
                honest.push(Arc::new(node));
            }

            let mut twins = Vec::new();
            for &equivocator in &equivocators {
                let pair = [0, 1].map(|twin| {
                    // Each twin must see its own address, and connects to the same twin index of
                    // other equivocators.
                    let view = if twin == 0 { 0 } else { num_views - 1 };
                    Arc::new(build_node(
                        &committee_view(view),
                        keypairs,
                        protocol_config,
                        equivocator,
                        0,
                        Arc::new(TwinTransactionVerifier { twin }),
                        |parameters| {
                            set_fast_commit_sync(parameters);
                            // Twins receive blocks for their own slots from the other twin.
                            parameters.internal.skip_equivocation_validation = true;
                        },
                    ))
                });
                for node in &pair {
                    node.start().await.unwrap();
                    node.spawn_committed_subdag_consumer().unwrap();
                }
                twins.push(pair);
            }

            Self {
                committee: committee_view(0),
                honest,
                twins,
            }
        }

        fn all_nodes(&self) -> Vec<Arc<AuthorityNode>> {
            self.honest
                .iter()
                .chain(self.twins.iter().flatten())
                .cloned()
                .collect()
        }
    }

    // The msim commit sync defaults fetch fewer commits than the committee produces, so a node
    // that restarts behind never catches up. A twin that cannot catch up stops proposing, which
    // ends the equivocation. Use production-like batches, as the observer catch-up test does.
    fn set_fast_commit_sync(parameters: &mut Parameters) {
        parameters.commit_sync_batch_size = 10;
        parameters.max_blocks_per_fetch = 1000;
    }

    // What the honest validators committed in the equivocation tests.
    #[derive(Default)]
    struct EquivocationStats {
        commit_refs: BTreeMap<CommitIndex, CommitRef>,
        committed_equivocator_leaders: BTreeMap<AuthorityIndex, u64>,
        committed_honest_transactions: u64,
        accepted_honest_transactions: u64,
        rejected_honest_transactions: u64,
        // Transactions that cannot reach a certificate of accept votes.
        required_rejections: u64,
        // Committed blocks in each equivocator slot, with the twins whose transactions each block
        // contains. Each twin includes only its own transactions in its blocks.
        equivocator_slot_blocks:
            BTreeMap<(AuthorityIndex, Round), BTreeMap<BlockRef, BTreeSet<usize>>>,
        // Committed transactions submitted to each twin, by (equivocator position, twin).
        committed_twin_transactions: BTreeMap<(usize, usize), u64>,
    }

    impl EquivocationStats {
        fn record_commit(
            &mut self,
            sub_dag: &CommittedSubDag,
            committee: &Committee,
            equivocators: &[AuthorityIndex],
            equivocator_stake: Stake,
        ) {
            self.commit_refs
                .insert(sub_dag.commit_ref.index, sub_dag.commit_ref);
            if equivocators.contains(&sub_dag.leader.author) {
                *self
                    .committed_equivocator_leaders
                    .entry(sub_dag.leader.author)
                    .or_default() += 1;
            }
            for block in &sub_dag.blocks {
                let author = block.author();
                if equivocators.contains(&author) {
                    let mut twins = BTreeSet::new();
                    for transaction in block.transactions() {
                        let id = transaction_id(transaction.data());
                        let (equivocator_position, twin) = twin_of_transaction(id)
                            .expect("An equivocator block contains an honest transaction");
                        assert_eq!(
                            equivocators[equivocator_position],
                            author,
                            "Block {} contains a transaction of another equivocator",
                            block.reference()
                        );
                        twins.insert(twin);
                        *self
                            .committed_twin_transactions
                            .entry((equivocator_position, twin))
                            .or_default() += 1;
                    }
                    self.equivocator_slot_blocks
                        .entry((author, block.round()))
                        .or_default()
                        .insert(block.reference(), twins);
                    continue;
                }

                let rejected: BTreeSet<TransactionIndex> = sub_dag
                    .rejected_transactions_by_block
                    .get(&block.reference())
                    .into_iter()
                    .flatten()
                    .copied()
                    .collect();
                for (index, transaction) in block.transactions().iter().enumerate() {
                    let mask = accept_mask(transaction.data());
                    let honest_accept_stake: Stake = committee
                        .authorities()
                        .filter(|(authority_index, _)| {
                            !equivocators.contains(authority_index)
                                && (*authority_index == author
                                    || (mask >> authority_index.value()) & 1 == 1)
                        })
                        .map(|(_, authority)| authority.stake)
                        .sum();
                    let id = transaction_id(transaction.data());
                    let equivocator_accept_stake = if (0..2).any(|twin| twin_accepts(twin, id)) {
                        equivocator_stake
                    } else {
                        0
                    };
                    let is_rejected = rejected.contains(&(index as TransactionIndex));
                    if honest_accept_stake + equivocator_accept_stake
                        < committee.certification_threshold()
                    {
                        assert!(
                            is_rejected,
                            "Transaction {index} in block {} of commit {:?} was accepted with at \
                             most {honest_accept_stake} honest accept stake and \
                             {equivocator_accept_stake} equivocator accept stake, below the \
                             certification threshold {}",
                            block.reference(),
                            sub_dag.commit_ref,
                            committee.certification_threshold(),
                        );
                        self.required_rejections += 1;
                    }
                    if is_rejected {
                        self.rejected_honest_transactions += 1;
                    } else {
                        self.accepted_honest_transactions += 1;
                    }
                    self.committed_honest_transactions += 1;
                }
            }
        }

        // Checks that the run covered equivocating blocks and conflicting transaction votes.
        fn log_and_check(&self, equivocators: &[AuthorityIndex]) {
            tracing::info!(
                commits = self.commit_refs.len(),
                committed_equivocator_leaders = ?self.committed_equivocator_leaders,
                committed = self.committed_honest_transactions,
                accepted = self.accepted_honest_transactions,
                rejected = self.rejected_honest_transactions,
                required_rejections = self.required_rejections,
                committed_twin_transactions = ?self.committed_twin_transactions,
                "Committed honest transactions"
            );
            assert!(self.accepted_honest_transactions > 0);
            assert!(self.required_rejections > 0);

            for (position, equivocator) in equivocators.iter().enumerate() {
                let slots: Vec<_> = self
                    .equivocator_slot_blocks
                    .iter()
                    .filter(|((author, _), _)| author == equivocator)
                    .map(|(_, blocks)| blocks)
                    .collect();
                let equivocating_slots = slots.iter().filter(|blocks| blocks.len() > 1).count();
                // These slots have committed blocks from both chains. The two chains cast
                // conflicting votes on a third of the transactions.
                let both_twin_slots = slots
                    .iter()
                    .filter(|blocks| {
                        let twins: BTreeSet<_> = blocks.values().flatten().collect();
                        twins.len() == 2
                    })
                    .count();
                tracing::info!(
                    %equivocator,
                    committed_slots = slots.len(),
                    equivocating_slots,
                    both_twin_slots,
                    "Committed equivocator slots"
                );
                assert!(
                    equivocating_slots > 0,
                    "No slot of equivocator {equivocator} has more than one committed block"
                );
                assert!(
                    both_twin_slots > 0,
                    "No slot of equivocator {equivocator} has committed blocks from both twins"
                );
                for twin in 0..2 {
                    assert!(
                        self.committed_twin_transactions
                            .get(&(position, twin))
                            .is_some_and(|count| *count > 0),
                        "No transaction of twin {twin} of equivocator {equivocator} was committed"
                    );
                }
            }
        }
    }

    // Transactions in the equivocation tests have an accept mask, then an id. Bit `i` of the
    // accept mask means that honest validator `i` votes to accept the transaction.
    fn mask_transaction(accept_mask: u64, id: u64) -> Vec<u8> {
        let mut transaction = accept_mask.to_le_bytes().to_vec();
        transaction.extend_from_slice(&id.to_le_bytes());
        transaction
    }

    fn accept_mask(transaction: &[u8]) -> u64 {
        u64::from_le_bytes(transaction[..8].try_into().unwrap())
    }

    fn transaction_id(transaction: &[u8]) -> u64 {
        u64::from_le_bytes(transaction[8..16].try_into().unwrap())
    }

    // Ids of transactions submitted to twins have the highest bit set, and identify the twin.
    const TWIN_TRANSACTION_ID_FLAG: u64 = 1 << 63;
    const TWIN_TRANSACTION_ID_SHIFT: u32 = 48;

    fn twin_transaction_id(equivocator_position: usize, twin: usize, sequence: u64) -> u64 {
        let twin_tag = (equivocator_position * 2 + twin) as u64;
        TWIN_TRANSACTION_ID_FLAG | (twin_tag << TWIN_TRANSACTION_ID_SHIFT) | sequence
    }

    // Returns the (equivocator position, twin) that a transaction was submitted to.
    fn twin_of_transaction(id: u64) -> Option<(usize, usize)> {
        if id & TWIN_TRANSACTION_ID_FLAG == 0 {
            return None;
        }
        let twin_tag = ((id & !TWIN_TRANSACTION_ID_FLAG) >> TWIN_TRANSACTION_ID_SHIFT) as usize;
        Some((twin_tag / 2, twin_tag % 2))
    }

    // Votes on transactions created by `mask_transaction()`. The validator accepts a transaction
    // only when the transaction's accept mask contains the validator's index.
    struct AcceptMaskTransactionVerifier {
        authority_index: AuthorityIndex,
    }

    impl TransactionVerifier for AcceptMaskTransactionVerifier {
        fn verify_batch(
            &self,
            _block_ref: &BlockRef,
            _transactions: &[&[u8]],
        ) -> Result<(), ValidationError> {
            Ok(())
        }

        fn verify_and_vote_batch(
            &self,
            _block_ref: &BlockRef,
            transactions: &[&[u8]],
        ) -> Result<Vec<TransactionIndex>, ValidationError> {
            Ok(transactions
                .iter()
                .enumerate()
                .filter(|(_, transaction)| {
                    (accept_mask(transaction) >> self.authority_index.value()) & 1 == 0
                })
                .map(|(index, _)| index as TransactionIndex)
                .collect())
        }
    }

    // Returns whether a twin of an equivocator votes to accept a transaction. Both twins accept
    // when `id % 3 == 0`, only the first twin accepts when `id % 3 == 1`, and both twins reject
    // when `id % 3 == 2`. Agreeing twins can expose an equivocator counted twice, and conflicting
    // twins can expose an equivocator counted on both sides.
    fn twin_accepts(twin: usize, id: u64) -> bool {
        match id % 3 {
            0 => true,
            1 => twin == 0,
            _ => false,
        }
    }

    struct TwinTransactionVerifier {
        twin: usize,
    }

    impl TransactionVerifier for TwinTransactionVerifier {
        fn verify_batch(
            &self,
            _block_ref: &BlockRef,
            _transactions: &[&[u8]],
        ) -> Result<(), ValidationError> {
            Ok(())
        }

        fn verify_and_vote_batch(
            &self,
            _block_ref: &BlockRef,
            transactions: &[&[u8]],
        ) -> Result<Vec<TransactionIndex>, ValidationError> {
            Ok(transactions
                .iter()
                .enumerate()
                .filter(|(_, transaction)| !twin_accepts(self.twin, transaction_id(transaction)))
                .map(|(index, _)| index as TransactionIndex)
                .collect())
        }
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

        // Start validators with observer server support enabled
        // Note: In production, validators would need observer server ports configured
        let authorities = start_committee(
            &committee,
            &keypairs,
            &protocol_config,
            &[0; NUM_OF_AUTHORITIES],
            Arc::new(NoopTransactionVerifier {}),
            |authority_index, parameters| {
                parameters.observer.server_port = Some(9600 + authority_index.value() as u16);
            },
        )
        .await;
        let transaction_clients = authorities
            .iter()
            .map(|authority| authority.transaction_client())
            .collect::<Vec<_>>();

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
            peer_address_overrides: BTreeMap::new(),
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
            peer_address_overrides: BTreeMap::new(),
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
                .submit(vec![txn], Priority::Normal)
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
        observer1.stop().await;
        observer2.stop().await;
        for authority in authorities {
            authority.stop().await;
        }
    }

    /// An observer starting mid-run must catch up while its block stream subscription is
    /// suspended on commit lag, and resume the live stream once caught up: commits arrive via
    /// commit sync during catch-up, and via the block stream afterwards.
    #[sim_test(config = "test_config()")]
    async fn test_observer_catchup_with_suspended_block_stream() {
        telemetry_subscribers::init_for_testing();
        let db_registry = Registry::new();
        DBMetrics::init(RegistryService::new(db_registry));

        const NUM_OF_AUTHORITIES: usize = 4;
        const NUM_TRANSACTIONS: u16 = 300;
        // Give commit sync production-like throughput: the msim defaults (batch size 3,
        // 10 blocks per fetch) trigger the intra-fetch chunk pacing on every range and
        // cap pull sync below the commit production rate, so a late observer could never
        // converge, with or without subscription suspension.
        const COMMIT_SYNC_BATCH_SIZE: u32 = 10;
        const MAX_BLOCKS_PER_FETCH: usize = 1000;
        // Validators must be at least this far ahead before the observer starts, so it
        // begins commit-lagging (suspension threshold is COMMIT_SYNC_BATCH_SIZE * 5).
        const CATCHUP_START_COMMITS: u32 = 60;

        let (committee, keypairs) = local_committee_and_keys(0, [1; NUM_OF_AUTHORITIES].to_vec());
        let mut protocol_config = ConsensusProtocolConfig::for_testing();
        protocol_config.set_gc_depth_for_testing(5);

        let mut authorities = Vec::with_capacity(committee.size());
        let mut transaction_clients = Vec::with_capacity(committee.size());

        for (authority_index, _) in committee.authorities() {
            let db_dir = Arc::new(TempDir::new().unwrap());
            let mut parameters = default_parameters();
            parameters.db_path = db_dir.path().to_path_buf();
            parameters.observer.server_port = Some(9600 + authority_index.value() as u16);
            parameters.commit_sync_batch_size = COMMIT_SYNC_BATCH_SIZE;
            parameters.max_blocks_per_fetch = MAX_BLOCKS_PER_FETCH;

            let config = Config {
                authority_index,
                db_dir,
                committee: committee.clone(),
                keypairs: keypairs.clone(),
                boot_counter: 0,
                protocol_config: protocol_config.clone(),
                clock_drift: 0,
                transaction_verifier: Arc::new(NoopTransactionVerifier {}),
                parameters,
                peer_address_overrides: BTreeMap::new(),
                observer_network_keypair: None,
                observer_ip: None,
            };
            let node = AuthorityNode::new(config);
            node.start().await.unwrap();
            node.spawn_committed_subdag_consumer().unwrap();
            transaction_clients.push(node.transaction_client());
            authorities.push(node);
        }

        // Submit transactions and wait until validators build up a commit history for
        // the observer to catch up on.
        tracing::info!("Submitting {} transactions", NUM_TRANSACTIONS);
        for i in 0..NUM_TRANSACTIONS {
            let txn = vec![i as u8; 16];
            transaction_clients[i as usize % transaction_clients.len()]
                .submit(vec![txn], Priority::Normal)
                .await
                .unwrap();
            if i % 50 == 0 {
                sleep(Duration::from_millis(100)).await;
            }
        }
        timeout(Duration::from_secs(120), async {
            while authorities[0]
                .commit_consumer_monitor()
                .highest_handled_commit()
                < CATCHUP_START_COMMITS
            {
                sleep(Duration::from_millis(500)).await;
            }
        })
        .await
        .expect("Validators should reach the catch-up start commit index");
        let catchup_target = authorities[0]
            .commit_consumer_monitor()
            .highest_handled_commit();
        tracing::info!("Starting observer, catch-up target is commit {catchup_target}");

        // Late-start the observer against validator 0.
        let observer_dir = Arc::new(TempDir::new().unwrap());
        let observer_keypair =
            NetworkKeyPair::generate(&mut rand::rngs::StdRng::from_seed([42; 32]));
        let observer_ip = local_ip_utils::get_new_ip();
        let validator_info = committee.authority(AuthorityIndex::new_for_test(0));
        let validator_observer_address = replace_port_in_multiaddr(&validator_info.address, 9600)
            .expect("Failed to create observer address");

        let mut observer_params = default_parameters();
        observer_params.db_path = observer_dir.path().to_path_buf();
        observer_params.commit_sync_batch_size = COMMIT_SYNC_BATCH_SIZE;
        observer_params.max_blocks_per_fetch = MAX_BLOCKS_PER_FETCH;
        observer_params.observer = consensus_config::ObserverParameters {
            peers: vec![consensus_config::PeerRecord {
                public_key: validator_info.network_key.clone(),
                address: validator_observer_address,
            }],
            ..Default::default()
        };

        let observer_config = Config {
            authority_index: AuthorityIndex::new_for_test(100),
            db_dir: observer_dir,
            committee: committee.clone(),
            keypairs: keypairs.clone(),
            boot_counter: 0,
            protocol_config: protocol_config.clone(),
            clock_drift: 0,
            transaction_verifier: Arc::new(NoopTransactionVerifier {}),
            parameters: observer_params,
            peer_address_overrides: BTreeMap::new(),
            observer_network_keypair: Some(observer_keypair),
            observer_ip: Some(observer_ip),
        };
        let observer = AuthorityNode::new(observer_config);
        observer.start().await.unwrap();
        observer.spawn_committed_subdag_consumer().unwrap();
        let observer_monitor = observer.commit_consumer_monitor();

        // The observer must catch up to the target even with its block stream suspended
        // during commit sync.
        timeout(Duration::from_secs(180), async {
            while observer_monitor.highest_handled_commit() < catchup_target {
                sleep(Duration::from_millis(500)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "Observer did not catch up to commit {} in time, reached {}",
                catchup_target,
                observer_monitor.highest_handled_commit(),
            )
        });

        // After catching up, the observer must keep up via the resumed block stream.
        tracing::info!("Submitting more transactions to verify the observer keeps up");
        for i in 0..NUM_TRANSACTIONS {
            let txn = vec![i as u8; 16];
            transaction_clients[i as usize % transaction_clients.len()]
                .submit(vec![txn], Priority::Normal)
                .await
                .unwrap();
            if i % 50 == 0 {
                sleep(Duration::from_millis(100)).await;
            }
        }
        sleep(Duration::from_secs(10)).await;
        let validator_commits = authorities[0]
            .commit_consumer_monitor()
            .highest_handled_commit();
        let observer_commits = observer_monitor.highest_handled_commit();
        const MAX_COMMIT_DIFFERENCE: u32 = 10;
        assert!(
            validator_commits.saturating_sub(observer_commits) <= MAX_COMMIT_DIFFERENCE,
            "Observer should keep up with validators after catch-up: {} vs {}",
            observer_commits,
            validator_commits,
        );

        // Clean up
        observer.stop().await;
        for authority in authorities {
            authority.stop().await;
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

    // Clock drifts for the first few nodes (their time will be ahead of the others), providing
    // extra reassurance around the block timestamp checks.
    fn test_clock_drifts<const N: usize>() -> [BlockTimestampMs; N] {
        std::array::from_fn(|index| test_clock_drift(AuthorityIndex::new_for_test(index as u32)))
    }

    fn test_clock_drift(authority_index: AuthorityIndex) -> BlockTimestampMs {
        match authority_index.value() {
            0 => 50,
            1 => 100,
            2 => 120,
            _ => 0,
        }
    }

    // Returns the committee and protocol config for a test that injects only crash faults.
    //
    // V2 runs with GC depth 3. V3 keeps its testing GC depth, and uses v3 committee thresholds
    // that tolerate one crashed validator. Committees of at least 9 validators also tolerate one
    // Byzantine validator, like the production fault budget with equal Byzantine and crash stake.
    fn crash_fault_test_setup(
        committee: Committee,
        v3: bool,
    ) -> (Committee, ConsensusProtocolConfig) {
        let mut protocol_config = ConsensusProtocolConfig::for_testing();
        if !v3 {
            protocol_config.set_gc_depth_for_testing(3);
            return (committee, protocol_config);
        }
        protocol_config.set_enable_v3_for_testing(true);
        let (malicious_stake, crash_stake) = if committee.size() >= 9 {
            (1, 1)
        } else {
            (0, 1)
        };
        let committee = Committee::new_v3(
            committee.epoch(),
            committee.authorities_slice().to_vec(),
            malicious_stake,
            crash_stake,
        );
        let max_stake = committee
            .authorities()
            .map(|(_, authority)| authority.stake)
            .max()
            .unwrap();
        assert!(
            committee.quorum_threshold() + max_stake <= committee.total_stake(),
            "The v3 committee must tolerate a crashed validator"
        );
        (committee, protocol_config)
    }

    // Builds an AuthorityNode with its own temp DB dir and default parameters, which
    // `configure_params` can adjust (e.g. to enable the observer server).
    fn build_node(
        committee: &Committee,
        keypairs: &[(NetworkKeyPair, ProtocolKeyPair)],
        protocol_config: &ConsensusProtocolConfig,
        authority_index: AuthorityIndex,
        clock_drift: BlockTimestampMs,
        transaction_verifier: Arc<dyn TransactionVerifier>,
        configure_params: impl FnOnce(&mut Parameters),
    ) -> AuthorityNode {
        let db_dir = Arc::new(TempDir::new().unwrap());
        let mut parameters = default_parameters();
        parameters.db_path = db_dir.path().to_path_buf();
        configure_params(&mut parameters);
        AuthorityNode::new(Config {
            authority_index,
            db_dir,
            committee: committee.clone(),
            keypairs: keypairs.to_vec(),
            boot_counter: 0,
            protocol_config: protocol_config.clone(),
            clock_drift,
            transaction_verifier,
            parameters,
            peer_address_overrides: BTreeMap::new(),
            observer_network_keypair: None,
            observer_ip: None,
        })
    }

    // Builds and starts every authority in the committee, and spawns their committed subdag
    // consumers.
    async fn start_committee(
        committee: &Committee,
        keypairs: &[(NetworkKeyPair, ProtocolKeyPair)],
        protocol_config: &ConsensusProtocolConfig,
        clock_drifts: &[BlockTimestampMs],
        transaction_verifier: Arc<dyn TransactionVerifier>,
        configure_params: impl Fn(AuthorityIndex, &mut Parameters),
    ) -> Vec<Arc<AuthorityNode>> {
        let mut authorities = Vec::with_capacity(committee.size());
        for (authority_index, _) in committee.authorities() {
            let node = build_node(
                committee,
                keypairs,
                protocol_config,
                authority_index,
                clock_drifts[authority_index.value()],
                transaction_verifier.clone(),
                |parameters| configure_params(authority_index, parameters),
            );
            node.start().await.unwrap();
            node.spawn_committed_subdag_consumer().unwrap();
            authorities.push(Arc::new(node));
        }
        authorities
    }

    const CRASH_FAIL_POINTS: [&str; 4] = [
        "consensus-store-before-write",
        "consensus-store-after-write",
        "consensus-after-propose",
        "consensus-after-leader-schedule-change",
    ];

    // Handle to random crash injection, started with inject_random_crashes().
    struct RandomCrashes {
        crash_in_progress: Arc<AtomicBool>,
        completed_crashes: Arc<AtomicU64>,
        restart_controller: tokio::task::JoinHandle<()>,
    }

    // Injects random crashes into the committee: at each of the CRASH_FAIL_POINTS a node has a
    // small chance to kill itself (the first fail point hit always crashes), and a controller
    // task recreates the killed node after a short delay, forcing it to recover from its
    // persisted on-disk state. Only one crash is in flight at a time, and `crash_interval` must
    // pass after a restart before the next crash: with Duration::ZERO the fail points fire often
    // enough that some node is always down or restarting, while a longer interval lets the
    // committee run at full strength between crashes.
    fn inject_random_crashes(
        authorities: &[Arc<AuthorityNode>],
        crash_interval: Duration,
    ) -> RandomCrashes {
        let authorities_by_node_id = Arc::new(Mutex::new(
            authorities
                .iter()
                .map(|authority| (authority.sim_node_id(), authority.clone()))
                .collect::<BTreeMap<_, _>>(),
        ));
        let (crash_sender, mut crash_receiver) = tokio::sync::mpsc::unbounded_channel();
        let crash_in_progress = Arc::new(AtomicBool::new(false));
        let completed_crashes = Arc::new(AtomicU64::new(0));

        // The full-node fixture used by test_simulated_load_restarts deletes a stopped simulator
        // node and creates a new one. Do the same here: AuthorityNode owns ConsensusAuthority
        // outside the simulated task, so an automatic simulator restart would leave the old DB
        // and network resources alive in the harness.
        let controller_crash_in_progress = crash_in_progress.clone();
        let controller_completed_crashes = completed_crashes.clone();
        let restart_controller = tokio::spawn(async move {
            while let Some(node_id) = crash_receiver.recv().await {
                sleep(Duration::from_secs(10)).await;
                let authority = authorities_by_node_id
                    .lock()
                    .remove(&node_id)
                    .expect("Unknown simulator node requested a restart");
                authority.stop().await;
                authority.start().await.unwrap();
                authority.spawn_committed_subdag_consumer().unwrap();
                authorities_by_node_id
                    .lock()
                    .insert(authority.sim_node_id(), authority);
                controller_completed_crashes.fetch_add(1, Ordering::Relaxed);
                sleep(crash_interval).await;
                controller_crash_in_progress.store(false, Ordering::Release);
            }
        });

        let failpoint_crash_in_progress = crash_in_progress.clone();
        let failpoint_completed_crashes = completed_crashes.clone();
        register_fail_points(&CRASH_FAIL_POINTS, move || {
            let should_crash = failpoint_completed_crashes.load(Ordering::Relaxed) == 0
                || rand::thread_rng().gen_range(0..100) == 0;
            if should_crash
                && failpoint_crash_in_progress
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
                let node_id = sui_simulator::current_simnode_id();
                tracing::error!(%node_id, "Killing current node");
                crash_sender.send(node_id).unwrap();
                sui_simulator::task::shutdown_current_node();
            }
        });

        RandomCrashes {
            crash_in_progress,
            completed_crashes,
            restart_controller,
        }
    }

    impl RandomCrashes {
        // Stops injecting crashes, waits for an in-flight restart to complete, and returns the
        // number of injected crashes. Panics if no crash happened.
        async fn stop(self) -> u64 {
            for fail_point in CRASH_FAIL_POINTS {
                clear_fail_point(fail_point);
            }
            while self.crash_in_progress.load(Ordering::Acquire) {
                sleep(Duration::from_secs(1)).await;
            }
            self.restart_controller.abort();
            let completed_crashes = self.completed_crashes.load(Ordering::Relaxed);
            assert!(
                completed_crashes > 0,
                "Test completed without crashing an authority"
            );
            completed_crashes
        }
    }

    // Commits already observed from an authority, keyed by commit index. Used to check that commits
    // replayed after a crash match what the authority output before the crash.
    type ObservedCommits =
        HashMap<CommitIndex, (CommitRef, BTreeMap<BlockRef, Vec<TransactionIndex>>)>;

    // Receives the next committed sub dag from the authority, tolerating crashes and restarts of
    // the authority. When the authority crashes, its commit consumer channel closes: re-acquire
    // the channel created by the restart. After a restart all commits are replayed from the
    // start, so commits at or below `last_seen_commit_index` are skipped, after checking that the
    // replay did not diverge from what was observed before the crash.
    // Must only be called when no crash can start, e.g. after RandomCrashes::stop(), so the
    // restarted authority is guaranteed to have a new commit consumer channel available.
    async fn next_sub_dag(
        authority: &AuthorityNode,
        receiver: &mut UnboundedReceiver<CommittedSubDag>,
        last_seen_commit_index: CommitIndex,
        observed_commits: &mut ObservedCommits,
    ) -> CommittedSubDag {
        loop {
            let Some(sub_dag) = receiver.recv().await else {
                *receiver = authority.commit_consumer_receiver();
                continue;
            };
            if sub_dag.commit_ref.index <= last_seen_commit_index {
                // Commit indexes are contiguous and delivered in order, so every commit at or
                // below `last_seen_commit_index` has been observed and recorded.
                let (commit_ref, rejected_transactions_by_block) = observed_commits
                    .get(&sub_dag.commit_ref.index)
                    .expect("Commits at or below last_seen_commit_index must have been observed");
                // Commit contents are recovered from the commit store, so the replayed commit
                // must be identical.
                assert_eq!(
                    *commit_ref, sub_dag.commit_ref,
                    "Replayed commit at index {} diverged from the commit observed before the \
                     crash",
                    sub_dag.commit_ref.index
                );
                // A commit only reaches the consumer after CommitFinalizer has persisted its
                // rejected transactions and flushed storage. So a replayed copy of an observed
                // commit must have recovered its rejected transactions from storage, and they
                // must be identical: with the randomized verifier, re-voting instead of reading
                // storage would diverge. (Commits that were unfinalized at crash time are
                // legitimately re-voted during recovery, but those never reached the consumer
                // before the crash and so are never in `observed_commits`.)
                assert!(
                    sub_dag.recovered_rejected_transactions,
                    "Replayed commit {:?} lost its persisted rejected transactions",
                    sub_dag.commit_ref
                );
                assert_eq!(
                    *rejected_transactions_by_block, sub_dag.rejected_transactions_by_block,
                    "Replayed commit {:?} diverged from the rejected transactions observed before \
                     the crash",
                    sub_dag.commit_ref
                );
                continue;
            }
            observed_commits.insert(
                sub_dag.commit_ref.index,
                (
                    sub_dag.commit_ref,
                    sub_dag.rejected_transactions_by_block.clone(),
                ),
            );
            return sub_dag;
        }
    }

    // Commit index handled by each authority.
    //
    // The commit consumer replays the persisted commit sequence from the start on every restart.
    // When this snapshot is taken well after the replays have drained (observed replay duration is
    // in the microseconds, against a drain period of tens of simulated seconds), progress past it
    // can be attributed to the network — committing as part of the committee or commit sync —
    // rather than to replay of local state.
    fn commit_indexes(authorities: &[Arc<AuthorityNode>]) -> Vec<CommitIndex> {
        authorities
            .iter()
            .map(|authority| authority.commit_consumer_monitor().highest_handled_commit())
            .collect()
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
        fn verify_batch(
            &self,
            _block_ref: &BlockRef,
            _transactions: &[&[u8]],
        ) -> Result<(), ValidationError> {
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
