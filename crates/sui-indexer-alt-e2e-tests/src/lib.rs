// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

use anyhow::{bail, Context};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl};
use diesel_async::RunQueryDsl;
use simulacrum::Simulacrum;
use sui_indexer_alt::{config::IndexerConfig, setup_indexer};
use sui_indexer_alt_framework::{ingestion::ClientArgs, schema::watermarks, IndexerArgs};
use sui_indexer_alt_jsonrpc::{
    config::RpcConfig, data::system_package_task::SystemPackageTaskArgs, start_rpc, RpcArgs,
};
use sui_pg_db::{
    temp::{get_available_port, TempDb},
    Db, DbArgs,
};
use sui_types::{
    base_types::{ObjectRef, SuiAddress},
    effects::{TransactionEffects, TransactionEffectsAPI},
    error::ExecutionError,
    execution_status::ExecutionStatus,
    messages_checkpoint::VerifiedCheckpoint,
    object::Owner,
    transaction::Transaction,
};
use tempfile::TempDir;
use tokio::{task::JoinHandle, time::error::Elapsed};
use tokio_util::sync::CancellationToken;
use url::Url;

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

/// A collection of the off-chain services (an indexer, a database and a JSON-RPC server that reads
/// from that database), grouped together to simplify set-up and tear-down for tests.
///
/// The database is temporary, and will be cleaned up when the cluster is dropped, and the RPC is
/// set-up to listen on a random, available port, to avoid conflicts when multiple instances are
/// running concurrently in the same process.
pub struct OffchainCluster {
    /// The address the JSON-RPC server is listening on.
    rpc_listen_address: SocketAddr,

    /// Read access to the temporary database.
    db: Db,

    /// The pipelines that the indexer is populating.
    pipelines: Vec<&'static str>,

    /// A handle to the indexer task -- it will stop when the `cancel` token is triggered (or
    /// earlier of its own accord).
    indexer: JoinHandle<()>,

    /// A handle to the JSON-RPC server task -- it will stop when the `cancel` token is triggered
    /// (or earlier of its own accord).
    jsonrpc: JoinHandle<()>,

    /// Hold on to the database so it doesn't get dropped until the cluster is stopped.
    #[allow(unused)]
    database: TempDb,

    /// This token controls the clean up of the cluster.
    cancel: CancellationToken,
}

impl FullCluster {
    /// Creates a cluster with a fresh executor where the off-chain services are set up with a
    /// default configuration.
    pub async fn new() -> anyhow::Result<Self> {
        Self::new_with_configs(
            Simulacrum::new(),
            IndexerArgs::default(),
            SystemPackageTaskArgs::default(),
            IndexerConfig::example(),
            RpcConfig::default(),
            &prometheus::Registry::new(),
            CancellationToken::new(),
        )
        .await
    }

    /// Creates a new cluster executing transactions using `executor`. The indexer is configured
    /// using `indexer_args` and `indexer_config, and the JSON-RPC server is configured using
    /// `system_package_task_args` and `rpc_config`.
    pub async fn new_with_configs(
        mut executor: Simulacrum,
        indexer_args: IndexerArgs,
        system_package_task_args: SystemPackageTaskArgs,
        indexer_config: IndexerConfig,
        rpc_config: RpcConfig,
        registry: &prometheus::Registry,
        cancel: CancellationToken,
    ) -> anyhow::Result<Self> {
        let temp_dir = tempfile::tempdir().context("Failed to create data ingestion path")?;
        executor.set_data_ingestion_path(temp_dir.path().to_owned());

        let client_args = ClientArgs {
            local_ingestion_path: Some(temp_dir.path().to_owned()),
            remote_store_url: None,
        };

        let offchain = OffchainCluster::new(
            indexer_args,
            client_args,
            system_package_task_args,
            indexer_config,
            rpc_config,
            registry,
            cancel,
        )
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
        self.offchain
            .wait_for_checkpoint(checkpoint.sequence_number, Duration::from_secs(10))
            .await
            .expect("Timed out waiting for a checkpoint");

        checkpoint
    }

    /// The URL to talk to the database on.
    pub fn db_url(&self) -> Url {
        self.offchain.db_url()
    }

    /// The URL to send JSON-RPC requests to.
    pub fn rpc_url(&self) -> Url {
        self.offchain.rpc_url()
    }

    /// Returns the latest checkpoint that we have all data for in the database, according to the
    /// watermarks table. Returns `None` if any of the expected pipelines are missing data.
    pub async fn latest_checkpoint(&self) -> anyhow::Result<Option<u64>> {
        self.offchain.latest_checkpoint().await
    }

