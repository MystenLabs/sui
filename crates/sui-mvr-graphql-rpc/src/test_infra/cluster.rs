// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::ConnectionConfig;
use crate::config::ServerConfig;
use crate::config::ServiceConfig;
use crate::config::Version;
use crate::server::graphiql_server::start_graphiql_server;
use rand::rngs::StdRng;
use rand::SeedableRng;
use simulacrum::Simulacrum;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_graphql_rpc_client::simple_client::SimpleClient;
pub use sui_indexer::config::RetentionConfig;
pub use sui_indexer::config::SnapshotLagConfig;
use sui_indexer::errors::IndexerError;
use sui_indexer::store::PgIndexerStore;
use sui_indexer::test_utils::start_indexer_writer_for_testing_with_mvr_mode;
use sui_pg_db::temp::{get_available_port, TempDb};
use sui_swarm_config::genesis_config::{AccountConfig, DEFAULT_GAS_AMOUNT};
use sui_types::storage::RpcStateReader;
use tempfile::tempdir;
use tempfile::TempDir;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use tokio::join;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;

const VALIDATOR_COUNT: usize = 4;
/// Set default epoch duration to 300s. This high value is to turn the TestCluster into a lockstep
/// network of sorts. Tests should call `trigger_reconfiguration` to advance the network's epoch.
const EPOCH_DURATION_MS: u64 = 300_000;

const ACCOUNT_NUM: usize = 20;
const GAS_OBJECT_COUNT: usize = 3;

pub struct ExecutorCluster {
    pub executor_server_handle: JoinHandle<()>,
    pub indexer_store: PgIndexerStore,
    pub indexer_join_handle: JoinHandle<Result<(), IndexerError>>,
    pub graphql_server_join_handle: JoinHandle<()>,
    pub graphql_client: SimpleClient,
    pub snapshot_config: SnapshotLagConfig,
    pub graphql_connection_config: ConnectionConfig,
    pub cancellation_token: CancellationToken,
    #[allow(unused)]
    database: TempDb,
    tempdir: Option<TempDir>,
}

pub struct Cluster {
    pub network: NetworkCluster,
    pub graphql_server_join_handle: JoinHandle<()>,
    pub graphql_client: SimpleClient,
}

pub struct NetworkCluster {
    pub validator_fullnode_handle: TestCluster,
    pub indexer_store: PgIndexerStore,
    pub indexer_join_handle: JoinHandle<Result<(), IndexerError>>,
    pub cancellation_token: CancellationToken,
    #[allow(unused)]
    data_ingestion_path: TempDir,
    #[allow(unused)]
    database: TempDb,
    pub graphql_connection_config: ConnectionConfig,
}

/// Starts a validator, fullnode, indexer, and graphql service for testing.
pub async fn start_cluster(service_config: ServiceConfig) -> Cluster {
    let network_cluster = start_network_cluster().await;
    let graphql_connection_config = network_cluster.graphql_connection_config.clone();

    let fn_rpc_url: String = network_cluster
        .validator_fullnode_handle
        .rpc_url()
        .to_string();

    let server_url = format!(
        "http://{}:{}/",
        graphql_connection_config.host, graphql_connection_config.port
    );

    let graphql_server_handle = start_graphql_server_with_fn_rpc(
        graphql_connection_config,
        Some(fn_rpc_url),
        Some(network_cluster.cancellation_token.clone()),
        service_config,
    )
    .await;

    // Starts graphql client
    let client = SimpleClient::new(server_url);
    wait_for_graphql_server(&client).await;

    Cluster {
        network: network_cluster,
        graphql_server_join_handle: graphql_server_handle,
        graphql_client: client,
    }
}

/// Starts a validator, fullnode, indexer (using data ingestion). Re-using GraphQL's ConnectionConfig for convenience.
/// This does not start any GraphQL services, only the network cluster. You can start a GraphQL service
/// calling `start_graphql_server`.
pub async fn start_network_cluster() -> NetworkCluster {
    let database = TempDb::new().unwrap();
    let graphql_connection_config = ConnectionConfig {
        port: get_available_port(),
        host: "127.0.0.1".to_owned(),
        db_url: database.database().url().as_str().to_owned(),
        db_pool_size: 5,
        prom_host: "127.0.0.1".to_owned(),
        prom_port: get_available_port(),
        skip_migration_consistency_check: false,
    };
    let data_ingestion_path = tempfile::tempdir().unwrap();
    let db_url = graphql_connection_config.db_url.clone();
    let cancellation_token = CancellationToken::new();

    // Starts validator+fullnode
    let val_fn = start_validator_with_fullnode(data_ingestion_path.path().to_path_buf()).await;

    // Starts indexer
    let (pg_store, pg_handle, _) = start_indexer_writer_for_testing_with_mvr_mode(
        db_url,
        None,
        None,
        Some(data_ingestion_path.path().to_path_buf()),
        Some(cancellation_token.clone()),
        None, /* start_checkpoint */
        None, /* end_checkpoint */
        true,
    )
    .await;

    NetworkCluster {
        validator_fullnode_handle: val_fn,
        indexer_store: pg_store,
        indexer_join_handle: pg_handle,
        cancellation_token,
        data_ingestion_path,
        database,
        graphql_connection_config,
    }
}

