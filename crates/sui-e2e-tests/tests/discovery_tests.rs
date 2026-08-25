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
            .with_epoch_duration_ms(600_000)
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
            // Use a short failed-probe interval so the prober cycles quickly within the wait below
            // (the production default is 1 minute).
            config.address_prober = Some(sui_config::AddressProberConfig {
                failed_interval: Some(Duration::from_secs(2)),
                ..Default::default()
            });
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
                n.connection_monitor_handle_for_testing()
                    .connection_statuses()
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

        // Verify the address prober flags exactly the bad-address validators. The differential we
        // expect, from the good validator's prober, for every bad-address validator: its on-chain
        // (chain) P2P address probes as reachable, but its gossiped (discovery) address — the bad
        // one — never does. We assert on the cumulative `attempts` counters rather than the smoothed
        // `connectable` boolean, so the assertion doesn't depend on exactly when the smoothing window
        // crosses its failure threshold.
        info!("Waiting for address prober cycles...");
        sleep(Duration::from_secs(60)).await;

        let good_metrics = test_cluster
            .swarm
            .node(&validator_names[0])
            .unwrap()
            .get_node_handle()
            .unwrap()
            .with(|n| n.address_prober_metrics_for_testing());

        for bad_peer_id in &bad_peer_ids {
            let peer = bad_peer_id.to_string();
            assert!(
                good_metrics.attempts_value_for_testing(&peer, "p2p", "chain", "reachable") > 0,
                "good validator should reach bad validator {peer} via its chain P2P address"
            );
            assert_eq!(
                good_metrics.attempts_value_for_testing(&peer, "p2p", "discovery", "reachable"),
                0,
                "bad validator {peer}'s discovery P2P address must never probe as reachable"
            );
            let discovery_failures =
                good_metrics.attempts_value_for_testing(&peer, "p2p", "discovery", "timeout")
                    + good_metrics.attempts_value_for_testing(
                        &peer,
                        "p2p",
                        "discovery",
                        "unreachable",
                    );
            assert!(
                discovery_failures > 0,
                "bad validator {peer}'s discovery P2P address should fail probing"
            );
        }

        // The good validator must NOT be flagged: from a bad validator's prober, the good
        // validator's gossiped (discovery) address is the correct, reachable one and never fails.
        let bad_validator_metrics = test_cluster
            .swarm
            .node(&validator_names[1])
            .unwrap()
            .get_node_handle()
            .unwrap()
            .with(|n| n.address_prober_metrics_for_testing());
        let good_peer = good_peer_id.to_string();
        assert!(
            bad_validator_metrics.attempts_value_for_testing(
                &good_peer,
                "p2p",
                "discovery",
                "reachable"
            ) > 0,
            "good validator's discovery P2P address should probe as reachable (not flagged)"
        );
        assert_eq!(
            bad_validator_metrics.attempts_value_for_testing(
                &good_peer,
                "p2p",
                "discovery",
                "timeout"
            ) + bad_validator_metrics.attempts_value_for_testing(
                &good_peer,
                "p2p",
                "discovery",
                "unreachable"
            ),
            0,
            "good validator's discovery P2P address should never fail probing"
        );

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
                n.connection_monitor_handle_for_testing()
                    .connection_statuses()
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
                n.connection_monitor_handle_for_testing()
                    .connection_statuses()
            });
            let status = statuses.get(&removed_peer_id).map(|e| e.value().clone());
            assert_eq!(
                status,
                Some(ConnectionStatus::Disconnected),
                "remaining validator should be disconnected from removed validator after reconfig"
            );
        }
    }

    /// A validator that advertises a bad *consensus* address via discovery is flagged by the prober
    /// on its consensus `discovery` override, while its on-chain (`chain`) consensus address stays
    /// reachable. This exercises the consensus-probe path end-to-end.
    #[sim_test]
    async fn test_prober_detects_bad_consensus_address() {
        let test_cluster = TestClusterBuilder::new()
            .with_num_validators(4)
            .with_epoch_duration_ms(600_000)
            .build()
            .await;

        let validator_names: Vec<_> = test_cluster.get_validator_pubkeys();

        test_cluster.stop_all_validators().await;
        let bad_consensus_addr: sui_types::multiaddr::Multiaddr =
            "/ip4/1.1.1.1/udp/9998".parse().unwrap();
        for name in &validator_names {
            let node = test_cluster.swarm.node(name).unwrap();
            let mut config = node.config();
            config.p2p_config.seed_peers.clear();
            config
                .p2p_config
                .discovery
                .get_or_insert_with(Default::default)
                .use_get_known_peers_v3 = Some(true);
            // Use a short failed-probe interval so the bad consensus address flips the
            // connectability gauge to 0 quickly (the production default is 1 minute).
            config.address_prober = Some(sui_config::AddressProberConfig {
                failed_interval: Some(Duration::from_secs(2)),
                ..Default::default()
            });
        }
        // Validator 1 advertises a bad consensus external address; it propagates as a consensus
        // `Discovery` override on the other validators.
        {
            let node = test_cluster.swarm.node(&validator_names[1]).unwrap();
            let mut config = node.config();
            if let Some(consensus_config) = config.consensus_config.as_mut() {
                consensus_config.external_address = Some(bad_consensus_addr.clone());
            }
        }
        test_cluster.start_all_validators().await;

        // Let discovery propagate the override and the prober run several cycles (the gauge flips
        // after 3 consecutive failures at the 2s failed-probe interval set above).
        info!("Waiting for consensus override propagation and prober cycles...");
        sleep(Duration::from_secs(30)).await;

        // The prober labels consensus endpoints by the hex of the peer's network public key.
        let bad_consensus_label = test_cluster
            .swarm
            .node(&validator_names[1])
            .unwrap()
            .get_node_handle()
            .unwrap()
            .with(|n| {
                sui_node::address_prober::consensus_peer_label_for_testing(
                    n.get_config().network_key_pair().public().0.to_bytes(),
                )
            });

        // From validator 0's prober.
        let metrics = test_cluster
            .swarm
            .node(&validator_names[0])
            .unwrap()
            .get_node_handle()
            .unwrap()
            .with(|n| n.address_prober_metrics_for_testing());

        // The on-chain consensus address is reachable.
        assert!(
            metrics.attempts_value_for_testing(
                &bad_consensus_label,
                "consensus",
                "chain",
                "reachable"
            ) > 0,
            "validator 1's chain consensus address should be reachable"
        );
        // The gossiped (discovery) consensus override — the bad one — is never reachable and fails.
        assert_eq!(
            metrics.attempts_value_for_testing(
                &bad_consensus_label,
                "consensus",
                "discovery",
                "reachable"
            ),
            0,
            "validator 1's bad discovery consensus address must never be reachable"
        );
        assert!(
            metrics.total_attempts_for_testing(&bad_consensus_label, "consensus", "discovery") > 0,
            "validator 1's discovery consensus override should have been probed"
        );
        // With a stable (long) epoch the smoothed differential holds.
        assert_eq!(
            metrics.connectable_for_testing(&bad_consensus_label, "consensus", "chain"),
            1,
            "chain consensus address should be marked connectable"
        );
        assert_eq!(
            metrics.connectable_for_testing(&bad_consensus_label, "consensus", "discovery"),
            0,
            "bad discovery consensus address should be flagged unconnectable"
        );
    }

    /// An all-correct cluster (V3 enabled, no bad addresses) — the prober flags nothing. Guards
    /// against false positives / alert fatigue.
    #[sim_test]
    async fn test_prober_no_false_positives_when_all_correct() {
        let test_cluster = TestClusterBuilder::new()
            .with_num_validators(4)
            .with_epoch_duration_ms(600_000)
            .build()
            .await;

        let validator_names: Vec<_> = test_cluster.get_validator_pubkeys();

        test_cluster.stop_all_validators().await;
        for name in &validator_names {
            let node = test_cluster.swarm.node(name).unwrap();
            let mut config = node.config();
            config.p2p_config.seed_peers.clear();
            config
                .p2p_config
                .discovery
                .get_or_insert_with(Default::default)
                .use_get_known_peers_v3 = Some(true);
        }
        test_cluster.start_all_validators().await;

        // Let discovery propagate and the prober run several cycles.
        info!("Waiting for prober cycles in all-correct cluster...");
        sleep(Duration::from_secs(80)).await;

        let validator_peer_ids: Vec<_> = validator_names
            .iter()
            .map(|name| {
                let node = test_cluster.swarm.node(name).unwrap();
                anemo::PeerId(node.config().network_key_pair().public().0.to_bytes())
            })
            .collect();

        // From validator 0's prober, no probed P2P address (any source, any other validator) failed,
        // and nothing is flagged unconnectable.
        let metrics = test_cluster
            .swarm
            .node(&validator_names[0])
            .unwrap()
            .get_node_handle()
            .unwrap()
            .with(|n| n.address_prober_metrics_for_testing());

        for peer_id in &validator_peer_ids[1..] {
            let peer = peer_id.to_string();
            for source in ["chain", "discovery"] {
                let total = metrics.total_attempts_for_testing(&peer, "p2p", source);
                if total == 0 {
                    continue; // this source was not advertised for this peer
                }
                let failures =
                    total - metrics.attempts_value_for_testing(&peer, "p2p", source, "reachable");
                assert_eq!(
                    failures, 0,
                    "no probe failures expected for {peer}/{source} in an all-correct cluster"
                );
                assert_ne!(
                    metrics.connectable_for_testing(&peer, "p2p", source),
                    0,
                    "no peer should be flagged unconnectable in an all-correct cluster ({peer}/{source})"
                );
            }
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
