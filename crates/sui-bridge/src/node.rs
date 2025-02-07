// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::WatchdogConfig;
use crate::crypto::BridgeAuthorityPublicKeyBytes;
use crate::metered_eth_provider::MeteredEthHttpProvier;
use crate::sui_bridge_watchdog::eth_bridge_status::EthBridgeStatus;
use crate::sui_bridge_watchdog::eth_vault_balance::{EthereumVaultBalance, VaultAsset};
use crate::sui_bridge_watchdog::metrics::WatchdogMetrics;
use crate::sui_bridge_watchdog::sui_bridge_status::SuiBridgeStatus;
use crate::sui_bridge_watchdog::total_supplies::TotalSupplies;
use crate::sui_bridge_watchdog::{BridgeWatchDog, Observable};
use crate::sui_client::SuiBridgeClient;
use crate::types::BridgeCommittee;
use crate::utils::{
    get_committee_voting_power_by_name, get_eth_contract_addresses, get_validator_names_by_pub_keys,
};
use crate::{
    action_executor::BridgeActionExecutor,
    client::bridge_authority_aggregator::BridgeAuthorityAggregator,
    config::{BridgeClientConfig, BridgeNodeConfig},
    eth_syncer::EthSyncer,
    events::init_all_struct_tags,
    metrics::BridgeMetrics,
    monitor::BridgeMonitor,
    orchestrator::BridgeOrchestrator,
    server::{handler::BridgeRequestHandler, run_server, BridgeNodePublicMetadata},
    storage::BridgeOrchestratorTables,
    sui_syncer::SuiSyncer,
};
use arc_swap::ArcSwap;
use ethers::providers::Provider;
use ethers::types::Address as EthAddress;
use mysten_metrics::spawn_logged_monitored_task;
use std::collections::BTreeMap;
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use sui_types::{
    bridge::{
        BRIDGE_COMMITTEE_MODULE_NAME, BRIDGE_LIMITER_MODULE_NAME, BRIDGE_MODULE_NAME,
        BRIDGE_TREASURY_MODULE_NAME,
    },
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
    let watchdog_config = config.watchdog_config.clone();
    let (server_config, client_config) = config.validate(metrics.clone()).await?;
    let sui_chain_identifier = server_config
        .sui_client
        .get_chain_identifier()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get sui chain identifier: {:?}", e))?;
    let eth_chain_identifier = server_config
        .eth_client
        .get_chain_id()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get eth chain identifier: {:?}", e))?;
    prometheus_registry
        .register(mysten_metrics::bridge_uptime_metric(
            "bridge",
            metadata.version,
            &sui_chain_identifier,
            &eth_chain_identifier.to_string(),
            client_config.is_some(),
        ))
        .unwrap();

    let committee = Arc::new(
        server_config
            .sui_client
            .get_bridge_committee()
            .await
            .expect("Failed to get committee"),
    );
    let mut handles = vec![];

    // Start watchdog
    let eth_provider = server_config.eth_client.provider();
    let eth_bridge_proxy_address = server_config.eth_bridge_proxy_address;
    let sui_client = server_config.sui_client.clone();
    handles.push(spawn_logged_monitored_task!(start_watchdog(
        watchdog_config,
        &prometheus_registry,
        eth_provider,
        eth_bridge_proxy_address,
        sui_client
    )));

    // Update voting right metrics
    // Before reconfiguration happens we only set it once when the node starts
    let sui_system = server_config
        .sui_client
        .sui_client()
        .governance_api()
        .get_latest_sui_system_state()
        .await?;

    // Start Client
    if let Some(client_config) = client_config {
        let committee_keys_to_names =
            Arc::new(get_validator_names_by_pub_keys(&committee, &sui_system).await);
        let client_components = start_client_components(
            client_config,
            committee.clone(),
            committee_keys_to_names,
            metrics.clone(),
        )
        .await?;
        handles.extend(client_components);
    }

    let committee_name_mapping = get_committee_voting_power_by_name(&committee, &sui_system).await;
    for (name, voting_power) in committee_name_mapping.into_iter() {
        metrics
            .current_bridge_voting_rights
            .with_label_values(&[name.as_str()])
            .set(voting_power as i64);
    }

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

async fn start_watchdog(
    watchdog_config: Option<WatchdogConfig>,
    registry: &prometheus::Registry,
    eth_provider: Arc<Provider<MeteredEthHttpProvier>>,
    eth_bridge_proxy_address: EthAddress,
    sui_client: Arc<SuiBridgeClient>,
) {
    let watchdog_metrics = WatchdogMetrics::new(registry);
    let (
        _committee_address,
        _limiter_address,
        vault_address,
        _config_address,
        weth_address,
        usdt_address,
        _wbtc_address,
    ) = get_eth_contract_addresses(eth_bridge_proxy_address, &eth_provider)
        .await
        .unwrap_or_else(|e| panic!("get_eth_contract_addresses should not fail: {}", e));

    let eth_vault_balance = EthereumVaultBalance::new(
        eth_provider.clone(),
        vault_address,
        weth_address,
        VaultAsset::WETH,
        watchdog_metrics.eth_vault_balance.clone(),
    )
    .await
    .unwrap_or_else(|e| panic!("Failed to create eth vault balance: {}", e));

    let usdt_vault_balance = EthereumVaultBalance::new(
        eth_provider.clone(),
        vault_address,
        usdt_address,
        VaultAsset::USDT,
        watchdog_metrics.usdt_vault_balance.clone(),
    )
    .await
    .unwrap_or_else(|e| panic!("Failed to create usdt vault balance: {}", e));

    let wbtc_vault_balance = EthereumVaultBalance::new(
        eth_provider.clone(),
        vault_address,
        _wbtc_address,
        VaultAsset::WBTC,
        watchdog_metrics.wbtc_vault_balance.clone(),
    )
    .await
    .unwrap_or_else(|e| panic!("Failed to create wbtc vault balance: {}", e));

    let eth_bridge_status = EthBridgeStatus::new(
        eth_provider,
        eth_bridge_proxy_address,
        watchdog_metrics.eth_bridge_paused.clone(),
    );

    let sui_bridge_status = SuiBridgeStatus::new(
        sui_client.clone(),
        watchdog_metrics.sui_bridge_paused.clone(),
    );

    let mut observables: Vec<Box<dyn Observable + Send + Sync>> = vec![
        Box::new(eth_vault_balance),
        Box::new(usdt_vault_balance),
        Box::new(wbtc_vault_balance),
        Box::new(eth_bridge_status),
        Box::new(sui_bridge_status),
    ];
    if let Some(watchdog_config) = watchdog_config {
        if !watchdog_config.total_supplies.is_empty() {
            let total_supplies = TotalSupplies::new(
                Arc::new(sui_client.sui_client().clone()),
                watchdog_config.total_supplies,
                watchdog_metrics.total_supplies.clone(),
            );
            observables.push(Box::new(total_supplies));
        }
    }

    BridgeWatchDog::new(observables).run().await
}

// TODO: is there a way to clean up the overrides after it's stored in DB?
async fn start_client_components(
    client_config: BridgeClientConfig,
    committee: Arc<BridgeCommittee>,
    committee_keys_to_names: Arc<BTreeMap<BridgeAuthorityPublicKeyBytes, String>>,
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

    let (task_handles, sui_events_rx) = SuiSyncer::new(
        client_config.sui_client,
        sui_modules_to_watch,
        metrics.clone(),
    )
    .run(Duration::from_secs(2))
    .await
    .expect("Failed to start sui syncer");
    all_handles.extend(task_handles);

    let bridge_auth_agg = Arc::new(ArcSwap::from(Arc::new(BridgeAuthorityAggregator::new(
        committee,
        metrics.clone(),
        committee_keys_to_names,
    ))));
    // TODO: should we use one query instead of two?
    let sui_token_type_tags = sui_client.get_token_id_map().await.unwrap();
    let is_bridge_paused = sui_client.is_bridge_paused().await.unwrap();

    let (bridge_pause_tx, bridge_pause_rx) = tokio::sync::watch::channel(is_bridge_paused);

    let (sui_monitor_tx, sui_monitor_rx) = mysten_metrics::metered_channel::channel(
        10000,
        &mysten_metrics::get_metrics()
            .unwrap()
            .channel_inflight
            .with_label_values(&["sui_monitor_queue"]),
    );
    let (eth_monitor_tx, eth_monitor_rx) = mysten_metrics::metered_channel::channel(
        10000,
        &mysten_metrics::get_metrics()
            .unwrap()
            .channel_inflight
            .with_label_values(&["eth_monitor_queue"]),
    );

    let sui_token_type_tags = Arc::new(ArcSwap::from(Arc::new(sui_token_type_tags)));
    let bridge_action_executor = BridgeActionExecutor::new(
        sui_client.clone(),
        bridge_auth_agg.clone(),
        store.clone(),
        client_config.key,
        client_config.sui_address,
        client_config.gas_object_ref.0,
        sui_token_type_tags.clone(),
        bridge_pause_rx,
        metrics.clone(),
    )
    .await;

    let monitor = BridgeMonitor::new(
        sui_client.clone(),
        sui_monitor_rx,
        eth_monitor_rx,
        bridge_auth_agg.clone(),
        bridge_pause_tx,
        sui_token_type_tags,
        metrics.clone(),
    );
    all_handles.push(spawn_logged_monitored_task!(monitor.run()));

    let orchestrator = BridgeOrchestrator::new(
        sui_client,
        sui_events_rx,
        eth_events_rx,
        store.clone(),
        sui_monitor_tx,
        eth_monitor_tx,
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
        BRIDGE_TREASURY_MODULE_NAME.to_owned(),
        BRIDGE_LIMITER_MODULE_NAME.to_owned(),
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
    use crate::config::default_ed25519_key_pair;
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
        let treasury_module = BRIDGE_TREASURY_MODULE_NAME.to_owned();
        let limiter_module = BRIDGE_LIMITER_MODULE_NAME.to_owned();
        // No override, no stored watermark, use None
        let sui_modules_to_watch = get_sui_modules_to_watch(&store, None);
        assert_eq!(
            sui_modules_to_watch,
            vec![
                (bridge_module.clone(), None),
                (committee_module.clone(), None),
                (treasury_module.clone(), None),
                (limiter_module.clone(), None)
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
                (committee_module.clone(), Some(override_cursor)),
                (treasury_module.clone(), Some(override_cursor)),
                (limiter_module.clone(), Some(override_cursor))
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
                (committee_module.clone(), None),
                (treasury_module.clone(), None),
                (limiter_module.clone(), None)
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
                (committee_module.clone(), Some(override_cursor)),
                (treasury_module.clone(), Some(override_cursor)),
                (limiter_module.clone(), Some(override_cursor))
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
            metrics_key_pair: default_ed25519_key_pair(),
            metrics: None,
            watchdog_config: None,
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
            .inner
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
            metrics_key_pair: default_ed25519_key_pair(),
            metrics: None,
            watchdog_config: None,
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
            .inner
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
            metrics_key_pair: default_ed25519_key_pair(),
            metrics: None,
            watchdog_config: None,
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
            .with_num_validators(2)
            .build()
            .await
    }
}
