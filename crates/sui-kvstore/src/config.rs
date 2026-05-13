// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use sui_default_config::DefaultConfig;
use sui_indexer_alt_framework::config::ConcurrencyConfig;
use sui_indexer_alt_framework::pipeline;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_indexer_alt_framework::pipeline::sequential::SequentialConfig;
use sui_indexer_alt_framework::{self as framework};
use tracing::warn;

use crate::bigtable::client::PoolConfig;

/// Default maximum rows per BigTable write batch. Matches the official Google
/// Java client default.
pub(crate) const DEFAULT_MAX_ROWS_PER_BIGTABLE_BATCH: usize = 100;

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
    /// Deprecated: use `bigtable-pool` section instead. If set, overrides
    /// `bigtable-pool.initial-pool-size`. Will be removed in a future release.
    pub bigtable_connection_pool_size: Option<usize>,
    /// Channel-level timeout in milliseconds for BigTable gRPC calls (default: 60000).
    pub bigtable_channel_timeout_ms: Option<u64>,
    /// Bigtable connection pool configuration.
    pub bigtable_pool: BigtablePoolLayer,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct BigtablePoolLayer {
    /// Number of channels to create at startup (default: 10).
    pub initial_pool_size: Option<usize>,
    /// Minimum number of channels the pool will maintain (default: 1).
    pub min_pool_size: Option<usize>,
    /// Maximum number of channels the pool can scale to (default: 200).
    pub max_pool_size: Option<usize>,
    /// Average load per channel below which the pool considers scaling down (default: 5).
    pub min_rpcs_per_channel: Option<usize>,
    /// Average load per channel above which the pool scales up (default: 50).
    pub max_rpcs_per_channel: Option<usize>,
    /// Maximum channels to remove in a single scale-down operation (default: 2).
    pub max_resize_delta: Option<usize>,
    /// Consecutive low-load observations required before scaling down (default: 3).
    pub downscale_threshold: Option<usize>,
    /// Milliseconds between maintenance cycles (resize + refresh) (default: 60000).
    pub maintenance_interval_ms: Option<u64>,
    /// Channel age in milliseconds before it is eligible for refresh (default: 2700000 = 45 min).
    pub refresh_age_ms: Option<u64>,
    /// Random jitter in milliseconds added to refresh age (default: 300000 = 5 min).
    pub refresh_jitter_ms: Option<u64>,
}

impl BigtablePoolLayer {
    pub fn finish(self, deprecated_pool_size: Option<usize>) -> PoolConfig {
        if deprecated_pool_size.is_some() {
            warn!(
                "bigtable-connection-pool-size is deprecated; \
                 use the [bigtable-pool] section instead"
            );
        }

        let base = PoolConfig::default();
        PoolConfig {
            initial_pool_size: self
                .initial_pool_size
                .or(deprecated_pool_size)
                .unwrap_or(base.initial_pool_size),
            min_pool_size: self.min_pool_size.unwrap_or(base.min_pool_size),
            max_pool_size: self.max_pool_size.unwrap_or(base.max_pool_size),
            min_rpcs_per_channel: self
                .min_rpcs_per_channel
                .unwrap_or(base.min_rpcs_per_channel),
            max_rpcs_per_channel: self
                .max_rpcs_per_channel
                .unwrap_or(base.max_rpcs_per_channel),
            max_resize_delta: self.max_resize_delta.unwrap_or(base.max_resize_delta),
            downscale_threshold: self.downscale_threshold.unwrap_or(base.downscale_threshold),
            maintenance_interval: self
                .maintenance_interval_ms
                .map(Duration::from_millis)
                .unwrap_or(base.maintenance_interval),
            refresh_age: self
                .refresh_age_ms
                .map(Duration::from_millis)
                .unwrap_or(base.refresh_age),
            refresh_jitter: self
                .refresh_jitter_ms
                .map(Duration::from_millis)
                .unwrap_or(base.refresh_jitter),
        }
    }
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
    pub ingestion: Option<PipelineIngestionLayer>,
    /// Maximum rows per BigTable batch for this pipeline.
    pub max_rows: Option<usize>,
    /// Per-pipeline rate limit (rows per second). Overrides the default
    /// `IndexerConfig::max_rows_per_second` when set.
    pub max_rows_per_second: Option<u64>,
    pub fanout: Option<ConcurrencyConfig>,
    pub min_eager_rows: Option<usize>,
    pub max_pending_rows: Option<usize>,
    pub max_watermark_updates: Option<usize>,
    pub processor_channel_size: Option<usize>,
    pub collector_channel_size: Option<usize>,
    pub committer_channel_size: Option<usize>,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct PipelineIngestionLayer {
    pub subscriber_channel_size: Option<usize>,
}

impl ConcurrentLayer {
    pub(crate) fn max_rows_or_default(&self) -> usize {
        self.max_rows.unwrap_or(DEFAULT_MAX_ROWS_PER_BIGTABLE_BATCH)
    }

