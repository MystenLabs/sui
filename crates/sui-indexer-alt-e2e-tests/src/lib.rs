// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
    time::Duration,
};

use anyhow::{Context, ensure};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl};
use diesel_async::RunQueryDsl;
use reqwest::Client;
use serde_json::{Value, json};
use simulacrum::Simulacrum;
use sui_indexer_alt::{BootstrapGenesis, config::IndexerConfig, setup_indexer};
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::{
    AvailableRangeRequest, consistent_service_client::ConsistentServiceClient,
};
use sui_indexer_alt_consistent_store::{
    args::RpcArgs as ConsistentArgs, args::TlsArgs as ConsistentTlsArgs,
    config::ServiceConfig as ConsistentConfig, start_service as start_consistent_store,
};
use sui_indexer_alt_framework::{IndexerArgs, ingestion::ClientArgs, postgres::schema::watermarks};
use sui_indexer_alt_graphql::{
    RpcArgs as GraphQlArgs, config::RpcConfig as GraphQlConfig, start_rpc as start_graphql,
};
use sui_indexer_alt_jsonrpc::{
    NodeArgs as JsonRpcNodeArgs, RpcArgs as JsonRpcArgs, config::RpcConfig as JsonRpcConfig,
    start_rpc as start_jsonrpc,
};
use sui_indexer_alt_reader::{
    bigtable_reader::BigtableArgs, consistent_reader::ConsistentReaderArgs,
    fullnode_client::FullnodeArgs, system_package_task::SystemPackageTaskArgs,
};
use sui_pg_db::{
    Db, DbArgs,
    temp::{TempDb, get_available_port},
};
use sui_storage::blob::{Blob, BlobEncoding};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::{
    base_types::{ObjectRef, SuiAddress},
    crypto::AccountKeyPair,
    effects::TransactionEffects,
    error::ExecutionError,
    messages_checkpoint::VerifiedCheckpoint,
    transaction::Transaction,
};
use tempfile::TempDir;
use tokio::{
    task::JoinHandle,
    time::{error::Elapsed, interval},
    try_join,
};
use tokio_util::sync::CancellationToken;
use url::Url;

pub mod coin_registry;
pub mod find;

/// A simulation of the network, accompanied by off-chain services (database, indexer, RPC),
/// connected by local data ingestion.
pub struct FullCluster {
    /// A simulation of the network, executing transactions and producing checkpoints.
    executor: Simulacrum,

    /// The off-chain services (database, indexer, RPC) that are ingesting data from the
    /// simulation.
    offchain: OffchainCluster,

    /// Temporary directory to store checkpoint information in, so that the indexer can pick it up.
    #[allow(unused)]
    temp_dir: TempDir,
}

/// A collection of the off-chain services (an indexer, a database, and JSON-RPC/GraphQL servers
/// that read from that database), grouped together to simplify set-up and tear-down for tests. The
/// included RPC servers do not support transaction dry run and execution.
///
/// The database is temporary, and will be cleaned up when the cluster is dropped, and the RPCs are
/// set-up to listen on a random, available port, to avoid conflicts when multiple instances are
/// running concurrently in the same process.
pub struct OffchainCluster {
    /// The address the consistent store is listening on.
    consistent_listen_address: SocketAddr,

    /// The address the JSON-RPC server is listening on.
    jsonrpc_listen_address: SocketAddr,

    /// The address the GraphQL server is listening on.
    graphql_listen_address: SocketAddr,

    /// Read access to the temporary database.
    db: Db,

    /// The pipelines that the indexer is populating.
    pipelines: Vec<&'static str>,

    /// A handle to the indexer task -- it will stop when the `cancel` token is triggered (or
    /// earlier of its own accord).
    indexer: JoinHandle<()>,

    /// A handle to the consistent store task -- it will stop when the `cancel` token is triggered
    /// (or earlier of its own accord).
    consistent_store: JoinHandle<()>,

