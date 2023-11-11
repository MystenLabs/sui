// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::client::simple_client::SimpleClient;
use crate::config::ConnectionConfig;
use crate::config::ServerConfig;
use crate::server::simple_server::start_example_server;
use mysten_metrics::init_metrics;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use sui_indexer::errors::IndexerError;
use sui_indexer::indexer_v2::IndexerV2;
use sui_indexer::metrics::IndexerMetrics;
use sui_indexer::new_pg_connection_pool_impl;
use sui_indexer::store::PgIndexerStoreV2;
use sui_indexer::utils::reset_database;
use sui_indexer::IndexerConfig;
use sui_rest_api::node_state_getter::NodeStateGetter;
use sui_swarm_config::genesis_config::{AccountConfig, DEFAULT_GAS_AMOUNT};
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use tokio::task::JoinHandle;

const VALIDATOR_COUNT: usize = 7;
const EPOCH_DURATION_MS: u64 = 15000;

const ACCOUNT_NUM: usize = 20;
const GAS_OBJECT_COUNT: usize = 3;

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
        start_test_indexer(Some(db_url), val_fn.rpc_url().to_string()).await;

    // Starts graphql server
    let graphql_server_handle = start_graphql_server(graphql_connection_config.clone()).await;
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

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
    let (pg_store, pg_handle) =
        start_test_indexer(Some(db_url), format!("http://{}", executor_server_url)).await;

    // Starts graphql server
    let graphql_server_handle = start_graphql_server(graphql_connection_config.clone()).await;
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

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

async fn start_graphql_server(graphql_connection_config: ConnectionConfig) -> JoinHandle<()> {
    let server_config = ServerConfig {
        connection: graphql_connection_config,
        ..ServerConfig::default()
    };

    // Starts graphql server
    tokio::spawn(async move {
        start_example_server(&server_config).await.unwrap();
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

pub async fn start_test_indexer(
    db_url: Option<String>,
    rpc_url: String,
) -> (PgIndexerStoreV2, JoinHandle<Result<(), IndexerError>>) {
    let db_url = db_url.unwrap_or_else(|| {
        let pg_host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".into());
        let pg_port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "32770".into());
        let pw = env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgrespw".into());
        format!("postgres://postgres:{pw}@{pg_host}:{pg_port}")
    });

    let config = IndexerConfig {
        db_url: Some(db_url.clone()),
        rpc_client_url: rpc_url,
        migrated_methods: IndexerConfig::all_implemented_methods(),
        reset_db: true,
        fullnode_sync_worker: true,
        rpc_server_worker: false,
        use_v2: true,
        ..Default::default()
    };

    let parsed_url = config.get_db_url().unwrap();
    let blocking_pool = new_pg_connection_pool_impl(&parsed_url, Some(5)).unwrap();
    if config.reset_db {
        reset_database(&mut blocking_pool.get().unwrap(), true, config.use_v2).unwrap();
    }

    let registry = prometheus::Registry::default();

    init_metrics(&registry);

    let indexer_metrics = IndexerMetrics::new(&registry);

    let store = PgIndexerStoreV2::new(blocking_pool, indexer_metrics.clone());
    let store_clone = store.clone();
    let handle = tokio::spawn(async move {
        IndexerV2::start_writer(&config, store_clone, indexer_metrics).await
    });
    (store, handle)
}

pub async fn simulator_commands_test_impl() {
    let test_file_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test_infra")
        .join("data")
        .join("example.move");

    let output_file_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test_infra")
        .join("data")
        .join("example.exp");

    // Read the file into a string
    let file_contents = std::fs::read_to_string(&test_file_path).unwrap();

    let (output, adapter) = move_transactional_test_runner::framework::handle_actual_output::<
        sui_transactional_test_runner::test_adapter::SuiTestAdapter,
    >(
        &test_file_path,
        Some(&*sui_transactional_test_runner::test_adapter::PRE_COMPILED),
    )
    .await
    .unwrap();

    // Read the file into a string
    let output_file_contents = match std::fs::read_to_string(&output_file_path) {
        Ok(contents) => contents,
        Err(_) => {
            // If the file doesn't exist, create it
            std::fs::write(&output_file_path, output.clone()).unwrap();
            output.clone()
        }
    };

    // Check that the output matches the expected output
    assert_eq!(output_file_contents, output);

    let checkpoint = adapter.get_latest_checkpoint_sequence_number().unwrap();
    let checkpoint_info = adapter
        .get_verified_checkpoint_by_sequence_number(checkpoint)
        .unwrap();

    let num_actual_checkpoint_commands = file_contents.match_indices("create-checkpoint").count();
    let num_actual_epoch_commands = file_contents.match_indices("advance-epoch").count();

    // Each epoch creation also creates a checkpoint
    assert_eq!(
        checkpoint as usize,
        num_actual_checkpoint_commands + num_actual_epoch_commands
    );

    // Since we start at epoch 0, we need to subtract 1
    assert_eq!(
        checkpoint_info.epoch() as usize,
        num_actual_epoch_commands - 1
    );

    // Serve the data, start an indexer, and graphql server
    let cluster = serve_executor(
        ConnectionConfig::ci_integration_test_cfg(),
        3000,
        Arc::new(adapter),
    )
    .await;

    let query = r#"{
      checkpointConnection (last: 1) {
        nodes {
          sequenceNumber
        }
      }
    }"#
    .to_string();

    // Get the latest checkpoint via graphql
    let resp = cluster.graphql_client.execute(query, vec![]).await.unwrap();

    // Result should be something like {"data":{"checkpointConnection":{"nodes":[{"sequenceNumber":XYZ}]}}}
    // Where XYZ is the seq number of the latest checkpoint
    let fetched_chk = resp
        .get("data")
        .unwrap()
        .get("checkpointConnection")
        .unwrap()
        .get("nodes")
        .unwrap()
        .as_array()
        .unwrap()[0]
        .get("sequenceNumber")
        .unwrap()
        .as_i64()
        .unwrap();

    assert_eq!(fetched_chk as u64, checkpoint);
}
