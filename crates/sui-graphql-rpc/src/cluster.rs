// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::client::simple_client::SimpleClient;
use crate::config::ConnectionConfig;
use crate::config::ServiceConfig;
use crate::server::simple_server::start_example_server;
use std::env;
use sui_indexer::errors::IndexerError;
use sui_indexer::indexer_v2::IndexerV2;
use sui_indexer::metrics::IndexerMetrics;
use sui_indexer::new_pg_connection_pool_impl;
use sui_indexer::store::PgIndexerStoreV2;
use sui_indexer::utils::reset_database;
use sui_indexer::IndexerConfig;
use sui_swarm_config::genesis_config::{AccountConfig, DEFAULT_GAS_AMOUNT};
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use tokio::task::JoinHandle;

const VALIDATOR_COUNT: usize = 7;
const EPOCH_DURATION_MS: u64 = 15000;

const ACCOUNT_NUM: usize = 20;
const GAS_OBJECT_COUNT: usize = 3;

pub struct Cluster {
    pub validator_fullnode_handle: TestCluster,
    pub indexer_store: PgIndexerStoreV2,
    pub indexer_join_handle: JoinHandle<Result<(), IndexerError>>,
    pub graphql_server_join_handle: JoinHandle<()>,
    pub graphql_client: SimpleClient,
}

pub async fn start_cluster(connection_config: ConnectionConfig) -> Cluster {
    let db_url = connection_config.db_url.clone();
    // Starts validator+fullnode+indexer cluster
    let (val_fn, pg_store, pg_handle, db_url) = start_test_cluster(Some(db_url)).await;

    // Starts graphql server
    let conn = ConnectionConfig::new(
        None,
        None,
        Some(val_fn.rpc_url().to_string()),
        Some(db_url),
        None,
        None,
    );

    let addr = format!("http://{}:{}/", conn.host, conn.port);

    let graphql_server_handle = tokio::spawn(async move {
        start_example_server(conn, ServiceConfig::default())
            .await
            .unwrap();
    });

    let client = SimpleClient::new(addr);

    Cluster {
        validator_fullnode_handle: val_fn,
        indexer_store: pg_store,
        indexer_join_handle: pg_handle,
        graphql_server_join_handle: graphql_server_handle,
        graphql_client: client,
    }
}

async fn start_test_cluster(
    db_url: Option<String>,
) -> (
    TestCluster,
    PgIndexerStoreV2,
    JoinHandle<Result<(), IndexerError>>,
    String,
) {
    let db_url = db_url.unwrap_or_else(|| {
        let pg_host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".into());
        let pg_port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "32770".into());
        let pw = env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgrespw".into());
        format!("postgres://postgres:{pw}@{pg_host}:{pg_port}")
    });

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(VALIDATOR_COUNT)
        .with_epoch_duration_ms(EPOCH_DURATION_MS)
        .with_accounts(vec![
            AccountConfig {
                address: None,
                gas_amounts: vec![DEFAULT_GAS_AMOUNT; GAS_OBJECT_COUNT],
            };
            ACCOUNT_NUM
        ])
        .build()
        .await;

    let config = IndexerConfig {
        db_url: Some(db_url.clone()),
        rpc_client_url: test_cluster.rpc_url().to_string(),
        migrated_methods: IndexerConfig::all_implemented_methods(),
        reset_db: true,
        fullnode_sync_worker: true,
        rpc_server_worker: false,
        use_v2: true,
        ..Default::default()
    };

    let (store, handle) = start_test_indexer(config).await;
    tokio::time::sleep(std::time::Duration::from_secs(20)).await;

    (test_cluster, store, handle, db_url)
}

pub async fn start_test_indexer(
    config: IndexerConfig,
) -> (PgIndexerStoreV2, JoinHandle<Result<(), IndexerError>>) {
    let parsed_url = config.get_db_url().unwrap();
    let blocking_pool = new_pg_connection_pool_impl(&parsed_url, Some(50)).unwrap();
    if config.reset_db {
        reset_database(&mut blocking_pool.get().unwrap(), true, config.use_v2).unwrap();
    }

    let registry = prometheus::Registry::default();
    let indexer_metrics = IndexerMetrics::new(&registry);

    let store = PgIndexerStoreV2::new(blocking_pool, indexer_metrics.clone());
    let store_clone = store.clone();
    let handle = tokio::spawn(async move {
        IndexerV2::start_writer(&config, store_clone, indexer_metrics).await
    });
    (store, handle)
}
