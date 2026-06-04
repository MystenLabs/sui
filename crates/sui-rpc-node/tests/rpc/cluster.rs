// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! In-process integration-test harness for `sui-rpc-node`.
//!
//! [`LocalCluster`] glues together:
//!
//! - A [`Simulacrum`] that executes transactions and produces real
//!   checkpoints, behind a shared `Arc<Mutex<_>>` so the ingestion
//!   client (read-only) and the test code (`execute_transaction` /
//!   `create_checkpoint`, which mutate) can both hold it.
//! - [`SimulacrumIngestion`], an [`IngestionClientTrait`] impl that
//!   serves checkpoints to the indexer directly from Simulacrum's
//!   in-memory store. Avoids the file-on-disk
//!   `local_ingestion_path` dance the framework's other test
//!   harnesses use, and also exercises the
//!   `IngestionClient::from_trait` shape we expose for the
//!   eventual embedded-fullnode integration.
//! - A [`sui_rpc_store::Indexer`] over a temp-dir RocksDB,
//!   running every pipeline (`PipelineLayer::all`), with the
//!   tightened [`ServiceConfig::for_test`] timings.
//! - The `sui-rpc-api` HTTP server bound to an ephemeral
//!   `127.0.0.1` port, so multiple clusters can run in parallel
//!   without colliding.
//!
//! Test code drives the cluster via [`LocalCluster::execute_transaction`]
//! /  [`LocalCluster::create_checkpoint`] /
//! [`LocalCluster::funded_account`] (passthrough helpers mirroring
//! `simulacrum::Simulacrum`'s own API), and asserts on the rpc-api
//! responses via [`LocalCluster::grpc_url`] /
//! [`LocalCluster::sui_rpc_client`].

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context as _;
use async_trait::async_trait;
use prometheus::Registry;
use simulacrum::Simulacrum;
use sui_consistent_store::Db;
use sui_consistent_store::FrameworkSchema;
use sui_consistent_store::PipelineTaskKey;
use sui_consistent_store::Store;
use sui_consistent_store::metrics::ColumnFamilyStatsCollector;
use sui_futures::service::Service;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ingestion_client::CheckpointError;
use sui_indexer_alt_framework::ingestion::ingestion_client::CheckpointResult;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClient;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientTrait;
use sui_indexer_alt_framework::metrics::IngestionMetrics;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_move_build::BuildConfig;
use sui_rpc_node::METRICS_PREFIX;
use sui_rpc_node::config::ServiceConfig;
use sui_rpc_node::rpc::build_rpc_service;
use sui_rpc_store::Indexer;
use sui_rpc_store::PipelineLayer;
use sui_rpc_store::RpcStoreSchema;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::digests::ChainIdentifier;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::ExecutionError;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::storage::ReadStore;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;
use sui_types::utils::to_sender_signed_transaction;
use tempfile::TempDir;
use tokio::sync::Mutex;
use url::Url;

/// Default polling interval and timeout for `wait_for_*`. The
/// cluster uses `ServiceConfig::for_test`'s tight committer
/// timings, so the indexer typically catches up in 10s of ms;
/// these bounds are generous so a CI hiccup doesn't flake the
/// test.
const POLL_INTERVAL: Duration = Duration::from_millis(20);
const WAIT_TIMEOUT: Duration = Duration::from_secs(10);

/// Adapter exposing [`Simulacrum`] as an
/// [`IngestionClientTrait`]. Lives inside the test crate (rather
/// than as a fixture in `sui-rpc-node`'s library) because it
/// only makes sense in test contexts and keeps the prod crate
/// free of `Simulacrum` / test dependencies.
struct SimulacrumIngestion {
    simulacrum: Arc<Mutex<Simulacrum>>,
}

impl SimulacrumIngestion {
    fn new(simulacrum: Arc<Mutex<Simulacrum>>) -> Self {
        Self { simulacrum }
    }
}

