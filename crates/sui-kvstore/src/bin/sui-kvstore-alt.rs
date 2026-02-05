// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use sui_concurrency_limiter::{AimdConfig, Gradient2Config};
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_indexer_alt_framework::service::Error;
use sui_indexer_alt_metrics::MetricsArgs;
use sui_kvstore::BIGTABLE_MAX_MUTATIONS;
use sui_kvstore::BigTableClient;
use sui_kvstore::BigTableIndexer;
use sui_kvstore::BigTableStore;
use sui_kvstore::set_max_mutations;
use sui_kvstore::set_write_legacy_data;
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
    #[arg(long, value_parser = parse_max_mutations)]
    max_mutations: Option<usize>,

    /// Enable writing legacy data: watermark \[0\] row, epoch DEFAULT_COLUMN, and transaction tx column
    #[arg(long)]
    write_legacy_data: bool,

    /// Enable AIMD dynamic write concurrency with this initial limit.
    /// When set, --write-concurrency is ignored.
    #[arg(long)]
    aimd_initial_limit: Option<usize>,

    /// AIMD minimum concurrency limit (default: 1)
    #[arg(long)]
    aimd_min_limit: Option<usize>,

    /// AIMD maximum concurrency limit (default: 1000)
    #[arg(long)]
    aimd_max_limit: Option<usize>,

    /// AIMD backoff ratio on failure, in [0.5, 1.0) (default: 0.9)
    #[arg(long)]
    aimd_backoff_ratio: Option<f64>,

    /// Enable AIMD dynamic ingestion concurrency with this initial limit.
    /// When set, --ingest-concurrency is ignored.
    #[arg(long)]
    ingest_aimd_initial_limit: Option<usize>,

    /// Ingestion AIMD minimum concurrency limit (default: 1)
    #[arg(long)]
    ingest_aimd_min_limit: Option<usize>,

    /// Ingestion AIMD maximum concurrency limit (default: 1000)
    #[arg(long)]
    ingest_aimd_max_limit: Option<usize>,

    /// Ingestion AIMD backoff ratio on failure, in [0.5, 1.0) (default: 0.9)
    #[arg(long)]
    ingest_aimd_backoff_ratio: Option<f64>,

    /// Enable Gradient2 dynamic write concurrency with this initial limit.
    /// When set, takes priority over both --write-concurrency and --aimd-*.
    #[arg(long)]
    gradient2_initial_limit: Option<usize>,

    /// Gradient2 minimum concurrency limit (default: 20)
    #[arg(long)]
    gradient2_min_limit: Option<usize>,

    /// Gradient2 maximum concurrency limit (default: 200)
    #[arg(long)]
    gradient2_max_limit: Option<usize>,

    /// Gradient2 smoothing factor in (0.0, 1.0] (default: 0.2)
    #[arg(long)]
    gradient2_smoothing: Option<f64>,

    /// Gradient2 additive growth per update (default: 4)
    #[arg(long)]
    gradient2_queue_size: Option<usize>,

    /// Gradient2 tolerance multiplier on RTT ratio (default: 1.5)
    #[arg(long)]
    gradient2_tolerance: Option<f64>,

    /// Gradient2 long-term RTT EMA window size (default: 600)
    #[arg(long)]
    gradient2_long_window: Option<usize>,

    /// Enable Gradient2 dynamic ingestion concurrency with this initial limit.
    /// When set, takes priority over both --ingest-concurrency and --ingest-aimd-*.
    #[arg(long)]
    ingest_gradient2_initial_limit: Option<usize>,

    /// Ingestion Gradient2 minimum concurrency limit (default: 20)
    #[arg(long)]
    ingest_gradient2_min_limit: Option<usize>,

    /// Ingestion Gradient2 maximum concurrency limit (default: 200)
    #[arg(long)]
    ingest_gradient2_max_limit: Option<usize>,

    /// Ingestion Gradient2 smoothing factor (default: 0.2)
    #[arg(long)]
    ingest_gradient2_smoothing: Option<f64>,

    /// Ingestion Gradient2 additive growth per update (default: 4)
    #[arg(long)]
    ingest_gradient2_queue_size: Option<usize>,

    /// Ingestion Gradient2 tolerance multiplier (default: 1.5)
    #[arg(long)]
    ingest_gradient2_tolerance: Option<f64>,

    /// Ingestion Gradient2 long-term RTT EMA window size (default: 600)
    #[arg(long)]
    ingest_gradient2_long_window: Option<usize>,

    #[command(flatten)]
    metrics_args: MetricsArgs,

    #[command(flatten)]
    client_args: ClientArgs,

    #[command(flatten)]
    indexer_args: IndexerArgs,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Install ring as the default rustls crypto provider. Required because hyper-rustls
    // (via gcp_auth) enables aws-lc-rs by default, and we also use ring elsewhere.
    // With both providers compiled in, rustls can't auto-detect which to use.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let _guard = TelemetryConfig::new().with_env().init();

    let args = Args::parse();
    let is_bounded = args.indexer_args.last_checkpoint.is_some();
    set_write_legacy_data(args.write_legacy_data);
    if let Some(v) = args.max_mutations {
        set_max_mutations(v);
    }

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

    let mut config = ConcurrentConfig::default();
    if let Some(v) = args.write_concurrency {
        config.committer.write_concurrency = v;
    }
    if let Some(v) = args.watermark_interval {
        config.committer.watermark_interval_ms = v.as_millis() as u64;
    }

    if let Some(initial_limit) = args.aimd_initial_limit {
        let mut aimd = AimdConfig {
            initial_limit,
            ..AimdConfig::default()
        };
        if let Some(v) = args.aimd_min_limit {
            aimd.min_limit = v;
        }
        if let Some(v) = args.aimd_max_limit {
            aimd.max_limit = v;
        }
        if let Some(v) = args.aimd_backoff_ratio {
            aimd.backoff_ratio = v;
        }
        config.committer.aimd = Some(aimd);
    }

    if let Some(initial_limit) = args.gradient2_initial_limit {
        let mut g2 = Gradient2Config {
            initial_limit,
            ..Gradient2Config::default()
        };
        if let Some(v) = args.gradient2_min_limit {
            g2.min_limit = v;
        }
        if let Some(v) = args.gradient2_max_limit {
            g2.max_limit = v;
        }
        if let Some(v) = args.gradient2_smoothing {
            g2.smoothing = v;
        }
        if let Some(v) = args.gradient2_queue_size {
            g2.queue_size = v;
        }
        if let Some(v) = args.gradient2_tolerance {
            g2.tolerance = v;
        }
        if let Some(v) = args.gradient2_long_window {
            g2.long_window = v;
        }
        config.committer.gradient2 = Some(g2);
    }

    if let Some(initial_limit) = args.ingest_aimd_initial_limit {
        let mut aimd = AimdConfig {
            initial_limit,
            ..ingestion_config.default_aimd()
        };
        if let Some(v) = args.ingest_aimd_min_limit {
            aimd.min_limit = v;
        }
        if let Some(v) = args.ingest_aimd_max_limit {
            aimd.max_limit = v;
        }
        if let Some(v) = args.ingest_aimd_backoff_ratio {
            aimd.backoff_ratio = v;
        }
        ingestion_config.aimd = Some(aimd);
    }

    if let Some(initial_limit) = args.ingest_gradient2_initial_limit {
        let mut g2 = Gradient2Config {
            initial_limit,
            ..ingestion_config.default_gradient2()
        };
        if let Some(v) = args.ingest_gradient2_min_limit {
            g2.min_limit = v;
        }
        if let Some(v) = args.ingest_gradient2_max_limit {
            g2.max_limit = v;
        }
        if let Some(v) = args.ingest_gradient2_smoothing {
            g2.smoothing = v;
        }
        if let Some(v) = args.ingest_gradient2_queue_size {
            g2.queue_size = v;
        }
        if let Some(v) = args.ingest_gradient2_tolerance {
            g2.tolerance = v;
        }
        if let Some(v) = args.ingest_gradient2_long_window {
            g2.long_window = v;
        }
        ingestion_config.gradient2 = Some(g2);
    }

    let bigtable_indexer = BigTableIndexer::new(
        store,
        args.indexer_args,
        args.client_args,
        ingestion_config,
        config,
        &registry,
    )
    .await?;

    let metrics_handle = metrics_service.run().await?;
    let service = bigtable_indexer.indexer.run().await?;

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
