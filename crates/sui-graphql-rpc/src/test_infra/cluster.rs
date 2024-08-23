// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::ConnectionConfig;
use crate::config::ServerConfig;
use crate::config::ServiceConfig;
use crate::config::Version;
use crate::server::graphiql_server::start_graphiql_server;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_graphql_rpc_client::simple_client::SimpleClient;
use sui_indexer::errors::IndexerError;
pub use sui_indexer::handlers::objects_snapshot_processor::SnapshotLagConfig;
use sui_indexer::store::indexer_store::IndexerStore;
use sui_indexer::store::PgIndexerStore;
use sui_indexer::test_utils::force_delete_database;
use sui_indexer::test_utils::start_test_indexer_impl;
use sui_indexer::test_utils::ReaderWriterConfig;
use sui_swarm_config::genesis_config::{AccountConfig, DEFAULT_GAS_AMOUNT};
use sui_types::storage::RestStateReader;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use tokio::join;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;

const VALIDATOR_COUNT: usize = 7;
const EPOCH_DURATION_MS: u64 = 15000;

const ACCOUNT_NUM: usize = 20;
const GAS_OBJECT_COUNT: usize = 3;

pub const DEFAULT_INTERNAL_DATA_SOURCE_PORT: u16 = 3000;

pub struct ExecutorCluster {
    pub executor_server_handle: JoinHandle<()>,
    pub indexer_store: PgIndexerStore<diesel::PgConnection>,
    pub indexer_join_handle: JoinHandle<Result<(), IndexerError>>,
    pub graphql_server_join_handle: JoinHandle<()>,
    pub graphql_client: SimpleClient,
    pub snapshot_config: SnapshotLagConfig,
    pub graphql_connection_config: ConnectionConfig,
    pub cancellation_token: CancellationToken,
}

pub struct Cluster {
    pub validator_fullnode_handle: TestCluster,
    pub indexer_store: PgIndexerStore<diesel::PgConnection>,
    pub indexer_join_handle: JoinHandle<Result<(), IndexerError>>,
    pub graphql_server_join_handle: JoinHandle<()>,
    pub graphql_client: SimpleClient,
    pub cancellation_token: CancellationToken,
}

/// Starts a validator, fullnode, indexer, and graphql service for testing.
pub async fn start_cluster(
    graphql_connection_config: ConnectionConfig,
    internal_data_source_rpc_port: Option<u16>,
) -> Cluster {
    let data_ingestion_path = tempfile::tempdir().unwrap().into_path();
    let db_url = graphql_connection_config.db_url.clone();
    let cancellation_token = CancellationToken::new();
    // Starts validator+fullnode
    let val_fn =
        start_validator_with_fullnode(internal_data_source_rpc_port, data_ingestion_path.clone())
            .await;

    // Starts indexer
    let (pg_store, pg_handle) = start_test_indexer_impl(
        Some(db_url),
        val_fn.rpc_url().to_string(),
        ReaderWriterConfig::writer_mode(None),
        /* reset_database */ true,
        Some(data_ingestion_path),
        cancellation_token.clone(),
    )
    .await;

    // Starts graphql server
    let fn_rpc_url = val_fn.rpc_url().to_string();
    let graphql_server_handle = start_graphql_server_with_fn_rpc(
        graphql_connection_config.clone(),
        Some(fn_rpc_url),
        Some(cancellation_token.clone()),
    )
    .await;

    let server_url = format!(
        "http://{}:{}/",
        graphql_connection_config.host, graphql_connection_config.port
    );

    // Starts graphql client
    let client = SimpleClient::new(server_url);
    wait_for_graphql_server(&client).await;

    Cluster {
        validator_fullnode_handle: val_fn,
        indexer_store: pg_store,
        indexer_join_handle: pg_handle,
        graphql_server_join_handle: graphql_server_handle,
        graphql_client: client,
        cancellation_token,
    }
}

