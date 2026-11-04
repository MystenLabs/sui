// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroUsize;

use serde::Deserialize;
use serde::Serialize;
use sui_indexer_alt_framework as framework;
use sui_indexer_alt_framework::config::ConcurrencyConfig;
use sui_indexer_alt_framework::config::warn_ignored_config_key;
use sui_indexer_alt_framework::pipeline;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use tracing::warn;

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct IndexerConfig {
    pub ingestion: IngestionConfig,
    pub committer: CommitterLayer,
    pub pipeline: PipelineLayer,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CommitterLayer {
    pub write_concurrency: Option<usize>,
    pub watermark_interval_ms: Option<u64>,
    pub watermark_interval_jitter_ms: Option<u64>,

    /// Minimum number of pending rows the collector waits for before cutting a batch. `0` cuts a
    /// batch as soon as the committer has room for it.
    pub min_batch_rows: Option<usize>,

    /// Longest time, in milliseconds, the oldest pending row waits for `min-batch-rows` to
    /// accumulate before a batch is cut anyway.
    pub max_batch_delay_ms: Option<u64>,

    /// Deprecated and ignored: batches are cut as soon as the committer has room; see
    /// `min-batch-rows` and `max-batch-delay-ms`.
    pub collect_interval_ms: Option<u64>,
}

impl CommitterLayer {
    pub fn finish(self, base: CommitterConfig) -> CommitterConfig {
        if self.collect_interval_ms.is_some() {
            warn_ignored_config_key("collect-interval-ms");
        }

        CommitterConfig {
            write_concurrency: self.write_concurrency.unwrap_or(base.write_concurrency),
            watermark_interval_ms: self
                .watermark_interval_ms
                .unwrap_or(base.watermark_interval_ms),
            watermark_interval_jitter_ms: self
                .watermark_interval_jitter_ms
                .unwrap_or(base.watermark_interval_jitter_ms),
            min_batch_rows: self.min_batch_rows.unwrap_or(base.min_batch_rows),
            max_batch_delay_ms: self.max_batch_delay_ms.unwrap_or(base.max_batch_delay_ms),
        }
    }
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ConcurrentLayer {
    pub committer: Option<CommitterLayer>,
    pub ingestion: Option<PipelineIngestionLayer>,
    pub fanout: Option<ConcurrencyConfig>,
    /// Deprecated and ignored: batch size is controlled by the committer's `min-batch-rows` and
    /// `max-batch-delay-ms`.
    pub min_eager_rows: Option<usize>,
    pub max_pending_rows: Option<usize>,
    pub max_watermark_updates: Option<usize>,
    pub processor_channel_size: Option<usize>,
    pub collector_channel_size: Option<usize>,
    pub committer_channel_size: Option<usize>,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct PipelineIngestionLayer {
    pub subscriber_channel_size: Option<usize>,
}

impl ConcurrentLayer {
    pub fn finish(self, base: ConcurrentConfig) -> ConcurrentConfig {
        if self.min_eager_rows.is_some() {
            warn_ignored_config_key("min-eager-rows");
        }

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

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct PipelineLayer {
    pub checkpoint_blob: ConcurrentLayer,
    pub epochs: ConcurrentLayer,
    pub checkpoint_bcs: ConcurrentLayer,
}

/// This type is identical to [`framework::ingestion::IngestionConfig`], but is set-up to be
/// serialized and deserialized by `serde`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct IngestionConfig {
    pub ingest_concurrency: framework::config::ConcurrencyConfig,
    pub retry_interval_ms: u64,
    pub streaming_backoff_initial_batch_size: NonZeroUsize,
    pub streaming_backoff_max_batch_size: usize,
    pub streaming_connection_timeout_ms: u64,
    pub streaming_statement_timeout_ms: u64,
    pub min_cohort_boundary: u64,

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
            min_cohort_boundary: config.min_cohort_boundary,
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
            min_cohort_boundary: config.min_cohort_boundary,
        }
    }
}
