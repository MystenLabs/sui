// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! # `sui-rpc-node`
//!
//! Standalone, non-executing fullnode-replacement that exercises
//! every piece of the rpc-store stack end to end:
//!
//! - the [`sui_rpc_store`] indexer (raw chain data + derived
//!   indexes + bitmap CFs), driven by the
//!   [`sui_indexer_alt_framework`] ingestion service;
//! - the [`sui_consistent_store`]-backed RocksDB layout with
//!   cross-pipeline snapshot consistency;
//! - the [`sui_rpc_api`] gRPC / HTTP read surface mounted over an
//!   [`RpcStoreReader`].
//!
//! It is a proof-of-concept gating the embedded-fullnode
//! integration: anything we want `sui-node` to host has to work
//! here first. There is no execution path — the binary serves
//! reads from the indexed state.
//!
//! Three entry points:
//!
//! - [`start_service`] opens the database, builds the
//!   [`sui_rpc_store::Indexer`] with every pipeline enabled, mounts
//!   the [`sui_rpc_api`] HTTP server, and returns a composed
//!   [`sui_futures::Service`] driving both.
//! - [`start_serve`] opens an existing database and mounts only the
//!   [`sui_rpc_api`] server — no indexer, no ingestion source — so
//!   the database can be queried exactly as it is on disk.
//! - [`start_restorer`] restores from a Sui formal snapshot via
//!   [`sui_rpc_store::restore_indexes`] and floors the unrestored
//!   pipelines' watermarks via
//!   [`sui_rpc_store::floor_unrestored_pipelines`] so a subsequent
//!   `Run` resumes at `target_checkpoint + 1` across the board.
//!
//! `Run` without a prior `Restore` just starts at genesis
//! (checkpoint 0) — the framework's default lower bound when no
//! `__watermark` is persisted.

pub mod args;
pub mod config;
mod consistent_service;
pub mod rpc;

use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use prometheus::Registry;
use sui_consistent_store::ChainId;
use sui_consistent_store::Db;
use sui_consistent_store::DbOptions;
use sui_consistent_store::FrameworkSchema;
use sui_consistent_store::Schema as _;
use sui_consistent_store::Watermark;
use sui_consistent_store::metrics::ColumnFamilyStatsCollector;
use sui_consistent_store::restore::RestoreDriverConfig;
use sui_consistent_store::restore::RestoreSource;
use sui_consistent_store::restore::StorageConnectionArgs;
use sui_consistent_store::restore::formal_snapshot::FormalSnapshot;
use sui_consistent_store::restore::formal_snapshot::FormalSnapshotArgs;
use sui_consistent_store::restore::metrics::FormalSnapshotMetrics;
use sui_consistent_store::restore::metrics::RestoreMetrics;
use sui_futures::service::Service;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::BoxedStreamingClient;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClient;
use sui_indexer_alt_framework::ingestion::streaming_client::GrpcStreamingClient;
use sui_indexer_alt_framework::metrics::IngestionMetrics;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_rpc_api::subscription::IndexedCheckpointFn;
use sui_rpc_api::subscription::SubscriptionService;
use sui_rpc_store::Indexer;
use sui_rpc_store::PipelineLayer;
use sui_rpc_store::RestoreLayer;
use sui_rpc_store::RpcStoreSchema;
use sui_rpc_store::floor_unrestored_pipelines;
use sui_rpc_store::restore_indexes;

use crate::config::ServiceConfig;
use crate::rpc::build_rpc_service;

/// Metrics namespace shared by every prometheus collector this
/// binary registers.
pub const METRICS_PREFIX: &str = "sui_rpc_node";