/// Takes in a simulated instantiation of a Sui blockchain and builds a cluster around it. This
/// cluster is typically used in e2e tests to emulate and test behaviors.
pub async fn serve_executor(
    graphql_connection_config: ConnectionConfig,
    internal_data_source_rpc_port: u16,
    executor: Arc<dyn RestStateReader + Send + Sync>,
    snapshot_config: Option<SnapshotLagConfig>,
    data_ingestion_path: PathBuf,
) -> ExecutorCluster {
    let db_url = graphql_connection_config.db_url.clone();
    // Creates a cancellation token and adds this to the ExecutorCluster, so that we can send a
    // cancellation token on cleanup
    let cancellation_token = CancellationToken::new();

    let executor_server_url: SocketAddr = format!("127.0.0.1:{}", internal_data_source_rpc_port)
        .parse()
        .unwrap();

    let executor_server_handle = tokio::spawn(async move {
        sui_rest_api::RestService::new_without_version(executor)
            .start_service(executor_server_url)
            .await;
    });

    let (pg_store, pg_handle) = start_test_indexer_impl(
        Some(db_url),
        format!("http://{}", executor_server_url),
        ReaderWriterConfig::writer_mode(snapshot_config.clone()),
        /* reset_database */ true,
        Some(data_ingestion_path),
        cancellation_token.clone(),
    )
    .await;

    // Starts graphql server
    let graphql_server_handle = start_graphql_server(
        graphql_connection_config.clone(),
        cancellation_token.clone(),
    )
    .await;

    let server_url = format!(
        "http://{}:{}/",
        graphql_connection_config.host, graphql_connection_config.port
    );

    // Starts graphql client
    let client = SimpleClient::new(server_url);
    wait_for_graphql_server(&client).await;

    ExecutorCluster {
        executor_server_handle,
        indexer_store: pg_store,
        indexer_join_handle: pg_handle,
        graphql_server_join_handle: graphql_server_handle,
        graphql_client: client,
        snapshot_config: snapshot_config.unwrap_or_default(),
        graphql_connection_config,
        cancellation_token,
    }
}

pub async fn start_graphql_server(
    graphql_connection_config: ConnectionConfig,
    cancellation_token: CancellationToken,
) -> JoinHandle<()> {
    start_graphql_server_with_fn_rpc(graphql_connection_config, None, Some(cancellation_token))
        .await
}

pub async fn start_graphql_server_with_fn_rpc(
    graphql_connection_config: ConnectionConfig,
    fn_rpc_url: Option<String>,
    cancellation_token: Option<CancellationToken>,
) -> JoinHandle<()> {
    let cancellation_token = cancellation_token.unwrap_or_default();
    let mut server_config = ServerConfig {
        connection: graphql_connection_config,
        service: ServiceConfig::test_defaults(),
        ..ServerConfig::default()
    };
    if let Some(fn_rpc_url) = fn_rpc_url {
        server_config.tx_exec_full_node.node_rpc_url = Some(fn_rpc_url);
    };

    // Starts graphql server
    tokio::spawn(async move {
        start_graphiql_server(&server_config, &Version::for_testing(), cancellation_token)
            .await
            .unwrap();
    })
}