    /// A handle to the JSON-RPC server task -- it will stop when the `cancel` token is triggered
    /// (or earlier of its own accord).
    jsonrpc: JoinHandle<()>,

    /// A handle to the GraphQL server task -- it will stop when the `cancel` token is triggered
    /// (or earlier of its own accord).
    graphql: JoinHandle<()>,

    /// Hold on to the database so it doesn't get dropped until the cluster is stopped.
    #[allow(unused)]
    database: TempDb,

    /// Hold on to the temporary directory where the consistent store writes its data, so it
    /// doesn't get cleaned up until the cluster is stopped.
    #[allow(unused)]
    dir: TempDir,

    /// This token controls the clean up of the cluster.
    cancel: CancellationToken,
}

pub struct OffchainClusterConfig {
    pub indexer_args: IndexerArgs,
    pub consistent_indexer_args: IndexerArgs,
    pub fullnode_args: FullnodeArgs,
    pub indexer_config: IndexerConfig,
    pub consistent_config: ConsistentConfig,
    pub jsonrpc_config: JsonRpcConfig,
    pub graphql_config: GraphQlConfig,
    pub bootstrap_genesis: Option<BootstrapGenesis>,
}

impl FullCluster {
    /// Creates a cluster with a fresh executor where the off-chain services are set up with a
    /// default configuration.
    pub async fn new() -> anyhow::Result<Self> {
        Self::new_with_configs(
            Simulacrum::new(),
            OffchainClusterConfig::default(),
            &prometheus::Registry::new(),
            CancellationToken::new(),
        )
        .await
    }

    /// Creates a new cluster executing transactions using `executor`. The indexer is configured
    /// using `indexer_args` and `indexer_config, the JSON-RPC server is configured using
    /// `jsonrpc_config`, and the GraphQL server is configured using `graphql_config`.
    pub async fn new_with_configs(
        mut executor: Simulacrum,
        offchain_cluster_config: OffchainClusterConfig,
        registry: &prometheus::Registry,
        cancel: CancellationToken,
    ) -> anyhow::Result<Self> {
        let (client_args, temp_dir) = local_ingestion_client_args();
        executor.set_data_ingestion_path(temp_dir.path().to_owned());

        let offchain = OffchainCluster::new(client_args, offchain_cluster_config, registry, cancel)
            .await
            .context("Failed to create off-chain cluster")?;

        Ok(Self {
            executor,
            offchain,
            temp_dir,
        })
    }

    /// Return the reference gas price for the current epoch
    pub fn reference_gas_price(&self) -> u64 {
        self.executor.reference_gas_price()
    }

    /// Create a new account and credit it with `amount` gas units from a faucet account. Returns
    /// the account, its keypair, and a reference to the gas object it was funded with.
    pub fn funded_account(
        &mut self,
        amount: u64,
    ) -> anyhow::Result<(SuiAddress, AccountKeyPair, ObjectRef)> {
        self.executor.funded_account(amount)
    }

    /// Request gas from the faucet, sent to `address`. Return the object reference of the gas
    /// object that was sent.
    pub fn request_gas(
        &mut self,
        address: SuiAddress,
        amount: u64,
    ) -> anyhow::Result<TransactionEffects> {
        self.executor.request_gas(address, amount)
    }

    /// Execute a signed transaction, returning its effects.
    pub fn execute_transaction(
        &mut self,
        tx: Transaction,
    ) -> anyhow::Result<(TransactionEffects, Option<ExecutionError>)> {
        self.executor.execute_transaction(tx)
    }

    /// Execute a system transaction advancing the lock by the given `duration`.
    pub fn advance_clock(&mut self, duration: Duration) -> TransactionEffects {
        self.executor.advance_clock(duration)
    }

