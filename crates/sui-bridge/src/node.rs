// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    action_executor::BridgeActionExecutor,
    client::bridge_authority_aggregator::BridgeAuthorityAggregator,
    config::{BridgeClientConfig, BridgeNodeConfig},
    eth_syncer::EthSyncer,
    events::init_all_struct_tags,
    metrics::BridgeMetrics,
    orchestrator::BridgeOrchestrator,
    server::{handler::BridgeRequestHandler, run_server, BridgeNodePublicMetadata},
    storage::BridgeOrchestratorTables,
    sui_syncer::SuiSyncer,
};
use ethers::types::Address as EthAddress;
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use sui_types::{
    bridge::{BRIDGE_COMMITTEE_MODULE_NAME, BRIDGE_MODULE_NAME},
    event::EventID,
    Identifier,
};
use tokio::task::JoinHandle;
use tracing::info;

pub async fn run_bridge_node(
    config: BridgeNodeConfig,
    metadata: BridgeNodePublicMetadata,
    prometheus_registry: prometheus::Registry,
) -> anyhow::Result<JoinHandle<()>> {
    init_all_struct_tags();
    let metrics = Arc::new(BridgeMetrics::new(&prometheus_registry));
    let (server_config, client_config) = config.validate(metrics.clone()).await?;

    // Start Client
    let _handles = if let Some(client_config) = client_config {
        start_client_components(client_config, metrics.clone()).await
    } else {
        Ok(vec![])
    }?;

    // Start Server
    let socket_address = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        server_config.server_listen_port,
    );
    Ok(run_server(
        &socket_address,
        BridgeRequestHandler::new(
            server_config.key,
            server_config.sui_client,
            server_config.eth_client,
            server_config.approved_governance_actions,
            metrics.clone(),
        ),
        metrics,
        Arc::new(metadata),
    ))
}

// TODO: is there a way to clean up the overrides after it's stored in DB?
async fn start_client_components(
    client_config: BridgeClientConfig,
    metrics: Arc<BridgeMetrics>,
) -> anyhow::Result<Vec<JoinHandle<()>>> {
    let store: std::sync::Arc<BridgeOrchestratorTables> =
        BridgeOrchestratorTables::new(&client_config.db_path.join("client"));
    let sui_modules_to_watch = get_sui_modules_to_watch(
        &store,
        client_config.sui_bridge_module_last_processed_event_id_override,
    );
    let eth_contracts_to_watch = get_eth_contracts_to_watch(
        &store,
        &client_config.eth_contracts,
        client_config.eth_contracts_start_block_fallback,
        client_config.eth_contracts_start_block_override,
    );

    let sui_client = client_config.sui_client.clone();

    let mut all_handles = vec![];
    let (task_handles, eth_events_rx, _) =
        EthSyncer::new(client_config.eth_client.clone(), eth_contracts_to_watch)
            .run(metrics.clone())
            .await
            .expect("Failed to start eth syncer");
    all_handles.extend(task_handles);

    let (task_handles, sui_events_rx) =
        SuiSyncer::new(client_config.sui_client, sui_modules_to_watch)
            .run(Duration::from_secs(2))
            .await
            .expect("Failed to start sui syncer");
    all_handles.extend(task_handles);

    let committee = Arc::new(
        sui_client
            .get_bridge_committee()
            .await
            .expect("Failed to get committee"),
    );
    let bridge_auth_agg = BridgeAuthorityAggregator::new(committee);
    let sui_token_type_tags = sui_client.get_token_id_map().await.unwrap();
    let (token_type_tags_tx, token_type_tags_rx) = tokio::sync::watch::channel(sui_token_type_tags);

    let bridge_action_executor = BridgeActionExecutor::new(
        sui_client.clone(),
        Arc::new(bridge_auth_agg),
        store.clone(),
        client_config.key,
        client_config.sui_address,
        client_config.gas_object_ref.0,
        token_type_tags_rx,
        metrics.clone(),
    )
    .await;

    let orchestrator = BridgeOrchestrator::new(
        sui_client,
        sui_events_rx,
        eth_events_rx,
        store.clone(),
        token_type_tags_tx,
        metrics,
    );

    all_handles.extend(orchestrator.run(bridge_action_executor).await);
    Ok(all_handles)
}

