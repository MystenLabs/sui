// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::client::simple_client::SimpleClient;
use crate::config::ConnectionConfig;
use crate::config::ServerConfig;
use crate::server::graphiql_server::start_graphiql_server;
use mysten_metrics::init_metrics;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use sui_indexer::errors::IndexerError;
use sui_indexer::indexer_v2::IndexerV2;
use sui_indexer::metrics::IndexerMetrics;
use sui_indexer::new_pg_connection_pool_impl;
use sui_indexer::store::indexer_store_v2::IndexerStoreV2;
use sui_indexer::store::PgIndexerStoreV2;
use sui_indexer::utils::reset_database;
use sui_indexer::IndexerConfig;
use sui_rest_api::node_state_getter::NodeStateGetter;
use sui_swarm_config::genesis_config::{AccountConfig, DEFAULT_GAS_AMOUNT};
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use tokio::task::JoinHandle;
use tracing::info;

const VALIDATOR_COUNT: usize = 7;
const EPOCH_DURATION_MS: u64 = 15000;

const ACCOUNT_NUM: usize = 20;
const GAS_OBJECT_COUNT: usize = 3;

pub const DEFAULT_INTERNAL_DATA_SOURCE_PORT: u16 = 3000;

pub struct ExecutorCluster {
    pub executor_server_handle: JoinHandle<()>,
    pub indexer_store: PgIndexerStoreV2,
    pub indexer_join_handle: JoinHandle<Result<(), IndexerError>>,
    pub graphql_server_join_handle: JoinHandle<()>,
    pub graphql_client: SimpleClient,
}

pub struct Cluster {
    pub validator_fullnode_handle: TestCluster,
    pub indexer_store: PgIndexerStoreV2,
    pub indexer_join_handle: JoinHandle<Result<(), IndexerError>>,
    pub graphql_server_join_handle: JoinHandle<()>,
    pub graphql_client: SimpleClient,
}

pub async fn start_cluster(
    graphql_connection_config: ConnectionConfig,
    internal_data_source_rpc_port: Option<u16>,
) -> Cluster {
    let db_url = graphql_connection_config.db_url.clone();
    // Starts validator+fullnode
    let val_fn = start_validator_with_fullnode(internal_data_source_rpc_port).await;

    // Starts indexer
    let (pg_store, pg_handle) =
        start_test_indexer_v2(Some(db_url), val_fn.rpc_url().to_string(), None, true).await;

    // Starts graphql server
    let fn_rpc_url = val_fn.rpc_url().to_string();
    let graphql_server_handle =
        start_graphql_server_with_fn_rpc(graphql_connection_config.clone(), Some(fn_rpc_url)).await;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let server_url = format!(
        "http://{}:{}/",
        graphql_connection_config.host, graphql_connection_config.port
    );

    // Starts graphql client
    let client = SimpleClient::new(server_url);

    Cluster {
        validator_fullnode_handle: val_fn,
        indexer_store: pg_store,
        indexer_join_handle: pg_handle,
        graphql_server_join_handle: graphql_server_handle,
        graphql_client: client,
    }
}

pub async fn serve_executor(
    graphql_connection_config: ConnectionConfig,
    internal_data_source_rpc_port: u16,
    executor: Arc<dyn NodeStateGetter>,
) -> ExecutorCluster {
    let db_url = graphql_connection_config.db_url.clone();

    let executor_server_url: SocketAddr = format!("127.0.0.1:{}", internal_data_source_rpc_port)
        .parse()
        .unwrap();

    let executor_server_handle = tokio::spawn(async move {
        sui_rest_api::start_service(executor_server_url, executor, Some("/rest".to_owned())).await;
    });

    // Starts indexer
    let (pg_store, pg_handle) = start_test_indexer_v2(
        Some(db_url),
        format!("http://{}", executor_server_url),
        None,
        true,
    )
    .await;

    // Starts graphql server
    let graphql_server_handle = start_graphql_server(graphql_connection_config.clone()).await;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let server_url = format!(
        "http://{}:{}/",
        graphql_connection_config.host, graphql_connection_config.port
    );

    // Starts graphql client
    let client = SimpleClient::new(server_url);

    ExecutorCluster {
        executor_server_handle,
        indexer_store: pg_store,
        indexer_join_handle: pg_handle,
        graphql_server_join_handle: graphql_server_handle,
        graphql_client: client,
    }
}

pub async fn start_graphql_server(graphql_connection_config: ConnectionConfig) -> JoinHandle<()> {
    start_graphql_server_with_fn_rpc(graphql_connection_config, None).await
}