    /// Create a new checkpoint containing the transactions executed since the last checkpoint that
    /// was created, and wait for the off-chain services to ingest it. Returns the checkpoint
    /// contents.
    pub async fn create_checkpoint(&mut self) -> VerifiedCheckpoint {
        let checkpoint = self.executor.create_checkpoint();
        let indexer = self
            .offchain
            .wait_for_indexer(checkpoint.sequence_number, Duration::from_secs(10));
        let consistent_store = self
            .offchain
            .wait_for_consistent_store(checkpoint.sequence_number, Duration::from_secs(10));
        let graphql = self
            .offchain
            .wait_for_graphql(checkpoint.sequence_number, Duration::from_secs(10));

        try_join!(indexer, consistent_store, graphql)
            .expect("Timed out waiting for indexer and consistent store");

        checkpoint
    }

    /// The URL to talk to the database on.
    pub fn db_url(&self) -> Url {
        self.offchain.db_url()
    }

    /// The URL to send Consistent Store requests to.
    pub fn consistent_store_url(&self) -> Url {
        self.offchain.consistent_store_url()
    }

    /// The URL to send JSON-RPC requests to.
    pub fn jsonrpc_url(&self) -> Url {
        self.offchain.jsonrpc_url()
    }

    /// The URL to send GraphQL requests to.
    pub fn graphql_url(&self) -> Url {
        self.offchain.graphql_url()
    }

    /// Returns the latest checkpoint that we have all data for in the database, according to the
    /// watermarks table. Returns `None` if any of the expected pipelines are missing data.
    pub async fn latest_checkpoint(&self) -> anyhow::Result<Option<u64>> {
        self.offchain.latest_checkpoint().await
    }

    /// Waits until the indexer has caught up to the given `checkpoint`, or the `timeout` is
    /// reached (an error).
    pub async fn wait_for_indexer(
        &self,
        checkpoint: u64,
        timeout: Duration,
    ) -> Result<(), Elapsed> {
        self.offchain.wait_for_indexer(checkpoint, timeout).await
    }

    /// Waits until the indexer's pruner has caught up to the given `checkpoint`, for the given
    /// `pipeline`, or the `timeout` is reached (an error).
    pub async fn wait_for_pruner(
        &self,
        pipeline: &str,
        checkpoint: u64,
        timeout: Duration,
    ) -> Result<(), Elapsed> {
        self.offchain
            .wait_for_pruner(pipeline, checkpoint, timeout)
            .await
    }

    /// Waits until GraphQL has caught up to the given `checkpoint`, or the `timeout` is
    /// reached (an error).
    pub async fn wait_for_graphql(
        &self,
        checkpoint: u64,
        timeout: Duration,
    ) -> Result<(), Elapsed> {
        self.offchain.wait_for_graphql(checkpoint, timeout).await
    }

    /// Triggers cancellation of all downstream services, waits for them to stop, cleans up the
    /// temporary database, and the temporary directory used for ingestion.
    pub async fn stopped(self) {
        self.offchain.stopped().await;
    }
}