fn get_sui_modules_to_watch(
    store: &std::sync::Arc<BridgeOrchestratorTables>,
    sui_bridge_module_last_processed_event_id_override: Option<EventID>,
) -> HashMap<Identifier, Option<EventID>> {
    let sui_bridge_modules = vec![
        BRIDGE_MODULE_NAME.to_owned(),
        BRIDGE_COMMITTEE_MODULE_NAME.to_owned(),
    ];
    if let Some(cursor) = sui_bridge_module_last_processed_event_id_override {
        info!("Overriding cursor for sui bridge modules to {:?}", cursor);
        return HashMap::from_iter(
            sui_bridge_modules
                .iter()
                .map(|module| (module.clone(), Some(cursor))),
        );
    }

    let sui_bridge_module_stored_cursor = store
        .get_sui_event_cursors(&sui_bridge_modules)
        .expect("Failed to get eth sui event cursors from storage");
    let mut sui_modules_to_watch = HashMap::new();
    for (module_identifier, cursor) in sui_bridge_modules
        .iter()
        .zip(sui_bridge_module_stored_cursor)
    {
        if cursor.is_none() {
            info!(
                "No cursor found for sui bridge module {} in storage or config override, query start from the beginning.",
                module_identifier
            );
        }
        sui_modules_to_watch.insert(module_identifier.clone(), cursor);
    }
    sui_modules_to_watch
}

fn get_eth_contracts_to_watch(
    store: &std::sync::Arc<BridgeOrchestratorTables>,
    eth_contracts: &[EthAddress],
    eth_contracts_start_block_fallback: u64,
    eth_contracts_start_block_override: Option<u64>,
) -> HashMap<EthAddress, u64> {
    let stored_eth_cursors = store
        .get_eth_event_cursors(eth_contracts)
        .expect("Failed to get eth event cursors from storage");
    let mut eth_contracts_to_watch = HashMap::new();
    for (contract, stored_cursor) in eth_contracts.iter().zip(stored_eth_cursors) {
        // start block precedence:
        // eth_contracts_start_block_override > stored cursor > eth_contracts_start_block_fallback
        match (eth_contracts_start_block_override, stored_cursor) {
            (Some(override_), _) => {
                eth_contracts_to_watch.insert(*contract, override_);
                info!(
                    "Overriding cursor for eth bridge contract {} to {}. Stored cursor: {:?}",
                    contract, override_, stored_cursor
                );
            }
            (None, Some(stored_cursor)) => {
                // +1: The stored value is the last block that was processed, so we start from the next block.
                eth_contracts_to_watch.insert(*contract, stored_cursor + 1);
            }
            (None, None) => {
                // If no cursor is found, start from the fallback block.
                eth_contracts_to_watch.insert(*contract, eth_contracts_start_block_fallback);
            }
        }
    }
    eth_contracts_to_watch
}

#[cfg(test)]
mod tests {
    use ethers::types::Address as EthAddress;
    use prometheus::Registry;