    /// Waits until the indexer has caught up to the given `checkpoint`, or the `timeout` is
    /// reached (an error).
    pub async fn wait_for_checkpoint(
        &self,
        checkpoint: u64,
        timeout: Duration,
    ) -> Result<(), Elapsed> {
        self.offchain.wait_for_checkpoint(checkpoint, timeout).await
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
    /// - `system_package_task_args`, and `rpc_config` control the JSON-RPC server.
    /// - `registry` is used to register metrics for the indexer and JSON-RPC server.
    pub async fn new(
        indexer_args: IndexerArgs,
        client_args: ClientArgs,
        system_package_task_args: SystemPackageTaskArgs,
        indexer_config: IndexerConfig,
        rpc_config: RpcConfig,
        registry: &prometheus::Registry,
        cancel: CancellationToken,
    ) -> anyhow::Result<Self> {
        let rpc_port = get_available_port();
        let rpc_listen_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), rpc_port);

        let database = TempDb::new().context("Failed to create database")?;

        let db_args = DbArgs {
            database_url: database.database().url().clone(),
            ..Default::default()
        };

        let rpc_args = RpcArgs {
            rpc_listen_address,
            ..Default::default()
        };

        let db = Db::for_read(db_args.clone())
            .await
            .context("Failed to connect to database")?;

        let with_genesis = true;
        let indexer = setup_indexer(
            db_args.clone(),
            indexer_args,
            client_args,
            indexer_config,
            with_genesis,
            registry,
            cancel.child_token(),
        )
        .await
        .context("Failed to setup indexer")?;

        let pipelines = indexer.pipelines().collect();
        let indexer = indexer.run().await.context("Failed to start indexer")?;

        let jsonrpc = start_rpc(
            db_args,
            rpc_args,
            system_package_task_args,
            rpc_config,
            registry,
            cancel.child_token(),
        )
        .await
        .context("Failed to start JSON-RPC server")?;

        Ok(Self {
            rpc_listen_address,
            db,
            pipelines,
            indexer,
            jsonrpc,
            database,
            cancel,
        })
    }

    /// The URL to talk to the database on.
    pub fn db_url(&self) -> Url {
        self.database.database().url().clone()
    }

    /// The URL to send JSON-RPC requests to.
    pub fn rpc_url(&self) -> Url {
        Url::parse(&format!("http://{}/", self.rpc_listen_address))
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

    /// Waits until the indexer has caught up to the given `checkpoint`, or the `timeout` is
    /// reached (an error).
    pub async fn wait_for_checkpoint(
        &self,
        checkpoint: u64,
        timeout: Duration,
    ) -> Result<(), Elapsed> {
        tokio::time::timeout(timeout, async move {
            loop {
                if matches!(self.latest_checkpoint().await, Ok(Some(l)) if l >= checkpoint) {
                    break;
                } else {
                    tokio::time::sleep(Duration::from_millis(200)).await;
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
            loop {
                if matches!(self.latest_pruner_checkpoint(pipeline).await, Ok(Some(l)) if l >= checkpoint) {
                    break;
                } else {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
        }).await
    }

    /// Triggers cancellation of all downstream services, waits for them to stop, and cleans up the
    /// temporary database.
    pub async fn stopped(self) {
        self.cancel.cancel();
        let _ = self.indexer.await;
        let _ = self.jsonrpc.await;
    }
}

/// Returns the reference for the first address-owned object created in the effects, or an error if
/// there is none.
pub fn find_address_owned(fx: &TransactionEffects) -> anyhow::Result<ObjectRef> {
    if let ExecutionStatus::Failure { error, command } = fx.status() {
        bail!("Transaction failed: {error} (command {command:?})");
    }

    fx.created()
        .into_iter()
        .find_map(|(oref, owner)| matches!(owner, Owner::AddressOwner(_)).then_some(oref))
        .context("Could not find created object")
}

/// Returns the reference for the first immutable object created in the effects, or an error if
/// there is none.
pub fn find_immutable(fx: &TransactionEffects) -> anyhow::Result<ObjectRef> {
    if let ExecutionStatus::Failure { error, command } = fx.status() {
        bail!("Transaction failed: {error} (command {command:?})");
    }

    fx.created()
        .into_iter()
        .find_map(|(oref, owner)| matches!(owner, Owner::Immutable).then_some(oref))
        .context("Could not find created object")
}

/// Returns the reference for the first shared object created in the effects, or an error if there
/// is none.
pub fn find_shared(fx: &TransactionEffects) -> anyhow::Result<ObjectRef> {
    if let ExecutionStatus::Failure { error, command } = fx.status() {
        bail!("Transaction failed: {error} (command {command:?})");
    }

    fx.created()
        .into_iter()
        .find_map(|(oref, owner)| matches!(owner, Owner::Shared { .. }).then_some(oref))
        .context("Could not find created object")
}