#[async_trait]
impl IngestionClientTrait for SimulacrumIngestion {
    async fn chain_id(&self) -> anyhow::Result<ChainIdentifier> {
        let sim = self.simulacrum.lock().await;
        let genesis = sim
            .get_checkpoint_by_sequence_number(0)
            .context("Simulacrum has no genesis checkpoint")?;
        Ok((*genesis.digest()).into())
    }

    async fn checkpoint(&self, checkpoint: u64) -> CheckpointResult {
        let sim = self.simulacrum.lock().await;
        let Some(verified) = sim.get_checkpoint_by_sequence_number(checkpoint) else {
            return Err(CheckpointError::NotFound);
        };
        // Simulacrum's `get_checkpoint_contents_by_sequence_number`
        // is unimplemented (it panics), so route through the
        // content digest on the verified header instead.
        let content_digest = verified.content_digest;
        let Some(contents) = sim.get_checkpoint_contents_by_digest(&content_digest) else {
            return Err(CheckpointError::Fetch(anyhow::anyhow!(
                "checkpoint {checkpoint} present but contents missing in Simulacrum store",
            )));
        };
        sim.get_checkpoint_data(verified, contents)
            .map_err(|e| CheckpointError::Fetch(anyhow::anyhow!("{e:#}")))
    }

    async fn latest_checkpoint_number(&self) -> anyhow::Result<u64> {
        let sim = self.simulacrum.lock().await;
        sim.get_latest_checkpoint_sequence_number()
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }
}

/// In-process harness pairing a [`Simulacrum`] executor with a
/// running [`sui_rpc_node`] service.
pub struct LocalCluster {
    simulacrum: Arc<Mutex<Simulacrum>>,

    /// Handle into the rpc-store's RocksDB. Cloned out of the
    /// indexer's [`Store`] at construction time so the test
    /// helpers can read framework CFs (pipeline watermarks)
    /// directly without going through the rpc-api.
    db: Db,

    /// Cached list of pipelines registered with the indexer.
    /// Used by [`Self::latest_indexed_checkpoint`] to determine
    /// "indexer has caught up" — the minimum committed checkpoint
    /// across every pipeline.
    pipelines: Vec<&'static str>,

    grpc_listen_address: SocketAddr,

    /// Prometheus registry the indexer, RPC server, and RocksDB
    /// column-family stats collector all register into. Exposed so
    /// tests can scrape it.
    registry: Registry,

    /// Composite [`Service`] for the indexer + RPC server. Held
    /// to keep the spawned tasks alive; dropped on cluster
    /// teardown which signals graceful shutdown.
    #[allow(dead_code)]
    services: Service,

    /// Temp-dir backing the RocksDB. Held so it isn't cleaned up
    /// before the indexer finishes.
    #[allow(dead_code)]
    db_dir: TempDir,
}

