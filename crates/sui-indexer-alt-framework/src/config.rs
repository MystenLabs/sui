// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;
use serde::Serialize;
use sui_concurrency_limiter::ConcurrencyLimit;

use crate::ingestion::ClientArgs;
use crate::ingestion::IngestionConfig;
use crate::pipeline::CommitterConfig;

/// Trait for merging configuration structs together.
pub trait Merge: Sized {
    fn merge(self, other: Self) -> anyhow::Result<Self>;
}

impl<T: Merge> Merge for Option<T> {
    fn merge(self, other: Option<T>) -> anyhow::Result<Option<T>> {
        Ok(match (self, other) {
            (Some(a), Some(b)) => Some(a.merge(b)?),
            (Some(a), _) | (_, Some(a)) => Some(a),
            (None, None) => None,
        })
    }
}

/// A configuration layer for `CommitterConfig` where every field is optional. Used to merge
/// user-provided overrides on top of mode-aware defaults.
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct CommitterLayer {
    pub write_concurrency: Option<ConcurrencyLimit>,
    pub collect_interval_ms: Option<u64>,
    pub watermark_interval_ms: Option<u64>,
    pub watermark_interval_jitter_ms: Option<u64>,
}

impl CommitterLayer {
    /// Resolve this layer into a concrete config. Mode-aware defaults are inferred from the client
    /// args (remote store vs full node).
    pub fn finish(self, client_args: &ClientArgs) -> CommitterConfig {
        let base = CommitterConfig::for_mode(client_args.ingestion_mode());
        self.finish_with_base(base)
    }

    /// Resolve this layer against an already-resolved base config. Used for per-pipeline overrides
    /// where the base committer has already been resolved from mode-aware defaults.
    pub fn finish_with_base(self, base: CommitterConfig) -> CommitterConfig {
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

impl Merge for CommitterLayer {
    fn merge(self, other: CommitterLayer) -> anyhow::Result<CommitterLayer> {
        Ok(CommitterLayer {
            write_concurrency: other.write_concurrency.or(self.write_concurrency),
            collect_interval_ms: other.collect_interval_ms.or(self.collect_interval_ms),
            watermark_interval_ms: other.watermark_interval_ms.or(self.watermark_interval_ms),
            watermark_interval_jitter_ms: other
                .watermark_interval_jitter_ms
                .or(self.watermark_interval_jitter_ms),
        })
    }
}

impl From<CommitterConfig> for CommitterLayer {
    fn from(config: CommitterConfig) -> Self {
        Self {
            write_concurrency: Some(config.write_concurrency),
            collect_interval_ms: Some(config.collect_interval_ms),
            watermark_interval_ms: Some(config.watermark_interval_ms),
            watermark_interval_jitter_ms: Some(config.watermark_interval_jitter_ms),
        }
    }
}

/// A configuration layer for `IngestionConfig` where every field is optional. Used to merge
/// user-provided overrides on top of mode-aware defaults.
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct IngestionLayer {
    pub checkpoint_buffer_size: Option<usize>,
    pub subscriber_channel_size: Option<usize>,
    pub ingest_concurrency: Option<ConcurrencyLimit>,
    pub retry_interval_ms: Option<u64>,
    pub streaming_backoff_initial_batch_size: Option<usize>,
    pub streaming_backoff_max_batch_size: Option<usize>,
    pub streaming_connection_timeout_ms: Option<u64>,
    pub streaming_statement_timeout_ms: Option<u64>,
    pub max_pending_rows: Option<Option<usize>>,
}

impl IngestionLayer {
    /// Resolve this layer into a concrete config. Mode-aware defaults are inferred from the client
    /// args (remote store vs full node).
    pub fn finish(self, client_args: &ClientArgs) -> IngestionConfig {
        let base = IngestionConfig::for_mode(client_args.ingestion_mode());
        IngestionConfig {
            checkpoint_buffer_size: self
                .checkpoint_buffer_size
                .unwrap_or(base.checkpoint_buffer_size),
            subscriber_channel_size: self
                .subscriber_channel_size
                .unwrap_or(base.subscriber_channel_size),
            ingest_concurrency: self.ingest_concurrency.unwrap_or(base.ingest_concurrency),
            retry_interval_ms: self.retry_interval_ms.unwrap_or(base.retry_interval_ms),
            streaming_backoff_initial_batch_size: self
                .streaming_backoff_initial_batch_size
                .unwrap_or(base.streaming_backoff_initial_batch_size),
            streaming_backoff_max_batch_size: self
                .streaming_backoff_max_batch_size
                .unwrap_or(base.streaming_backoff_max_batch_size),
            streaming_connection_timeout_ms: self
                .streaming_connection_timeout_ms
                .unwrap_or(base.streaming_connection_timeout_ms),
            streaming_statement_timeout_ms: self
                .streaming_statement_timeout_ms
                .unwrap_or(base.streaming_statement_timeout_ms),
            max_pending_rows: self.max_pending_rows.unwrap_or(base.max_pending_rows),
        }
    }
}

impl Merge for IngestionLayer {
    fn merge(self, other: IngestionLayer) -> anyhow::Result<IngestionLayer> {
        Ok(IngestionLayer {
            checkpoint_buffer_size: other.checkpoint_buffer_size.or(self.checkpoint_buffer_size),
            subscriber_channel_size: other
                .subscriber_channel_size
                .or(self.subscriber_channel_size),
            ingest_concurrency: other.ingest_concurrency.or(self.ingest_concurrency),
            retry_interval_ms: other.retry_interval_ms.or(self.retry_interval_ms),
            streaming_backoff_initial_batch_size: other
                .streaming_backoff_initial_batch_size
                .or(self.streaming_backoff_initial_batch_size),
            streaming_backoff_max_batch_size: other
                .streaming_backoff_max_batch_size
                .or(self.streaming_backoff_max_batch_size),
            streaming_connection_timeout_ms: other
                .streaming_connection_timeout_ms
                .or(self.streaming_connection_timeout_ms),
            streaming_statement_timeout_ms: other
                .streaming_statement_timeout_ms
                .or(self.streaming_statement_timeout_ms),
            max_pending_rows: other.max_pending_rows.or(self.max_pending_rows),
        })
    }
}

impl From<IngestionConfig> for IngestionLayer {
    fn from(config: IngestionConfig) -> Self {
        Self {
            checkpoint_buffer_size: Some(config.checkpoint_buffer_size),
            subscriber_channel_size: Some(config.subscriber_channel_size),
            ingest_concurrency: Some(config.ingest_concurrency),
            retry_interval_ms: Some(config.retry_interval_ms),
            streaming_backoff_initial_batch_size: Some(config.streaming_backoff_initial_batch_size),
            streaming_backoff_max_batch_size: Some(config.streaming_backoff_max_batch_size),
            streaming_connection_timeout_ms: Some(config.streaming_connection_timeout_ms),
            streaming_statement_timeout_ms: Some(config.streaming_statement_timeout_ms),
            max_pending_rows: Some(config.max_pending_rows),
        }
    }
}
