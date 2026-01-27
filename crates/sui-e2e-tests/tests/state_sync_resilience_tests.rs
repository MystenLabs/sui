// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod test {
    use std::sync::Arc;
    use std::time::Duration;
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
}
