// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod test {

    use std::str::FromStr;
    use std::sync::Arc;
    use std::time::Duration;
    use sui_config::SUI_KEYSTORE_FILENAME;
    use sui_core::authority_aggregator::AuthorityAggregatorBuilder;
    use test_utils::{
        messages::get_gas_object_with_wallet_context, network::init_cluster_builder_env_aware,
    };

    use sui_benchmark::{
        drivers::{bench_driver::BenchDriver, driver::Driver, Interval},
        util::get_ed25519_keypair_from_keystore,
        workloads::make_combination_workload,
        LocalValidatorAggregatorProxy, ValidatorProxy,
    };

    use sui_macros::sim_test;
    use sui_simulator::{configs::*, SimConfig};

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
    async fn test_simulated_load() {
        let test_cluster = init_cluster_builder_env_aware()
            .with_num_validators(get_var("SIM_STRESS_TEST_NUM_VALIDATORS", 4))
            .build()
            .await
            .unwrap();
        let swarm = &test_cluster.swarm;
        let context = &test_cluster.wallet;
        let sender = test_cluster.get_address_0();

        let keystore_path = swarm.dir().join(SUI_KEYSTORE_FILENAME);
        let ed25519_keypair = get_ed25519_keypair_from_keystore(keystore_path, &sender).unwrap();

        let gas = get_gas_object_with_wallet_context(context, &sender)
            .await
            .expect("Expect {sender} to have at least one gas object");

        // The default test parameters are somewhat conservative in order to keep the running time
        // of the test reasonable in CI.
        let mut workloads = vec![make_combination_workload(
            get_var("SIM_STRESS_TEST_QPS", 10),
            get_var("SIM_STRESS_TEST_WORKERS", 10),
            get_var("SIM_STRESS_TEST_IFR", 2),
            gas.0,
            sender,
            Arc::new(ed25519_keypair),
            10, // num_transfer_accounts
            1,  // shared_counter_weight
            1,  // transfer_object_weight
        )];

        let (aggregator, _) = AuthorityAggregatorBuilder::from_network_config(swarm.config())
            .build()
            .unwrap();
        let proxy: Arc<dyn ValidatorProxy + Send + Sync> = Arc::new(
            LocalValidatorAggregatorProxy::from_auth_agg(Arc::new(aggregator)),
        );

        for w in workloads.iter_mut() {
            w.workload
                .init(
                    get_var("SIM_STRESS_TEST_NUM_SHARED_OBJECTS", 5),
                    proxy.clone(),
                )
                .await;
        }

        let driver = BenchDriver::new(5);
        let registry = prometheus::Registry::new();

        // Use 0 for unbounded
        let test_duration_secs = get_var("SIM_STRESS_TEST_DURATION_SECS", 10);
        let test_duration = if test_duration_secs == 0 {
            Duration::MAX
        } else {
            Duration::from_secs(test_duration_secs)
        };
        let interval = Interval::Time(test_duration);

        let show_progress = interval.is_unbounded();
        let stats = driver
            .run(workloads, proxy, &registry, show_progress, interval)
            .await
            .unwrap();

        assert_eq!(stats.num_error, 0);
    }
}
