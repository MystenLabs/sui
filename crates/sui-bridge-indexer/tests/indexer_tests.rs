// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::associations::HasTable;
use diesel::QueryDsl;
use diesel_async::RunQueryDsl;
use diesel_migrations::{embed_migrations, EmbeddedMigrations};
use prometheus::Registry;
use std::time::Duration;
use sui_bridge::e2e_tests::test_utils::{
    initiate_bridge_eth_to_sui, BridgeTestCluster, BridgeTestClusterBuilder,
};
use sui_bridge_indexer::config::IndexerConfig;
use sui_bridge_indexer::metrics::BridgeIndexerMetrics;
use sui_bridge_indexer::models::{GovernanceAction, TokenTransfer};
use sui_bridge_indexer::postgres_manager::get_connection_pool;
use sui_bridge_indexer::storage::PgBridgePersistent;
use sui_bridge_indexer::{create_sui_indexer, schema};
use sui_data_ingestion_core::DataIngestionMetrics;
use sui_indexer::database::Connection;
use sui_indexer_builder::indexer_builder::IndexerProgressStore;
use sui_pg_db::temp::TempDb;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("src/migrations");

#[tokio::test]
async fn test_indexing_transfer() {
    let metrics = BridgeIndexerMetrics::new_for_testing();
    let registry = Registry::new();
    let ingestion_metrics = DataIngestionMetrics::new(&registry);

    let (config, cluster, _db) = setup_bridge_env(false).await;

    let pool = get_connection_pool(config.db_url.clone()).await;
    let indexer = create_sui_indexer(pool.clone(), metrics.clone(), ingestion_metrics, &config)
        .await
        .unwrap();
    let storage = indexer.test_only_storage().clone();
    let indexer_name = indexer.test_only_name();
    let indexer_handle = tokio::spawn(indexer.start());

    // wait until backfill finish
    wait_for_back_fill_to_finish(&storage, &indexer_name)
        .await
        .unwrap();

    let data: Vec<TokenTransfer> = schema::token_transfer::dsl::token_transfer::table()
        .load(&mut pool.get().await.unwrap())
        .await
        .unwrap();

    // token transfer data should be empty
    assert!(data.is_empty());

    use schema::governance_actions::columns;
    let data = schema::governance_actions::dsl::governance_actions::table()
        .select((
            columns::nonce,
            columns::data_source,
            columns::txn_digest,
            columns::sender_address,
            columns::timestamp_ms,
            columns::action,
            columns::data,
        ))
        .load::<GovernanceAction>(&mut pool.get().await.unwrap())
        .await
        .unwrap();

    // 8 governance actions in total, token registration and approval events for ETH USDC, USDT and BTC.
    assert_eq!(8, data.len());

    // transfer eth to sui
    initiate_bridge_eth_to_sui(&cluster, 1000, 0).await.unwrap();

    let current_block_height = cluster
        .sui_client()
        .read_api()
        .get_latest_checkpoint_sequence_number()
        .await
        .unwrap();
    wait_for_block(&storage, &indexer_name, current_block_height)
        .await
        .unwrap();

    let data = schema::token_transfer::dsl::token_transfer::table()
        .load::<TokenTransfer>(&mut pool.get().await.unwrap())
        .await
        .unwrap()
        .iter()
        .map(|t| (t.chain_id, t.nonce, t.status.clone()))
        .collect::<Vec<_>>();

    assert_eq!(2, data.len());
    assert_eq!(
        vec![
            (12, 0, "Approved".to_string()),
            (12, 0, "Claimed".to_string())
        ],
        data
    );

    indexer_handle.abort()
}

async fn wait_for_block(
    storage: &PgBridgePersistent,
    task: &str,
    block: u64,
) -> Result<(), anyhow::Error> {
    while storage
        .get_ongoing_tasks(task)
        .await?
        .live_task()
        .map(|t| t.start_checkpoint)
        .unwrap_or_default()
        < block
    {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}

async fn wait_for_back_fill_to_finish(
    storage: &PgBridgePersistent,
    task: &str,
) -> Result<(), anyhow::Error> {
    // wait until tasks are set up
    while storage.get_ongoing_tasks(task).await?.live_task().is_none() {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // wait until all backfill tasks have completed
    while !storage
        .get_ongoing_tasks(task)
        .await?
        .backfill_tasks_ordered_desc()
        .is_empty()
    {
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    Ok(())
}

async fn setup_bridge_env(with_eth_env: bool) -> (IndexerConfig, BridgeTestCluster, TempDb) {
    let bridge_test_cluster = BridgeTestClusterBuilder::new()
        .with_eth_env(with_eth_env)
        .with_bridge_cluster(true)
        .with_num_validators(3)
        .build()
        .await;

    let db = TempDb::new().unwrap();

    // Run database migration
    let conn = Connection::dedicated(db.database().url()).await.unwrap();
    conn.run_pending_migrations(MIGRATIONS).await.unwrap();

    let config = IndexerConfig {
        remote_store_url: format!("{}/rest", bridge_test_cluster.sui_rpc_url()),
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
    };

    (config, bridge_test_cluster, db)
}
