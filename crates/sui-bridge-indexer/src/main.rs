// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::*;
use ethers::types::Address as EthAddress;
use prometheus::Registry;
use std::collections::HashSet;
use std::env;
use std::net::IpAddr;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use sui_bridge::eth_client::EthClient;
use sui_bridge::metered_eth_provider::{new_metered_eth_provider, MeteredEthHttpProvier};
use sui_bridge::sui_bridge_watchdog::Observable;
use sui_bridge::sui_client::SuiBridgeClient;
use sui_bridge::utils::get_eth_contract_addresses;
use sui_config::Config;
use tokio::task::JoinHandle;
use tracing::info;

use mysten_metrics::metered_channel::channel;
use mysten_metrics::spawn_logged_monitored_task;
use mysten_metrics::start_prometheus_server;

use sui_bridge::metrics::BridgeMetrics;
use sui_bridge::sui_bridge_watchdog::{
    eth_bridge_status::EthBridgeStatus,
    eth_vault_balance::{EthereumVaultBalance, VaultAsset},
    metrics::WatchdogMetrics,
    sui_bridge_status::SuiBridgeStatus,
    BridgeWatchDog,
};
use sui_bridge_indexer::config::IndexerConfig;
use sui_bridge_indexer::metrics::BridgeIndexerMetrics;
use sui_bridge_indexer::postgres_manager::{get_connection_pool, read_sui_progress_store};
use sui_bridge_indexer::sui_transaction_handler::handle_sui_transactions_loop;
use sui_bridge_indexer::sui_transaction_queries::start_sui_tx_polling_task;
use sui_bridge_indexer::{
    create_eth_subscription_indexer, create_eth_sync_indexer, create_sui_indexer,
};
use sui_data_ingestion_core::DataIngestionMetrics;
use sui_sdk::SuiClientBuilder;

#[derive(Parser, Clone, Debug)]
struct Args {
    /// Path to a yaml config
    #[clap(long, short)]
    config_path: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let args = Args::parse();

    // load config
    let config_path = if let Some(path) = args.config_path {
        path
    } else {
        env::current_dir()
            .expect("Couldn't get current directory")
            .join("config.yaml")
    };
    let config = IndexerConfig::load(&config_path)?;

    // Init metrics server
    let metrics_address =
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), config.metric_port);
    let registry_service = start_prometheus_server(metrics_address);
    let registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&registry);
    info!("Metrics server started at port {}", config.metric_port);

    let indexer_meterics = BridgeIndexerMetrics::new(&registry);
    let ingestion_metrics = DataIngestionMetrics::new(&registry);
    let bridge_metrics = Arc::new(BridgeMetrics::new(&registry));

    let db_url = config.db_url.clone();
    let pool = get_connection_pool(db_url.clone()).await;

    let eth_client: Arc<EthClient<MeteredEthHttpProvier>> = Arc::new(
        EthClient::<MeteredEthHttpProvier>::new(
            &config.eth_rpc_url,
            HashSet::from_iter(vec![]), // dummy
            bridge_metrics.clone(),
        )
        .await?,
    );
    let eth_bridge_proxy_address = EthAddress::from_str(&config.eth_sui_bridge_contract_address)?;
    let mut tasks = vec![];
    // Start the eth subscription indexer
    let eth_subscription_indexer = create_eth_subscription_indexer(
        pool.clone(),
        indexer_meterics.clone(),
        &config,
        eth_client.clone(),
    )
    .await?;
    tasks.push(spawn_logged_monitored_task!(
        eth_subscription_indexer.start()
    ));

    // Start the eth sync data source
    let eth_sync_indexer = create_eth_sync_indexer(
        pool.clone(),
        indexer_meterics.clone(),
        bridge_metrics.clone(),
        &config,
        eth_client,
    )
    .await?;
    tasks.push(spawn_logged_monitored_task!(eth_sync_indexer.start()));

    let indexer = create_sui_indexer(pool, indexer_meterics, ingestion_metrics, &config).await?;
    tasks.push(spawn_logged_monitored_task!(indexer.start()));

    let sui_bridge_client =
        Arc::new(SuiBridgeClient::new(&config.sui_rpc_url, bridge_metrics.clone()).await?);
    start_watchdog(
        config,
        eth_bridge_proxy_address,
        sui_bridge_client,
        &registry,
        bridge_metrics.clone(),
    )
    .await?;

    // Wait for tasks in `tasks` to finish. Return when anyone of them returns an error.
    futures::future::try_join_all(tasks).await?;
    unreachable!("Indexer tasks finished unexpectedly");
}

async fn start_watchdog(
    config: IndexerConfig,
    eth_bridge_proxy_address: EthAddress,
    sui_client: Arc<SuiBridgeClient>,
    registry: &Registry,
    bridge_metrics: Arc<BridgeMetrics>,
) -> Result<()> {
    let watchdog_metrics = WatchdogMetrics::new(registry);
    let eth_provider =
        Arc::new(new_metered_eth_provider(&config.eth_rpc_url, bridge_metrics.clone()).unwrap());
    let (
        _committee_address,
        _limiter_address,
        vault_address,
        _config_address,
        weth_address,
        usdt_address,
        wbtc_address,
    ) = get_eth_contract_addresses(eth_bridge_proxy_address, &eth_provider).await?;

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
        wbtc_address,
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

    let sui_bridge_status =
        SuiBridgeStatus::new(sui_client, watchdog_metrics.sui_bridge_paused.clone());
    let observables: Vec<Box<dyn Observable + Send + Sync>> = vec![
        Box::new(eth_vault_balance),
        Box::new(usdt_vault_balance),
        Box::new(wbtc_vault_balance),
        Box::new(eth_bridge_status),
        Box::new(sui_bridge_status),
    ];
    BridgeWatchDog::new(observables).run().await;
    Ok(())
}

#[allow(unused)]
async fn start_processing_sui_checkpoints_by_querying_txns(
    sui_rpc_url: String,
    db_url: String,
    indexer_metrics: BridgeIndexerMetrics,
) -> Result<Vec<JoinHandle<()>>> {
    let pg_pool = get_connection_pool(db_url.clone()).await;
    let (tx, rx) = channel(
        100,
        &mysten_metrics::get_metrics()
            .unwrap()
            .channel_inflight
            .with_label_values(&["sui_transaction_processing_queue"]),
    );
    let mut handles = vec![];
    let cursor = read_sui_progress_store(&pg_pool)
        .await
        .expect("Failed to read cursor from sui progress store");
    let sui_client = SuiClientBuilder::default().build(sui_rpc_url).await?;
    handles.push(spawn_logged_monitored_task!(
        start_sui_tx_polling_task(sui_client, cursor, tx),
        "start_sui_tx_polling_task"
    ));
    handles.push(spawn_logged_monitored_task!(
        handle_sui_transactions_loop(pg_pool.clone(), rx, indexer_metrics.clone()),
        "handle_sui_transcations_loop"
    ));
    Ok(handles)
}