    use super::*;
    use crate::config::BridgeNodeConfig;
    use crate::config::EthConfig;
    use crate::config::SuiConfig;
    use crate::e2e_tests::test_utils::BridgeTestCluster;
    use crate::e2e_tests::test_utils::BridgeTestClusterBuilder;
    use crate::utils::wait_for_server_to_be_up;
    use fastcrypto::secp256k1::Secp256k1KeyPair;
    use sui_config::local_ip_utils::get_available_port;
    use sui_types::base_types::SuiAddress;
    use sui_types::bridge::BridgeChainId;
    use sui_types::crypto::get_key_pair;
    use sui_types::crypto::EncodeDecodeBase64;
    use sui_types::crypto::KeypairTraits;
    use sui_types::crypto::SuiKeyPair;
    use sui_types::digests::TransactionDigest;
    use sui_types::event::EventID;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_get_eth_contracts_to_watch() {
        telemetry_subscribers::init_for_testing();
        let temp_dir = tempfile::tempdir().unwrap();
        let eth_contracts = vec![
            EthAddress::from_low_u64_be(1),
            EthAddress::from_low_u64_be(2),
        ];
        let store = BridgeOrchestratorTables::new(temp_dir.path());

        // No override, no watermark found in DB, use fallback
        let contracts = get_eth_contracts_to_watch(&store, &eth_contracts, 10, None);
        assert_eq!(
            contracts,
            vec![(eth_contracts[0], 10), (eth_contracts[1], 10)]
                .into_iter()
                .collect::<HashMap<_, _>>()
        );

        // no watermark found in DB, use override
        let contracts = get_eth_contracts_to_watch(&store, &eth_contracts, 10, Some(420));
        assert_eq!(
            contracts,
            vec![(eth_contracts[0], 420), (eth_contracts[1], 420)]
                .into_iter()
                .collect::<HashMap<_, _>>()
        );

        store
            .update_eth_event_cursor(eth_contracts[0], 100)
            .unwrap();
        store
            .update_eth_event_cursor(eth_contracts[1], 102)
            .unwrap();

        // No override, found watermarks in DB, use +1
        let contracts = get_eth_contracts_to_watch(&store, &eth_contracts, 10, None);
        assert_eq!(
            contracts,
            vec![(eth_contracts[0], 101), (eth_contracts[1], 103)]
                .into_iter()
                .collect::<HashMap<_, _>>()
        );

        // use override
        let contracts = get_eth_contracts_to_watch(&store, &eth_contracts, 10, Some(200));
        assert_eq!(
            contracts,
            vec![(eth_contracts[0], 200), (eth_contracts[1], 200)]
                .into_iter()
                .collect::<HashMap<_, _>>()
        );
    }