impl LocalCluster {
    /// Spin up a fresh cluster: a Simulacrum at genesis, a
    /// brand-new RocksDB in a temp-dir, every rpc-store pipeline
    /// running, and the rpc-api HTTP server bound to an ephemeral
    /// `127.0.0.1` port.
    ///
    /// The genesis checkpoint Simulacrum produces is what feeds
    /// the indexer's `chain_id` resolution and the first
    /// `latest_checkpoint_number` reply, so the indexer can start
    /// ingesting from checkpoint 0 the moment the cluster
    /// returns.
    pub async fn new() -> anyhow::Result<Self> {
        let simulacrum = Arc::new(Mutex::new(Simulacrum::new()));
        let registry = Registry::new();

        let db_dir = tempfile::tempdir().context("Failed to create temp database directory")?;
        let db_path: PathBuf = db_dir.path().join("rpc-store");

        let grpc_port = pick_available_port()?;
        let grpc_listen_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), grpc_port);
        let config = ServiceConfig::for_test(grpc_listen_address);
        let ServiceConfig {
            ingestion,
            consistency,
            committer,
            rpc,
            db: db_config,
            restore: _,
        } = config;

        // Open the database explicitly so the cluster can hold a
        // Db handle for watermark reads, using the same resolved
        // RocksDB options the production path would. `Db` is
        // Arc-backed, so this clone is cheap; the indexer's `Store`
        // shares the same underlying database.
        let (db, schema) = Db::open::<RpcStoreSchema>(&db_path, db_config.to_db_options())
            .context("Failed to open rpc-store database")?;
        let schema = Arc::new(schema);
        let store = Store::new(db.clone(), schema.clone());

        // Mirror the production paths (`start_service` / `start_restorer`)
        // and expose per-CF RocksDB stats on the shared registry.
        registry
            .register(Box::new(ColumnFamilyStatsCollector::new(
                Some(METRICS_PREFIX),
                &db,
            )))
            .context("Failed to register RocksDB column-family stats collector")?;

        let consistency_for_rpc = consistency.clone();

        // Stand up the ingestion client over Simulacrum. The
        // framework's `IngestionService` shares its
        // `IngestionMetrics` with the client we hand it via
        // `from_trait`; if we built a fresh `IngestionMetrics`
        // from `registry` and passed it down, we'd double-register
        // it inside the `Indexer`. The pattern matches
        // `sui_rpc_node::start_service`.
        let ingestion_metrics = IngestionMetrics::new(Some(METRICS_PREFIX), &registry);
        let ingestion_client = IngestionClient::from_trait(
            Arc::new(SimulacrumIngestion::new(simulacrum.clone())),
            ingestion_metrics,
        );

        let mut indexer = Indexer::from_store(
            store,
            IndexerArgs::default(),
            ingestion_client,
            None, // no streaming client for the in-memory harness.
            consistency,
            ingestion,
            &registry,
        )
        .await
        .context("Failed to construct rpc-store indexer")?;

        indexer
            .add_pipelines(
                PipelineLayer::all(),
                committer.finish(CommitterConfig::default()),
            )
            .await
            .context("Failed to register rpc-store pipelines")?;

        let pipelines: Vec<&'static str> = indexer.pipelines().collect();

        // Start the indexer first so it can ingest the genesis
        // checkpoint Simulacrum auto-publishes. Without this, the
        // `RpcService::new` constructor (which eagerly calls
        // `get_chain_identifier().unwrap()`) would panic — the
        // framework `__chain_id` rows are written by the
        // pipelines on first commit, so they only exist after
        // checkpoint 0 has been processed.
        let s_indexer = indexer
            .run()
            .await
            .context("Failed to start indexer service")?;

        wait_for_checkpoint(&db, &pipelines, 0, WAIT_TIMEOUT)
            .await
            .context("indexer never committed the genesis checkpoint")?;

        let rpc_service = build_rpc_service(
            db.clone(),
            schema.clone(),
            consistency_for_rpc,
            rpc,
            "sui-rpc-node-tests",
            "0.0",
            &registry,
        )
        .await
        .context("Failed to start in-process RPC server")?;

        let services = s_indexer.merge(rpc_service);

        Ok(Self {
            simulacrum,
            db,
            pipelines,
            grpc_listen_address,
            registry,
            services,
            db_dir,
        })
    }

    /// Gather the current Prometheus metric families from the
    /// cluster's registry.
    pub fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.registry.gather()
    }

    /// The gRPC URL clients should connect to.
    pub fn grpc_url(&self) -> Url {
        Url::parse(&format!("http://{}/", self.grpc_listen_address))
            .expect("ephemeral SocketAddr round-trips through Url")
    }

    /// Reference gas price for the current epoch, as Simulacrum
    /// reports it. Useful for building gas-data on transactions
    /// the test wants to submit.
    pub async fn reference_gas_price(&self) -> u64 {
        self.simulacrum.lock().await.reference_gas_price()
    }

    /// Allocate a new funded account from Simulacrum's faucet.
    /// Returns the address, its keypair, and a gas object owned
    /// by it. Mirrors [`Simulacrum::funded_account`].
    pub async fn funded_account(
        &self,
        amount: u64,
    ) -> anyhow::Result<(SuiAddress, AccountKeyPair, ObjectRef)> {
        self.simulacrum.lock().await.funded_account(amount)
    }

    /// Request `amount` SUI from Simulacrum's faucet, sent to
    /// `address`. Returns the effects of the gas-grant
    /// transaction (which created the new gas coin owned by
    /// `address`). Mirrors [`Simulacrum::request_gas`].
    pub async fn request_gas(
        &self,
        address: SuiAddress,
        amount: u64,
    ) -> anyhow::Result<TransactionEffects> {
        self.simulacrum.lock().await.request_gas(address, amount)
    }

    /// Submit a fully signed transaction to Simulacrum. Returns
    /// the effects + any execution error. The transaction is *not*
    /// committed to a checkpoint until [`Self::create_checkpoint`]
    /// is called.
    pub async fn execute_transaction(
        &self,
        tx: Transaction,
    ) -> anyhow::Result<(TransactionEffects, Option<ExecutionError>)> {
        self.simulacrum.lock().await.execute_transaction(tx)
    }

    /// Look up an object directly in Simulacrum's in-memory
    /// store. Useful when a test needs to filter newly-created
    /// objects from a transaction's effects by Move type — the
    /// effects only carry IDs, not types.
    pub async fn get_object(&self, id: ObjectID) -> Option<sui_types::object::Object> {
        self.simulacrum.lock().await.store().get_object(&id).cloned()
    }

    /// Compile the Move package at `path` with
    /// [`BuildConfig::new_for_testing`], submit a publish
    /// transaction signed by `sender` using `gas` for gas payment,
    /// and return the resulting [`ObjectID`] of the published
    /// package plus the [`TransactionEffects`] (so callers can
    /// pluck out treasury caps, metadata objects, etc.). The
    /// transaction is queued; the caller still needs to invoke
    /// [`Self::create_checkpoint`] before the indexer surfaces
    /// the new state.
    pub async fn publish_package(
        &self,
        sender: SuiAddress,
        keypair: &AccountKeyPair,
        gas: ObjectRef,
        path: impl AsRef<Path>,
    ) -> anyhow::Result<(ObjectID, TransactionEffects)> {
        let compiled_package = BuildConfig::new_for_testing()
            .build_async(path.as_ref())
            .await
            .context("compiling Move package")?;
        let modules = compiled_package.get_package_bytes(/* with_unpublished_deps */ false);
        let dependencies = compiled_package.get_dependency_storage_package_ids();

        let rgp = self.reference_gas_price().await;
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.publish_immutable(modules, dependencies);
        let pt = builder.finish();
        // Match the e2e helper's 100M MIST budget — large enough
        // for any of the test Move packages without depending on
        // protocol-specific publish cost formulas.
        let tx_data = TransactionData::new_programmable(sender, vec![gas], pt, 100_000_000, rgp);
        let signed = to_sender_signed_transaction(tx_data, keypair);

        let (effects, err) = self.execute_transaction(signed).await?;
        if let Some(err) = err {
            anyhow::bail!("publish transaction failed: {err}");
        }

        // Move packages and frozen `CoinMetadata` both show up as
        // `Immutable` in `created()`, so we have to disambiguate
        // by looking up each object and checking `is_package()`.
        let mut package_id = None;
        for (oref, owner) in effects.created() {
            if !matches!(owner, sui_types::object::Owner::Immutable) {
                continue;
            }
            if self
                .get_object(oref.0)
                .await
                .map(|obj| obj.is_package())
                .unwrap_or(false)
            {
                package_id = Some(oref.0);
                break;
            }
        }
        let package_id = package_id.context("publish effects missing a Move package object")?;

        Ok((package_id, effects))
    }

    /// Roll Simulacrum forward by producing a checkpoint over
    /// every executed transaction since the last call. Blocks
    /// until the indexer has committed the new checkpoint across
    /// every pipeline.
    pub async fn create_checkpoint(&self) -> anyhow::Result<VerifiedCheckpoint> {
        let checkpoint = {
            let mut sim = self.simulacrum.lock().await;
            sim.create_checkpoint()
        };

        self.wait_for_indexer(checkpoint.sequence_number, WAIT_TIMEOUT)
            .await
            .with_context(|| {
                format!(
                    "indexer failed to catch up to checkpoint {} within {WAIT_TIMEOUT:?}",
                    checkpoint.sequence_number,
                )
            })?;

        Ok(checkpoint)
    }

    /// The lowest `__watermark.checkpoint_hi_inclusive` across
    /// every registered pipeline. `None` if any pipeline hasn't
    /// committed yet. Mirrors `OffchainCluster::latest_checkpoint`.
    /// Exposed for future tests that want to assert on indexer
    /// progress directly.
    #[allow(dead_code)]
    pub fn latest_indexed_checkpoint(&self) -> anyhow::Result<Option<u64>> {
        latest_indexed_checkpoint(&self.db, &self.pipelines)
    }

    /// Block until [`Self::latest_indexed_checkpoint`] reaches
    /// `target`, or `timeout` elapses.
    pub async fn wait_for_indexer(&self, target: u64, timeout: Duration) -> anyhow::Result<()> {
        wait_for_checkpoint(&self.db, &self.pipelines, target, timeout).await
    }
}

