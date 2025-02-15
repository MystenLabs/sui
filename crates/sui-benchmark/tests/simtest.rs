// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod test {
    use rand::{distributions::uniform::SampleRange, thread_rng, Rng};
    use std::collections::HashSet;
    use std::num::NonZeroUsize;
    use std::path::PathBuf;
    use std::str::FromStr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use sui_benchmark::bank::BenchmarkBank;
    use sui_benchmark::system_state_observer::SystemStateObserver;
    use sui_benchmark::workloads::adversarial::AdversarialPayloadCfg;
    use sui_benchmark::workloads::expected_failure::ExpectedFailurePayloadCfg;
    use sui_benchmark::workloads::workload::ExpectedFailureType;
    use sui_benchmark::workloads::workload_configuration::{
        WorkloadConfig, WorkloadConfiguration, WorkloadWeights,
    };
    use sui_benchmark::{
        drivers::{bench_driver::BenchDriver, driver::Driver, Interval},
        util::get_ed25519_keypair_from_keystore,
        LocalValidatorAggregatorProxy, ValidatorProxy,
    };
    use sui_config::node::AuthorityOverloadConfig;
    use sui_config::ExecutionCacheConfig;
    use sui_config::{AUTHORITIES_DB_NAME, SUI_KEYSTORE_FILENAME};
    use sui_core::authority::authority_store_tables::AuthorityPerpetualTables;
    use sui_core::authority::framework_injection;
    use sui_core::authority::AuthorityState;
    use sui_core::checkpoints::{CheckpointStore, CheckpointWatermark};
    use sui_framework::BuiltInFramework;
    use sui_macros::{
        clear_fail_point, nondeterministic, register_fail_point, register_fail_point_arg,
        register_fail_point_async, register_fail_point_if, register_fail_points, sim_test,
    };
    use sui_protocol_config::{PerObjectCongestionControlMode, ProtocolConfig, ProtocolVersion};
    use sui_simulator::tempfile::TempDir;
    use sui_simulator::{configs::*, SimConfig};
    use sui_storage::blob::Blob;
    use sui_surfer::surf_strategy::SurfStrategy;
    use sui_swarm_config::network_config_builder::ConfigBuilder;
    use sui_types::base_types::{ConciseableName, ObjectID, SequenceNumber};
    use sui_types::digests::TransactionDigest;
    use sui_types::full_checkpoint_content::CheckpointData;
    use sui_types::messages_checkpoint::VerifiedCheckpoint;
    use sui_types::supported_protocol_versions::SupportedProtocolVersions;
    use sui_types::traffic_control::{FreqThresholdConfig, PolicyConfig, PolicyType};
    use sui_types::transaction::{
        DEFAULT_VALIDATOR_GAS_PRICE, TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE,
    };
    use test_cluster::{TestCluster, TestClusterBuilder};
    use tracing::{error, info, trace};
    use typed_store::traits::Map;

    struct DeadValidator {
        node_id: sui_simulator::task::NodeId,
        dead_until: std::time::Instant,
    }

    fn test_config() -> SimConfig {
        env_config(
            uniform_latency_ms(10..20),
            [
                (
                    "regional_high_variance",
                    bimodal_latency_ms(30..40, 300..800, 0.005),
                ),
                (
                    "global_high_variance",
                    bimodal_latency_ms(60..80, 500..1500, 0.01),
                ),
            ],
        )
    }

    fn test_config_low_latency() -> SimConfig {
        env_config(constant_latency_ms(1), [])
    }

    fn get_var<T: FromStr>(name: &str, default: T) -> T
    where
        <T as FromStr>::Err: std::fmt::Debug,
    {
        std::env::var(name)
            .ok()
            .map(|v| v.parse().unwrap())
            .unwrap_or(default)
    }

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_with_reconfig() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();
        let test_cluster = build_test_cluster(4, 1000, 1).await;
        test_simulated_load(test_cluster, 60).await;
    }

    // Ensure that with half the committee enabling v2 and half not,
    // we still arrive at the same root state hash (we do not split brain
    // fork).
    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_with_accumulator_v2_partial_upgrade() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();
        let test_cluster = init_test_cluster_builder(4, 1000)
            .with_authority_overload_config(AuthorityOverloadConfig {
                // Disable system overload checks for the test - during tests with crashes,
                // it is possible for overload protection to trigger due to validators
                // having queued certs which are missing dependencies.
                check_system_overload_at_execution: false,
                check_system_overload_at_signing: false,
                ..Default::default()
            })
            .with_submit_delay_step_override_millis(3000)
            .with_state_accumulator_v2_enabled_callback(Arc::new(|idx| idx % 2 == 0))
            .build()
            .await
            .into();
        test_simulated_load(test_cluster, 60).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_with_reconfig_and_correlated_crashes() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();

        register_fail_point_if("correlated-crash-after-consensus-commit-boundary", || true);
        // TODO: enable this - right now it causes rocksdb errors when re-opening DBs
        //register_fail_point_if("correlated-crash-process-certificate", || true);

        let test_cluster = build_test_cluster(4, 10000, 1).await;
        test_simulated_load(test_cluster, 60).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_basic() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();
        let test_cluster = build_test_cluster(7, 0, 1).await;
        test_simulated_load(test_cluster, 15).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_restarts() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();
        let test_cluster = build_test_cluster(4, 0, 1).await;
        let node_restarter = test_cluster
            .random_node_restarter()
            .with_kill_interval_secs(5, 15)
            .with_restart_delay_secs(1, 10);
        node_restarter.run();
        test_simulated_load(test_cluster, 120).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_rolling_restarts_all_validators() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();
        let test_cluster = build_test_cluster(4, 330_000, 1).await;

        let validators = test_cluster.get_validator_pubkeys();
        let test_cluster_clone = test_cluster.clone();
        let restarter_task = tokio::task::spawn(async move {
            for _ in 0..4 {
                for validator in validators.iter() {
                    info!("Killing validator {:?}", validator.concise());
                    test_cluster_clone.stop_node(validator);
                    tokio::time::sleep(Duration::from_secs(20)).await;
                    info!("Starting validator {:?}", validator.concise());
                    test_cluster_clone.start_node(validator).await;
                }
            }
        });
        test_simulated_load(test_cluster.clone(), 330).await;
        restarter_task.await.unwrap();
        test_cluster.wait_for_epoch_all_nodes(1).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_reconfig_restarts() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();
        let test_cluster = build_test_cluster(4, 5_000, 1).await;
        let node_restarter = test_cluster
            .random_node_restarter()
            .with_kill_interval_secs(5, 15)
            .with_restart_delay_secs(1, 10);
        node_restarter.run();
        test_simulated_load(test_cluster, 120).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_small_committee_reconfig() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();
        let test_cluster = build_test_cluster(1, 5_000, 0).await;
        test_simulated_load(test_cluster, 120).await;
    }

    /// Get a list of nodes that we don't want to kill in the crash recovery tests.
    /// This includes the client node which is the node that is running the test, as well as
    /// rpc fullnode which are needed to run the benchmark.
    fn get_keep_alive_nodes(cluster: &TestCluster) -> HashSet<sui_simulator::task::NodeId> {
        let mut keep_alive_nodes = HashSet::new();
        // The first fullnode in the swarm ins the rpc fullnode.
        keep_alive_nodes.insert(
            cluster
                .swarm
                .fullnodes()
                .next()
                .unwrap()
                .get_node_handle()
                .unwrap()
                .with(|n| n.get_sim_node_id()),
        );
        keep_alive_nodes.insert(sui_simulator::current_simnode_id());
        keep_alive_nodes
    }

    fn handle_failpoint(
        dead_validator: Arc<Mutex<Option<DeadValidator>>>,
        keep_alive_nodes: HashSet<sui_simulator::task::NodeId>,
        grace_period: Arc<Mutex<Option<Instant>>>,
        probability: f64,
    ) {
        let mut dead_validator = dead_validator.lock().unwrap();
        let mut grace_period = grace_period.lock().unwrap();
        let cur_node = sui_simulator::current_simnode_id();

        if keep_alive_nodes.contains(&cur_node) {
            return;
        }

        // do not fail multiple nodes at a time.
        if let Some(dead) = &*dead_validator {
            if dead.node_id != cur_node && dead.dead_until > Instant::now() {
                return;
            }
        }

        // otherwise, possibly fail the current node
        let mut rng = thread_rng();
        if rng.gen_range(0.0..1.0) < probability {
            // clear grace period if expired
            if let Some(t) = *grace_period {
                if t < Instant::now() {
                    *grace_period = None;
                }
            }

            // check if any node is in grace period
            if grace_period.is_some() {
                trace!(?cur_node, "grace period in effect, not failing node");
                return;
            }

            let restart_after = Duration::from_millis(rng.gen_range(10000..20000));
            let dead_until = Instant::now() + restart_after;

            // Prevent the same node from being restarted again rapidly.
            let alive_until = dead_until + Duration::from_millis(rng.gen_range(5000..30000));
            *grace_period = Some(alive_until);

            error!(?cur_node, ?dead_until, ?alive_until, "killing node");

            *dead_validator = Some(DeadValidator {
                node_id: cur_node,
                dead_until,
            });

            // must manually release lock before calling kill_current_node, which panics
            // and would poison the lock.
            drop(grace_period);
            drop(dead_validator);

            sui_simulator::task::kill_current_node(Some(restart_after));
        }
    }

    // Runs object pruning and compaction for object table in `state` probabistically.
    async fn handle_failpoint_prune_and_compact(state: Arc<AuthorityState>, probability: f64) {
        {
            let mut rng = thread_rng();
            if rng.gen_range(0.0..1.0) > probability {
                return;
            }
        }
        state
            .database_for_testing()
            .prune_objects_and_compact_for_testing(state.get_checkpoint_store(), None)
            .await;
    }

    async fn delay_failpoint<R>(range_ms: R, probability: f64)
    where
        R: SampleRange<u64>,
    {
        let duration = {
            let mut rng = thread_rng();
            if rng.gen_range(0.0..1.0) < probability {
                info!("Matched probability threshold for delay failpoint. Delaying...");
                Some(Duration::from_millis(rng.gen_range(range_ms)))
            } else {
                None
            }
        };
        if let Some(duration) = duration {
            tokio::time::sleep(duration).await;
        }
    }

    // Tests load with aggressive pruning and compaction.
    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_reconfig_with_prune_and_compact() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();
        let test_cluster = build_test_cluster(4, 1000, 0).await;

        let node_state = test_cluster.fullnode_handle.sui_node.clone().state();
        register_fail_point_async("prune-and-compact", move || {
            handle_failpoint_prune_and_compact(node_state.clone(), 0.5)
        });

        test_simulated_load(test_cluster, 60).await;
        // The fail point holds a reference to `node_state`, which we need to release before the test ends.
        clear_fail_point("prune-and-compact");
    }

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_reconfig_with_crashes_and_delays() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();

        register_fail_point_if("select-random-cache", || true);

        let test_cluster = Arc::new(
            init_test_cluster_builder(4, 1000)
                .with_num_unpruned_validators(4)
                .build()
                .await,
        );

        let dead_validator_orig: Arc<Mutex<Option<DeadValidator>>> = Default::default();
        let grace_period: Arc<Mutex<Option<Instant>>> = Default::default();

        let dead_validator = dead_validator_orig.clone();
        let keep_alive_nodes = get_keep_alive_nodes(&test_cluster);
        let keep_alive_nodes_clone = keep_alive_nodes.clone();
        let grace_period_clone = grace_period.clone();
        register_fail_points(
            &[
                "batch-write-before",
                "batch-write-after",
                "put-cf-before",
                "put-cf-after",
                "delete-cf-before",
                "delete-cf-after",
                "transaction-commit",
                "highest-executed-checkpoint",
            ],
            move || {
                handle_failpoint(
                    dead_validator.clone(),
                    keep_alive_nodes_clone.clone(),
                    grace_period_clone.clone(),
                    0.02,
                );
            },
        );

        let dead_validator = dead_validator_orig.clone();
        let keep_alive_nodes_clone = keep_alive_nodes.clone();
        let grace_period_clone = grace_period.clone();
        register_fail_point("crash", move || {
            handle_failpoint(
                dead_validator.clone(),
                keep_alive_nodes_clone.clone(),
                grace_period_clone.clone(),
                0.01,
            );
        });

        // Narwhal & Consensus 2.0 fail points.
        let dead_validator = dead_validator_orig.clone();
        let keep_alive_nodes_clone = keep_alive_nodes.clone();
        let grace_period_clone = grace_period.clone();
        register_fail_points(
            &[
                "narwhal-rpc-response",
                "narwhal-store-before-write",
                "narwhal-store-after-write",
                "consensus-store-before-write",
                "consensus-store-after-write",
                "consensus-after-propose",
                "consensus-after-leader-schedule-change",
            ],
            move || {
                handle_failpoint(
                    dead_validator.clone(),
                    keep_alive_nodes_clone.clone(),
                    grace_period_clone.clone(),
                    0.001,
                );
            },
        );
        register_fail_point_async("narwhal-delay", || delay_failpoint(10..20, 0.001));

        let dead_validator = dead_validator_orig.clone();
        let keep_alive_nodes_clone = keep_alive_nodes.clone();
        let grace_period_clone = grace_period.clone();
        register_fail_point_async("consensus-rpc-response", move || {
            let dead_validator = dead_validator.clone();
            let keep_alive_nodes_clone = keep_alive_nodes_clone.clone();
            let grace_period_clone = grace_period_clone.clone();
            async move {
                handle_failpoint(
                    dead_validator.clone(),
                    keep_alive_nodes_clone.clone(),
                    grace_period_clone.clone(),
                    0.001,
                );
            }
        });
        register_fail_point_async("consensus-delay", || delay_failpoint(10..20, 0.001));

        test_simulated_load(test_cluster, 120).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_reconfig_crashes_during_epoch_change() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();
        let test_cluster = build_test_cluster(4, 10000, 1).await;

        let dead_validator: Arc<Mutex<Option<DeadValidator>>> = Default::default();
        let keep_alive_nodes = get_keep_alive_nodes(&test_cluster);
        let grace_period: Arc<Mutex<Option<Instant>>> = Default::default();
        register_fail_points(&["before-open-new-epoch-store"], move || {
            handle_failpoint(
                dead_validator.clone(),
                keep_alive_nodes.clone(),
                grace_period.clone(),
                1.0,
            );
        });
        test_simulated_load(test_cluster, 120).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_checkpoint_pruning() {
        let test_cluster = build_test_cluster(10, 1000, 0).await;
        test_simulated_load(test_cluster.clone(), 30).await;

        let swarm_dir = test_cluster.swarm.dir().join(AUTHORITIES_DB_NAME);
        let random_validator_path = std::fs::read_dir(swarm_dir).unwrap().next().unwrap();
        let validator_path = random_validator_path.unwrap().path();
        let checkpoint_store =
            CheckpointStore::open_readonly(&validator_path.join("live").join("checkpoints"));

        let pruned = checkpoint_store
            .watermarks
            .get(&CheckpointWatermark::HighestPruned)
            .unwrap()
            .unwrap()
            .0;
        assert!(pruned > 0);
    }

    // Tests cluster liveness when shared object congestion control is on.
    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_shared_object_congestion_control() {
        let mode;
        let checkpoint_budget_factor; // The checkpoint congestion control budget in respect to transaction budget.
        let txn_count_limit; // When using transaction count as congestion control mode, the limit of transactions per object per commit.
        let max_deferral_rounds;
        let cap_factor_denominator;
        let absolute_cap_factor;
        let mut allow_overage_factor = 0;
        let mut burst_limit_factor = 0;
        let separate_randomness_budget;
        {
            let mut rng = thread_rng();
            mode = if rng.gen_bool(0.33) {
                PerObjectCongestionControlMode::TotalGasBudget
            } else if rng.gen_bool(0.5) {
                PerObjectCongestionControlMode::TotalTxCount
            } else {
                PerObjectCongestionControlMode::TotalGasBudgetWithCap
            };
            checkpoint_budget_factor = rng.gen_range(1..20);
            txn_count_limit = rng.gen_range(1..=10);
            max_deferral_rounds = if rng.gen_bool(0.5) {
                rng.gen_range(0..20) // Short deferral round (testing cancellation)
            } else {
                rng.gen_range(1000..10000) // Large deferral round (testing liveness)
            };
            if rng.gen_bool(0.5) {
                allow_overage_factor = rng.gen_range(1..100);
            }
            cap_factor_denominator = rng.gen_range(1..100);
            absolute_cap_factor = rng.gen_range(2..50);
            if allow_overage_factor > 1 && rng.gen_bool(0.5) {
                burst_limit_factor = rng.gen_range(1..allow_overage_factor);
            }
            separate_randomness_budget = rng.gen_bool(0.5);
        }

        info!(
            "test_simulated_load_shared_object_congestion_control setup.
             mode: {mode:?}, checkpoint_budget_factor: {checkpoint_budget_factor:?},
             max_deferral_rounds: {max_deferral_rounds:?},
             txn_count_limit: {txn_count_limit:?},
             allow_overage_factor: {allow_overage_factor:?},
             burst_limit_factor: {burst_limit_factor:?},
             cap_factor_denominator: {cap_factor_denominator:?},
             absolute_cap_factor: {absolute_cap_factor:?},
             separate_randomness_budget: {separate_randomness_budget:?}",
        );

        let _guard = ProtocolConfig::apply_overrides_for_testing(move |_, mut config| {
            let total_gas_limit = checkpoint_budget_factor
                * DEFAULT_VALIDATOR_GAS_PRICE
                * TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE;
            config.set_per_object_congestion_control_mode_for_testing(mode);
            match mode {
                PerObjectCongestionControlMode::None => panic!("Congestion control mode cannot be None in test_simulated_load_shared_object_congestion_control"),
                PerObjectCongestionControlMode::TotalGasBudget => {
                    config.set_max_accumulated_txn_cost_per_object_in_narwhal_commit_for_testing(total_gas_limit);
                    config.set_max_accumulated_txn_cost_per_object_in_mysticeti_commit_for_testing(total_gas_limit);
                },
                PerObjectCongestionControlMode::TotalTxCount => {
                    config.set_max_accumulated_txn_cost_per_object_in_narwhal_commit_for_testing(
                        txn_count_limit
                    );
                    config.set_max_accumulated_txn_cost_per_object_in_mysticeti_commit_for_testing(
                        txn_count_limit
                    );
                },
                PerObjectCongestionControlMode::TotalGasBudgetWithCap => {
                    config.set_max_accumulated_txn_cost_per_object_in_narwhal_commit_for_testing(total_gas_limit);
                    config.set_max_accumulated_txn_cost_per_object_in_mysticeti_commit_for_testing(total_gas_limit);
                    config.set_gas_budget_based_txn_cost_cap_factor_for_testing(total_gas_limit/cap_factor_denominator);
                    config.set_gas_budget_based_txn_cost_absolute_cap_commit_count_for_testing(absolute_cap_factor);
                },
                // TODO: Enable once ExecutionTimeEstimate mode is functional across epochs.
                PerObjectCongestionControlMode::ExecutionTimeEstimate => unimplemented!(),
            }
            config.set_max_deferral_rounds_for_congestion_control_for_testing(max_deferral_rounds);
            config.set_max_txn_cost_overage_per_object_in_commit_for_testing(
                allow_overage_factor * total_gas_limit,
            );
            config.set_allowed_txn_cost_overage_burst_per_object_in_commit_for_testing(
                burst_limit_factor * total_gas_limit,
            );
            if separate_randomness_budget {
                config
                .set_max_accumulated_randomness_txn_cost_per_object_in_mysticeti_commit_for_testing(
                    std::cmp::max(
                        1,
                        config.max_accumulated_txn_cost_per_object_in_mysticeti_commit() / 10,
                    ),
                );
            } else {
                config
                .disable_max_accumulated_randomness_txn_cost_per_object_in_mysticeti_commit_for_testing();
            }
            config
        });

        let test_cluster = build_test_cluster(4, 5000, 2).await;
        let mut simulated_load_config = SimulatedLoadConfig::default();
        {
            let mut rng = thread_rng();
            simulated_load_config.shared_counter_weight = if rng.gen_bool(0.5) { 5 } else { 50 };
            simulated_load_config.num_shared_counters = match rng.gen_range(0..=2) {
                0 => None, // shared_counter_hotness_factor is in play in this case.
                n => Some(n),
            };
            simulated_load_config.shared_counter_hotness_factor = rng.gen_range(50..=100);

            // Use shared_counter_max_tip to make transactions to have different gas prices.
            simulated_load_config.use_shared_counter_max_tip = rng.gen_bool(0.25);
            simulated_load_config.shared_counter_max_tip = rng.gen_range(1..=1000);

            // Always enable the randomized tx workload in this test.
            simulated_load_config.randomized_transaction_weight = 1;
            info!("Simulated load config: {:?}", simulated_load_config);
        }

        test_simulated_load_with_test_config(test_cluster, 180, simulated_load_config, None, None)
            .await;
    }

    // Tests cluster defense against failing transaction floods Traffic Control
    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_expected_failure_traffic_control() {
        // TODO: can we get away with significantly increasing this?
        let target_qps = get_var("SIM_STRESS_TEST_QPS", 10);
        let num_workers = get_var("SIM_STRESS_TEST_WORKERS", 10);

        let expected_tps = target_qps * num_workers;
        let error_policy_type = PolicyType::FreqThreshold(FreqThresholdConfig {
            client_threshold: expected_tps / 2,
            window_size_secs: 5,
            update_interval_secs: 1,
            ..Default::default()
        });
        info!(
            "test_simulated_load_expected_failure_traffic_control setup.
             Policy type: {:?}",
            error_policy_type
        );

        let policy_config = PolicyConfig {
            connection_blocklist_ttl_sec: 1,
            error_policy_type,
            dry_run: false,
            ..Default::default()
        };
        let network_config = ConfigBuilder::new_with_temp_dir()
            .committee_size(NonZeroUsize::new(4).unwrap())
            .with_policy_config(Some(policy_config))
            .with_epoch_duration(5000)
            .build();
        let test_cluster = Arc::new(
            TestClusterBuilder::new()
                .set_network_config(network_config)
                .build()
                .await,
        );

        let mut simulated_load_config = SimulatedLoadConfig::default();
        {
            simulated_load_config.expected_failure_weight = 20;
            simulated_load_config.expected_failure_config.failure_type =
                ExpectedFailureType::try_from(0).unwrap();
            info!("Simulated load config: {:?}", simulated_load_config);
        }

        test_simulated_load_with_test_config(
            test_cluster,
            50,
            simulated_load_config,
            Some(target_qps),
            Some(num_workers),
        )
        .await;
    }

    // Tests cluster liveness when DKG has failed.
    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_dkg_failure() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(move |_, mut config| {
            config.set_random_beacon_dkg_timeout_round_for_testing(0);
            config
        });

        let test_cluster = build_test_cluster(4, 30_000, 1).await;
        test_simulated_load(test_cluster, 120).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_data_ingestion_pipeline() {
        let path = nondeterministic!(TempDir::new().unwrap()).into_path();
        let test_cluster = Arc::new(
            init_test_cluster_builder(4, 5000)
                .with_data_ingestion_dir(path.clone())
                .build()
                .await,
        );
        test_simulated_load(test_cluster, 30).await;

        let checkpoint_files = std::fs::read_dir(path)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| {
                        entry.path().is_file()
                            && entry.path().extension() == Some(std::ffi::OsStr::new("chk"))
                    })
                    .map(|entry| entry.path())
                    .collect()
            })
            .unwrap_or_else(|_| vec![]);
        assert!(checkpoint_files.len() > 0);
        let bytes = std::fs::read(checkpoint_files.first().unwrap()).unwrap();

        let _checkpoint: CheckpointData =
            Blob::from_bytes(&bytes).expect("failed to load checkpoint");
    }

    // Tests the correctness of large consensus commit transaction due to large number
    // of cancelled transactions. Note that we use a low latency configuration since
    // simtest has low timeout tolerance and it is not designed to test performance.
    #[sim_test(config = "test_config_low_latency()")]
    async fn test_simulated_load_large_consensus_commit_prologue_size() {
        let test_cluster = build_test_cluster(4, 5_000, 1).await;

        let mut additional_cancelled_txns = Vec::new();
        let num_txns = thread_rng().gen_range(500..2000);
        info!("Adding additional {num_txns} cancelled txns in consensus commit prologue.");

        // Note that we need to construct the additional assigned object versions outside of
        // fail point arg so that the same assigned object versions are used for all nodes in
        // all consensus commit to preserve the determinism.
        for _ in 0..num_txns {
            let num_objs = thread_rng().gen_range(1..15);
            let mut assigned_object_versions = Vec::new();
            for _ in 0..num_objs {
                assigned_object_versions.push((
                    (ObjectID::random(), SequenceNumber::UNKNOWN),
                    SequenceNumber::CONGESTED,
                ));
            }
            additional_cancelled_txns.push((TransactionDigest::random(), assigned_object_versions));
        }

        register_fail_point_arg("additional_cancelled_txns_for_tests", move || {
            Some(additional_cancelled_txns.clone())
        });

        test_simulated_load(test_cluster.clone(), 30).await;
    }

    // TODO add this back once flakiness is resolved
    #[ignore]
    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_pruning() {
        let epoch_duration_ms = 5000;
        let test_cluster = build_test_cluster(4, epoch_duration_ms, 0).await;
        test_simulated_load(test_cluster.clone(), 30).await;

        let swarm_dir = test_cluster.swarm.dir().join(AUTHORITIES_DB_NAME);
        let random_validator_path = std::fs::read_dir(swarm_dir).unwrap().next().unwrap();
        let validator_path = random_validator_path.unwrap().path();
        let store = AuthorityPerpetualTables::open_readonly(&validator_path.join("store"));
        let checkpoint_store = CheckpointStore::open_readonly(&validator_path.join("checkpoints"));

        let pruned = store.pruned_checkpoint.get(&()).unwrap().unwrap();
        assert!(pruned > 0);
        let pruned_checkpoint: VerifiedCheckpoint = checkpoint_store
            .certified_checkpoints
            .get(&pruned)
            .unwrap()
            .unwrap()
            .into();
        let pruned_epoch = pruned_checkpoint.epoch();
        let expected_checkpoint = checkpoint_store
            .epoch_last_checkpoint_map
            .get(&pruned_epoch)
            .unwrap()
            .unwrap();
        assert_eq!(expected_checkpoint, pruned);
    }

    #[sim_test(config = "test_config()")]
    async fn test_upgrade_compatibility() {
        // This test is intended to test the compatibility of the latest protocol version with
        // the previous protocol version. It does this by starting a network with
        // the previous protocol version that this binary supports, and then upgrading the network
        // to the latest protocol version.
        tokio::time::timeout(
            Duration::from_secs(1000),
            test_protocol_upgrade_compatibility_impl(),
        )
        .await
        .expect("testnet upgrade compatibility test timed out");
    }

    async fn test_protocol_upgrade_compatibility_impl() {
        let max_ver = ProtocolVersion::MAX.as_u64();
        let manifest = sui_framework_snapshot::load_bytecode_snapshot_manifest();

        let Some((&starting_version, _)) = manifest.range(..max_ver).last() else {
            panic!("Couldn't find previously supported version");
        };

        let init_framework =
            sui_framework_snapshot::load_bytecode_snapshot(starting_version).unwrap();
        let test_cluster = Arc::new(
            init_test_cluster_builder(4, 15000)
                .with_protocol_version(ProtocolVersion::new(starting_version))
                .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                    starting_version,
                    starting_version,
                ))
                .with_fullnode_supported_protocol_versions_config(
                    SupportedProtocolVersions::new_for_testing(starting_version, max_ver),
                )
                .with_objects(init_framework.into_iter().map(|p| p.genesis_object()))
                .with_stake_subsidy_start_epoch(10)
                .build()
                .await,
        );
        let test_cluster_clone = test_cluster.clone();

        let finished = Arc::new(AtomicBool::new(false));
        let finished_clone = finished.clone();
        let _handle = tokio::task::spawn(async move {
            info!("Running from version {starting_version} to version {max_ver}");
            for version in starting_version..=max_ver {
                info!("Targeting protocol version: {version}");
                test_cluster.wait_for_all_nodes_upgrade_to(version).await;
                info!("All nodes are at protocol version: {version}");
                // Let all nodes run for a few epochs at this version.
                tokio::time::sleep(Duration::from_secs(30)).await;
                if version == max_ver {
                    break;
                }
                let next_version = version + 1;
                let new_framework = sui_framework_snapshot::load_bytecode_snapshot(next_version);
                let new_framework_ref = match &new_framework {
                    Ok(f) => Some(f.iter().collect::<Vec<_>>()),
                    Err(_) => {
                        if next_version == max_ver {
                            Some(BuiltInFramework::iter_system_packages().collect::<Vec<_>>())
                        } else {
                            // Often we want to be able to create multiple protocol config versions
                            // on main that none have shipped to any production network. In this case,
                            // some of the protocol versions may not have a framework snapshot.
                            None
                        }
                    }
                };
                if let Some(new_framework_ref) = new_framework_ref {
                    for package in new_framework_ref {
                        framework_injection::set_override(package.id, package.modules().clone());
                    }
                    info!("Framework injected for next_version {next_version}");
                } else {
                    info!("No framework snapshot to inject for next_version {next_version}");
                }
                test_cluster
                    .update_validator_supported_versions(
                        SupportedProtocolVersions::new_for_testing(starting_version, next_version),
                    )
                    .await;
                info!("Updated validator supported versions to include next_version {next_version}")
            }
            finished_clone.store(true, Ordering::SeqCst);
        });

        test_simulated_load(test_cluster_clone, 150).await;
        for _ in 0..150 {
            if finished.load(Ordering::Relaxed) {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        assert!(finished.load(Ordering::SeqCst));
    }

    #[sim_test(config = "test_config()")]
    async fn test_randomness_partial_sig_failures() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();
        let test_cluster = build_test_cluster(6, 20_000, 1).await;

        // Network should continue as long as f+1 nodes (in this case 3/6) are sending partial signatures.
        let eligible_nodes: HashSet<_> = test_cluster
            .swarm
            .validator_nodes()
            .take(3)
            .map(|v| v.get_node_handle().unwrap().with(|n| n.get_sim_node_id()))
            .collect();

        register_fail_point_if("rb-send-partial-signatures", move || {
            handle_bool_failpoint(&eligible_nodes, 1.0)
        });

        test_simulated_load(test_cluster, 60).await
    }

    #[sim_test(config = "test_config()")]
    async fn test_randomness_dkg_failures() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();
        let test_cluster = build_test_cluster(6, 20_000, 1).await;

        // Network should continue as long as nodes are participating in DKG representing
        // stake equal to 2f+1 PLUS proportion of stake represented by the
        // `random_beacon_reduction_allowed_delta` ProtocolConfig option.
        // In this case we make sure it still works with 5/6 validators.
        let eligible_nodes: HashSet<_> = test_cluster
            .swarm
            .validator_nodes()
            .take(1)
            .map(|v| v.get_node_handle().unwrap().with(|n| n.get_sim_node_id()))
            .collect();

        register_fail_point_if("rb-dkg", move || {
            handle_bool_failpoint(&eligible_nodes, 1.0)
        });

        test_simulated_load(test_cluster, 60).await
    }

    #[sim_test(config = "test_config()")]
    async fn test_backpressure() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();

        let mut cache_config: ExecutionCacheConfig = Default::default();
        // make sure we don't halt even with absurdly low backpressure threshold
        // To validate this, change backpressure::Watermarks::is_backpressure_suppressed() to
        // always return false and verify the test fails.
        match &mut cache_config {
            ExecutionCacheConfig::WritebackCache {
                backpressure_threshold,
                backpressure_threshold_for_rpc,
                ..
            } => {
                *backpressure_threshold = Some(1);
                // for the tests to pass we still need to be able to submit transactions
                // during backpressure.
                *backpressure_threshold_for_rpc = Some(10000);
            }
            _ => panic!(),
        }

        let test_cluster = init_test_cluster_builder(4, 10000)
            .with_authority_overload_config(AuthorityOverloadConfig {
                // Disable system overload checks for the test - during tests with crashes,
                // it is possible for overload protection to trigger due to validators
                // having queued certs which are missing dependencies.
                check_system_overload_at_execution: false,
                check_system_overload_at_signing: false,
                max_txn_age_in_queue: Duration::from_secs(10000),
                max_transaction_manager_queue_length: 10000,
                max_transaction_manager_per_object_queue_length: 10000,
                ..Default::default()
            })
            .with_execution_cache_config(cache_config)
            .with_submit_delay_step_override_millis(3000)
            .build()
            .await
            .into();

        test_simulated_load(test_cluster, 60).await;
    }

    fn handle_bool_failpoint(
        eligible_nodes: &HashSet<sui_simulator::task::NodeId>, // only given eligible nodes may fail
        probability: f64,
    ) -> bool {
        if !eligible_nodes.contains(&sui_simulator::current_simnode_id()) {
            return false; // don't fail ineligible nodes
        }
        let mut rng = thread_rng();
        if rng.gen_range(0.0..1.0) < probability {
            true
        } else {
            false
        }
    }

    async fn build_test_cluster(
        default_num_validators: usize,
        default_epoch_duration_ms: u64,
        default_num_of_unpruned_validators: usize,
    ) -> Arc<TestCluster> {
        assert!(
            default_num_of_unpruned_validators <= default_num_validators,
            "Provided number of unpruned validators is greater than the total number of validators"
        );
        init_test_cluster_builder(default_num_validators, default_epoch_duration_ms)
            .with_authority_overload_config(AuthorityOverloadConfig {
                // Disable system overload checks for the test - during tests with crashes,
                // it is possible for overload protection to trigger due to validators
                // having queued certs which are missing dependencies.
                check_system_overload_at_execution: false,
                check_system_overload_at_signing: false,
                ..Default::default()
            })
            .with_submit_delay_step_override_millis(3000)
            .with_num_unpruned_validators(default_num_of_unpruned_validators)
            .build()
            .await
            .into()
    }

    fn init_test_cluster_builder(
        default_num_validators: usize,
        default_epoch_duration_ms: u64,
    ) -> TestClusterBuilder {
        let mut builder = TestClusterBuilder::new().with_num_validators(get_var(
            "SIM_STRESS_TEST_NUM_VALIDATORS",
            default_num_validators,
        ));
        if std::env::var("CHECKPOINTS_PER_EPOCH").is_ok() {
            eprintln!("CHECKPOINTS_PER_EPOCH env var is deprecated, use EPOCH_DURATION_MS");
        }
        let epoch_duration_ms = get_var("EPOCH_DURATION_MS", default_epoch_duration_ms);
        if epoch_duration_ms > 0 {
            builder = builder.with_epoch_duration_ms(epoch_duration_ms);
        }
        builder
    }

    #[derive(Debug)]
    struct SimulatedLoadConfig {
        num_transfer_accounts: u64,
        shared_counter_weight: u32,
        transfer_object_weight: u32,
        delegation_weight: u32,
        batch_payment_weight: u32,
        shared_deletion_weight: u32,
        shared_counter_hotness_factor: u32,
        randomness_weight: u32,
        randomized_transaction_weight: u32,
        num_shared_counters: Option<u64>,
        use_shared_counter_max_tip: bool,
        shared_counter_max_tip: u64,
        expected_failure_weight: u32,
        expected_failure_config: ExpectedFailurePayloadCfg,
    }

    impl Default for SimulatedLoadConfig {
        fn default() -> Self {
            Self {
                shared_counter_weight: 1,
                transfer_object_weight: 1,
                num_transfer_accounts: 2,
                delegation_weight: 1,
                batch_payment_weight: 1,
                shared_deletion_weight: 1,
                shared_counter_hotness_factor: 50,
                randomness_weight: 1,
                randomized_transaction_weight: 0,
                num_shared_counters: Some(1),
                use_shared_counter_max_tip: false,
                shared_counter_max_tip: 0,
                expected_failure_weight: 0,
                expected_failure_config: ExpectedFailurePayloadCfg {
                    failure_type: ExpectedFailureType::try_from(0).unwrap(),
                },
            }
        }
    }

    async fn test_simulated_load(test_cluster: Arc<TestCluster>, test_duration_secs: u64) {
        test_simulated_load_with_test_config(
            test_cluster,
            test_duration_secs,
            SimulatedLoadConfig::default(),
            None,
            None,
        )
        .await;
    }

    async fn test_simulated_load_with_test_config(
        test_cluster: Arc<TestCluster>,
        test_duration_secs: u64,
        config: SimulatedLoadConfig,
        target_qps: Option<u64>,
        num_workers: Option<u64>,
    ) {
        let sender = test_cluster.get_address_0();
        let keystore_path = test_cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
        let genesis = test_cluster.swarm.config().genesis.clone();
        let primary_gas = test_cluster
            .wallet
            .get_one_gas_object_owned_by_address(sender)
            .await
            .unwrap()
            .unwrap();

        let ed25519_keypair =
            Arc::new(get_ed25519_keypair_from_keystore(keystore_path, &sender).unwrap());
        let primary_coin = (primary_gas, sender, ed25519_keypair.clone());

        let registry = prometheus::Registry::new();
        let proxy: Arc<dyn ValidatorProxy + Send + Sync> =
            Arc::new(LocalValidatorAggregatorProxy::from_genesis(&genesis, &registry, None).await);

        let bank = BenchmarkBank::new(proxy.clone(), primary_coin);
        let system_state_observer = {
            let mut system_state_observer = SystemStateObserver::new(proxy.clone());
            if let Ok(_) = system_state_observer.state.changed().await {
                info!("Got the new state (reference gas price and/or protocol config) from system state object");
            }
            Arc::new(system_state_observer)
        };

        // The default test parameters are somewhat conservative in order to keep the running time
        // of the test reasonable in CI.
        let target_qps = target_qps.unwrap_or(get_var("SIM_STRESS_TEST_QPS", 20));
        let num_workers = num_workers.unwrap_or(get_var("SIM_STRESS_TEST_WORKERS", 10));
        let in_flight_ratio = get_var("SIM_STRESS_TEST_IFR", 2);
        let batch_payment_size = get_var("SIM_BATCH_PAYMENT_SIZE", 15);

        // Run random payloads at 100% load
        let adversarial_cfg = AdversarialPayloadCfg::from_str("0-1.0").unwrap();
        let duration = Interval::from_str("unbounded").unwrap();

        // TODO: re-enable this when we figure out why it is causing connection errors and making
        // TODO: move adversarial cfg to TestSimulatedLoadConfig once enabled.
        // tests run for ever
        let adversarial_weight = 0;

        let shared_counter_max_tip = if config.use_shared_counter_max_tip {
            config.shared_counter_max_tip
        } else {
            0
        };
        let gas_request_chunk_size = 100;

        let weights = WorkloadWeights {
            shared_counter: config.shared_counter_weight,
            transfer_object: config.transfer_object_weight,
            delegation: config.delegation_weight,
            batch_payment: config.batch_payment_weight,
            shared_deletion: config.shared_deletion_weight,
            randomness: config.randomness_weight,
            adversarial: adversarial_weight,
            expected_failure: config.expected_failure_weight,
            randomized_transaction: config.randomized_transaction_weight,
        };

        let workload_config = WorkloadConfig {
            group: 0,
            num_workers,
            num_transfer_accounts: config.num_transfer_accounts,
            weights,
            adversarial_cfg,
            expected_failure_cfg: config.expected_failure_config,
            batch_payment_size,
            shared_counter_hotness_factor: config.shared_counter_hotness_factor,
            num_shared_counters: config.num_shared_counters,
            shared_counter_max_tip,
            target_qps,
            in_flight_ratio,
            duration,
        };

        let workloads_builders = WorkloadConfiguration::create_workload_builders(
            workload_config,
            system_state_observer.clone(),
        )
        .await;

        let workloads = WorkloadConfiguration::build(
            workloads_builders,
            bank,
            system_state_observer.clone(),
            gas_request_chunk_size,
        )
        .await
        .unwrap();

        let test_duration_secs = get_var("SIM_STRESS_TEST_DURATION_SECS", test_duration_secs);
        let test_duration = if test_duration_secs == 0 {
            Duration::MAX
        } else {
            Duration::from_secs(test_duration_secs)
        };

        let bench_task = tokio::spawn(async move {
            let driver = BenchDriver::new(5, false);

            // Use 0 for unbounded
            let interval = Interval::Time(test_duration);

            let show_progress = interval.is_unbounded();
            let (benchmark_stats, _) = driver
                .run(
                    vec![proxy],
                    workloads,
                    system_state_observer,
                    &registry,
                    show_progress,
                    interval,
                )
                .await
                .unwrap();

            // TODO: make this stricter (== 0) when we have reliable error retrying on the client.
            tracing::info!("end of test {:?}", benchmark_stats);
            assert!(benchmark_stats.num_error_txes < 100);
        });

        let surfer_task = tokio::spawn(async move {
            // now do a sui-surfer test
            let mut test_packages_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            test_packages_dir.extend(["..", "..", "crates", "sui-surfer", "tests"]);
            let test_package_paths: Vec<PathBuf> = std::fs::read_dir(test_packages_dir)
                .unwrap()
                .flat_map(|entry| {
                    let entry = entry.unwrap();
                    entry.metadata().unwrap().is_dir().then_some(entry.path())
                })
                .collect();
            info!("using sui_surfer test packages: {test_package_paths:?}");

            let surf_strategy = SurfStrategy::new(Duration::from_millis(400));
            let results = sui_surfer::run_with_test_cluster_and_strategy(
                surf_strategy,
                test_duration,
                test_package_paths,
                test_cluster,
                1, // skip first account for use by bench_task
            )
            .await;
            info!("sui_surfer test complete with results: {results:?}");
            assert!(results.num_successful_transactions > 0);
            assert!(!results.unique_move_functions_called.is_empty());
        });

        let _ = futures::join!(bench_task, surfer_task);
    }
}
