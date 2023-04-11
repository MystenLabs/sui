// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod test {

    use move_core_types::language_storage::StructTag;
    use rand::{distributions::uniform::SampleRange, thread_rng, Rng};
    use std::path::PathBuf;
    use std::str::FromStr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use sui_benchmark::bank::BenchmarkBank;
    use sui_benchmark::system_state_observer::SystemStateObserver;
    use sui_benchmark::workloads::adversarial::AdversarialPayloadCfg;
    use sui_benchmark::workloads::workload_configuration::WorkloadConfiguration;
    use sui_benchmark::{
        drivers::{bench_driver::BenchDriver, driver::Driver, Interval},
        util::get_ed25519_keypair_from_keystore,
        LocalValidatorAggregatorProxy, ValidatorProxy,
    };
    use sui_config::genesis::Genesis;
    use sui_config::{AUTHORITIES_DB_NAME, SUI_KEYSTORE_FILENAME};
    use sui_core::authority::authority_store_tables::AuthorityPerpetualTables;
    use sui_core::authority::framework_injection;
    use sui_core::checkpoints::CheckpointStore;
    use sui_framework::BuiltInFramework;
    use sui_macros::{register_fail_point_async, register_fail_points, sim_test};
    use sui_protocol_config::{ProtocolVersion, SupportedProtocolVersions};
    use sui_simulator::{configs::*, SimConfig};
    use sui_types::base_types::{ObjectRef, SuiAddress};
    use sui_types::messages_checkpoint::VerifiedCheckpoint;
    use test_utils::messages::get_sui_gas_object_with_wallet_context;
    use test_utils::network::{TestCluster, TestClusterBuilder};
    use tracing::{error, info};
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
        let test_cluster = build_test_cluster(4, 1000).await;
        test_simulated_load(TestInitData::new(&test_cluster).await, 60).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_basic() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();
        let test_cluster = build_test_cluster(7, 0).await;
        test_simulated_load(TestInitData::new(&test_cluster).await, 15).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_restarts() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();
        let test_cluster = Arc::new(build_test_cluster(4, 0).await);
        let node_restarter = test_cluster
            .random_node_restarter()
            .with_kill_interval_secs(5, 15)
            .with_restart_delay_secs(1, 10);
        node_restarter.run();
        test_simulated_load(TestInitData::new(&test_cluster).await, 120).await;
    }

    #[ignore = "MUSTFIX"]
    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_reconfig_restarts() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();
        let test_cluster = Arc::new(build_test_cluster(4, 1000).await);
        let node_restarter = test_cluster
            .random_node_restarter()
            .with_kill_interval_secs(5, 15)
            .with_restart_delay_secs(1, 10);
        node_restarter.run();
        test_simulated_load(TestInitData::new(&test_cluster).await, 120).await;
    }

    fn handle_failpoint(
        dead_validator: Arc<Mutex<Option<DeadValidator>>>,
        client_node: sui_simulator::task::NodeId,
        probability: f64,
    ) {
        let mut dead_validator = dead_validator.lock().unwrap();
        let cur_node = sui_simulator::current_simnode_id();

        // never kill the client node (which is running the test)
        if cur_node == client_node {
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
            error!("Matched probability threshold for failpoint. Failing...");
            let restart_after = Duration::from_millis(rng.gen_range(10000..20000));

            *dead_validator = Some(DeadValidator {
                node_id: cur_node,
                dead_until: Instant::now() + restart_after,
            });

            // must manually release lock before calling kill_current_node, which panics
            // and would poison the lock.
            drop(dead_validator);

            sui_simulator::task::kill_current_node(Some(restart_after));
        }
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

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_reconfig_with_crashes_and_delays() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();
        let test_cluster = build_test_cluster(4, 1000).await;

        let dead_validator_orig: Arc<Mutex<Option<DeadValidator>>> = Default::default();

        let dead_validator = dead_validator_orig.clone();
        let client_node = sui_simulator::current_simnode_id();
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
                handle_failpoint(dead_validator.clone(), client_node, 0.02);
            },
        );

        let dead_validator = dead_validator_orig.clone();
        register_fail_point_async("crash", move || {
            let dead_validator = dead_validator.clone();
            async move {
                handle_failpoint(dead_validator.clone(), client_node, 0.01);
            }
        });

        // Narwhal fail points.
        let dead_validator = dead_validator_orig.clone();
        register_fail_points(
            &[
                "narwhal-rpc-response",
                "narwhal-store-before-write",
                "narwhal-store-after-write",
            ],
            move || {
                handle_failpoint(dead_validator.clone(), client_node, 0.001);
            },
        );
        register_fail_point_async("narwhal-delay", || delay_failpoint(10..20, 0.001));

        test_simulated_load(TestInitData::new(&test_cluster).await, 120).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_reconfig_crashes_during_epoch_change() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();
        let test_cluster = build_test_cluster(4, 10000).await;

        let dead_validator: Arc<Mutex<Option<DeadValidator>>> = Default::default();
        let client_node = sui_simulator::current_simnode_id();
        register_fail_points(&["before-open-new-epoch-store"], move || {
            handle_failpoint(dead_validator.clone(), client_node, 1.0);
        });
        test_simulated_load(TestInitData::new(&test_cluster).await, 120).await;
    }

    // TODO add this back once flakiness is resolved
    #[ignore]
    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_pruning() {
        let epoch_duration_ms = 5000;
        let test_cluster = build_test_cluster(4, epoch_duration_ms).await;
        test_simulated_load(TestInitData::new(&test_cluster).await, 30).await;

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
    async fn test_testnet_upgrade_compatibility() {
        // This test is intended to test the compatibility of the latest protocol version with
        // the previous protocol version on testnet. It does this by starting a testnet with
        // the previous protocol version that this binary supports, and then upgrading the network
        // to the latest protocol version.
        let max_ver = ProtocolVersion::MAX.as_u64();
        let min_ver = max_ver - 1;
        let init_framework =
            sui_framework_snapshot::load_bytecode_snapshot("testnet", min_ver).unwrap();
        let mut test_cluster = init_test_cluster_builder(7, 5000)
            .with_protocol_version(ProtocolVersion::new(min_ver))
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                min_ver, min_ver,
            ))
            .with_fullnode_supported_protocol_versions_config(
                SupportedProtocolVersions::new_for_testing(min_ver, max_ver),
            )
            .with_objects(init_framework.into_iter().map(|p| p.genesis_object()))
            .build()
            .await
            .unwrap();

        let test_init_data = TestInitData::new(&test_cluster).await;

        let finished = Arc::new(AtomicBool::new(false));
        let finished_clone = finished.clone();
        let _handle = tokio::task::spawn(async move {
            for version in min_ver..=max_ver {
                info!("Targeting protocol version: {}", version);
                test_cluster.wait_for_all_nodes_upgrade_to(version).await;
                info!("All nodes are at protocol version: {}", version);
                // Let all nodes run for a bit at this version.
                tokio::time::sleep(Duration::from_secs(50)).await;
                if version == max_ver {
                    break;
                }
                let next_version = version + 1;
                let new_framework =
                    sui_framework_snapshot::load_bytecode_snapshot("testnet", next_version);
                let new_framework_ref: Vec<_> = match &new_framework {
                    Ok(f) => f.iter().collect(),
                    Err(_) => {
                        // The only time where we could not load an existing snapshot is when
                        // it's the next version to be pushed out, and hence we don't have a
                        // snapshot yet. This must be the current max version.
                        assert_eq!(next_version, max_ver);
                        BuiltInFramework::iter_system_packages().collect()
                    }
                };
                for package in new_framework_ref {
                    framework_injection::set_override(*package.id(), package.modules().clone());
                }
                info!("Framework injected");
                test_cluster
                    .upgrade_protocol(SupportedProtocolVersions::new_for_testing(
                        min_ver,
                        next_version,
                    ))
                    .await;
            }
            finished_clone.store(true, Ordering::SeqCst);
        });
        test_simulated_load(test_init_data.clone(), 120).await;
        loop {
            if finished.load(Ordering::Relaxed) {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn build_test_cluster(
        default_num_validators: usize,
        default_epoch_duration_ms: u64,
    ) -> TestCluster {
        init_test_cluster_builder(default_num_validators, default_epoch_duration_ms)
            .build()
            .await
            .unwrap()
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

    #[derive(Clone)]
    struct TestInitData {
        keystore_path: PathBuf,
        genesis: Genesis,
        all_gas: Vec<(StructTag, ObjectRef)>,
        sender: SuiAddress,
    }

    impl TestInitData {
        pub async fn new(test_cluster: &TestCluster) -> Self {
            let sender = test_cluster.get_address_0();
            Self {
                keystore_path: test_cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME),
                genesis: test_cluster.swarm.config().genesis.clone(),
                all_gas: get_sui_gas_object_with_wallet_context(&test_cluster.wallet, &sender)
                    .await,
                sender,
            }
        }
    }

    async fn test_simulated_load(init_data: TestInitData, test_duration_secs: u64) {
        let TestInitData {
            keystore_path,
            genesis,
            all_gas,
            sender,
        } = init_data;

        let ed25519_keypair =
            Arc::new(get_ed25519_keypair_from_keystore(keystore_path, &sender).unwrap());
        let (_, gas) = all_gas.get(0).unwrap();
        let (_move_struct, pay_coin) = all_gas.get(1).unwrap();
        let primary_gas = (gas.clone(), sender, ed25519_keypair.clone());
        let pay_coin = (pay_coin.clone(), sender, ed25519_keypair.clone());

        let registry = prometheus::Registry::new();
        let proxy: Arc<dyn ValidatorProxy + Send + Sync> =
            Arc::new(LocalValidatorAggregatorProxy::from_genesis(&genesis, &registry, None).await);

        let bank = BenchmarkBank::new(proxy.clone(), primary_gas, vec![pay_coin]);
        let system_state_observer = {
            let mut system_state_observer = SystemStateObserver::new(proxy.clone());
            if let Ok(_) = system_state_observer.state.changed().await {
                info!("Got the new state (reference gas price and/or protocol config) from system state object");
            }
            Arc::new(system_state_observer)
        };

        // The default test parameters are somewhat conservative in order to keep the running time
        // of the test reasonable in CI.
        let target_qps = get_var("SIM_STRESS_TEST_QPS", 10);
        let num_workers = get_var("SIM_STRESS_TEST_WORKERS", 10);
        let in_flight_ratio = get_var("SIM_STRESS_TEST_IFR", 2);
        let batch_payment_size = get_var("SIM_BATCH_PAYMENT_SIZE", 15);
        let shared_counter_weight = 1;
        let transfer_object_weight = 1;
        let num_transfer_accounts = 2;
        let delegation_weight = 1;
        let batch_payment_weight = 1;

        // Run random payloads at 100% load
        let adversarial_cfg = AdversarialPayloadCfg::from_str("0-1.0").unwrap();

        // TODO: re-enable this when we figure out why it is causing connection errors and making
        // tests run for ever
        let adversarial_weight = 0;

        let shared_counter_hotness_factor = 50;

        let workloads = WorkloadConfiguration::build_workloads(
            num_workers,
            num_transfer_accounts,
            shared_counter_weight,
            transfer_object_weight,
            delegation_weight,
            batch_payment_weight,
            adversarial_weight,
            adversarial_cfg,
            batch_payment_size,
            shared_counter_hotness_factor,
            target_qps,
            in_flight_ratio,
            bank,
            system_state_observer.clone(),
            100,
        )
        .await
        .unwrap();

        let driver = BenchDriver::new(5, false);

        // Use 0 for unbounded
        let test_duration_secs = get_var("SIM_STRESS_TEST_DURATION_SECS", test_duration_secs);
        let test_duration = if test_duration_secs == 0 {
            Duration::MAX
        } else {
            Duration::from_secs(test_duration_secs)
        };
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
        assert!(benchmark_stats.num_error_txes < 30);

        tracing::info!("end of test {:?}", benchmark_stats);
    }
}