pub async fn start_graphql_server_with_fn_rpc(
    graphql_connection_config: ConnectionConfig,
    fn_rpc_url: Option<String>,
) -> JoinHandle<()> {
    let mut server_config = ServerConfig {
        connection: graphql_connection_config,
        ..ServerConfig::default()
    };
    if let Some(fn_rpc_url) = fn_rpc_url {
        server_config.tx_exec_full_node.node_rpc_url = Some(fn_rpc_url);
    };

    // Starts graphql server
    tokio::spawn(async move {
        start_graphiql_server(&server_config).await.unwrap();
    })
}

async fn start_validator_with_fullnode(internal_data_source_rpc_port: Option<u16>) -> TestCluster {
    let mut test_cluster_builder = TestClusterBuilder::new()
        .with_num_validators(VALIDATOR_COUNT)
        .with_epoch_duration_ms(EPOCH_DURATION_MS)
        .with_accounts(vec![
            AccountConfig {
                address: None,
                gas_amounts: vec![DEFAULT_GAS_AMOUNT; GAS_OBJECT_COUNT],
            };
            ACCOUNT_NUM
        ]);

    if let Some(internal_data_source_rpc_port) = internal_data_source_rpc_port {
        test_cluster_builder =
            test_cluster_builder.with_fullnode_rpc_port(internal_data_source_rpc_port);
    };
    test_cluster_builder.build().await
}

pub async fn start_test_indexer_v2(
    db_url: Option<String>,
    rpc_url: String,
    reader_mode_rpc_url: Option<String>,
    use_indexer_experimental_methods: bool,
) -> (PgIndexerStoreV2, JoinHandle<Result<(), IndexerError>>) {
    // Reduce the connection pool size to 20 for testing
    // to prevent maxing out
    info!("Setting DB_POOL_SIZE to 20");
    std::env::set_var("DB_POOL_SIZE", "20");

    let db_url = db_url.unwrap_or_else(|| {
        let pg_host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".into());
        let pg_port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "32770".into());
        let pw = env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgrespw".into());
        format!("postgres://postgres:{pw}@{pg_host}:{pg_port}")
    });

    let migrated_methods = if use_indexer_experimental_methods {
        IndexerConfig::all_implemented_methods()
    } else {
        vec![]
    };

    // Default weiter mode
    let mut config = IndexerConfig {
        db_url: Some(db_url.clone()),
        rpc_client_url: rpc_url,
        migrated_methods,
        reset_db: true,
        fullnode_sync_worker: true,
        rpc_server_worker: false,
        use_v2: true,
        ..Default::default()
    };

    if let Some(reader_mode_rpc_url) = &reader_mode_rpc_url {
        let reader_mode_rpc_url = reader_mode_rpc_url
            .parse::<SocketAddr>()
            .expect("Unable to parse fullnode address");
        config.fullnode_sync_worker = false;
        config.rpc_server_worker = true;
        config.rpc_server_url = reader_mode_rpc_url.ip().to_string();
        config.rpc_server_port = reader_mode_rpc_url.port();
    }

    let parsed_url = config.get_db_url().unwrap();
    let blocking_pool = new_pg_connection_pool_impl(&parsed_url, Some(5)).unwrap();
    if config.reset_db && reader_mode_rpc_url.is_none() {
        reset_database(&mut blocking_pool.get().unwrap(), true, config.use_v2).unwrap();
    }

    let registry = prometheus::Registry::default();

    init_metrics(&registry);

    let indexer_metrics = IndexerMetrics::new(&registry);

    let store = PgIndexerStoreV2::new(blocking_pool, indexer_metrics.clone());
    let store_clone = store.clone();
    let handle = if reader_mode_rpc_url.is_some() {
        tokio::spawn(async move { IndexerV2::start_reader(&config, &registry, db_url).await })
    } else {
        tokio::spawn(
            async move { IndexerV2::start_writer(&config, store_clone, indexer_metrics).await },
        )
    };

    (store, handle)
}

impl ExecutorCluster {
    pub async fn wait_for_checkpoint_catchup(&self, checkpoint: u64, base_timeout: Duration) {
        let current_checkpoint = self
            .indexer_store
            .get_latest_tx_checkpoint_sequence_number()
            .await
            .unwrap()
            .unwrap();

        let checkpoint_diff = std::cmp::max(1, checkpoint.saturating_sub(current_checkpoint));
        let timeout = base_timeout.mul_f64(checkpoint_diff as f64);

        tokio::time::timeout(timeout, async {
            while self
                .indexer_store
                .get_latest_tx_checkpoint_sequence_number()
                .await
                .unwrap()
                .unwrap()
                < checkpoint
            {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        })
        .await
        .expect("Timeout waiting for indexer to catchup to checkpoint");
    }
}
