// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_default_config::DefaultConfig;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_indexer_alt_framework::pipeline::ProcessorConcurrencyConfig;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_indexer_alt_framework::{self as framework};

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct IndexerConfig {
    pub ingestion: IngestionConfig,
    pub committer: CommitterLayer,
    pub pipeline: PipelineLayer,
    /// Global rate limit (rows per second) shared across all pipelines.
    pub total_max_rows_per_second: Option<u64>,
    /// Default per-pipeline rate limit (rows per second).
    /// Individual pipelines can override via their `ConcurrentLayer`.
    pub max_rows_per_second: Option<u64>,
    /// Number of gRPC channels in the Bigtable connection pool (default: 10).
    /// A good rule of thumb is to target ~25 concurrent RPCs per channel, so
    /// ceil(sum of write_concurrency across all pipelines / 25).
    pub bigtable_connection_pool_size: Option<usize>,
    /// Channel-level timeout in milliseconds for BigTable gRPC calls (default: 60000).
    pub bigtable_channel_timeout_ms: Option<u64>,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct CommitterLayer {
    pub write_concurrency: Option<usize>,
    pub collect_interval_ms: Option<u64>,
    pub watermark_interval_ms: Option<u64>,
    pub watermark_interval_jitter_ms: Option<u64>,
}

impl CommitterLayer {
    pub fn finish(self, base: CommitterConfig) -> CommitterConfig {
        CommitterConfig {
            write_concurrency: self.write_concurrency.unwrap_or(base.write_concurrency),
            collect_interval_ms: self.collect_interval_ms.unwrap_or(base.collect_interval_ms),
            watermark_interval_ms: self
                .watermark_interval_ms
                .unwrap_or(base.watermark_interval_ms),
            watermark_interval_jitter_ms: self
                .watermark_interval_jitter_ms
                .unwrap_or(base.watermark_interval_jitter_ms),
        }
    }
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct ConcurrentLayer {
    pub committer: Option<CommitterLayer>,
    /// Maximum rows per BigTable batch for this pipeline.
    pub max_rows: Option<usize>,
    /// Per-pipeline rate limit (rows per second). Overrides the default
    /// `IndexerConfig::max_rows_per_second` when set.
    pub max_rows_per_second: Option<u64>,
    pub fanout: Option<ProcessorConcurrencyConfig>,
    pub min_eager_rows: Option<usize>,
    pub max_pending_rows: Option<usize>,
    pub max_watermark_updates: Option<usize>,
    pub channel_size: Option<usize>,
}

impl ConcurrentLayer {
    pub fn finish(self, base: ConcurrentConfig) -> ConcurrentConfig {
        ConcurrentConfig {
            committer: if let Some(c) = self.committer {
                c.finish(base.committer)
            } else {
                base.committer
            },
            pruner: None,
            fanout: self
                .fanout
                .or(base.fanout)
                .or(Some(ProcessorConcurrencyConfig::Fixed(num_cpus::get()))),
            min_eager_rows: self.min_eager_rows.or(base.min_eager_rows),
            max_pending_rows: self.max_pending_rows.or(base.max_pending_rows),
            max_watermark_updates: self.max_watermark_updates.or(base.max_watermark_updates),
            channel_size: self.channel_size.unwrap_or(base.channel_size),
        }
    }
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct PipelineLayer {
    pub checkpoints: ConcurrentLayer,
    pub checkpoints_by_digest: ConcurrentLayer,
    pub transactions: ConcurrentLayer,
    pub objects: ConcurrentLayer,
    pub epoch_start: ConcurrentLayer,
    pub epoch_end: ConcurrentLayer,
    pub protocol_configs: ConcurrentLayer,
    pub epoch_legacy: ConcurrentLayer,
    pub packages: ConcurrentLayer,
    pub packages_by_id: ConcurrentLayer,
    pub packages_by_checkpoint: ConcurrentLayer,
    pub system_packages: ConcurrentLayer,
}

/// This type is identical to [`framework::ingestion::IngestionConfig`], but is set-up to be
/// serialized and deserialized by `serde`.
#[DefaultConfig]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct IngestionConfig {
    pub checkpoint_buffer_size: usize,
    pub ingest_concurrency: framework::ingestion::IngestConcurrencyConfig,
    pub retry_interval_ms: u64,
    pub streaming_backoff_initial_batch_size: usize,
    pub streaming_backoff_max_batch_size: usize,
    pub streaming_connection_timeout_ms: u64,
    pub streaming_statement_timeout_ms: u64,
}

impl Default for IngestionConfig {
    fn default() -> Self {
        framework::ingestion::IngestionConfig::default().into()
    }
}

impl From<framework::ingestion::IngestionConfig> for IngestionConfig {
    fn from(config: framework::ingestion::IngestionConfig) -> Self {
        Self {
            checkpoint_buffer_size: config.checkpoint_buffer_size,
            ingest_concurrency: config.ingest_concurrency,
            retry_interval_ms: config.retry_interval_ms,
            streaming_backoff_initial_batch_size: config.streaming_backoff_initial_batch_size,
            streaming_backoff_max_batch_size: config.streaming_backoff_max_batch_size,
            streaming_connection_timeout_ms: config.streaming_connection_timeout_ms,
            streaming_statement_timeout_ms: config.streaming_statement_timeout_ms,
        }
    }
}

impl From<IngestionConfig> for framework::ingestion::IngestionConfig {
    fn from(config: IngestionConfig) -> Self {
        framework::ingestion::IngestionConfig {
            checkpoint_buffer_size: config.checkpoint_buffer_size,
            ingest_concurrency: config.ingest_concurrency,
            retry_interval_ms: config.retry_interval_ms,
            streaming_backoff_initial_batch_size: config.streaming_backoff_initial_batch_size,
            streaming_backoff_max_batch_size: config.streaming_backoff_max_batch_size,
            streaming_connection_timeout_ms: config.streaming_connection_timeout_ms,
            streaming_statement_timeout_ms: config.streaming_statement_timeout_ms,
        }
    }
}