    pub fn finish(self, base: ConcurrentConfig) -> ConcurrentConfig {
        ConcurrentConfig {
            committer: if let Some(c) = self.committer {
                c.finish(base.committer)
            } else {
                base.committer
            },
            ingestion: if let Some(i) = self.ingestion {
                i.finish(base.ingestion)
            } else {
                base.ingestion
            },
            pruner: None,
            fanout: self.fanout.or(base.fanout),
            min_eager_rows: self.min_eager_rows.or(base.min_eager_rows),
            max_pending_rows: self.max_pending_rows.or(base.max_pending_rows),
            max_watermark_updates: self.max_watermark_updates.or(base.max_watermark_updates),
            processor_channel_size: self.processor_channel_size.or(base.processor_channel_size),
            collector_channel_size: self.collector_channel_size.or(base.collector_channel_size),
            committer_channel_size: self.committer_channel_size.or(base.committer_channel_size),
        }
    }
}

impl PipelineIngestionLayer {
    pub fn finish(self, base: pipeline::IngestionConfig) -> pipeline::IngestionConfig {
        pipeline::IngestionConfig {
            subscriber_channel_size: self
                .subscriber_channel_size
                .or(base.subscriber_channel_size),
        }
    }
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct SequentialLayer {
    // Framework sequential surface — mirrors the fields actually read by
    // `sui_indexer_alt_framework::pipeline::sequential`.
    pub committer: Option<CommitterLayer>,
    pub ingestion: Option<PipelineIngestionLayer>,
    pub fanout: Option<ConcurrencyConfig>,
    pub min_eager_rows: Option<usize>,
    pub max_pending_rows: Option<usize>,
    pub max_batch_checkpoints: Option<usize>,
    pub processor_channel_size: Option<usize>,
    pub pipeline_depth: Option<usize>,

    // sui-kvstore-specific config extensions

    // Controls the concurrency of the bitmap flushes. The framework doesn't
    // perform commits for sequential pipelines concurrently, but our store
    // implementation doesn't actually write to the database on commit for
    // the bitmap pipelines. The store buffers the bitmaps for the current
    // working "bucket" ranges internally, merges in rows from each framework
    // batch on commit (parallelized on background tasks), then finally flushes
    // the updated bitmaps to bigtable concurrently. Same semantic as
    // `ConcurrentLayer::write_concurrency`.
    pub write_concurrency: Option<usize>,
    /// Maximum rows per in-handler BigTable write RPC. Same semantic as
    /// `ConcurrentLayer::max_rows`.
    pub max_rows: Option<usize>,
    /// Per-pipeline rate limit (rows per second). Overrides the default
    /// `IndexerConfig::max_rows_per_second` when set.
    pub max_rows_per_second: Option<u64>,
}

impl SequentialLayer {
    pub(crate) fn max_rows_or_default(&self) -> usize {
        self.max_rows.unwrap_or(DEFAULT_MAX_ROWS_PER_BIGTABLE_BATCH)
    }

