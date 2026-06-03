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
//! Two entry points:
//!
//! - [`start_service`] opens the database, builds the
//!   [`sui_rpc_store::Indexer`] with every pipeline enabled, mounts
//!   the [`sui_rpc_api`] HTTP server, and returns a composed
//!   [`sui_futures::Service`] driving both.
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
pub mod rpc;

use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use prometheus::Registry;
use sui_consistent_store::ChainId;
use sui_consistent_store::Db;
use sui_consistent_store::DbOptions;
use sui_consistent_store::Schema as _;
use sui_consistent_store::Watermark;
use sui_consistent_store::restore::RestoreDriverConfig;
use sui_consistent_store::restore::RestoreSource;
use sui_consistent_store::restore::StorageConnectionArgs;
use sui_consistent_store::restore::formal_snapshot::FormalSnapshot;
use sui_consistent_store::restore::formal_snapshot::FormalSnapshotArgs;
use sui_consistent_store::restore::metrics::FormalSnapshotMetrics;
use sui_futures::service::Service;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::BoxedStreamingClient;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClient;
use sui_indexer_alt_framework::ingestion::streaming_client::GrpcStreamingClient;
use sui_indexer_alt_framework::metrics::IngestionMetrics;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_rpc_store::Indexer;
use sui_rpc_store::PipelineLayer;
use sui_rpc_store::RestoreLayer;
use sui_rpc_store::RpcStoreSchema;
use sui_rpc_store::floor_unrestored_pipelines;
use sui_rpc_store::restore_indexes;

use crate::args::RestoreArgs;
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
        consistency,
        ingestion,
        DbOptions::default(),
        registry,
    )
    .await
    .context("Failed to construct rpc-store indexer")?;

    // Every pipeline runs in this binary — the whole point of the
    // proof-of-concept is to drive the full stack.
    let committer_config = committer.finish(CommitterConfig::default());
    indexer
        .add_pipelines(PipelineLayer::all(), committer_config)
        .await
        .context("Failed to register rpc-store pipelines")?;

    // Build the RPC server over the same `Db` the indexer writes
    // to. The schema is cheap to re-bind (one `DbMap` handle per
    // CF) and is owned by the reader rather than the indexer's
    // store.
    let db = indexer.store().db().clone();
    let rpc_service = build_rpc_service(
        db.clone(),
        Arc::new(RpcStoreSchema::open(&db).context("Failed to bind RpcStoreSchema for reads")?),
        rpc,
        bin_name,
        version,
        registry,
    )
    .await
    .context("Failed to start RPC HTTP server")?;

    let s_indexer = indexer.run().await?;
    Ok(s_indexer.merge(rpc_service))
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
    restore_args: RestoreArgs,
    registry: &Registry,
) -> anyhow::Result<(Service, RestoreFinalizer)> {
    let (db, schema) = Db::open::<RpcStoreSchema>(database_path, DbOptions::default())
        .context("Failed to open rpc-store database")?;
    let schema = Arc::new(schema);

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
        shard_concurrency: restore_args.shard_concurrency,
    };
    let layer = RestoreLayer::all();

    let primary = restore_indexes(db.clone(), schema, source, driver_config, layer.clone())?;

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