async fn start_validator_with_fullnode(
    internal_data_source_rpc_port: Option<u16>,
    data_ingestion_dir: PathBuf,
) -> TestCluster {
    let mut test_cluster_builder = TestClusterBuilder::new()
        .with_num_validators(VALIDATOR_COUNT)
        .with_epoch_duration_ms(EPOCH_DURATION_MS)
        .with_data_ingestion_dir(data_ingestion_dir)
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

/// Repeatedly ping the GraphQL server for 10s, until it responds
async fn wait_for_graphql_server(client: &SimpleClient) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while client.ping().await.is_err() {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .expect("Timeout waiting for graphql server to start");
}

/// Ping the GraphQL server until its background task has updated the checkpoint watermark to the
/// desired checkpoint.
async fn wait_for_graphql_checkpoint_catchup(
    client: &SimpleClient,
    checkpoint: u64,
    base_timeout: Duration,
) {
    info!(
        "Waiting for graphql to catchup to checkpoint {}, base time out is {}",
        checkpoint,
        base_timeout.as_secs()
    );
    let query = r#"
    {
        availableRange {
            last {
                sequenceNumber
            }
        }
    }"#;

    let timeout = base_timeout.mul_f64(checkpoint.max(1) as f64);

    tokio::time::timeout(timeout, async {
        loop {
            let resp = client
                .execute_to_graphql(query.to_string(), false, vec![], vec![])
                .await
                .unwrap()
                .response_body_json();

            let current_checkpoint = resp["data"]["availableRange"]["last"].get("sequenceNumber");
            info!("Current checkpoint: {:?}", current_checkpoint);
            // Indexer has not picked up any checkpoints yet
            let Some(current_checkpoint) = current_checkpoint else {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            };

            // Indexer has picked up a checkpoint, but it's not the one we're waiting for
            let current_checkpoint = current_checkpoint.as_u64().unwrap();
            if current_checkpoint < checkpoint {
                tokio::time::sleep(Duration::from_secs(1)).await;
            } else {
                break;
            }
        }
    })
    .await
    .expect("Timeout waiting for graphql to catchup to checkpoint");
}

impl Cluster {
    /// Waits for the indexer to index up to the given checkpoint, then waits for the graphql
    /// service's background task to update the checkpoint watermark to the given checkpoint.
    pub async fn wait_for_checkpoint_catchup(&self, checkpoint: u64, base_timeout: Duration) {
        wait_for_graphql_checkpoint_catchup(&self.graphql_client, checkpoint, base_timeout).await
    }

    /// Sends a cancellation signal to the graphql and indexer services and waits for them to
    /// shutdown.
    pub async fn cleanup_resources(self) {
        self.cancellation_token.cancel();
        let _ = join!(self.graphql_server_join_handle, self.indexer_join_handle);
    }
}

impl ExecutorCluster {
    /// Waits for the indexer to index up to the given checkpoint, then waits for the graphql
    /// service's background task to update the checkpoint watermark to the given checkpoint.
    pub async fn wait_for_checkpoint_catchup(&self, checkpoint: u64, base_timeout: Duration) {
        wait_for_graphql_checkpoint_catchup(&self.graphql_client, checkpoint, base_timeout).await
    }

    /// The ObjectsSnapshotProcessor is a long-running task that periodically takes a snapshot of
    /// the objects table. This leads to flakiness in tests, so we wait until the objects_snapshot
    /// has reached the expected state.
    pub async fn wait_for_objects_snapshot_catchup(&self, base_timeout: Duration) {
        let mut latest_snapshot_cp = 0;

        let latest_cp = self
            .indexer_store
            .get_latest_checkpoint_sequence_number()
            .await
            .unwrap()
            .unwrap();

        tokio::time::timeout(base_timeout, async {
            while latest_cp > latest_snapshot_cp + self.snapshot_config.snapshot_max_lag as u64 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                latest_snapshot_cp = self
                    .indexer_store
                    .get_latest_object_snapshot_checkpoint_sequence_number()
                    .await
                    .unwrap()
                    .unwrap_or_default();
            }
        })
        .await
        .unwrap_or_else(|_| panic!("Timeout waiting for indexer to update objects snapshot - latest_cp: {}, latest_snapshot_cp: {}",
        latest_cp, latest_snapshot_cp));
    }

    /// Sends a cancellation signal to the graphql and indexer services, waits for them to complete,
    /// and then deletes the database created for the test.
    pub async fn cleanup_resources(self) {
        self.cancellation_token.cancel();
        let _ = join!(self.graphql_server_join_handle, self.indexer_join_handle);
        let db_url = self.graphql_connection_config.db_url.clone();
        force_delete_database::<diesel::PgConnection>(db_url).await;
    }

    pub async fn force_objects_snapshot_catchup(&self, start_cp: u64, end_cp: u64) {
        self.indexer_store
            .update_objects_snapshot(start_cp, end_cp)
            .await
            .unwrap();

        let mut latest_snapshot_cp = self
            .indexer_store
            .get_latest_object_snapshot_checkpoint_sequence_number()
            .await
            .unwrap()
            .unwrap_or_default();

        tokio::time::timeout(Duration::from_secs(60), async {
            while latest_snapshot_cp < end_cp - 1 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                latest_snapshot_cp = self
                    .indexer_store
                    .get_latest_object_snapshot_checkpoint_sequence_number()
                    .await
                    .unwrap()
                    .unwrap_or_default();
            }
        })
        .await
        .unwrap_or_else(|_| panic!("Timeout waiting for indexer to update objects snapshot - latest_snapshot_cp: {}, end_cp: {}",
        latest_snapshot_cp, end_cp));

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