/// Read the `__watermark` CF for every pipeline in `pipelines`
/// against the live [`Db`] and return the minimum
/// `checkpoint_hi_inclusive` (i.e. the indexer's "latest
/// committed everywhere" point). `None` if any pipeline is
/// missing a watermark row — that's the framework's way of
/// saying "this pipeline hasn't committed yet".
fn latest_indexed_checkpoint(db: &Db, pipelines: &[&'static str]) -> anyhow::Result<Option<u64>> {
    let framework = FrameworkSchema::new(db.clone());
    let mut min: Option<u64> = None;
    for name in pipelines {
        let key = PipelineTaskKey::new(*name);
        match framework
            .watermarks
            .get(&key)
            .with_context(|| format!("read __watermark for {name:?}"))?
        {
            Some(wm) => {
                min = Some(min.map_or(wm.checkpoint_hi_inclusive, |m| {
                    m.min(wm.checkpoint_hi_inclusive)
                }));
            }
            None => return Ok(None),
        }
    }
    Ok(min)
}

/// Poll [`latest_indexed_checkpoint`] every [`POLL_INTERVAL`]
/// until it reaches `target` or `timeout` elapses.
async fn wait_for_checkpoint(
    db: &Db,
    pipelines: &[&'static str],
    target: u64,
    timeout: Duration,
) -> anyhow::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(latest) = latest_indexed_checkpoint(db, pipelines)?
            && latest >= target
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for indexer to reach checkpoint {target}; latest={:?}",
                latest_indexed_checkpoint(db, pipelines)?,
            );
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Bind a TCP listener on `127.0.0.1:0`, capture the OS-assigned
/// port, then drop the listener so a subsequent
/// `axum::serve(TcpListener::bind(...))` can rebind. The window
/// between the two binds is short enough not to matter for
/// in-process tests; the same pattern is used in `sui-fork`'s
/// `ServerHarness`.
fn pick_available_port() -> anyhow::Result<u16> {
    let probe = TcpListener::bind(("127.0.0.1", 0)).context("Failed to probe-bind a free port")?;
    let port = probe
        .local_addr()
        .context("Probe listener missing local_addr")?
        .port();
    drop(probe);
    Ok(port)
}
