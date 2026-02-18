// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_default_config::DefaultConfig;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_indexer_alt_framework::{self as framework};

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct IndexerConfig {
    pub ingestion: IngestionConfig,
    pub committer: CommitterLayer,
    pub pipeline: PipelineLayer,
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
}

/// This type is identical to [`framework::ingestion::IngestionConfig`], but is set-up to be
/// serialized and deserialized by `serde`.
#[DefaultConfig]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct IngestionConfig {
    pub checkpoint_buffer_size: usize,
    pub ingest_concurrency: usize,
    pub retry_interval_ms: u64,
    pub streaming_backoff_initial_batch_size: usize,
    pub streaming_backoff_max_batch_size: usize,
    pub streaming_connection_timeout_ms: u64,
    pub streaming_statement_timeout_ms: u64,
    pub checkpoint_channel_size: Option<usize>,
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
            checkpoint_channel_size: config.checkpoint_channel_size,
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
            checkpoint_channel_size: config.checkpoint_channel_size,
        }
    }
}
