// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_default_config::DefaultConfig;
use sui_indexer_alt_framework::{self as framework, pipeline::CommitterConfig};

use crate::DbConfig;

#[DefaultConfig]
#[derive(Default)]
#[serde(deny_unknown_fields)]
pub struct ServiceConfig {
    /// How checkpoints are read by the indexer.
    pub ingestion: IngestionConfig,

    /// The size and density of the consistent range.
    pub consistency: ConsistencyConfig,

    /// Parameters for the database.
    pub rocksdb: DbConfig,

    /// Default configuration for committers that is shared by all pipelines. Pipelines can
    /// override individual settings in their own configuration sections.
    pub committer: CommitterLayer,

    /// Per-pipeline configuration.
    pub pipeline: PipelineLayer,
}

/// This type is identical to [`framework::ingestion::IngestionConfig`], but is set-up to be
/// serialized and deserialized by `serde`.
#[DefaultConfig]
#[serde(deny_unknown_fields)]
pub struct IngestionConfig {
    pub checkpoint_buffer_size: usize,
    pub ingest_concurrency: usize,
    pub retry_interval_ms: u64,
}

#[DefaultConfig]
#[serde(deny_unknown_fields)]
pub struct ConsistencyConfig {
    /// The number of snapshots to keep in the buffer.
    pub snapshots: u64,

    /// The stride between checkpoints.
    pub stride: u64,

    /// The size of the buffer for storing checkpoints.
    pub buffer_size: usize,
}

#[DefaultConfig]
#[derive(Default)]
pub struct PipelineLayer {
    pub object_by_owner: Option<CommitterLayer>,
}

#[DefaultConfig]
#[derive(Default)]
#[serde(deny_unknown_fields)]
pub struct CommitterLayer {
    pub write_concurrency: Option<usize>,
    pub collect_interval_ms: Option<u64>,
    pub watermark_interval_ms: Option<u64>,
}

impl ServiceConfig {
    /// Generate an example configuration, suitable for demonstrating the fields available to
    /// configure.
    pub fn example() -> Self {
        let mut example = Self::default();

        example.committer = CommitterConfig::default().into();
        example.pipeline = PipelineLayer::example();

        example
    }
}

impl PipelineLayer {
    /// Generate an example configuration, suitable for demonstrating the fields available to
    /// configure.
    pub fn example() -> Self {
        Self {
            object_by_owner: Some(Default::default()),
        }
    }
}

impl CommitterLayer {
    pub fn finish(self, base: CommitterConfig) -> CommitterConfig {
        CommitterConfig {
            write_concurrency: self.write_concurrency.unwrap_or(base.write_concurrency),
            collect_interval_ms: self.collect_interval_ms.unwrap_or(base.collect_interval_ms),
            watermark_interval_ms: self
                .watermark_interval_ms
                .unwrap_or(base.watermark_interval_ms),
        }
    }
}

impl From<framework::ingestion::IngestionConfig> for IngestionConfig {
    fn from(config: framework::ingestion::IngestionConfig) -> Self {
        Self {
            checkpoint_buffer_size: config.checkpoint_buffer_size,
            ingest_concurrency: config.ingest_concurrency,
            retry_interval_ms: config.retry_interval_ms,
        }
    }
}

impl From<IngestionConfig> for framework::ingestion::IngestionConfig {
    fn from(config: IngestionConfig) -> Self {
        framework::ingestion::IngestionConfig {
            checkpoint_buffer_size: config.checkpoint_buffer_size,
            ingest_concurrency: config.ingest_concurrency,
            retry_interval_ms: config.retry_interval_ms,
        }
    }
}

impl From<CommitterConfig> for CommitterLayer {
    fn from(config: CommitterConfig) -> Self {
        Self {
            write_concurrency: Some(config.write_concurrency),
            collect_interval_ms: Some(config.collect_interval_ms),
            watermark_interval_ms: Some(config.watermark_interval_ms),
        }
    }
}

impl Default for IngestionConfig {
    fn default() -> Self {
        framework::ingestion::IngestionConfig::default().into()
    }
}

impl Default for ConsistencyConfig {
    fn default() -> Self {
        Self {
            snapshots: 15000,
            stride: 1,
            buffer_size: 5000,
        }
    }
}
