// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod test {

    use rand::{thread_rng, Rng};
    use std::str::FromStr;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use sui_benchmark::system_state_observer::SystemStateObserver;
    use sui_benchmark::util::generate_all_gas_for_test;
    use sui_benchmark::workloads::delegation::DelegationWorkload;
    use sui_benchmark::workloads::shared_counter::SharedCounterWorkload;
    use sui_benchmark::workloads::transfer_object::TransferObjectWorkload;
    use sui_benchmark::workloads::WorkloadGasConfig;
    use sui_benchmark::{
        drivers::{bench_driver::BenchDriver, driver::Driver, Interval},
        util::get_ed25519_keypair_from_keystore,
        workloads::make_combination_workload,
        LocalValidatorAggregatorProxy, ValidatorProxy,
    };
    use sui_config::SUI_KEYSTORE_FILENAME;
    use sui_macros::{register_fail_points, sim_test};
    use sui_simulator::{configs::*, SimConfig};
    use sui_types::object::Owner;
    use test_utils::messages::get_sui_gas_object_with_wallet_context;
    use test_utils::network::{TestCluster, TestClusterBuilder};
    use tracing::info;

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
        let test_cluster = build_test_cluster(4, 10).await;
        test_simulated_load(test_cluster, 60).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_basic() {
        let test_cluster = build_test_cluster(7, 0).await;
        test_simulated_load(test_cluster, 15).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_restarts() {
        let test_cluster = build_test_cluster(4, 0).await;
        let node_restarter = test_cluster
            .random_node_restarter()
            .with_kill_interval_secs(5, 15)
            .with_restart_delay_secs(1, 10);
        node_restarter.run();
        test_simulated_load(test_cluster, 120).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_reconfig_restarts() {
        let test_cluster = build_test_cluster(4, 10).await;
        let node_restarter = test_cluster
            .random_node_restarter()
            .with_kill_interval_secs(5, 15)
            .with_restart_delay_secs(1, 10);
        node_restarter.run();
        test_simulated_load(test_cluster, 120).await;
    }

    #[sim_test(config = "test_config()")]
    async fn test_simulated_load_reconfig_crashes() {
        let test_cluster = build_test_cluster(4, 10).await;

        struct DeadValidator {
            node_id: sui_simulator::task::NodeId,
            dead_until: std::time::Instant,
        }
        let dead_validator: Arc<Mutex<Option<DeadValidator>>> = Default::default();

        let client_node = sui_simulator::runtime::NodeHandle::current().id();
        register_fail_points(
            &["batch-write", "transaction-commit", "put-cf"],
            move || {
                let mut dead_validator = dead_validator.lock().unwrap();
                let cur_node = sui_simulator::runtime::NodeHandle::current().id();

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
                if rng.gen_range(0.0..1.0) < 0.01 {
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
            },
        );

        test_simulated_load(test_cluster, 120).await;
    }

    async fn build_test_cluster(
        default_num_validators: usize,
        default_checkpoints_per_epoch: u64,
    ) -> Arc<TestCluster> {
        let mut builder = TestClusterBuilder::new().with_num_validators(get_var(
            "SIM_STRESS_TEST_NUM_VALIDATORS",
            default_num_validators,
        ));

        let checkpoints_per_epoch = get_var("CHECKPOINTS_PER_EPOCH", default_checkpoints_per_epoch);
        if checkpoints_per_epoch > 0 {
            builder = builder.with_checkpoints_per_epoch(checkpoints_per_epoch);
        }

        Arc::new(builder.build().await.unwrap())
    }

    async fn test_simulated_load(test_cluster: Arc<TestCluster>, test_duration_secs: u64) {
        let swarm = &test_cluster.swarm;
        let context = &test_cluster.wallet;
        let sender = test_cluster.get_address_0();
        let fullnode_rpc_url = &test_cluster.fullnode_handle.rpc_url;

        let keystore_path = swarm.dir().join(SUI_KEYSTORE_FILENAME);
        let ed25519_keypair =
            Arc::new(get_ed25519_keypair_from_keystore(keystore_path, &sender).unwrap());
        let all_gas = get_sui_gas_object_with_wallet_context(context, &sender).await;
        let (_, gas) = all_gas.get(0).unwrap();
        let (move_struct, pay_coin) = all_gas.get(1).unwrap();
        let primary_gas = (
            gas.clone(),
            Owner::AddressOwner(sender),
            ed25519_keypair.clone(),
        );
        let coin = (
            pay_coin.clone(),
            Owner::AddressOwner(sender),
            ed25519_keypair.clone(),
        );
        let coin_type_tag = move_struct.type_params[0].clone();

        let registry = prometheus::Registry::new();
        let proxy: Arc<dyn ValidatorProxy + Send + Sync> = Arc::new(
            LocalValidatorAggregatorProxy::from_network_config(
                swarm.config(),
                &registry,
                Some(fullnode_rpc_url),
            )
            .await,
        );

        let system_state_observer = {
            let mut system_state_observer = SystemStateObserver::new(proxy.clone());
            if let Ok(_) = system_state_observer.reference_gas_price.changed().await {
                info!("Got the reference gas price from system state object");
            }
            Arc::new(system_state_observer)
        };
        let reference_gas_price = *system_state_observer.reference_gas_price.borrow();

        for node in swarm.validators() {
            let watch_node_handle = node.subscribe_to_sui_node_handle();

            tokio::task::spawn(async move {
                while let Ok(()) = watch_node_handle.changed().await {
                    let (name, epoch_change_rx) =
                        {
                            let node_handle = watch_node_handle.borrow_and_update();
                            if let Some(node_handle) = node_handle.upgrade() {
                                Some(node_handle.with(|node| {
                                    (node.name.clone(), node.subscribe_to_epoch_change())
                                }))
                            } else {
                                None
                            }
                        };

                    if let Some(mut epoch_change_rx) = epoch_change_rx {
                        while let Ok(committee) = epoch_rx.recv().await {
                            info!("received epoch {} from {}", committee.epoch, name.concise());
                        }

                        info!("epoch sender was dropped (node has reset)");
                    }
                }

                info!("node handle sender was dropped (container has shut down)");
            });
        }

        // The default test parameters are somewhat conservative in order to keep the running time
        // of the test reasonable in CI.

        let target_qps = get_var("SIM_STRESS_TEST_QPS", 10);
        let num_workers = get_var("SIM_STRESS_TEST_WORKERS", 10);
        let in_flight_ratio = get_var("SIM_STRESS_TEST_IFR", 2);
        let max_ops = target_qps * in_flight_ratio;
        let num_shared_counters = max_ops;
        let shared_counter_workload_init_gas_config =
            SharedCounterWorkload::generate_coin_config_for_init(num_shared_counters);
        let shared_counter_workload_payload_gas_config =
            SharedCounterWorkload::generate_coin_config_for_payloads(max_ops);

        let (transfer_object_workload_tokens, transfer_object_workload_payload_gas_config) =
            TransferObjectWorkload::generate_coin_config_for_payloads(max_ops, 2, max_ops);
        let delegation_gas_configs = DelegationWorkload::generate_gas_config_for_payloads(max_ops);
        let (workload_init_gas, workload_payload_gas) = generate_all_gas_for_test(
            proxy.clone(),
            primary_gas,
            coin,
            coin_type_tag,
            WorkloadGasConfig {
                shared_counter_workload_init_gas_config,
                shared_counter_workload_payload_gas_config,
                transfer_object_workload_tokens,
                transfer_object_workload_payload_gas_config,
                delegation_gas_configs,
            },
            reference_gas_price,
        )
        .await
        .unwrap();
        let mut combination_workload = make_combination_workload(
            target_qps,
            num_workers,
            in_flight_ratio,
            2, // num transfer accounts
            1, // shared_counter_weight
            1, // transfer_object_weight
            1, // delegation_weight
            workload_payload_gas,
        );
        combination_workload
            .workload
            .init(
                workload_init_gas,
                proxy.clone(),
                system_state_observer.clone(),
            )
            .await;

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
                vec![combination_workload],
                proxy,
                system_state_observer,
                &registry,
                show_progress,
                interval,
            )
            .await
            .unwrap();

        assert_eq!(benchmark_stats.num_error, 0);

        info!("end of test {:?}", benchmark_stats);
    }
}