    pub fn finish(self, base: ConcurrentConfig) -> SequentialConfig {
        let committer = if let Some(c) = self.committer {
            c.finish(base.committer)
        } else {
            base.committer
        };
        SequentialConfig {
            committer,
            ingestion: if let Some(i) = self.ingestion {
                i.finish(base.ingestion)
            } else {
                base.ingestion
            },
            fanout: self.fanout.or(base.fanout),
            min_eager_rows: self.min_eager_rows.or(base.min_eager_rows),
            max_pending_rows: self.max_pending_rows.or(base.max_pending_rows),
            max_batch_checkpoints: self.max_batch_checkpoints,
            processor_channel_size: self.processor_channel_size.or(base.processor_channel_size),
            pipeline_depth: self.pipeline_depth,
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
    pub packages: ConcurrentLayer,
    pub packages_by_id: ConcurrentLayer,
    pub packages_by_checkpoint: ConcurrentLayer,
    pub system_packages: ConcurrentLayer,
    pub tx_seq_digest: ConcurrentLayer,
    pub transaction_bitmap_index: SequentialLayer,
    pub event_bitmap_index: SequentialLayer,
    pub checkpoint_bitmap_index: SequentialLayer,
}

/// This type is identical to [`framework::ingestion::IngestionConfig`], but is set-up to be
/// serialized and deserialized by `serde`.
#[DefaultConfig]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct IngestionConfig {
    pub ingest_concurrency: framework::config::ConcurrencyConfig,
    pub retry_interval_ms: u64,
    pub streaming_backoff_initial_batch_size: usize,
    pub streaming_backoff_max_batch_size: usize,
    pub streaming_connection_timeout_ms: u64,
    pub streaming_statement_timeout_ms: u64,

    /// Deprecated: accepted (and ignored) so old configs don't fail to parse. Replaced by
    /// per-pipeline `ingestion.subscriber-channel-size`.
    pub checkpoint_buffer_size: Option<usize>,
}

impl Default for IngestionConfig {
    fn default() -> Self {
        framework::ingestion::IngestionConfig::default().into()
    }
}

impl From<framework::ingestion::IngestionConfig> for IngestionConfig {
    fn from(config: framework::ingestion::IngestionConfig) -> Self {
        Self {
            ingest_concurrency: config.ingest_concurrency,
            retry_interval_ms: config.retry_interval_ms,
            streaming_backoff_initial_batch_size: config.streaming_backoff_initial_batch_size,
            streaming_backoff_max_batch_size: config.streaming_backoff_max_batch_size,
            streaming_connection_timeout_ms: config.streaming_connection_timeout_ms,
            streaming_statement_timeout_ms: config.streaming_statement_timeout_ms,
            checkpoint_buffer_size: None,
        }
    }
}

impl From<IngestionConfig> for framework::ingestion::IngestionConfig {
    fn from(config: IngestionConfig) -> Self {
        if config.checkpoint_buffer_size.is_some() {
            warn!(
                "Config field `checkpoint-buffer-size` is deprecated and ignored. Remove it from \
                 your config; set `subscriber-channel-size` under each pipeline's `ingestion` \
                 section if you need to override the default."
            );
        }

        framework::ingestion::IngestionConfig {
            ingest_concurrency: config.ingest_concurrency,
            retry_interval_ms: config.retry_interval_ms,
            streaming_backoff_initial_batch_size: config.streaming_backoff_initial_batch_size,
            streaming_backoff_max_batch_size: config.streaming_backoff_max_batch_size,
            streaming_connection_timeout_ms: config.streaming_connection_timeout_ms,
            streaming_statement_timeout_ms: config.streaming_statement_timeout_ms,
        }
    }
}
