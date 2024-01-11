// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::ConnectionConfig;
use crate::config::Limits;
use crate::config::ServerConfig;
use crate::config::ServiceConfig;
use crate::server::graphiql_server::start_graphiql_server;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use sui_graphql_rpc_client::simple_client::SimpleClient;
use sui_indexer::errors::IndexerError;
use sui_indexer::store::indexer_store_v2::IndexerStoreV2;
use sui_indexer::store::PgIndexerStoreV2;
use sui_indexer::test_utils::start_test_indexer_v2;
use sui_rest_api::node_state_getter::NodeStateGetter;
use sui_swarm_config::genesis_config::{AccountConfig, DEFAULT_GAS_AMOUNT};
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use tokio::task::JoinHandle;

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

// TODO (wlmyng) what's the diff between this and start_cluster? This yields an executor to do e2e tests, start_cluster only creates
pub async fn serve_executor(
    graphql_connection_config: ConnectionConfig,
    internal_data_source_rpc_port: u16,
    executor: Arc<dyn NodeStateGetter>,
    env_vars: BTreeMap<String, String>,
) -> ExecutorCluster {
    for (k, v) in env_vars {
        std::env::set_var(k, v);
    }

    let db_url = graphql_connection_config.db_url.clone();

    let executor_server_url: SocketAddr = format!("127.0.0.1:{}", internal_data_source_rpc_port)
        .parse()
        .unwrap();

    let executor_server_handle = tokio::spawn(async move {
        sui_rest_api::start_service(executor_server_url, executor, Some("/rest".to_owned())).await;
    });

    // set the env variables

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
        service: ServiceConfig {
            // Use special limits for testing
            limits: Limits::default_for_simulator_testing(),
            ..ServiceConfig::default()
        },
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
