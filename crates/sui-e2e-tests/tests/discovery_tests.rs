// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod test {
    use mysten_network::anemo_connection_monitor::ConnectionStatus;
    use std::collections::HashSet;
    use std::time::Duration;
    use sui_macros::sim_test;
    use sui_node::SuiNodeHandle;
    use sui_simulator::anemo;
    use sui_test_transaction_builder::{TestTransactionBuilder, emit_new_random_u128};
    use sui_types::crypto::KeypairTraits;
    use sui_types::effects::TransactionEffectsAPI;
    use sui_types::object::Owner;
    use test_cluster::{TestCluster, TestClusterBuilder};
    use tokio::time::sleep;
    use tracing::info;

    /// Tests that the network remains functional when validators advertise
    /// incorrect P2P addresses via discovery gossip, and that the bad
    /// Discovery addresses actually override Chain addresses (preventing
    /// connections between bad-address validators after restart).
    #[sim_test]
    async fn test_network_resilience_with_incorrect_discovery_addresses() {
        let test_cluster = TestClusterBuilder::new()
            .with_num_validators(4)
            .with_epoch_duration_ms(30000)
            .build()
            .await;

        let validator_names: Vec<_> = test_cluster.get_validator_pubkeys();
        assert_eq!(validator_names.len(), 4);

        // Stop all validators so we can modify configs.
        test_cluster.stop_all_validators().await;

        // Clear seed_peers on all validators so they rely on Chain addresses
        // for inter-validator connectivity (not the hardcoded seed addresses).
        // Enable V3 gossip so Discovery addresses flow through EndpointManager.
        // Set bad external_address on validators 1..3, leave validator 0 correct.
        let bad_addr: sui_types::multiaddr::Multiaddr = "/ip4/1.1.1.1/udp/9999".parse().unwrap();
        for name in &validator_names {
            let node = test_cluster.swarm.node(name).unwrap();
            let mut config = node.config();
            config.p2p_config.seed_peers.clear();
            let disc = config
                .p2p_config
                .discovery
                .get_or_insert_with(Default::default);
            disc.use_get_known_peers_v3 = Some(true);
        }
        for name in &validator_names[1..] {
            let node = test_cluster.swarm.node(name).unwrap();
            node.config().p2p_config.external_address = Some(bad_addr.clone());
        }

        // Start all validators — they initially connect via Chain addresses,
        // then discovery gossip propagates the bad addresses from validators 1..3.
        // The peer cache is saved as TrustedPeersUpdated fires during gossip.
        test_cluster.start_all_validators().await;

        // Wait for discovery gossip to propagate and peer cache to be written.
        info!("Waiting for discovery propagation...");
        sleep(Duration::from_secs(20)).await;

        // Restart all validators. On startup, cached NodeInfo with bad Discovery
        // addresses is loaded. When Chain addresses arrive, the cached bad
        // Discovery addresses are injected into peer_addresses (higher priority),
        // so known_peers is set to bad addresses from the start. No connections
        // to bad-address validators are established.
        info!("Restarting all validators...");
        test_cluster.stop_all_validators().await;
        test_cluster.start_all_validators().await;

        // Wait for connections to stabilize.
        sleep(Duration::from_secs(15)).await;

        // Verify star topology: validators with bad discovery addresses
        // should only be connected to the good validator (and possibly fullnodes),
        // not to other bad-address validators.
        info!("Checking validator connectivity...");
        let validator_peer_ids: Vec<_> = validator_names
            .iter()
            .map(|name| {
                let node = test_cluster.swarm.node(name).unwrap();
                anemo::PeerId(node.config().network_key_pair().public().0.to_bytes())
            })
            .collect();
        let good_peer_id = validator_peer_ids[0];
        let bad_peer_ids: HashSet<_> = validator_peer_ids[1..].iter().copied().collect();

        for (i, name) in validator_names[1..].iter().enumerate() {
            let node = test_cluster.swarm.node(name).unwrap();
            let handle = node.get_node_handle().unwrap();
            let statuses = handle.with(|n| {
                n.connection_monitor_status_for_testing()
                    .connection_statuses
                    .clone()
            });
            let connected_peer_ids: Vec<_> = statuses.iter().map(|entry| *entry.key()).collect();
            info!(
                bad_validator_index = i + 1,
                ?connected_peer_ids,
                "Bad-address validator connectivity"
            );
            assert!(
                connected_peer_ids.contains(&good_peer_id),
                "Bad-address validator should be connected to the good validator"
            );
            let connected_bad_validators: Vec<_> = connected_peer_ids
                .iter()
                .filter(|id| bad_peer_ids.contains(id))
                .collect();
            assert!(
                connected_bad_validators.is_empty(),
                "Bad-address validator should not be connected to other bad-address validators, \
                 but is connected to {connected_bad_validators:?}"
            );
        }

        // Publish the "basics" example package (needed for randomness tx).
        info!("Publishing basics package...");
        let package_id = {
            let sender = test_cluster.get_address_0();
            let gas = test_cluster
                .wallet
                .get_one_gas_object_owned_by_address(sender)
                .await
                .unwrap()
                .unwrap();
            let rgp = test_cluster.get_reference_gas_price().await;
            let publish_tx = test_cluster
                .wallet
                .sign_transaction(
                    &sui_test_transaction_builder::TestTransactionBuilder::new(sender, gas, rgp)
                        .publish_examples("basics")
                        .await
                        .build(),
                )
                .await;
            let response = test_cluster
                .wallet
                .execute_transaction_must_succeed(publish_tx)
                .await;
            response
                .effects
                .created()
                .iter()
                .find(|(_, owner)| *owner == Owner::Immutable)
                .expect("Should have created a package")
                .0
                .0
        };
        info!(?package_id, "Basics package published");

        // Send a randomness transaction to prove P2P randomness protocol works
        // despite degraded connectivity.
        info!("Sending randomness transaction...");
        let response = emit_new_random_u128(&test_cluster.wallet, package_id).await;
        assert!(
            response.effects.status().is_ok(),
            "Randomness transaction should succeed: {:?}",
            response.effects.status()
        );
        info!("Randomness transaction succeeded");

        // Trigger reconfiguration to prove epoch change works.
        info!("Triggering reconfiguration...");
        test_cluster.trigger_reconfiguration().await;
        info!("Reconfiguration succeeded");
    }

    /// Tests that when a validator is removed from the committee, remaining
    /// validators disconnect from it after reconfiguration (because its Chain
    /// addresses are cleared and no other address source is configured).
    #[sim_test]
    async fn test_removed_validator_disconnected_after_reconfig() {
        let test_cluster = TestClusterBuilder::new()
            .with_num_validators(5)
            .build()
            .await;

        // Stop all validators to clear seed_peers, so Chain is the only
        // address source between validators.
        test_cluster.stop_all_validators().await;
        let validator_names: Vec<_> = test_cluster.get_validator_pubkeys();
        for name in &validator_names {
            let node = test_cluster.swarm.node(name).unwrap();
            node.config().p2p_config.seed_peers.clear();
        }
        test_cluster.start_all_validators().await;

        // Wait for connections to stabilize via Chain addresses.
        sleep(Duration::from_secs(10)).await;

        // Pick the last validator to remove.
        let validator_to_remove = test_cluster.swarm.validator_node_handles().pop().unwrap();
        let removed_peer_id = validator_to_remove
            .with(|node| anemo::PeerId(node.get_config().network_key_pair().public().0.to_bytes()));

        // Verify remaining validators are connected to the to-be-removed validator.
        let remaining_handles: Vec<_> = test_cluster
            .swarm
            .validator_node_handles()
            .into_iter()
            .filter(|h| {
                h.with(|n| anemo::PeerId(n.get_config().network_key_pair().public().0.to_bytes()))
                    != removed_peer_id
            })
            .collect();

        for handle in &remaining_handles {
            let statuses = handle.with(|n| {
                n.connection_monitor_status_for_testing()
                    .connection_statuses
                    .clone()
            });
            let status = statuses.get(&removed_peer_id).map(|e| e.value().clone());
            assert_eq!(
                status,
                Some(ConnectionStatus::Connected),
                "remaining validator should be connected to the to-be-removed validator before reconfig"
            );
        }

        // Request removal and trigger reconfiguration.
        execute_remove_validator_tx(&test_cluster, &validator_to_remove).await;
        test_cluster.trigger_reconfiguration().await;

        // Stop the removed validator so it can't re-establish connections.
        let removed_name = validator_to_remove.with(|node| node.state().name);
        test_cluster.stop_node(&removed_name);

        // Wait for disconnect to take effect.
        sleep(Duration::from_secs(15)).await;

        // Verify remaining validators are no longer connected to the removed validator.
        for handle in &remaining_handles {
            let statuses = handle.with(|n| {
                n.connection_monitor_status_for_testing()
                    .connection_statuses
                    .clone()
            });
            let status = statuses.get(&removed_peer_id).map(|e| e.value().clone());
            assert_eq!(
                status,
                Some(ConnectionStatus::Disconnected),
                "remaining validator should be disconnected from removed validator after reconfig"
            );
        }
    }

    async fn execute_remove_validator_tx(test_cluster: &TestCluster, handle: &SuiNodeHandle) {
        let address = handle.with(|node| node.get_config().sui_address());
        let gas = test_cluster
            .wallet
            .get_one_gas_object_owned_by_address(address)
            .await
            .unwrap()
            .unwrap();

        let rgp = test_cluster.get_reference_gas_price().await;
        let tx = handle.with(|node| {
            TestTransactionBuilder::new(address, gas, rgp)
                .call_request_remove_validator()
                .build_and_sign(node.get_config().account_key_pair.keypair())
        });
        test_cluster.execute_transaction(tx).await;
    }
}
