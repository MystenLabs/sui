// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::IndexerConfig;
use crate::metrics::BridgeIndexerMetrics;
use crate::postgres_manager::get_connection_pool;
use prometheus::Registry;
use sui_bridge::e2e_tests::test_utils::BridgeTestClusterBuilder;
use sui_data_ingestion_core::DataIngestionMetrics;
use sui_indexer::tempdb::TempDb;
use tracing::info;

#[tokio::test]
async fn test() {
    let metrics = BridgeIndexerMetrics::new_for_testing();
    let registry = Registry::new();
    let ingestion_metrics = DataIngestionMetrics::new(&registry);

    let config = setup_bridge_env(false).await;
    let pool = get_connection_pool(config.db_url.clone()).await;
    /*    let indexer = create_sui_indexer(pool, metrics, ingestion_metrics, &config)
    .await
    .unwrap();*/

    /*    indexer.start().await.unwrap()
     */
}

async fn setup_bridge_env(with_eth_env: bool) -> IndexerConfig {
    let mut bridge_test_cluster = BridgeTestClusterBuilder::new()
        .with_eth_env(with_eth_env)
        .with_bridge_cluster(true)
        .with_num_validators(1)
        .build()
        .await;

    bridge_test_cluster.start_bridge_cluster().await;
    bridge_test_cluster
        .wait_for_bridge_cluster_to_be_up(10)
        .await;
    info!("Bridge cluster is up");

    let db = TempDb::new().unwrap();

    IndexerConfig {
        remote_store_url: bridge_test_cluster.sui_rpc_url(),
        checkpoints_path: None,
        sui_rpc_url: bridge_test_cluster.sui_rpc_url(),
        eth_rpc_url: bridge_test_cluster.eth_rpc_url(),
        // TODO: add WS support
        eth_ws_url: "".to_string(),
        db_url: db.database().url().to_string(),
        concurrency: 10,
        sui_bridge_genesis_checkpoint: 0,
        eth_bridge_genesis_block: 0,
        eth_sui_bridge_contract_address: bridge_test_cluster.sui_bridge_address(),
        metric_port: 9001,
        disable_eth: None,
    }
}
