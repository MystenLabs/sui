// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//

use serde::{Deserialize, Serialize};
use sui_default_config::DefaultConfig;

use crate::{
    ingestion::IngestionConfig,
    pipeline::{
        concurrent::{ConcurrentConfig, PrunerConfig},
        sequential::SequentialConfig,
        CommitterConfig,
    },
};

#[DefaultConfig]
#[derive(Clone, Default)]
pub struct IndexerConfig {
    /// How checkpoints are read by the indexer.
    pub ingestion: IngestionLayer,

    /// How wide the consistent read range is.
    pub consistency: ConsistencyLayer,

    /// Default configuration for committers that is shared by all pipelines. Pipelines can
    /// override individual settings in their own configuration sections.
    pub committer: CommitterLayer,

    /// Per-pipeline configurations.
    pub pipeline: PipelineLayer,
}

#[DefaultConfig]
#[derive(Clone)]
pub struct ConsistencyConfig {
    /// How often to check whether write-ahead logs related to the consistent range can be
    /// pruned.
    pub consistent_pruning_interval_ms: u64,

    /// How long to wait before honouring reader low watermarks.
    pub pruner_delay_ms: u64,

    /// Number of checkpoints to delay indexing summary tables for.
    pub consistent_range: Option<u64>,
}

// Configuration layers apply overrides over a base configuration. When reading configs from a
// file, we read them into layer types, and then apply those layers onto an existing configuration
// (such as the default configuration) to `finish()` them.
//
// Treating configs as layers allows us to support configuration merging, where multiple
// configuration files can be combined into one final configuration.

#[DefaultConfig]
#[derive(Clone, Default)]
pub struct IngestionLayer {
    pub checkpoint_buffer_size: Option<usize>,
    pub ingest_concurrency: Option<usize>,
    pub retry_interval_ms: Option<u64>,
}

#[DefaultConfig]
#[derive(Clone, Default)]
pub struct ConsistencyLayer {
    consistent_pruning_interval_ms: Option<u64>,
    pruner_delay_ms: Option<u64>,
    consistent_range: Option<u64>,
}

#[DefaultConfig]
#[derive(Clone, Default)]
pub struct SequentialLayer {
    committer: Option<CommitterLayer>,
    checkpoint_lag: Option<u64>,
}

#[DefaultConfig]
#[derive(Clone, Default)]
pub struct ConcurrentLayer {
    committer: Option<CommitterLayer>,
    pruner: Option<PrunerLayer>,
}

#[DefaultConfig]
#[derive(Clone, Default)]
pub struct CommitterLayer {
    write_concurrency: Option<usize>,
    collect_interval_ms: Option<u64>,
    watermark_interval_ms: Option<u64>,
}

/// PrunerLayer is special in that its fields are not optional -- a layer needs to specify all or
/// none of the values, this means it has the same shape as [PrunerConfig], but we define it as its
/// own type so that it can implement the deserialization logic necessary for being read from a
/// TOML file.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PrunerLayer {
    pub interval_ms: u64,
    pub delay_ms: u64,
    pub retention: u64,
    pub max_chunk_size: u64,
}

#[DefaultConfig]
#[derive(Clone, Default)]
#[serde(rename_all = "snake_case")]
pub struct PipelineLayer {
    // Consistent pipelines (a sequential pipeline with a write-ahead log)
    pub sum_coin_balances: Option<CommitterLayer>,
    pub wal_coin_balances: Option<CommitterLayer>,
    pub sum_obj_types: Option<CommitterLayer>,
    pub wal_obj_types: Option<CommitterLayer>,

    // Sequential pipelines without a write-ahead log
    pub sum_displays: Option<SequentialLayer>,
    pub sum_packages: Option<SequentialLayer>,

    // All concurrent pipelines
    pub ev_emit_mod: Option<ConcurrentLayer>,
    pub ev_struct_inst: Option<ConcurrentLayer>,
    pub kv_checkpoints: Option<ConcurrentLayer>,
    pub kv_epoch_ends: Option<ConcurrentLayer>,
    pub kv_epoch_starts: Option<ConcurrentLayer>,
    pub kv_feature_flags: Option<ConcurrentLayer>,
    pub kv_objects: Option<ConcurrentLayer>,
    pub kv_protocol_configs: Option<ConcurrentLayer>,
    pub kv_transactions: Option<ConcurrentLayer>,
    pub obj_versions: Option<ConcurrentLayer>,
    pub tx_affected_addresses: Option<ConcurrentLayer>,
    pub tx_affected_objects: Option<ConcurrentLayer>,
    pub tx_balance_changes: Option<ConcurrentLayer>,
    pub tx_calls: Option<ConcurrentLayer>,
    pub tx_digests: Option<ConcurrentLayer>,
    pub tx_kinds: Option<ConcurrentLayer>,

    /// A catch all value to detect incorrectly labelled pipelines. If this is not empty, we will
    /// produce an error.
    #[serde(flatten)]
    pub extra: toml::Table,
}

impl IngestionLayer {
    pub fn finish(self, base: IngestionConfig) -> IngestionConfig {
        IngestionConfig {
            checkpoint_buffer_size: self
                .checkpoint_buffer_size
                .unwrap_or(base.checkpoint_buffer_size),
            ingest_concurrency: self.ingest_concurrency.unwrap_or(base.ingest_concurrency),
            retry_interval_ms: self.retry_interval_ms.unwrap_or(base.retry_interval_ms),
        }
    }
}

impl ConsistencyLayer {
    pub fn finish(self, base: ConsistencyConfig) -> ConsistencyConfig {
        ConsistencyConfig {
            consistent_pruning_interval_ms: self
                .consistent_pruning_interval_ms
                .unwrap_or(base.consistent_pruning_interval_ms),
            pruner_delay_ms: self.pruner_delay_ms.unwrap_or(base.pruner_delay_ms),
            consistent_range: self.consistent_range.or(base.consistent_range),
        }
    }
}

impl SequentialLayer {
    pub fn finish(self, base: SequentialConfig) -> SequentialConfig {
        SequentialConfig {
            committer: if let Some(committer) = self.committer {
                committer.finish(base.committer)
            } else {
                base.committer
            },
            checkpoint_lag: self.checkpoint_lag.unwrap_or(base.checkpoint_lag),
        }
    }
}

impl ConcurrentLayer {
    pub fn finish(self, base: ConcurrentConfig) -> ConcurrentConfig {
        ConcurrentConfig {
            committer: if let Some(committer) = self.committer {
                committer.finish(base.committer)
            } else {
                base.committer
            },
            // If the layer defines a pruner config, it takes precedence.
            pruner: self.pruner.map(Into::into).or(base.pruner),
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

impl Default for ConsistencyConfig {
    fn default() -> Self {
        Self {
            consistent_pruning_interval_ms: 300_000,
            pruner_delay_ms: 120_000,
            consistent_range: None,
        }
    }
}

// Planning for these types to be in different crates from each other in the long-run, so use
// `Into` rather than `From`.
#[allow(clippy::from_over_into)]
impl Into<PrunerConfig> for PrunerLayer {
    fn into(self) -> PrunerConfig {
        PrunerConfig {
            interval_ms: self.interval_ms,
            delay_ms: self.delay_ms,
            retention: self.retention,
            max_chunk_size: self.max_chunk_size,
        }
    }
}
