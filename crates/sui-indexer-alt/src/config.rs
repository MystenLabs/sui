// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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

    /// Default configuration for pruners that is shared by all concurrent pipelines. Pipelies can
    /// override individual settings in their own configuration sections. Concurrent pipelines
    /// still need to specify a pruner configuration (although it can be empty) to indicate that
    /// they want to enable pruning, but when they do, any missing values will be filled in by this
    /// config.
    pub pruner: PrunerLayer,

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
#[derive(Clone, Default, Debug)]
pub struct IngestionLayer {
    pub checkpoint_buffer_size: Option<usize>,
    pub ingest_concurrency: Option<usize>,
    pub retry_interval_ms: Option<u64>,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct ConsistencyLayer {
    consistent_pruning_interval_ms: Option<u64>,
    pruner_delay_ms: Option<u64>,
    consistent_range: Option<u64>,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct SequentialLayer {
    committer: Option<CommitterLayer>,
    checkpoint_lag: Option<u64>,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct ConcurrentLayer {
    committer: Option<CommitterLayer>,
    pruner: Option<PrunerLayer>,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct CommitterLayer {
    write_concurrency: Option<usize>,
    collect_interval_ms: Option<u64>,
    watermark_interval_ms: Option<u64>,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct PrunerLayer {
    pub interval_ms: Option<u64>,
    pub delay_ms: Option<u64>,
    pub retention: Option<u64>,
    pub max_chunk_size: Option<u64>,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
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

macro_rules! merge_recursive {
    ($self:expr, $other:expr) => {
        match ($self, $other) {
            (Some(a), Some(b)) => Some(a.merge(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    };
}

impl IndexerConfig {
    pub fn merge(self, other: IndexerConfig) -> IndexerConfig {
        IndexerConfig {
            ingestion: self.ingestion.merge(other.ingestion),
            consistency: self.consistency.merge(other.consistency),
            committer: self.committer.merge(other.committer),
            pruner: self.pruner.merge(other.pruner),
            pipeline: self.pipeline.merge(other.pipeline),
        }
    }
}

impl IngestionLayer {
    pub fn merge(self, other: IngestionLayer) -> IngestionLayer {
        IngestionLayer {
            checkpoint_buffer_size: other.checkpoint_buffer_size.or(self.checkpoint_buffer_size),
            ingest_concurrency: other.ingest_concurrency.or(self.ingest_concurrency),
            retry_interval_ms: other.retry_interval_ms.or(self.retry_interval_ms),
        }
    }

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
    pub fn merge(self, other: ConsistencyLayer) -> ConsistencyLayer {
        ConsistencyLayer {
            consistent_pruning_interval_ms: other
                .consistent_pruning_interval_ms
                .or(self.consistent_pruning_interval_ms),
            pruner_delay_ms: other.pruner_delay_ms.or(self.pruner_delay_ms),
            consistent_range: other.consistent_range.or(self.consistent_range),
        }
    }

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
    pub fn merge(self, other: SequentialLayer) -> SequentialLayer {
        SequentialLayer {
            committer: merge_recursive!(self.committer, other.committer),
            checkpoint_lag: other.checkpoint_lag.or(self.checkpoint_lag),
        }
    }

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
    pub fn merge(self, other: ConcurrentLayer) -> ConcurrentLayer {
        ConcurrentLayer {
            committer: merge_recursive!(self.committer, other.committer),
            pruner: merge_recursive!(self.pruner, other.pruner),
        }
    }

    /// Unlike other parameters, `pruner` will appear in the finished configuration only if they
    /// appear in the layer *and* in the base.
    pub fn finish(self, base: ConcurrentConfig) -> ConcurrentConfig {
        ConcurrentConfig {
            committer: if let Some(committer) = self.committer {
                committer.finish(base.committer)
            } else {
                base.committer
            },
            pruner: match (self.pruner, base.pruner) {
                (None, _) | (_, None) => None,
                (Some(pruner), Some(base)) => Some(pruner.finish(base)),
            },
        }
    }
}

impl CommitterLayer {
    pub fn merge(self, other: CommitterLayer) -> CommitterLayer {
        CommitterLayer {
            write_concurrency: other.write_concurrency.or(self.write_concurrency),
            collect_interval_ms: other.collect_interval_ms.or(self.collect_interval_ms),
            watermark_interval_ms: other.watermark_interval_ms.or(self.watermark_interval_ms),
        }
    }

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

impl PrunerLayer {
    /// Last write takes precedence for all fields except the `retention`, which takes the max of
    /// all available values.
    pub fn merge(self, other: PrunerLayer) -> PrunerLayer {
        PrunerLayer {
            interval_ms: other.interval_ms.or(self.interval_ms),
            delay_ms: other.delay_ms.or(self.delay_ms),
            retention: match (other.retention, self.retention) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (Some(a), _) | (_, Some(a)) => Some(a),
                (None, None) => None,
            },
            max_chunk_size: other.max_chunk_size.or(self.max_chunk_size),
        }
    }

    pub fn finish(self, base: PrunerConfig) -> PrunerConfig {
        PrunerConfig {
            interval_ms: self.interval_ms.unwrap_or(base.interval_ms),
            delay_ms: self.delay_ms.unwrap_or(base.delay_ms),
            retention: self.retention.unwrap_or(base.retention),
            max_chunk_size: self.max_chunk_size.unwrap_or(base.max_chunk_size),
        }
    }
}

impl PipelineLayer {
    pub fn merge(self, other: PipelineLayer) -> PipelineLayer {
        PipelineLayer {
            sum_coin_balances: merge_recursive!(self.sum_coin_balances, other.sum_coin_balances),
            wal_coin_balances: merge_recursive!(self.wal_coin_balances, other.wal_coin_balances),
            sum_obj_types: merge_recursive!(self.sum_obj_types, other.sum_obj_types),
            wal_obj_types: merge_recursive!(self.wal_obj_types, other.wal_obj_types),
            sum_displays: merge_recursive!(self.sum_displays, other.sum_displays),
            sum_packages: merge_recursive!(self.sum_packages, other.sum_packages),
            ev_emit_mod: merge_recursive!(self.ev_emit_mod, other.ev_emit_mod),
            ev_struct_inst: merge_recursive!(self.ev_struct_inst, other.ev_struct_inst),
            kv_checkpoints: merge_recursive!(self.kv_checkpoints, other.kv_checkpoints),
            kv_epoch_ends: merge_recursive!(self.kv_epoch_ends, other.kv_epoch_ends),
            kv_epoch_starts: merge_recursive!(self.kv_epoch_starts, other.kv_epoch_starts),
            kv_feature_flags: merge_recursive!(self.kv_feature_flags, other.kv_feature_flags),
            kv_objects: merge_recursive!(self.kv_objects, other.kv_objects),
            kv_protocol_configs: merge_recursive!(
                self.kv_protocol_configs,
                other.kv_protocol_configs
            ),
            kv_transactions: merge_recursive!(self.kv_transactions, other.kv_transactions),
            obj_versions: merge_recursive!(self.obj_versions, other.obj_versions),
            tx_affected_addresses: merge_recursive!(
                self.tx_affected_addresses,
                other.tx_affected_addresses
            ),
            tx_affected_objects: merge_recursive!(
                self.tx_affected_objects,
                other.tx_affected_objects
            ),
            tx_balance_changes: merge_recursive!(self.tx_balance_changes, other.tx_balance_changes),
            tx_calls: merge_recursive!(self.tx_calls, other.tx_calls),
            tx_digests: merge_recursive!(self.tx_digests, other.tx_digests),
            tx_kinds: merge_recursive!(self.tx_kinds, other.tx_kinds),
            extra: if self.extra.is_empty() {
                other.extra
            } else {
                self.extra
            },
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

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! assert_matches {
        ($value:expr, $pattern:pat $(,)?) => {
            let value = $value;
            assert!(
                matches!(value, $pattern),
                "Did not match pattern:\nexpected: {}\nactual: {value:#?}",
                stringify!($pattern)
            );
        };
    }

    #[test]
    fn merge_simple() {
        let this = ConsistencyLayer {
            consistent_pruning_interval_ms: None,
            pruner_delay_ms: Some(2000),
            consistent_range: Some(3000),
        };

        let that = ConsistencyLayer {
            consistent_pruning_interval_ms: Some(1000),
            pruner_delay_ms: None,
            consistent_range: Some(4000),
        };

        let this_then_that = this.clone().merge(that.clone());
        let that_then_this = that.clone().merge(this.clone());

        assert_matches!(
            this_then_that,
            ConsistencyLayer {
                consistent_pruning_interval_ms: Some(1000),
                pruner_delay_ms: Some(2000),
                consistent_range: Some(4000),
            }
        );

        assert_matches!(
            that_then_this,
            ConsistencyLayer {
                consistent_pruning_interval_ms: Some(1000),
                pruner_delay_ms: Some(2000),
                consistent_range: Some(3000),
            }
        );
    }

    #[test]
    fn merge_recursive() {
        let this = PipelineLayer {
            sum_coin_balances: None,
            sum_obj_types: Some(CommitterLayer {
                write_concurrency: Some(5),
                collect_interval_ms: Some(500),
                watermark_interval_ms: None,
            }),
            sum_displays: Some(SequentialLayer {
                committer: Some(CommitterLayer {
                    write_concurrency: Some(10),
                    collect_interval_ms: Some(1000),
                    watermark_interval_ms: None,
                }),
                checkpoint_lag: Some(100),
            }),
            ..Default::default()
        };

        let that = PipelineLayer {
            sum_coin_balances: Some(CommitterLayer {
                write_concurrency: Some(10),
                collect_interval_ms: None,
                watermark_interval_ms: Some(1000),
            }),
            sum_obj_types: None,
            sum_displays: Some(SequentialLayer {
                committer: Some(CommitterLayer {
                    write_concurrency: Some(5),
                    collect_interval_ms: None,
                    watermark_interval_ms: Some(500),
                }),
                checkpoint_lag: Some(200),
            }),
            ..Default::default()
        };

        let this_then_that = this.clone().merge(that.clone());
        let that_then_this = that.clone().merge(this.clone());

        assert_matches!(
            this_then_that,
            PipelineLayer {
                sum_coin_balances: Some(CommitterLayer {
                    write_concurrency: Some(10),
                    collect_interval_ms: None,
                    watermark_interval_ms: Some(1000),
                }),
                sum_obj_types: Some(CommitterLayer {
                    write_concurrency: Some(5),
                    collect_interval_ms: Some(500),
                    watermark_interval_ms: None,
                }),
                sum_displays: Some(SequentialLayer {
                    committer: Some(CommitterLayer {
                        write_concurrency: Some(5),
                        collect_interval_ms: Some(1000),
                        watermark_interval_ms: Some(500),
                    }),
                    checkpoint_lag: Some(200),
                }),
                ..
            },
        );

        assert_matches!(
            that_then_this,
            PipelineLayer {
                sum_coin_balances: Some(CommitterLayer {
                    write_concurrency: Some(10),
                    collect_interval_ms: None,
                    watermark_interval_ms: Some(1000),
                }),
                sum_obj_types: Some(CommitterLayer {
                    write_concurrency: Some(5),
                    collect_interval_ms: Some(500),
                    watermark_interval_ms: None,
                }),
                sum_displays: Some(SequentialLayer {
                    committer: Some(CommitterLayer {
                        write_concurrency: Some(10),
                        collect_interval_ms: Some(1000),
                        watermark_interval_ms: Some(500),
                    }),
                    checkpoint_lag: Some(100),
                }),
                ..
            },
        );
    }

    #[test]
    fn merge_pruner() {
        let this = PrunerLayer {
            interval_ms: None,
            delay_ms: Some(100),
            retention: Some(200),
            max_chunk_size: Some(300),
        };

        let that = PrunerLayer {
            interval_ms: Some(400),
            delay_ms: None,
            retention: Some(500),
            max_chunk_size: Some(600),
        };

        let this_then_that = this.clone().merge(that.clone());
        let that_then_this = that.clone().merge(this.clone());

        assert_matches!(
            this_then_that,
            PrunerLayer {
                interval_ms: Some(400),
                delay_ms: Some(100),
                retention: Some(500),
                max_chunk_size: Some(600),
            },
        );

        assert_matches!(
            that_then_this,
            PrunerLayer {
                interval_ms: Some(400),
                delay_ms: Some(100),
                retention: Some(500),
                max_chunk_size: Some(300),
            },
        );
    }

    #[test]
    fn finish_concurrent_unpruned_override() {
        let layer = ConcurrentLayer {
            committer: None,
            pruner: None,
        };

        let base = ConcurrentConfig {
            committer: CommitterConfig {
                write_concurrency: 5,
                collect_interval_ms: 50,
                watermark_interval_ms: 500,
            },
            pruner: Some(PrunerConfig::default()),
        };

        assert_matches!(
            layer.finish(base),
            ConcurrentConfig {
                committer: CommitterConfig {
                    write_concurrency: 5,
                    collect_interval_ms: 50,
                    watermark_interval_ms: 500,
                },
                pruner: None,
            },
        );
    }

    #[test]
    fn finish_concurrent_no_pruner() {
        let layer = ConcurrentLayer {
            committer: None,
            pruner: None,
        };

        let base = ConcurrentConfig {
            committer: CommitterConfig {
                write_concurrency: 5,
                collect_interval_ms: 50,
                watermark_interval_ms: 500,
            },
            pruner: None,
        };

        assert_matches!(
            layer.finish(base),
            ConcurrentConfig {
                committer: CommitterConfig {
                    write_concurrency: 5,
                    collect_interval_ms: 50,
                    watermark_interval_ms: 500,
                },
                pruner: None,
            },
        );
    }

    #[test]
    fn finish_concurrent_pruner() {
        let layer = ConcurrentLayer {
            committer: None,
            pruner: Some(PrunerLayer {
                interval_ms: Some(1000),
                ..Default::default()
            }),
        };

        let base = ConcurrentConfig {
            committer: CommitterConfig {
                write_concurrency: 5,
                collect_interval_ms: 50,
                watermark_interval_ms: 500,
            },
            pruner: Some(PrunerConfig {
                interval_ms: 100,
                delay_ms: 200,
                retention: 300,
                max_chunk_size: 400,
            }),
        };

        assert_matches!(
            layer.finish(base),
            ConcurrentConfig {
                committer: CommitterConfig {
                    write_concurrency: 5,
                    collect_interval_ms: 50,
                    watermark_interval_ms: 500,
                },
                pruner: Some(PrunerConfig {
                    interval_ms: 1000,
                    delay_ms: 200,
                    retention: 300,
                    max_chunk_size: 400,
                }),
            },
        );
    }
}