/// Takes in a simulated instantiation of a Sui blockchain and builds a cluster around it. This
/// cluster is typically used in e2e tests to emulate and test behaviors.
pub async fn serve_executor(
    executor: Arc<dyn RpcStateReader + Send + Sync>,
    snapshot_config: Option<SnapshotLagConfig>,
    retention_config: Option<RetentionConfig>,
    data_ingestion_path: PathBuf,
) -> ExecutorCluster {
    let database = TempDb::new().unwrap();
    let graphql_connection_config = ConnectionConfig {
        port: get_available_port(),
        host: "127.0.0.1".to_owned(),
        db_url: database.database().url().as_str().to_owned(),
        db_pool_size: 5,
        prom_host: "127.0.0.1".to_owned(),
        prom_port: get_available_port(),
        skip_migration_consistency_check: false,
    };
    let db_url = graphql_connection_config.db_url.clone();
    // Creates a cancellation token and adds this to the ExecutorCluster, so that we can send a
    // cancellation token on cleanup
    let cancellation_token = CancellationToken::new();

    let executor_server_url: SocketAddr = format!("127.0.0.1:{}", get_available_port())
        .parse()
        .unwrap();

    let executor_server_handle = tokio::spawn(async move {
        sui_rpc_api::RpcService::new_without_version(executor)
            .start_service(executor_server_url)
            .await;
    });

    let snapshot_config = snapshot_config.unwrap_or_default();

    let (pg_store, pg_handle, _) = start_indexer_writer_for_testing_with_mvr_mode(
        db_url,
        Some(snapshot_config.clone()),
        retention_config,
        Some(data_ingestion_path),
        Some(cancellation_token.clone()),
        None,
        None,
        true,
    )
    .await;

    // Starts graphql server
    let graphql_server_handle = start_graphql_server(
        graphql_connection_config.clone(),
        cancellation_token.clone(),
        ServiceConfig::test_defaults(),
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
        snapshot_config,
        graphql_connection_config,
        cancellation_token,
        database,
        tempdir: None,
    }
}

pub async fn prep_executor_cluster() -> ExecutorCluster {
    let rng = StdRng::from_seed([12; 32]);
    let data_ingestion_path = tempdir().unwrap();
    let mut sim = Simulacrum::new_with_rng(rng);
    sim.set_data_ingestion_path(data_ingestion_path.path().to_path_buf());

    sim.create_checkpoint();
    sim.create_checkpoint();
    sim.create_checkpoint();
    sim.advance_epoch(true);
    sim.create_checkpoint();
    sim.advance_clock(
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap(),
    );
    sim.create_checkpoint();

    let mut cluster = serve_executor(
        Arc::new(sim),
        None,
        None,
        data_ingestion_path.path().to_path_buf(),
    )
    .await;

    cluster
        .wait_for_checkpoint_catchup(6, Duration::from_secs(10))
        .await;

    cluster.tempdir = Some(data_ingestion_path);
    cluster
}

pub async fn start_graphql_server(
    graphql_connection_config: ConnectionConfig,
    cancellation_token: CancellationToken,
    service_config: ServiceConfig,
) -> JoinHandle<()> {
    start_graphql_server_with_fn_rpc(
        graphql_connection_config,
        None,
        Some(cancellation_token),
        service_config,
    )
    .await
}

