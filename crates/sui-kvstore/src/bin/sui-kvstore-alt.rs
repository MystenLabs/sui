// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use sui_indexer_alt_framework::Indexer;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_indexer_alt_framework::service::Error;
use sui_indexer_alt_metrics::MetricsArgs;
use sui_kvstore::BIGTABLE_MAX_MUTATIONS;
use sui_kvstore::BigTableClient;
use sui_kvstore::BigTableHandler;
use sui_kvstore::BigTableStore;
use sui_kvstore::CheckpointsByDigestPipeline;
use sui_kvstore::CheckpointsPipeline;
use sui_kvstore::EpochEndPipeline;
use sui_kvstore::EpochLegacyPipeline;
use sui_kvstore::EpochStartPipeline;
use sui_kvstore::ObjectTypesPipeline;
use sui_kvstore::ObjectsPipeline;
use sui_kvstore::TransactionsPipeline;
use sui_kvstore::set_max_mutations;
use telemetry_subscribers::TelemetryConfig;
use tracing::info;

fn parse_max_mutations(s: &str) -> Result<usize, String> {
    let value: usize = s.parse().map_err(|e| format!("invalid number: {e}"))?;
    if value >= BIGTABLE_MAX_MUTATIONS {
        return Err(format!(
            "args.max_mutations must be less than {BIGTABLE_MAX_MUTATIONS}"
        ));
    }
    Ok(value)
}

#[derive(Parser)]
#[command(name = "sui-kvstore-alt")]
#[command(about = "KVStore indexer using sui-indexer-alt-framework")]
struct Args {
    /// BigTable instance ID
    instance_id: String,

    /// BigTable app profile ID
    #[arg(long)]
    app_profile_id: Option<String>,

    /// Number of concurrent checkpoint writes
    #[arg(long)]
    write_concurrency: Option<usize>,

    /// Interval between watermark updates
    #[arg(long, value_parser = humantime::parse_duration)]
    watermark_interval: Option<Duration>,

    /// Maximum number of checkpoints to fetch concurrently
    #[arg(long)]
    ingest_concurrency: Option<usize>,

    /// Maximum size of checkpoint backlog across all workers
    #[arg(long)]
    checkpoint_buffer_size: Option<usize>,

    /// Maximum mutations per BigTable batch (must be < 100k)
    #[arg(long, default_value_t = 10_000, value_parser = parse_max_mutations)]
    max_mutations: usize,

    #[command(flatten)]
    metrics_args: MetricsArgs,

    #[command(flatten)]
    client_args: ClientArgs,

    #[command(flatten)]
    indexer_args: IndexerArgs,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = TelemetryConfig::new().with_env().init();

    let args = Args::parse();
    let is_bounded = args.indexer_args.last_checkpoint.is_some();
    set_max_mutations(args.max_mutations);

    info!("Starting sui-kvstore-alt indexer");
    info!(instance_id = %args.instance_id);

    let client = BigTableClient::new_remote(
        args.instance_id,
        false,
        None,
        "sui-kvstore-alt".to_string(),
        None,
        args.app_profile_id,
    )
    .await?;

    let store = BigTableStore::new(client);

    let registry = prometheus::Registry::new_custom(Some("kvstore_alt".into()), None)?;
    let metrics_service =
        sui_indexer_alt_metrics::MetricsService::new(args.metrics_args, registry.clone());

    let mut ingestion_config = IngestionConfig::default();
    if let Some(v) = args.ingest_concurrency {
        ingestion_config.ingest_concurrency = v;
    }
    if let Some(v) = args.checkpoint_buffer_size {
        ingestion_config.checkpoint_buffer_size = v;
    }

    let mut indexer = Indexer::new(
        store,
        args.indexer_args,
        args.client_args,
        ingestion_config,
        None,
        &registry,
    )
    .await?;

    let mut config = ConcurrentConfig::default();
    if let Some(v) = args.write_concurrency {
        config.committer.write_concurrency = v;
    }
    if let Some(v) = args.watermark_interval {
        config.committer.watermark_interval_ms = v.as_millis() as u64;
    }

    indexer
        .concurrent_pipeline(BigTableHandler::new(CheckpointsPipeline), config.clone())
        .await?;
    indexer
        .concurrent_pipeline(
            BigTableHandler::new(CheckpointsByDigestPipeline),
            config.clone(),
        )
        .await?;
    indexer
        .concurrent_pipeline(BigTableHandler::new(TransactionsPipeline), config.clone())
        .await?;
    indexer
        .concurrent_pipeline(BigTableHandler::new(ObjectsPipeline), config.clone())
        .await?;
    indexer
        .concurrent_pipeline(BigTableHandler::new(ObjectTypesPipeline), config.clone())
        .await?;
    indexer
        .concurrent_pipeline(BigTableHandler::new(EpochStartPipeline), config.clone())
        .await?;
    indexer
        .concurrent_pipeline(BigTableHandler::new(EpochEndPipeline), config.clone())
        .await?;

    // DEPRECATED. Delete after migrating readers to new columns.
    // EpochLegacyPipeline has its own Handler impl due to read-modify-write pattern.
    indexer
        .concurrent_pipeline(EpochLegacyPipeline, config)
        .await?;

    let metrics_handle = metrics_service.run().await?;
    let service = indexer.run().await?;

    match service.attach(metrics_handle).main().await {
        Ok(()) => {}
        Err(Error::Terminated) => {
            if is_bounded {
                std::process::exit(1);
            }
        }
        Err(Error::Aborted) => {
            std::process::exit(1);
        }
        Err(Error::Task(_)) => {
            std::process::exit(2);
        }
    }

    Ok(())
}
