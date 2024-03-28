// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    action_executor::BridgeActionExecutor,
    client::bridge_authority_aggregator::BridgeAuthorityAggregator,
    config::{BridgeClientConfig, BridgeNodeConfig},
    eth_syncer::EthSyncer,
    orchestrator::BridgeOrchestrator,
    server::{handler::BridgeRequestHandler, run_server},
    storage::BridgeOrchestratorTables,
    sui_syncer::SuiSyncer,
};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use sui_types::bridge::BRIDGE_MODULE_NAME;
use tokio::task::JoinHandle;
use tracing::info;

pub async fn run_bridge_node(config: BridgeNodeConfig) -> anyhow::Result<()> {
    let (server_config, client_config) = config.validate().await?;

    // Start Client
    let _handles = if let Some(client_config) = client_config {
        start_client_components(client_config).await
    } else {
        Ok(vec![])
    }?;

    // Start Server
    let socket_address = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        server_config.server_listen_port,
    );
    run_server(
        &socket_address,
        BridgeRequestHandler::new(
            server_config.key,
            server_config.sui_client,
            server_config.eth_client,
            server_config.approved_governance_actions,
        ),
    )
    .await;

    Ok(())
}

// TODO: is there a way to clean up the overrides after it's stored in DB?
async fn start_client_components(
    client_config: BridgeClientConfig,
) -> anyhow::Result<Vec<JoinHandle<()>>> {
    let store: std::sync::Arc<BridgeOrchestratorTables> =
        BridgeOrchestratorTables::new(&client_config.db_path.join("client"));
    let module_identifier = BRIDGE_MODULE_NAME.to_owned();
    let sui_bridge_modules = vec![module_identifier.clone()];
    let sui_bridge_module_stored_cursor = store
        .get_sui_event_cursors(&sui_bridge_modules)
        .map_err(|e| anyhow::anyhow!("Unable to get sui event cursors from storage: {e:?}"))?[0];
    let mut sui_modules_to_watch = HashMap::new();
    match client_config.sui_bridge_module_last_processed_event_id_override {
        Some(cursor) => {
            info!(
                "Overriding cursor for sui bridge module {} to {:?}. Stored cursor: {:?}",
                module_identifier, cursor, sui_bridge_module_stored_cursor,
            );
            sui_modules_to_watch.insert(module_identifier, Some(cursor));
        }
        None => {
            if sui_bridge_module_stored_cursor.is_none() {
                info!(
                    "No cursor found for sui bridge module {} in storage or config override",
                    module_identifier
                );
            }
            sui_modules_to_watch.insert(module_identifier, sui_bridge_module_stored_cursor);
        }
    };

    let stored_eth_cursors = store
        .get_eth_event_cursors(&client_config.eth_bridge_contracts)
        .map_err(|e| anyhow::anyhow!("Unable to get eth event cursors from storage: {e:?}"))?;
    let mut eth_contracts_to_watch = HashMap::new();
    for (contract, cursor) in client_config
        .eth_bridge_contracts
        .iter()
        .zip(stored_eth_cursors)
    {
        if client_config
            .eth_bridge_contracts_start_block_override
            .contains_key(contract)
        {
            eth_contracts_to_watch.insert(
                *contract,
                client_config.eth_bridge_contracts_start_block_override[contract],
            );
            info!(
                "Overriding cursor for eth bridge contract {} to {}. Stored cursor: {:?}",
                contract, client_config.eth_bridge_contracts_start_block_override[contract], cursor
            );
        } else if let Some(cursor) = cursor {
            // +1: The stored value is the last block that was processed, so we start from the next block.
            eth_contracts_to_watch.insert(*contract, cursor + 1);
        } else {
            // TODO: can we not rely on this when node starts for the first time?
            return Err(anyhow::anyhow!(
                "No cursor found for eth contract {} in storage or config override",
                contract
            ));
        }
    }

    let sui_client = client_config.sui_client.clone();

    let mut all_handles = vec![];
    let (task_handles, eth_events_rx, _) =
        EthSyncer::new(client_config.eth_client.clone(), eth_contracts_to_watch)
            .run()
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

    let bridge_action_executor = BridgeActionExecutor::new(
        sui_client.clone(),
        Arc::new(bridge_auth_agg),
        store.clone(),
        client_config.key,
        client_config.sui_address,
        client_config.gas_object_ref.0,
    )
    .await
    .expect("Failed to create bridge action executor");

    let orchestrator =
        BridgeOrchestrator::new(sui_client, sui_events_rx, eth_events_rx, store.clone());

    all_handles.extend(orchestrator.run(bridge_action_executor));
    Ok(all_handles)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::process::Child;

    use super::*;
    use crate::BRIDGE_ENABLE_PROTOCOL_VERSION;
    use crate::{config::BridgeNodeConfig, server::APPLICATION_JSON};
    use fastcrypto::secp256k1::Secp256k1KeyPair;
    use sui_config::local_ip_utils::get_available_port;
    use sui_types::base_types::SuiAddress;
    use sui_types::crypto::get_key_pair;
    use sui_types::crypto::EncodeDecodeBase64;
    use sui_types::crypto::KeypairTraits;
    use sui_types::crypto::SuiKeyPair;
    use sui_types::digests::TransactionDigest;
    use sui_types::event::EventID;
    use tempfile::tempdir;
    use test_cluster::TestClusterBuilder;

    const DUMMY_ETH_ADDRESS: &str = "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984";

    #[tokio::test]
    async fn test_starting_bridge_node() {
        telemetry_subscribers::init_for_testing();
        let (test_cluster, anvil_port, _child) = setup().await;

        // prepare node config (server only)
        let tmp_dir = tempdir().unwrap().into_path();
        let authority_key_path = "test_starting_bridge_node_bridge_authority_key";
        let server_listen_port = get_available_port("127.0.0.1");
        let kp = test_cluster.bridge_authority_keys.as_ref().unwrap()[0].copy();
        let base64_encoded = kp.encode_base64();
        std::fs::write(tmp_dir.join(authority_key_path), base64_encoded).unwrap();

        let config = BridgeNodeConfig {
            server_listen_port,
            metrics_port: get_available_port("127.0.0.1"),
            bridge_authority_key_path_base64_raw: tmp_dir.join(authority_key_path),
            sui_rpc_url: test_cluster.fullnode_handle.rpc_url,
            eth_rpc_url: format!("http://127.0.0.1:{}", anvil_port),
            eth_addresses: vec![DUMMY_ETH_ADDRESS.into()],
            approved_governance_actions: vec![],
            run_client: false,
            bridge_client_key_path_base64_sui_key: None,
            bridge_client_gas_object: None,
            db_path: None,
            eth_bridge_contracts_start_block_override: None,
            sui_bridge_module_last_processed_event_id_override: None,
        };
        // Spawn bridge node in memory
        tokio::spawn(async move {
            run_bridge_node(config).await.unwrap();
        });

        let server_url = format!("http://127.0.0.1:{}", server_listen_port);
        // Now we expect to see the server to be up and running.
        wait_for_server_to_be_up(server_url, 3).await;
    }

    #[tokio::test]
    async fn test_starting_bridge_node_with_client() {
        telemetry_subscribers::init_for_testing();
        let (test_cluster, anvil_port, _child) = setup().await;

        // prepare node config (server + client)
        let tmp_dir = tempdir().unwrap().into_path();
        let db_path = tmp_dir.join("test_starting_bridge_node_with_client_db");
        let authority_key_path = "test_starting_bridge_node_with_client_bridge_authority_key";
        let server_listen_port = get_available_port("127.0.0.1");

        let kp = test_cluster.bridge_authority_keys.as_ref().unwrap()[0].copy();
        let base64_encoded = kp.encode_base64();
        std::fs::write(tmp_dir.join(authority_key_path), base64_encoded).unwrap();

        let client_sui_address = SuiAddress::from(kp.public());
        // send some gas to this address
        test_cluster
            .transfer_sui_must_exceeed(client_sui_address, 1000000000)
            .await;

        let config = BridgeNodeConfig {
            server_listen_port,
            metrics_port: get_available_port("127.0.0.1"),
            bridge_authority_key_path_base64_raw: tmp_dir.join(authority_key_path),
            sui_rpc_url: test_cluster.fullnode_handle.rpc_url,
            eth_rpc_url: format!("http://127.0.0.1:{}", anvil_port),
            eth_addresses: vec![DUMMY_ETH_ADDRESS.into()],
            approved_governance_actions: vec![],
            run_client: true,
            bridge_client_key_path_base64_sui_key: None,
            bridge_client_gas_object: None,
            db_path: Some(db_path),
            eth_bridge_contracts_start_block_override: Some(BTreeMap::from_iter(vec![(
                DUMMY_ETH_ADDRESS.into(),
                0,
            )])),
            sui_bridge_module_last_processed_event_id_override: Some(EventID {
                tx_digest: TransactionDigest::random(),
                event_seq: 0,
            }),
        };
        // Spawn bridge node in memory
        let config_clone = config.clone();
        tokio::spawn(async move {
            run_bridge_node(config_clone).await.unwrap();
        });

        let server_url = format!("http://127.0.0.1:{}", server_listen_port);
        // Now we expect to see the server to be up and running.
        // client components are spawned earlier than server, so as long as the server is up,
        // we know the client components are already running.
        wait_for_server_to_be_up(server_url, 3).await;
    }

    #[tokio::test]
    async fn test_starting_bridge_node_with_client_and_separate_client_key() {
        telemetry_subscribers::init_for_testing();
        let (test_cluster, anvil_port, _child) = setup().await;

        // prepare node config (server + client)
        let tmp_dir = tempdir().unwrap().into_path();
        let db_path =
            tmp_dir.join("test_starting_bridge_node_with_client_and_separate_client_key_db");
        let authority_key_path =
            "test_starting_bridge_node_with_client_and_separate_client_key_bridge_authority_key";
        let server_listen_port = get_available_port("127.0.0.1");

        // prepare bridge authority key
        let kp = test_cluster.bridge_authority_keys.as_ref().unwrap()[0].copy();
        let base64_encoded = kp.encode_base64();
        std::fs::write(tmp_dir.join(authority_key_path), base64_encoded).unwrap();

        // prepare bridge client key
        let (_, kp): (_, Secp256k1KeyPair) = get_key_pair();
        let kp = SuiKeyPair::from(kp);
        let client_key_path =
            "test_starting_bridge_node_with_client_and_separate_client_key_bridge_client_key";
        std::fs::write(tmp_dir.join(client_key_path), kp.encode_base64()).unwrap();
        let client_sui_address = SuiAddress::from(&kp.public());

        // send some gas to this address
        let gas_obj = test_cluster
            .transfer_sui_must_exceeed(client_sui_address, 1000000000)
            .await;

        let config = BridgeNodeConfig {
            server_listen_port,
            metrics_port: get_available_port("127.0.0.1"),
            bridge_authority_key_path_base64_raw: tmp_dir.join(authority_key_path),
            sui_rpc_url: test_cluster.fullnode_handle.rpc_url,
            eth_rpc_url: format!("http://127.0.0.1:{}", anvil_port),
            eth_addresses: vec![DUMMY_ETH_ADDRESS.into()],
            approved_governance_actions: vec![],
            run_client: true,
            bridge_client_key_path_base64_sui_key: Some(tmp_dir.join(client_key_path)),
            bridge_client_gas_object: Some(gas_obj),
            db_path: Some(db_path),
            eth_bridge_contracts_start_block_override: Some(BTreeMap::from_iter(vec![(
                DUMMY_ETH_ADDRESS.into(),
                0,
            )])),
            sui_bridge_module_last_processed_event_id_override: Some(EventID {
                tx_digest: TransactionDigest::random(),
                event_seq: 0,
            }),
        };
        // Spawn bridge node in memory
        let config_clone = config.clone();
        tokio::spawn(async move {
            run_bridge_node(config_clone).await.unwrap();
        });

        let server_url = format!("http://127.0.0.1:{}", server_listen_port);
        // Now we expect to see the server to be up and running.
        // client components are spawned earlier than server, so as long as the server is up,
        // we know the client components are already running.
        wait_for_server_to_be_up(server_url, 3).await;
    }

    async fn setup() -> (test_cluster::TestCluster, u16, Child) {
        let test_cluster: test_cluster::TestCluster = TestClusterBuilder::new()
            .with_protocol_version(BRIDGE_ENABLE_PROTOCOL_VERSION.into())
            .with_epoch_duration_ms(10000)
            .build_with_bridge()
            .await;

        test_cluster
            .wait_for_next_epoch_and_assert_bridge_committee_initialized()
            .await;

        // Start eth node with anvil
        let anvil_port = get_available_port("127.0.0.1");
        let eth_node_process = std::process::Command::new("anvil")
            .arg("--port")
            .arg(anvil_port.to_string())
            .spawn()
            .expect("Failed to start anvil");

        (test_cluster, anvil_port, eth_node_process)
    }

    async fn wait_for_server_to_be_up(server_url: String, timeout_sec: u64) {
        let now = std::time::Instant::now();
        // Now we expect to see the server to be up and running, the max time to wait is 3 seconds.
        loop {
            if let Ok(true) = reqwest::Client::new()
                .get(server_url.clone())
                .header(reqwest::header::ACCEPT, APPLICATION_JSON)
                .send()
                .await
                .map(|res| res.status().is_success())
            {
                break;
            }
            if now.elapsed().as_secs() > timeout_sec {
                panic!("Server is not up and running after 3 seconds");
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }
}