    #[tokio::test]
    async fn test_get_sui_modules_to_watch() {
        telemetry_subscribers::init_for_testing();
        let temp_dir = tempfile::tempdir().unwrap();

        let store = BridgeOrchestratorTables::new(temp_dir.path());
        let bridge_module = BRIDGE_MODULE_NAME.to_owned();
        let committee_module = BRIDGE_COMMITTEE_MODULE_NAME.to_owned();
        // No override, no stored watermark, use None
        let sui_modules_to_watch = get_sui_modules_to_watch(&store, None);
        assert_eq!(
            sui_modules_to_watch,
            vec![
                (bridge_module.clone(), None),
                (committee_module.clone(), None)
            ]
            .into_iter()
            .collect::<HashMap<_, _>>()
        );

        // no stored watermark, use override
        let override_cursor = EventID {
            tx_digest: TransactionDigest::random(),
            event_seq: 42,
        };
        let sui_modules_to_watch = get_sui_modules_to_watch(&store, Some(override_cursor));
        assert_eq!(
            sui_modules_to_watch,
            vec![
                (bridge_module.clone(), Some(override_cursor)),
                (committee_module.clone(), Some(override_cursor))
            ]
            .into_iter()
            .collect::<HashMap<_, _>>()
        );

        // No override, found stored watermark for `bridge` module, use stored watermark for `bridge`
        // and None for `committee`
        let stored_cursor = EventID {
            tx_digest: TransactionDigest::random(),
            event_seq: 100,
        };
        store
            .update_sui_event_cursor(bridge_module.clone(), stored_cursor)
            .unwrap();
        let sui_modules_to_watch = get_sui_modules_to_watch(&store, None);
        assert_eq!(
            sui_modules_to_watch,
            vec![
                (bridge_module.clone(), Some(stored_cursor)),
                (committee_module.clone(), None)
            ]
            .into_iter()
            .collect::<HashMap<_, _>>()
        );

        // found stored watermark, use override
        let stored_cursor = EventID {
            tx_digest: TransactionDigest::random(),
            event_seq: 100,
        };
        store
            .update_sui_event_cursor(committee_module.clone(), stored_cursor)
            .unwrap();
        let sui_modules_to_watch = get_sui_modules_to_watch(&store, Some(override_cursor));
        assert_eq!(
            sui_modules_to_watch,
            vec![
                (bridge_module.clone(), Some(override_cursor)),
                (committee_module.clone(), Some(override_cursor))
            ]
            .into_iter()
            .collect::<HashMap<_, _>>()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_starting_bridge_node() {
        telemetry_subscribers::init_for_testing();
        let bridge_test_cluster = setup().await;
        let kp = bridge_test_cluster.bridge_authority_key(0);

        // prepare node config (server only)
        let tmp_dir = tempdir().unwrap().into_path();
        let authority_key_path = "test_starting_bridge_node_bridge_authority_key";
        let server_listen_port = get_available_port("127.0.0.1");
        let base64_encoded = kp.encode_base64();
        std::fs::write(tmp_dir.join(authority_key_path), base64_encoded).unwrap();

        let config = BridgeNodeConfig {
            server_listen_port,
            metrics_port: get_available_port("127.0.0.1"),
            bridge_authority_key_path: tmp_dir.join(authority_key_path),
            sui: SuiConfig {
                sui_rpc_url: bridge_test_cluster.sui_rpc_url(),
                sui_bridge_chain_id: BridgeChainId::SuiCustom as u8,
                bridge_client_key_path: None,
                bridge_client_gas_object: None,
                sui_bridge_module_last_processed_event_id_override: None,
            },
            eth: EthConfig {
                eth_rpc_url: bridge_test_cluster.eth_rpc_url(),
                eth_bridge_proxy_address: bridge_test_cluster.sui_bridge_address(),
                eth_bridge_chain_id: BridgeChainId::EthCustom as u8,
                eth_contracts_start_block_fallback: None,
                eth_contracts_start_block_override: None,
            },
            approved_governance_actions: vec![],
            run_client: false,
            db_path: None,
        };
        // Spawn bridge node in memory
        let _handle = run_bridge_node(
            config,
            BridgeNodePublicMetadata::empty_for_testing(),
            Registry::new(),
        )
        .await
        .unwrap();

        let server_url = format!("http://127.0.0.1:{}", server_listen_port);
        // Now we expect to see the server to be up and running.
        let res = wait_for_server_to_be_up(server_url, 5).await;
        res.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_starting_bridge_node_with_client() {
        telemetry_subscribers::init_for_testing();
        let bridge_test_cluster = setup().await;
        let kp = bridge_test_cluster.bridge_authority_key(0);

        // prepare node config (server + client)
        let tmp_dir = tempdir().unwrap().into_path();
        let db_path = tmp_dir.join("test_starting_bridge_node_with_client_db");
        let authority_key_path = "test_starting_bridge_node_with_client_bridge_authority_key";
        let server_listen_port = get_available_port("127.0.0.1");

        let base64_encoded = kp.encode_base64();
        std::fs::write(tmp_dir.join(authority_key_path), base64_encoded).unwrap();

        let client_sui_address = SuiAddress::from(kp.public());
        let sender_address = bridge_test_cluster.sui_user_address();
        // send some gas to this address
        bridge_test_cluster
            .test_cluster
            .transfer_sui_must_exceed(sender_address, client_sui_address, 1000000000)
            .await;

        let config = BridgeNodeConfig {
            server_listen_port,
            metrics_port: get_available_port("127.0.0.1"),
            bridge_authority_key_path: tmp_dir.join(authority_key_path),
            sui: SuiConfig {
                sui_rpc_url: bridge_test_cluster.sui_rpc_url(),
                sui_bridge_chain_id: BridgeChainId::SuiCustom as u8,
                bridge_client_key_path: None,
                bridge_client_gas_object: None,
                sui_bridge_module_last_processed_event_id_override: Some(EventID {
                    tx_digest: TransactionDigest::random(),
                    event_seq: 0,
                }),
            },
            eth: EthConfig {
                eth_rpc_url: bridge_test_cluster.eth_rpc_url(),
                eth_bridge_proxy_address: bridge_test_cluster.sui_bridge_address(),
                eth_bridge_chain_id: BridgeChainId::EthCustom as u8,
                eth_contracts_start_block_fallback: Some(0),
                eth_contracts_start_block_override: None,
            },
            approved_governance_actions: vec![],
            run_client: true,
            db_path: Some(db_path),
        };
        // Spawn bridge node in memory
        let _handle = run_bridge_node(
            config,
            BridgeNodePublicMetadata::empty_for_testing(),
            Registry::new(),
        )
        .await
        .unwrap();

        let server_url = format!("http://127.0.0.1:{}", server_listen_port);
        // Now we expect to see the server to be up and running.
        // client components are spawned earlier than server, so as long as the server is up,
        // we know the client components are already running.
        let res = wait_for_server_to_be_up(server_url, 5).await;
        res.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_starting_bridge_node_with_client_and_separate_client_key() {
        telemetry_subscribers::init_for_testing();
        let bridge_test_cluster = setup().await;
        let kp = bridge_test_cluster.bridge_authority_key(0);

        // prepare node config (server + client)
        let tmp_dir = tempdir().unwrap().into_path();
        let db_path =
            tmp_dir.join("test_starting_bridge_node_with_client_and_separate_client_key_db");
        let authority_key_path =
            "test_starting_bridge_node_with_client_and_separate_client_key_bridge_authority_key";
        let server_listen_port = get_available_port("127.0.0.1");

        // prepare bridge authority key
        let base64_encoded = kp.encode_base64();
        std::fs::write(tmp_dir.join(authority_key_path), base64_encoded).unwrap();

        // prepare bridge client key
        let (_, kp): (_, Secp256k1KeyPair) = get_key_pair();
        let kp = SuiKeyPair::from(kp);
        let client_key_path =
            "test_starting_bridge_node_with_client_and_separate_client_key_bridge_client_key";
        std::fs::write(tmp_dir.join(client_key_path), kp.encode_base64()).unwrap();
        let client_sui_address = SuiAddress::from(&kp.public());
        let sender_address = bridge_test_cluster.sui_user_address();
        // send some gas to this address
        let gas_obj = bridge_test_cluster
            .test_cluster
            .transfer_sui_must_exceed(sender_address, client_sui_address, 1000000000)
            .await;

        let config = BridgeNodeConfig {
            server_listen_port,
            metrics_port: get_available_port("127.0.0.1"),
            bridge_authority_key_path: tmp_dir.join(authority_key_path),
            sui: SuiConfig {
                sui_rpc_url: bridge_test_cluster.sui_rpc_url(),
                sui_bridge_chain_id: BridgeChainId::SuiCustom as u8,
                bridge_client_key_path: Some(tmp_dir.join(client_key_path)),
                bridge_client_gas_object: Some(gas_obj),
                sui_bridge_module_last_processed_event_id_override: Some(EventID {
                    tx_digest: TransactionDigest::random(),
                    event_seq: 0,
                }),
            },
            eth: EthConfig {
                eth_rpc_url: bridge_test_cluster.eth_rpc_url(),
                eth_bridge_proxy_address: bridge_test_cluster.sui_bridge_address(),
                eth_bridge_chain_id: BridgeChainId::EthCustom as u8,
                eth_contracts_start_block_fallback: Some(0),
                eth_contracts_start_block_override: Some(0),
            },
            approved_governance_actions: vec![],
            run_client: true,
            db_path: Some(db_path),
        };
        // Spawn bridge node in memory
        let _handle = run_bridge_node(
            config,
            BridgeNodePublicMetadata::empty_for_testing(),
            Registry::new(),
        )
        .await
        .unwrap();

        let server_url = format!("http://127.0.0.1:{}", server_listen_port);
        // Now we expect to see the server to be up and running.
        // client components are spawned earlier than server, so as long as the server is up,
        // we know the client components are already running.
        let res = wait_for_server_to_be_up(server_url, 5).await;
        res.unwrap();
    }

    async fn setup() -> BridgeTestCluster {
        BridgeTestClusterBuilder::new()
            .with_eth_env(true)
            .with_bridge_cluster(false)
            .build()
            .await
    }
}
