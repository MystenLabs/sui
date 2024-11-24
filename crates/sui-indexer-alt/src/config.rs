// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//

use sui_default_config::DefaultConfig;

use crate::{
    ingestion::IngestionConfig,
    pipeline::{concurrent::ConcurrentConfig, sequential::SequentialConfig, CommitterConfig},
};

#[DefaultConfig]
#[derive(Clone, Default)]
pub struct IndexerConfig {
    /// How checkpoints are read by the indexer.
    pub ingestion: IngestionConfig,

    /// How wide the consistent read range is.
    pub consistency: ConsistencyConfig,

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

#[DefaultConfig]
#[derive(Clone, Default)]
#[serde(rename_all = "snake_case")]
pub struct PipelineConfig {
    // Consistent pipelines (a sequential pipeline with a write-ahead log)
    pub sum_coin_balances: CommitterConfig,
    pub wal_coin_balances: CommitterConfig,
    pub sum_obj_types: CommitterConfig,
    pub wal_obj_types: CommitterConfig,

    // Sequential pipelines without a write-ahead log
    pub sum_displays: SequentialConfig,
    pub sum_packages: SequentialConfig,

    // All concurrent pipelines
    pub ev_emit_mod: ConcurrentConfig,
    pub ev_struct_inst: ConcurrentConfig,
    pub kv_checkpoints: ConcurrentConfig,
    pub kv_epoch_ends: ConcurrentConfig,
    pub kv_epoch_starts: ConcurrentConfig,
    pub kv_feature_flags: ConcurrentConfig,
    pub kv_objects: ConcurrentConfig,
    pub kv_protocol_configs: ConcurrentConfig,
    pub kv_transactions: ConcurrentConfig,
    pub obj_versions: ConcurrentConfig,
    pub tx_affected_addresses: ConcurrentConfig,
    pub tx_affected_objects: ConcurrentConfig,
    pub tx_balance_changes: ConcurrentConfig,
    pub tx_calls: ConcurrentConfig,
    pub tx_digests: ConcurrentConfig,
    pub tx_kinds: ConcurrentConfig,
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
