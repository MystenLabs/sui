// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Storage backend for `sui-rpc-api`.
//!
//! Built on top of [`sui_consistent_store`], this crate hosts the
//! column families that back every read the RPC service performs:
//!
//! - Raw chain data — objects, transactions, effects, events,
//!   checkpoints, committees — previously served by the validator's
//!   perpetual / checkpoint / committee stores.
//! - Indexes — owner, dynamic-field, coin, balance, package version,
//!   epoch info, ledger history — previously served by
//!   `sui-core::rpc_index` and `sui-indexer-alt-consistent-store`.
//!
//! Values are encoded with bespoke protobuf messages defined under
//! `proto/sui/rpc_store/`, mirroring the build setup in
//! `sui-consistent-store`.

pub mod config;
pub mod indexer;
pub mod proto;
pub mod reader;
pub mod schema;

use std::path::Path;
use std::sync::Arc;

use prometheus::Registry;
use sui_consistent_store::DbOptions;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ArcStreamingClient;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClient;
use sui_indexer_alt_framework::ingestion::streaming_client::GrpcStreamingClient;
use sui_indexer_alt_framework::metrics::IngestionMetrics;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_indexer_alt_framework::service::Service;

pub use crate::config::CommitterLayer;
pub use crate::config::ConsistencyConfig;
pub use crate::config::PipelineLayer;
pub use crate::config::PrunerConfig;
pub use crate::config::RestoreLayer;
pub use crate::config::ServiceConfig;
pub use crate::indexer::Indexer;
pub use crate::indexer::METRICS_PREFIX;
pub use crate::indexer::Store;
pub use crate::indexer::checkpoint_broadcast::CheckpointBroadcast;
pub use crate::indexer::checkpoint_broadcast::seed_watermark_to_tip as seed_checkpoint_broadcast_watermark;
pub use crate::indexer::pruner::prune_history_cohort;
pub use crate::indexer::restore::HISTORY_COHORT;
pub use crate::indexer::restore::LIVE_COHORT;
pub use crate::indexer::restore::floor_unrestored_pipelines;
pub use crate::indexer::restore::restore_indexes;
pub use crate::indexer::restore::seed_current_epoch_start;
pub use crate::indexer::restore::seed_history_cohort;
pub use crate::reader::RpcStoreReader;
pub use crate::schema::RpcStoreSchema;
pub use crate::schema::default_rocksdb_config;

/// Standalone-binary entry point. Opens the database at `path`,
/// constructs an [`Indexer`] from `ClientArgs`-driven ingestion /
/// streaming clients, registers every pipeline that is enabled in
/// `config.pipeline`, and runs the resulting indexer.
///
/// The embedded-fullnode path bypasses this helper and constructs
/// [`Indexer::from_store`] directly with its own
/// [`IngestionClientTrait`] /
/// [`CheckpointStreamingClient`] implementations.
///
/// [`IngestionClientTrait`]:
///   sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientTrait
/// [`CheckpointStreamingClient`]:
///   sui_indexer_alt_framework::ingestion::streaming_client::CheckpointStreamingClient
pub async fn start_indexer(
    path: impl AsRef<Path>,
    indexer_args: IndexerArgs,
    client_args: ClientArgs,
    db_options: DbOptions,
    ingestion_config: IngestionConfig,
    config: ServiceConfig,
    registry: &Registry,
) -> anyhow::Result<Service> {
    let metrics_prefix = Some(METRICS_PREFIX);

    // Build the metrics once; the same Arc threads through the
    // ingestion client and (via `IngestionClient::metrics`) the
    // ingestion service, avoiding double registration against
    // `registry`.
    let ingestion_metrics = IngestionMetrics::new(metrics_prefix, registry);
    let ingestion_client = IngestionClient::new(client_args.ingestion, ingestion_metrics)?;
    let streaming_client: Option<ArcStreamingClient> =
        client_args.streaming.streaming_url.map(|uri| {
            Arc::new(GrpcStreamingClient::new(
                uri,
                ingestion_config.streaming_connection_timeout(),
                ingestion_config.streaming_statement_timeout(),
            )) as ArcStreamingClient
        });

    let mut indexer = Indexer::new(
        path,
        indexer_args,
        ingestion_client,
        streaming_client,
        config.consistency,
        config.pruner,
        ingestion_config,
        db_options,
        registry,
    )
    .await?;

    let committer = config.committer.finish(CommitterConfig::default());
    indexer.add_pipelines(config.pipeline, committer).await?;

    indexer.run().await
}