/// `Run` entry point. Opens the database at `database_path`,
/// builds the indexer with every pipeline enabled, mounts the
/// `sui-rpc-api` HTTP server, and returns the composed
/// [`Service`].
///
/// `bin_name` / `version` show up in the `Server` header and the
/// `X-Sui-Rpc-Version` response header respectively, mirroring
/// the format `sui-node` uses.
pub async fn start_service(
    database_path: impl AsRef<Path>,
    indexer_args: IndexerArgs,
    client_args: ClientArgs,
    bin_name: &'static str,
    version: &'static str,
    config: ServiceConfig,
    registry: &Registry,
) -> anyhow::Result<Service> {
    let ServiceConfig {
        ingestion,
        consistency,
        committer,
        rpc,
        db,
        // Only the `restore` subcommand consults this section.
        restore: _,
        pruner,
    } = config;

    // Build the ingestion + (optional) streaming clients first so
    // the same `IngestionMetrics` instance is shared with the
    // ingestion service the indexer constructs internally — no
    // double registration against `registry`.
    let ingestion_metrics = IngestionMetrics::new(Some(METRICS_PREFIX), registry);
    let ingestion_client = IngestionClient::new(client_args.ingestion, ingestion_metrics)
        .context("Failed to construct ingestion client")?;
    let streaming_client: Option<BoxedStreamingClient> =
        client_args.streaming.streaming_url.map(|uri| {
            Box::new(GrpcStreamingClient::new(
                uri,
                ingestion.streaming_connection_timeout(),
                ingestion.streaming_statement_timeout(),
            )) as BoxedStreamingClient
        });

    let mut indexer = Indexer::new(
        database_path,
        indexer_args,
        ingestion_client,
        streaming_client,
        consistency.clone(),
        pruner,
        ingestion,
        db.to_db_options(),
        registry,
    )
    .await
    .context("Failed to construct rpc-store indexer")?;

    // Every pipeline runs in this binary — the whole point of the
    // proof-of-concept is to drive the full stack.
    let committer_config = committer.finish(CommitterConfig::default());
    indexer
        .add_pipelines(PipelineLayer::all(), committer_config.clone())
        .await
        .context("Failed to register rpc-store pipelines")?;

    // Build the RPC server over the same `Db` the indexer writes
    // to. The schema is cheap to re-bind (one `DbMap` handle per
    // CF) and is owned by the reader rather than the indexer's
    // store.
    let db = indexer.store().db().clone();

    // Host the checkpoint-subscription service over the checkpoints this
    // node indexes — the standalone analog of the fullnode's
    // checkpoint-executor broadcast. The `checkpoint_broadcast` pipeline
    // (registered below) feeds `checkpoint_sender` in checkpoint order;
    // the service holds each checkpoint back until the indexes have
    // committed it (`indexed_checkpoint`) so a subscriber can read its
    // indexed state as soon as it is delivered.
    //
    // Seed the broadcast pipeline's watermark to the current tip first,
    // so on the first run after a restore it follows live instead of
    // dragging the shared ingestion start back to genesis (a no-op on a
    // fresh database or once the watermark exists).
    if let Some(tip) = highest_indexed_checkpoint(&db) {
        sui_rpc_store::seed_checkpoint_broadcast_watermark(&db, tip)
            .context("Failed to seed checkpoint_broadcast watermark")?;
    }
    let indexed_checkpoint: IndexedCheckpointFn = {
        let db = db.clone();
        Arc::new(move || highest_indexed_checkpoint(&db))
    };
    let (checkpoint_sender, subscription_handle) =
        SubscriptionService::build(registry, Some(indexed_checkpoint));
    indexer
        .add_checkpoint_broadcast(checkpoint_sender, committer_config)
        .await
        .context("Failed to register checkpoint-broadcast pipeline")?;

    // Expose per-CF RocksDB stats (sizes, compaction backlog, write-stall
    // state). The collector holds only a weak handle to the database.
    registry
        .register(Box::new(ColumnFamilyStatsCollector::new(
            Some(METRICS_PREFIX),
            &db,
        )))
        .context("Failed to register RocksDB column-family stats collector")?;

    let rpc_service = build_rpc_service(
        db.clone(),
        Arc::new(RpcStoreSchema::open(&db).context("Failed to bind RpcStoreSchema for reads")?),
        consistency,
        rpc,
        Some(subscription_handle),
        bin_name,
        version,
        registry,
    )
    .await
    .context("Failed to start RPC HTTP server")?;

    let s_indexer = indexer.run().await?;
    Ok(s_indexer.merge(rpc_service))
}

/// The highest checkpoint every registered pipeline has committed
/// through — the minimum committed watermark across pipelines, i.e. the
/// point up to which all of the indexes are coherent. `None` before
/// anything has been indexed (a fresh database). Used both to seed the
/// broadcast pipeline at the tip and as the subscription service's
/// read-after-write gate.
fn highest_indexed_checkpoint(db: &Db) -> Option<u64> {
    let framework = FrameworkSchema::new(db.clone());
    let mut min: Option<u64> = None;
    for entry in framework.watermarks.iter(..).ok()? {
        let (_, watermark) = entry.ok()?;
        let hi = watermark.checkpoint_hi_inclusive;
        min = Some(min.map_or(hi, |m| m.min(hi)));
    }
    min
}