impl OffchainCluster {
    /// Construct a new off-chain cluster and spin up its constituent services.
    ///
    /// - `indexer_args`, `client_args`, and `indexer_config` control the indexer. In particular
    ///   `client_args` is used to configure the client that the indexer uses to fetch checkpoints.
    /// - `jsonrpc_config` controls the JSON-RPC server.
    /// - `graphql_config` controls the GraphQL server.
    /// - `registry` is used to register metrics for the indexer, JSON-RPC, and GraphQL servers.
    pub async fn new(
        client_args: ClientArgs,
        OffchainClusterConfig {
            indexer_args,
            consistent_indexer_args,
            fullnode_args,
            indexer_config,
            consistent_config,
            jsonrpc_config,
            graphql_config,
            bootstrap_genesis,
        }: OffchainClusterConfig,
        registry: &prometheus::Registry,
        cancel: CancellationToken,
    ) -> anyhow::Result<Self> {
        let consistent_port = get_available_port();
        let consistent_listen_address =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), consistent_port);

        let jsonrpc_port = get_available_port();
        let jsonrpc_listen_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), jsonrpc_port);

        let graphql_port = get_available_port();
        let graphql_listen_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), graphql_port);

        let database = TempDb::new().context("Failed to create database")?;
        let database_url = database.database().url();

        let dir = tempfile::tempdir().context("Failed to create temporary directory")?;
        let rocksdb_path = dir.path().join("rocksdb");

        let consistent_args = ConsistentArgs {
            rpc_listen_address: consistent_listen_address,
            tls: ConsistentTlsArgs::default(),
        };

        let jsonrpc_args = JsonRpcArgs {
            rpc_listen_address: jsonrpc_listen_address,
            ..Default::default()
        };

        let graphql_args = GraphQlArgs {
            rpc_listen_address: graphql_listen_address,
            no_ide: true,
        };

        let db = Db::for_read(database_url.clone(), DbArgs::default())
            .await
            .context("Failed to connect to database")?;

        let indexer = setup_indexer(
            database_url.clone(),
            DbArgs::default(),
            indexer_args,
            client_args.clone(),
            indexer_config,
            bootstrap_genesis,
            registry,
            cancel.child_token(),
        )
        .await
        .context("Failed to setup indexer")?;

        let pipelines: Vec<_> = indexer.pipelines().collect();
        let indexer = indexer.run().await.context("Failed to start indexer")?;

        let consistent_store = start_consistent_store(
            rocksdb_path,
            consistent_indexer_args,
            client_args,
            consistent_args,
            "0.0.0",
            consistent_config,
            registry,
            cancel.child_token(),
        )
        .await
        .context("Failed to start Consistent Store")?;

        let jsonrpc = start_jsonrpc(
            Some(database_url.clone()),
            None,
            DbArgs::default(),
            BigtableArgs::default(),
            jsonrpc_args,
            JsonRpcNodeArgs::default(),
            SystemPackageTaskArgs::default(),
            jsonrpc_config,
            registry,
            cancel.child_token(),
        )
        .await
        .context("Failed to start JSON-RPC server")?;

        let consistent_reader_args = ConsistentReaderArgs {
            consistent_store_url: Some(
                Url::parse(&format!("http://{consistent_listen_address}")).unwrap(),
            ),
            consistent_store_statement_timeout_ms: None,
        };

        let graphql = start_graphql(
            Some(database_url.clone()),
            None,
            fullnode_args,
            DbArgs::default(),
            BigtableArgs::default(),
            consistent_reader_args,
            graphql_args,
            SystemPackageTaskArgs::default(),
            "0.0.0",
            graphql_config,
            pipelines.iter().map(|p| p.to_string()).collect(),
            registry,
            cancel.child_token(),
        )
        .await
        .context("Failed to start GraphQL server")?;

        Ok(Self {
            consistent_listen_address,
            jsonrpc_listen_address,
            graphql_listen_address,
            db,
            pipelines,
            indexer,
            consistent_store,
            jsonrpc,
            graphql,
            database,
            dir,
            cancel,
        })
    }

    /// The URL to talk to the database on.
    pub fn db_url(&self) -> Url {
        self.database.database().url().clone()
    }

    /// The URL to send Consistent Store requests to.
    pub fn consistent_store_url(&self) -> Url {
        Url::parse(&format!("http://{}/", self.consistent_listen_address))
            .expect("Failed to parse RPC URL")
    }

    /// The URL to send JSON-RPC requests to.
    pub fn jsonrpc_url(&self) -> Url {
        Url::parse(&format!("http://{}/", self.jsonrpc_listen_address))
            .expect("Failed to parse RPC URL")
    }

    /// The URL to send GraphQL requests to.
    pub fn graphql_url(&self) -> Url {
        Url::parse(&format!("http://{}/graphql", self.graphql_listen_address))
            .expect("Failed to parse RPC URL")
    }

    /// Returns the latest checkpoint that we have all data for in the database, according to the
    /// watermarks table. Returns `None` if any of the expected pipelines are missing data.
    pub async fn latest_checkpoint(&self) -> anyhow::Result<Option<u64>> {
        use watermarks::dsl as w;

        let mut conn = self
            .db
            .connect()
            .await
            .context("Failed to connect to database")?;

        let latest: HashMap<String, i64> = w::watermarks
            .select((w::pipeline, w::checkpoint_hi_inclusive))
            .filter(w::pipeline.eq_any(&self.pipelines))
            .load(&mut conn)
            .await?
            .into_iter()
            .collect();

        if latest.len() != self.pipelines.len() {
            return Ok(None);
        }

        Ok(latest.into_values().min().map(|l| l as u64))
    }

    /// Returns the latest checkpoint that the pruner is willing to prune up to for the given
    /// `pipeline`.
    pub async fn latest_pruner_checkpoint(&self, pipeline: &str) -> anyhow::Result<Option<u64>> {
        use watermarks::dsl as w;

        let mut conn = self
            .db
            .connect()
            .await
            .context("Failed to connect to database")?;

        let latest: Option<i64> = w::watermarks
            .select(w::reader_lo)
            .filter(w::pipeline.eq(pipeline))
            .first(&mut conn)
            .await
            .optional()?;

        Ok(latest.map(|l| l as u64))
    }

    /// Returns the latest checkpoint that the consistent store is aware of.
    pub async fn latest_consistent_store_checkpoint(&self) -> anyhow::Result<u64> {
        ConsistentServiceClient::connect(self.consistent_store_url().to_string())
            .await
            .context("Failed to connect to Consistent Store")?
            .available_range(AvailableRangeRequest {})
            .await
            .context("Failed to fetch available range from Consistent Store")?
            .into_inner()
            .max_checkpoint
            .context("Consistent Store has not started yet")
    }

    /// Returns the latest checkpoint that the GraphQL service is aware of.
    pub async fn latest_graphql_checkpoint(&self) -> anyhow::Result<u64> {
        let query = json!({
            "query": "query { checkpoint { sequenceNumber } }"
        });

        let client = Client::new();
        let request = client.post(self.graphql_url()).json(&query);
        let response = request
            .send()
            .await
            .context("Request to GraphQL server failed")?;

        let body: Value = response
            .json()
            .await
            .context("Failed to parse GraphQL response")?;

        let sequence_number = body
            .pointer("/data/checkpoint/sequenceNumber")
            .context("Failed to find checkpoint sequence number in response")?;

        let sequence_number: i64 = serde_json::from_value(sequence_number.clone())
            .context("Failed to parse sequence number as i64")?;

        ensure!(sequence_number != i64::MAX, "Indexer has not started yet");

        Ok(sequence_number as u64)
    }

    /// Returns the latest epoch that the GraphQL service is aware of.
    pub async fn latest_graphql_epoch(&self) -> anyhow::Result<u64> {
        let query = json!({
            "query": "query { epoch { epochId } }"
        });

        let client = Client::new();
        let request = client.post(self.graphql_url()).json(&query);
        let response = request
            .send()
            .await
            .context("Request to GraphQL server failed")?;

        let body: Value = response
            .json()
            .await
            .context("Failed to parse GraphQL response")?;

        let epoch_id = body
            .pointer("/data/epoch/epochId")
            .context("Failed to find epochId in response")?;

        let epoch_id: i64 =
            serde_json::from_value(epoch_id.clone()).context("Failed to parse epochId as i64")?;

        ensure!(epoch_id != i64::MAX, "Indexer has not started yet");

        Ok(epoch_id as u64)
    }

    /// Waits until the indexer has caught up to the given `checkpoint`, or the `timeout` is
    /// reached (an error).
    pub async fn wait_for_indexer(
        &self,
        checkpoint: u64,
        timeout: Duration,
    ) -> Result<(), Elapsed> {
        tokio::time::timeout(timeout, async move {
            let mut interval = interval(Duration::from_millis(200));
            loop {
                interval.tick().await;
                if matches!(self.latest_checkpoint().await, Ok(Some(l)) if l >= checkpoint) {
                    break;
                }
            }
        })
        .await
    }

    /// Waits until the indexer's pruner has caught up to the given `checkpoint`, for the given
    /// `pipeline`, or the `timeout` is reached (an error).
    pub async fn wait_for_pruner(
        &self,
        pipeline: &str,
        checkpoint: u64,
        timeout: Duration,
    ) -> Result<(), Elapsed> {
        tokio::time::timeout(timeout, async move {
            let mut interval = interval(Duration::from_millis(200));
            loop {
                interval.tick().await;
                if matches!(self.latest_pruner_checkpoint(pipeline).await, Ok(Some(l)) if l >= checkpoint) {
                    break;
                }
            }
        }).await
    }

    /// Waits until the Consistent Store has caught up to the given `checkpoint`, or the `timeout`
    /// is reached (an error).
    pub async fn wait_for_consistent_store(
        &self,
        checkpoint: u64,
        timeout: Duration,
    ) -> Result<(), Elapsed> {
        tokio::time::timeout(timeout, async move {
            let mut interval = interval(Duration::from_millis(200));
            loop {
                interval.tick().await;
                if matches!(self.latest_consistent_store_checkpoint().await, Ok(l) if l >= checkpoint) {
                    break;
                }
            }
        })
        .await
    }

    /// Waits until GraphQL has caught up to the given `checkpoint`, or the `timeout` is reached
    /// (an error).
    pub async fn wait_for_graphql(
        &self,
        checkpoint: u64,
        timeout: Duration,
    ) -> Result<(), Elapsed> {
        tokio::time::timeout(timeout, async move {
            let mut interval = interval(Duration::from_millis(200));
            loop {
                interval.tick().await;
                if matches!(self.latest_graphql_checkpoint().await, Ok(l) if l >= checkpoint) {
                    break;
                }
            }
        })
        .await
    }

    /// Triggers cancellation of all downstream services, waits for them to stop, and cleans up the
    /// temporary database.
    pub async fn stopped(self) {
        self.cancel.cancel();
        let _ = self.indexer.await;
        let _ = self.consistent_store.await;
        let _ = self.jsonrpc.await;
        let _ = self.graphql.await;
    }
}

