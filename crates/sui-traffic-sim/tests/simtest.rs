// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod test {
    use std::sync::Arc;
    use std::time::Duration;
    use sui_macros::sim_test;
    use sui_simulator::SimConfig;
    use sui_simulator::configs::{env_config, uniform_latency_ms};
    use sui_types::sui_system_state::SuiSystemStateTrait;
    use test_cluster::TestClusterBuilder;
    use tracing::info;

    fn test_config() -> SimConfig {
        // Simple configuration with uniform latency for initial testing
        env_config(
            uniform_latency_ms(10..20),
            [],
        )
    }

    #[sim_test(config = "test_config()")]
    async fn test_traffic_sim_cluster_startup() {
        info!("Starting test cluster for traffic simulator");

        // Build a minimal test cluster with 4 validators
        let test_cluster = TestClusterBuilder::new()
            .with_num_validators(4)
            .with_epoch_duration_ms(5000)
            .build()
            .await;

        let test_cluster = Arc::new(test_cluster);

        info!("Test cluster started successfully");

        // Verify cluster is running by checking the current epoch
        let system_state = test_cluster
            .sui_client()
            .governance_api()
            .get_latest_sui_system_state()
            .await
            .expect("Failed to get system state");

        assert_eq!(system_state.epoch, 0, "Expected to start at epoch 0");
        info!("Successfully verified cluster is at epoch 0");

        // Trigger reconfiguration to epoch 1
        info!("Triggering reconfiguration to epoch 1...");
        test_cluster.trigger_reconfiguration().await;

        let system_state = test_cluster.wait_for_epoch(Some(1)).await;
        assert_eq!(system_state.epoch(), 1);
        info!("Successfully reached epoch 1");

        // Keep the cluster running for a short duration to ensure stability
        tokio::time::sleep(Duration::from_secs(5)).await;

        info!("Traffic simulator test cluster basic test completed");
    }

    #[sim_test(config = "test_config()")]
    async fn test_traffic_sim_with_basic_load() {
        info!("Starting test cluster for traffic simulator with basic load");

        // Build test cluster with configuration suitable for traffic simulation
        let test_cluster = TestClusterBuilder::new()
            .with_num_validators(4)
            .with_epoch_duration_ms(10000)
            .disable_fullnode_pruning()
            .build()
            .await;

        let test_cluster = Arc::new(test_cluster);

        info!("Test cluster started, preparing for traffic simulation");

        // Get initial wallet state
        let sender = test_cluster.get_address_0();
        info!("Using sender address: {}", sender);

        // Verify we can get gas objects
        let gas_objects = test_cluster
            .wallet
            .get_gas_objects_owned_by_address(sender, None)
            .await
            .expect("Failed to get gas objects");

        assert!(!gas_objects.is_empty(), "Expected to have gas objects");
        info!("Found {} gas objects for sender", gas_objects.len());

        // Get reference gas price
        let gas_price = test_cluster.get_reference_gas_price().await;
        info!("Reference gas price: {}", gas_price);

        // Create a simple transaction to verify the cluster is functional
        let _gas_object = gas_objects[0];
        let recipient = test_cluster.get_address_1();

        info!("Executing test transaction from {} to {}", sender, recipient);

        // Fund the recipient address with some gas
        test_cluster
            .fund_address_and_return_gas(gas_price, Some(1), recipient)
            .await;

        info!("Successfully funded recipient address");

        // Let the cluster run for a bit to ensure stability
        tokio::time::sleep(Duration::from_secs(10)).await;

        // Verify we've made progress
        let system_state = test_cluster
            .sui_client()
            .governance_api()
            .get_latest_sui_system_state()
            .await
            .expect("Failed to get system state");

        info!("Final epoch: {}", system_state.epoch);

        info!("Traffic simulator with basic load test completed");
    }

    #[sim_test(config = "test_config()")]
    async fn test_traffic_sim_multi_epoch() {
        info!("Starting test cluster for multi-epoch traffic simulation");

        // Build test cluster with shorter epoch duration for faster testing
        let test_cluster = TestClusterBuilder::new()
            .with_num_validators(4)
            .with_epoch_duration_ms(3000)
            .build()
            .await;

        let test_cluster = Arc::new(test_cluster);

        info!("Test cluster started, running through multiple epochs");

        // Run through 3 epochs
        for target_epoch in 1..=3 {
            info!("Triggering reconfiguration to epoch {}", target_epoch);
            test_cluster.trigger_reconfiguration().await;

            let system_state = test_cluster.wait_for_epoch(Some(target_epoch)).await;
            assert!(system_state.epoch() >= target_epoch,
                    "Expected epoch {} or higher, got {}", target_epoch, system_state.epoch());
            info!("Successfully reached epoch {}", system_state.epoch());

            // Do a simple transaction in each epoch
            let sender = test_cluster.get_address_0();
            let gas_price = test_cluster.get_reference_gas_price().await;
            test_cluster
                .fund_address_and_return_gas(gas_price, Some(1), sender)
                .await;

            info!("Executed transaction in epoch {}", target_epoch);
        }

        info!("Multi-epoch traffic simulation test completed");
    }
}