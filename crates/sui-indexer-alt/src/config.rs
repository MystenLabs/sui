// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//

use sui_default_config::DefaultConfig;

use crate::{
    ingestion::IngestionConfig,
    pipeline::{
        concurrent::{ConcurrentConfig, PrunerConfig},
        sequential::SequentialConfig,
        CommitterConfig, CommitterLayer,
    },
};

#[DefaultConfig]
#[derive(Clone, Default)]
pub struct IndexerConfig {
    /// How checkpoints are read by the indexer.
    pub ingestion: IngestionConfig,

    /// How wide the consistent read range is.
    pub consistency: ConsistencyConfig,

    /// Default configuration for committers that is shared by all pipelines. Pipelines can
    /// override individual settings in their own configuration sections.
    pub committer: CommitterConfig,

    /// Per-pipeline configurations.
    pub pipeline: PipelineConfig,
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

/// A layer of overrides on top of an existing [SequentialConfig]. In particular, the pipeline's
/// committer configuration is defined as overrides on top of a base configuration.
#[DefaultConfig]
#[derive(Clone, Default)]
pub struct SequentialLayer {
    committer: Option<CommitterLayer>,
    checkpoint_lag: Option<u64>,
}

/// A layer of overrides on top of an existing [ConcurrentConfig]. In particular, the pipeline's
/// committer configuration is defined as overrides on top of a base configuration.
#[DefaultConfig]
#[derive(Clone, Default)]
pub struct ConcurrentLayer {
    committer: Option<CommitterLayer>,
    pruner: Option<PrunerConfig>,
}

#[DefaultConfig]
#[derive(Clone, Default)]
#[serde(rename_all = "snake_case")]
pub struct PipelineConfig {
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
}

impl SequentialLayer {
    /// Apply the overrides in this layer on top of the base `committer` configuration, and return
    /// the result.
    pub fn finish(self, committer: &CommitterConfig) -> SequentialConfig {
        SequentialConfig {
            committer: self
                .committer
                .map_or_else(|| committer.clone(), |l| l.finish(committer)),
            checkpoint_lag: self.checkpoint_lag,
        }
    }
}

impl ConcurrentLayer {
    /// Apply the overrides in this layer on top of the base `committer` configuration, and return
    /// the result.
    pub fn finish(self, committer: &CommitterConfig) -> ConcurrentConfig {
        ConcurrentConfig {
            committer: self
                .committer
                .map_or_else(|| committer.clone(), |l| l.finish(committer)),
            pruner: self.pruner,
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