impl Default for OffchainClusterConfig {
    fn default() -> Self {
        Self {
            indexer_args: Default::default(),
            consistent_indexer_args: Default::default(),
            fullnode_args: Default::default(),
            indexer_config: IndexerConfig::for_test(),
            consistent_config: ConsistentConfig::for_test(),
            jsonrpc_config: Default::default(),
            graphql_config: Default::default(),
            bootstrap_genesis: None,
        }
    }
}

/// Returns ClientArgs that use a temporary local ingestion path and the TempDir of that path.
pub fn local_ingestion_client_args() -> (ClientArgs, TempDir) {
    let temp_dir = tempfile::tempdir()
        .context("Failed to create data ingestion path")
        .unwrap();
    let client_args = ClientArgs {
        local_ingestion_path: Some(temp_dir.path().to_owned()),
        remote_store_url: None,
        rpc_api_url: None,
        rpc_username: None,
        rpc_password: None,
    };
    (client_args, temp_dir)
}

/// Writes a checkpoint file to the given path.
pub async fn write_checkpoint(path: &Path, checkpoint_data: CheckpointData) -> anyhow::Result<()> {
    let file_name = format!("{}.chk", checkpoint_data.checkpoint_summary.sequence_number);
    let file_path = path.join(file_name);
    let blob = Blob::encode(&checkpoint_data, BlobEncoding::Bcs)?;
    fs::write(file_path, blob.to_bytes())?;
    Ok(())
}