/// `Serve` entry point. Opens the existing database at
/// `database_path` and mounts the `sui-rpc-api` server over an
/// [`RpcStoreReader`] **without** building or running the indexer —
/// no ingestion source is needed and nothing advances the watermarks
/// while the server runs. Reads reflect the database exactly as it is
/// on disk.
///
/// The composed [`Service`] is just the RPC server (and its HTTPS
/// listener if configured). Because no [`Synchronizer`] is running,
/// no new cross-pipeline snapshots are taken, so the snapshot-backed
/// v1alpha `ConsistentService` reads see only snapshots already
/// present; the v2 read APIs serve tip reads directly and are
/// unaffected.
///
/// [`Synchronizer`]: sui_consistent_store::Synchronizer
pub async fn start_serve(
    database_path: impl AsRef<Path>,
    bin_name: &'static str,
    version: &'static str,
    config: ServiceConfig,
    registry: &Registry,
) -> anyhow::Result<Service> {
    let ServiceConfig {
        consistency,
        rpc,
        db,
        // The indexer is not run when serving, so its ingestion,
        // committer, restore, and pruner settings are irrelevant
        // here.
        ingestion: _,
        committer: _,
        restore: _,
        pruner: _,
    } = config;

    let (database, schema) = Db::open::<RpcStoreSchema>(database_path, db.to_db_options())
        .context("Failed to open rpc-store database")?;
    let schema = Arc::new(schema);

    // Resume the bitmap CFs' compaction filters against the persisted
    // pruning floor, matching the indexer's open path.
    schema
        .refresh_pruning_atomics()
        .context("Failed to refresh pruning watermarks")?;

    // Expose per-CF RocksDB stats, same as the run / restore paths.
    registry
        .register(Box::new(ColumnFamilyStatsCollector::new(
            Some(METRICS_PREFIX),
            &database,
        )))
        .context("Failed to register RocksDB column-family stats collector")?;

    // No indexer runs in `serve` mode, so there is no broadcast source
    // to back a subscription service; `subscribe_checkpoints` stays
    // unimplemented here.
    build_rpc_service(
        database.clone(),
        schema,
        consistency,
        rpc,
        None,
        bin_name,
        version,
        registry,
    )
    .await
    .context("Failed to start RPC HTTP server")
}

/// `Restore` entry point. Opens (or creates) the database at
/// `database_path`, kicks off a formal-snapshot restore via
/// [`restore_indexes`], and returns the in-flight restore
/// [`Service`] together with a [`RestoreFinalizer`] the binary
/// runs *after* the restore completes.
///
/// Split this way (mirroring `sui-indexer-alt-consistent-store`'s
/// shape) so the main binary can compose the restore against the
/// metrics service and chain the finalize step only on success
/// without re-implementing the supervision plumbing.
pub async fn start_restorer(
    database_path: impl AsRef<Path>,
    formal_snapshot_args: FormalSnapshotArgs,
    storage_connection_args: StorageConnectionArgs,
    shard_concurrency: usize,
    db_options: DbOptions,
    registry: &Registry,
) -> anyhow::Result<(Service, RestoreFinalizer)> {
    let (db, schema) = Db::open::<RpcStoreSchema>(database_path, db_options)
        .context("Failed to open rpc-store database")?;
    let schema = Arc::new(schema);

    // Expose per-CF RocksDB stats (sizes, compaction backlog, write-stall
    // state) for the duration of the restore. The collector holds only a
    // weak handle, so it does not keep the database open.
    registry
        .register(Box::new(ColumnFamilyStatsCollector::new(
            Some(METRICS_PREFIX),
            &db,
        )))
        .context("Failed to register RocksDB column-family stats collector")?;

    let formal_snapshot_metrics = FormalSnapshotMetrics::new(Some(METRICS_PREFIX), registry);
    let source = FormalSnapshot::new(
        formal_snapshot_args,
        storage_connection_args,
        formal_snapshot_metrics,
    )
    .await
    .context("Failed to connect to formal snapshot")?;

    // Capture the snapshot's anchor metadata before we hand the
    // source to the driver — the driver consumes the source by
    // value and we need both pieces for the post-restore floor
    // step.
    let target_watermark = source.target_watermark();
    let target_chain_id = source.target_chain_id();

    let driver_config = RestoreDriverConfig {
        // Clamp to at least one so a misconfigured `0` does not stall
        // the driver, which would otherwise spawn no shard tasks.
        shard_concurrency: Some(shard_concurrency.max(1)),
    };
    let layer = RestoreLayer::all();

    let restore_metrics = RestoreMetrics::new(Some(METRICS_PREFIX), registry);
    let primary = restore_indexes(
        db.clone(),
        schema,
        source,
        driver_config,
        layer.clone(),
        restore_metrics,
    )?;

    Ok((
        primary,
        RestoreFinalizer {
            db,
            target_watermark,
            target_chain_id,
            layer,
        },
    ))
}

/// Post-restore step: writes watermark / chain-id rows for every
/// pipeline the formal snapshot can't cover and stamps the
/// singleton pruning watermark. Run by the binary only after the
/// restore [`Service`] completes successfully — failing to do so
/// would leave the raw-chain-data pipelines without a starting
/// floor, so the next `Run` would attempt to replay from
/// genesis.
pub struct RestoreFinalizer {
    db: Db,
    target_watermark: Watermark,
    target_chain_id: ChainId,
    layer: RestoreLayer,
}

impl RestoreFinalizer {
    /// Wrap the finalize work in a [`Service`] so it composes
    /// with the metrics service the same way the restore service
    /// does.
    pub fn run(self) -> Service {
        Service::new().spawn_aborting(async move {
            floor_unrestored_pipelines(
                &self.db,
                self.target_watermark,
                self.target_chain_id,
                &self.layer,
            )
            .context("Failed to floor unrestored pipelines after restore")?;
            Ok(())
        })
    }
}