pub async fn start_graphql_server_with_fn_rpc(
    graphql_connection_config: ConnectionConfig,
    fn_rpc_url: Option<String>,
    cancellation_token: Option<CancellationToken>,
    service_config: ServiceConfig,
) -> JoinHandle<()> {
    let cancellation_token = cancellation_token.unwrap_or_default();
    let mut server_config = ServerConfig {
        connection: graphql_connection_config,
        service: service_config,
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

async fn start_validator_with_fullnode(data_ingestion_dir: PathBuf) -> TestCluster {
    TestClusterBuilder::new()
        .with_num_validators(VALIDATOR_COUNT)
        .with_epoch_duration_ms(EPOCH_DURATION_MS)
        .with_data_ingestion_dir(data_ingestion_dir)
        .with_accounts(vec![
            AccountConfig {
                address: None,
                gas_amounts: vec![DEFAULT_GAS_AMOUNT; GAS_OBJECT_COUNT],
            };
            ACCOUNT_NUM
        ])
        .build()
        .await
}

/// Repeatedly ping the GraphQL server for 60s, until it responds
pub async fn wait_for_graphql_server(client: &SimpleClient) {
    tokio::time::timeout(Duration::from_secs(60), async {
        while client.ping().await.is_err() {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .expect("Timeout waiting for graphql server to start");
}

/// Ping the GraphQL server until its background task has updated the checkpoint watermark to the
/// desired checkpoint.
pub async fn wait_for_graphql_checkpoint_catchup(
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

/// Ping the GraphQL server until its background task has updated the checkpoint watermark to the
/// desired checkpoint.
pub async fn wait_for_graphql_epoch_catchup(
    client: &SimpleClient,
    epoch: u64,
    base_timeout: Duration,
) {
    info!(
        "Waiting for graphql to catchup to epoch {}, base time out is {}",
        epoch,
        base_timeout.as_secs()
    );
    let query = r#"
    {
        epoch {
            epochId
        }
    }"#;

    let timeout = base_timeout.mul_f64(epoch.max(1) as f64);

    tokio::time::timeout(timeout, async {
        loop {
            let resp = client
                .execute_to_graphql(query.to_string(), false, vec![], vec![])
                .await
                .unwrap()
                .response_body_json();

            let latest_epoch = resp["data"]["epoch"].get("epochId");
            info!("Latest epoch: {:?}", latest_epoch);
            // Indexer has not picked up any epochs yet
            let Some(latest_epoch) = latest_epoch else {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            };

            // Indexer has picked up an epoch, but it's not the one we're waiting for
            let latest_epoch = latest_epoch.as_u64().unwrap();
            if latest_epoch < epoch {
                tokio::time::sleep(Duration::from_secs(1)).await;
            } else {
                break;
            }
        }
    })
    .await
    .expect("Timeout waiting for graphql to catchup to epoch");
}

/// Ping the GraphQL server for a checkpoint until an empty response is returned, indicating that
/// the checkpoint has been pruned.
pub async fn wait_for_graphql_checkpoint_pruned(
    client: &SimpleClient,
    checkpoint: u64,
    base_timeout: Duration,
) {
    info!(
        "Waiting for checkpoint to be pruned {}, base time out is {}",
        checkpoint,
        base_timeout.as_secs()
    );
    let query = format!(
        r#"
        {{
            checkpoint(id: {{ sequenceNumber: {} }}) {{
                sequenceNumber
            }}
        }}"#,
        checkpoint
    );

    let timeout = base_timeout.mul_f64(checkpoint.max(1) as f64);

    tokio::time::timeout(timeout, async {
        loop {
            let resp = client
                .execute_to_graphql(query.to_string(), false, vec![], vec![])
                .await
                .unwrap()
                .response_body_json();

            let current_checkpoint = &resp["data"]["checkpoint"];
            if current_checkpoint.is_null() {
                break;
            } else {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    })
    .await
    .expect("Timeout waiting for checkpoint to be pruned");
}

impl Cluster {
    /// Waits for the indexer to index up to the given checkpoint, then waits for the graphql
    /// service's background task to update the checkpoint watermark to the given checkpoint.
    pub async fn wait_for_checkpoint_catchup(&self, checkpoint: u64, base_timeout: Duration) {
        wait_for_graphql_checkpoint_catchup(&self.graphql_client, checkpoint, base_timeout).await
    }

    /// Waits for the indexer to index up to the given epoch, then waits for the graphql service's
    /// background task to update the corresponding watermark.
    pub async fn wait_for_epoch_catchup(&self, epoch: u64, base_timeout: Duration) {
        wait_for_graphql_epoch_catchup(&self.graphql_client, epoch, base_timeout).await
    }

    /// Waits for the indexer to prune a given checkpoint.
    pub async fn wait_for_checkpoint_pruned(&self, checkpoint: u64, base_timeout: Duration) {
        wait_for_graphql_checkpoint_pruned(&self.graphql_client, checkpoint, base_timeout).await
    }
}

impl ExecutorCluster {
    /// Waits for the indexer to index up to the given checkpoint, then waits for the graphql
    /// service's background task to update the checkpoint watermark to the given checkpoint.
    pub async fn wait_for_checkpoint_catchup(&self, checkpoint: u64, base_timeout: Duration) {
        wait_for_graphql_checkpoint_catchup(&self.graphql_client, checkpoint, base_timeout).await
    }

    /// Waits for the indexer to prune a given checkpoint.
    pub async fn wait_for_checkpoint_pruned(&self, checkpoint: u64, base_timeout: Duration) {
        wait_for_graphql_checkpoint_pruned(&self.graphql_client, checkpoint, base_timeout).await
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
            while latest_cp > latest_snapshot_cp + self.snapshot_config.snapshot_min_lag as u64 {
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
    }
}
