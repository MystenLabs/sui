// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod test {
    use std::sync::Arc;
    use std::time::Duration;
    use sui_config::p2p::StateSyncConfig;
    use sui_macros::sim_test;
    use sui_simulator::SimConfig;
    use sui_simulator::configs::uniform_latency_ms;
    use sui_simulator::net::NetSim;
    use sui_simulator::net::config::{InterNodeLatencyMap, LatencyDistribution};
    use sui_simulator::plugin::simulator;
    use test_cluster::TestClusterBuilder;

    fn simple_latency_config() -> SimConfig {
        uniform_latency_ms(1..10)
    }

    #[sim_test(config = "simple_latency_config()")]
    async fn test_state_sync_with_degraded_peers() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();

        let mut test_cluster = TestClusterBuilder::new()
            .with_num_validators(1)
            .build()
            .await;

        let _healthy_fullnode = test_cluster.spawn_new_fullnode().await;
        let degraded_fullnode_1 = test_cluster.spawn_new_fullnode().await;
        let degraded_fullnode_2 = test_cluster.spawn_new_fullnode().await;
        let syncing_node = test_cluster.spawn_new_fullnode().await;

        let validator_id =
            test_cluster.swarm.validator_node_handles()[0].with(|n| n.get_sim_node_id());
        let degraded_id_1 = degraded_fullnode_1.sui_node.with(|n| n.get_sim_node_id());
        let degraded_id_2 = degraded_fullnode_2.sui_node.with(|n| n.get_sim_node_id());
        let syncing_node_id = syncing_node.sui_node.with(|n| n.get_sim_node_id());

        tokio::time::sleep(Duration::from_secs(30)).await;

        let net = simulator::<NetSim>();
        let high_latency =
            LatencyDistribution::uniform(Duration::from_secs(25)..Duration::from_secs(35));
        net.update_config(|cfg| {
            let latency_map = InterNodeLatencyMap::new()
                .with_symmetric_link(syncing_node_id, validator_id, high_latency.clone())
                .with_symmetric_link(syncing_node_id, degraded_id_1, high_latency.clone())
                .with_symmetric_link(syncing_node_id, degraded_id_2, high_latency.clone());
            cfg.latency.inter_node_latency = Some(Arc::new(latency_map));
        });

        tokio::time::sleep(Duration::from_secs(30)).await;

        let target = test_cluster.swarm.validator_node_handles()[0].with(|n| {
            n.state()
                .get_checkpoint_store()
                .get_highest_synced_checkpoint_seq_number()
                .unwrap()
                .unwrap_or(0)
        });

        let timeout = Duration::from_secs(60);
        let start = tokio::time::Instant::now();
        loop {
            let syncing = syncing_node.sui_node.with(|n| {
                n.state()
                    .get_checkpoint_store()
                    .get_highest_synced_checkpoint_seq_number()
                    .unwrap()
                    .unwrap_or(0)
            });

            if syncing >= target {
                break;
            }
            if start.elapsed() > timeout {
                panic!(
                    "Syncing node failed to catch up: got {}, expected {}",
                    syncing, target
                );
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }

        net.update_config(|cfg| cfg.latency.inter_node_latency = None);
    }

    #[sim_test(config = "simple_latency_config()")]
    async fn test_disconnect_consistently_failing_peers() {
        use rand::rngs::OsRng;

        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();

        let state_sync_config = StateSyncConfig {
            peer_disconnect_threshold_ms: Some(10_000), // 10 seconds of consistent failing
            min_peers_for_disconnect: Some(0),
            peer_scoring_window_ms: Some(120_000), // 2 minute window to accumulate samples
            interval_period_ms: Some(1_000),       // 1 second tick
            timeout_ms: Some(500),                 // 500ms timeout
            checkpoint_content_timeout_ms: Some(500),
            ..Default::default()
        };

        let mut test_cluster = TestClusterBuilder::new()
            .with_num_validators(1)
            .with_state_sync_config(state_sync_config.clone())
            .build()
            .await;

        let healthy_config = test_cluster
            .fullnode_config_builder()
            .with_state_sync_config(state_sync_config.clone())
            .build(&mut OsRng, test_cluster.swarm.config());
        let healthy_fullnode = test_cluster
            .start_fullnode_from_config(healthy_config)
            .await;

        let degraded_config = test_cluster
            .fullnode_config_builder()
            .with_state_sync_config(state_sync_config.clone())
            .build(&mut OsRng, test_cluster.swarm.config());
        let degraded_fullnode = test_cluster
            .start_fullnode_from_config(degraded_config)
            .await;

        let validator_id =
            test_cluster.swarm.validator_node_handles()[0].with(|n| n.get_sim_node_id());
        let healthy_id = healthy_fullnode.sui_node.with(|n| n.get_sim_node_id());
        let degraded_id = degraded_fullnode.sui_node.with(|n| n.get_sim_node_id());
        let default_fullnode_id = test_cluster
            .fullnode_handle
            .sui_node
            .with(|n| n.get_sim_node_id());

        // Let the cluster produce some checkpoints before starting the syncing node
        tokio::time::sleep(Duration::from_secs(10)).await;

        // Build and start the syncing node - it will be behind and need to sync
        let syncing_config = test_cluster
            .fullnode_config_builder()
            .with_state_sync_config(state_sync_config)
            .build(&mut OsRng, test_cluster.swarm.config());
        let syncing_node = test_cluster
            .start_fullnode_from_config(syncing_config)
            .await;
        let syncing_node_id = syncing_node.sui_node.with(|n| n.get_sim_node_id());

        // Wait until the syncing node has actually synced at least one checkpoint,
        // proving it discovered peers and the sync machinery is working.
        let wait_timeout = Duration::from_secs(30);
        let wait_start = tokio::time::Instant::now();
        loop {
            let synced = syncing_node.sui_node.with(|n| {
                n.state()
                    .get_checkpoint_store()
                    .get_highest_synced_checkpoint_seq_number()
                    .unwrap()
                    .unwrap_or(0)
            });
            if synced > 0 {
                tracing::info!("Syncing node reached checkpoint {synced}, injecting latency");
                break;
            }
            if wait_start.elapsed() > wait_timeout {
                panic!("Syncing node didn't sync any checkpoints within {wait_timeout:?}");
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Inject latency to ALL peers from the syncing node's perspective.
        // Use latency longer than RPC timeout (500ms) but short enough to not break connections.
        let net = simulator::<NetSim>();
        let timeout_latency =
            LatencyDistribution::uniform(Duration::from_millis(1000)..Duration::from_millis(2000));
        net.update_config(|cfg| {
            let latency_map = InterNodeLatencyMap::new()
                .with_symmetric_link(syncing_node_id, validator_id, timeout_latency.clone())
                .with_symmetric_link(syncing_node_id, healthy_id, timeout_latency.clone())
                .with_symmetric_link(syncing_node_id, degraded_id, timeout_latency.clone())
                .with_symmetric_link(
                    syncing_node_id,
                    default_fullnode_id,
                    timeout_latency.clone(),
                );
            cfg.latency.inter_node_latency = Some(Arc::new(latency_map));
        });

        // Poll for disconnects. We need:
        // - ~5s for 10 timeout failures at 500ms each to trigger is_failing()
        // - 10s of consistent failing to trigger disconnect
        // - Buffer for timing variability
        let timeout = Duration::from_secs(60);
        let start = tokio::time::Instant::now();
        let mut disconnects = 0;
        while start.elapsed() < timeout {
            disconnects = syncing_node
                .sui_node
                .with(|n| n.state_sync_handle().get_peers_disconnected_for_failure());
            if disconnects > 0 {
                tracing::info!("Peer disconnected for failure after {:?}", start.elapsed());
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        assert!(
            disconnects > 0,
            "Expected at least one peer disconnect for failure, got {} after {:?}",
            disconnects,
            start.elapsed()
        );

        // Remove latency so syncing can complete
        net.update_config(|cfg| cfg.latency.inter_node_latency = None);

        let target = test_cluster.swarm.validator_node_handles()[0].with(|n| {
            n.state()
                .get_checkpoint_store()
                .get_highest_synced_checkpoint_seq_number()
                .unwrap()
                .unwrap_or(0)
        });

        let timeout = Duration::from_secs(60);
        let start = tokio::time::Instant::now();
        loop {
            let syncing = syncing_node.sui_node.with(|n| {
                n.state()
                    .get_checkpoint_store()
                    .get_highest_synced_checkpoint_seq_number()
                    .unwrap()
                    .unwrap_or(0)
            });

            if syncing >= target {
                break;
            }
            if start.elapsed() > timeout {
                panic!(
                    "Syncing node failed to catch up: got {}, expected {}",
                    syncing, target
                );
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}
