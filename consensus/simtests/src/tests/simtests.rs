// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#[cfg(msim)]
mod test {
    use crate::node::{AuthorityNode, Config};
    use consensus_config::{
        Authority, AuthorityIndex, AuthorityKeyPair, Committee, Epoch, NetworkKeyPair,
        ProtocolKeyPair, Stake,
    };
    use mysten_network::Multiaddr;
    use prometheus::Registry;
    use rand::{rngs::StdRng, SeedableRng as _};
    use std::{sync::Arc, time::Duration};
    use sui_config::local_ip_utils;
    use sui_macros::sim_test;
    use sui_protocol_config::ProtocolConfig;
    use sui_simulator::{
        configs::{bimodal_latency_ms, env_config, uniform_latency_ms},
        SimConfig,
    };
    use tempfile::TempDir;
    use tokio::time::sleep;
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
    async fn test_committee_start_simple() {
        telemetry_subscribers::init_for_testing();
        for median_based_timestamp in vec![true, false] {
            tracing::info!("Running with median_based_timestamp = {median_based_timestamp}");
            let db_registry = Registry::new();
            DBMetrics::init(&db_registry);

            const NUM_OF_AUTHORITIES: usize = 10;
            let (committee, keypairs) =
                local_committee_and_keys(0, [1; NUM_OF_AUTHORITIES].to_vec());
            let mut protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
            protocol_config.set_consensus_gc_depth_for_testing(3);
            protocol_config
                .set_consensus_median_based_commit_timestamp_for_testing(median_based_timestamp);

            let mut authorities = Vec::with_capacity(committee.size());
            let mut transaction_clients = Vec::with_capacity(committee.size());
            let mut boot_counters = [0; NUM_OF_AUTHORITIES];
            let mut clock_drifts = [0; NUM_OF_AUTHORITIES];
            clock_drifts[0] = 50;
            clock_drifts[1] = 100;
            clock_drifts[2] = 120;

            for (authority_index, _authority_info) in committee.authorities() {
                // Introduce a non-trivial clock drift for the first node (it's time will be ahead of the others). This will provide extra reassurance
                // around the block timestamp checks.
                let config = Config {
                    authority_index,
                    db_dir: Arc::new(TempDir::new().unwrap()),
                    committee: committee.clone(),
                    keypairs: keypairs.clone(),
                    network_type: sui_protocol_config::ConsensusNetwork::Tonic,
                    boot_counter: boot_counters[authority_index],
                    protocol_config: protocol_config.clone(),
                    clock_drift: clock_drifts[authority_index.value() as usize],
                };
                let node = AuthorityNode::new(config);

                if authority_index != AuthorityIndex::new_for_test(NUM_OF_AUTHORITIES as u32 - 1) {
                    node.start().await.unwrap();
                    node.spawn_committed_subdag_consumer().unwrap();

                    let client = node.transaction_client();
                    transaction_clients.push(client);
                }

                boot_counters[authority_index] += 1;
                authorities.push(node);
            }

            let transaction_clients_clone = transaction_clients.clone();
            let _handle = tokio::spawn(async move {
                const NUM_TRANSACTIONS: u16 = 1000;

                for i in 0..NUM_TRANSACTIONS {
                    let txn = vec![i as u8; 16];
                    transaction_clients_clone[i as usize % transaction_clients_clone.len()]
                        .submit(vec![txn])
                        .await
                        .unwrap();
                }
            });

            // wait for authorities
            sleep(Duration::from_secs(60)).await;

            // Now start the fourth authority and let it start
            tracing::info!(authority =% NUM_OF_AUTHORITIES - 1, "Starting authority and waiting for it to catch up");
            authorities[NUM_OF_AUTHORITIES - 1].start().await.unwrap();
            authorities[NUM_OF_AUTHORITIES - 1]
                .spawn_committed_subdag_consumer()
                .unwrap();

            // Wait for it to catch up
            sleep(Duration::from_secs(230)).await;
            let commit_consumer_monitor =
                authorities[NUM_OF_AUTHORITIES - 1].commit_consumer_monitor();
            let highest_committed_index = commit_consumer_monitor.highest_handled_commit();
            assert!(
                highest_committed_index >= 80,
                "Highest handled commit {highest_committed_index} < 80"
            );
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
            let authority_keypair = AuthorityKeyPair::generate(&mut rng);
            let protocol_keypair = ProtocolKeyPair::generate(&mut rng);
            let network_keypair = NetworkKeyPair::generate(&mut rng);
            authorities.push(Authority {
                stake,
                address: get_available_local_address(),
                hostname: format!("test_host_{i}").to_string(),
                authority_key: authority_keypair.public(),
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
}
