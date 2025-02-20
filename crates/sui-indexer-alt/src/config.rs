// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::mem;

use sui_default_config::DefaultConfig;
use sui_indexer_alt_framework::{
    ingestion::IngestionConfig,
    pipeline::{
        concurrent::{ConcurrentConfig, PrunerConfig},
        sequential::SequentialConfig,
        CommitterConfig,
    },
};
use tracing::warn;

/// Trait for merging configuration structs together.
pub trait Merge {
    fn merge(self, other: Self) -> Self;
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct IndexerConfig {
    /// How checkpoints are read by the indexer.
    pub ingestion: IngestionLayer,

    /// Configuration for the retention on consistent pipelines.
    pub consistency: PrunerLayer,

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

    #[serde(flatten)]
    pub extra: toml::Table,
}

// Configuration layers apply overrides over a base configuration. When reading configs from a
// file, we read them into layer types, and then apply those layers onto an existing configuration
// (such as the default configuration) to `finish()` them.
//
// Treating configs as layers allows us to support configuration merging, where multiple
// configuration files can be combined into one final configuration. Having a separate type for
// reading configs also allows us to detect and warn against unrecognised fields.

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct IngestionLayer {
    pub checkpoint_buffer_size: Option<usize>,
    pub ingest_concurrency: Option<usize>,
    pub retry_interval_ms: Option<u64>,

    #[serde(flatten)]
    pub extra: toml::Table,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct SequentialLayer {
    pub committer: Option<CommitterLayer>,
    pub checkpoint_lag: Option<u64>,

    #[serde(flatten)]
    pub extra: toml::Table,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct ConcurrentLayer {
    pub committer: Option<CommitterLayer>,
    pub pruner: Option<PrunerLayer>,

    #[serde(flatten)]
    pub extra: toml::Table,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct CommitterLayer {
    pub write_concurrency: Option<usize>,
    pub collect_interval_ms: Option<u64>,
    pub watermark_interval_ms: Option<u64>,

    #[serde(flatten)]
    pub extra: toml::Table,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct PrunerLayer {
    pub interval_ms: Option<u64>,
    pub delay_ms: Option<u64>,
    pub retention: Option<u64>,
    pub max_chunk_size: Option<u64>,
    pub prune_concurrency: Option<u64>,

    #[serde(flatten)]
    pub extra: toml::Table,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
#[serde(rename_all = "snake_case")]
pub struct PipelineLayer {
    // Consistent pipelines
    pub coin_balance_buckets: Option<CommitterLayer>,
    pub obj_info: Option<CommitterLayer>,

    // Sequential pipelines
    pub sum_displays: Option<SequentialLayer>,
    pub sum_packages: Option<SequentialLayer>,

    // All concurrent pipelines
    pub cp_sequence_numbers: Option<ConcurrentLayer>,
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

    #[serde(flatten)]
    pub extra: toml::Table,
}

impl IndexerConfig {
    /// Generate an example configuration, suitable for demonstrating the fields available to
    /// configure.
    pub fn example() -> Self {
        let mut example: Self = Default::default();

        example.ingestion = IngestionConfig::default().into();
        example.consistency = PrunerLayer {
            interval_ms: Some(60_000),
            retention: Some(4 * 60 * 60),
            ..Default::default()
        };
        example.committer = CommitterConfig::default().into();
        example.pruner = PrunerConfig::default().into();
        example.pipeline = PipelineLayer::example();

        example
    }

    /// Generate a configuration suitable for testing. This is the same as the example
    /// configuration, but with reduced concurrency and faster polling intervals so tests spend
    /// less time waiting.
    pub fn for_test() -> Self {
        Self::example().merge(IndexerConfig {
            ingestion: IngestionLayer {
                retry_interval_ms: Some(10),
                ingest_concurrency: Some(1),
                ..Default::default()
            },
            committer: CommitterLayer {
                collect_interval_ms: Some(50),
                watermark_interval_ms: Some(50),
                write_concurrency: Some(1),
                ..Default::default()
            },
            consistency: PrunerLayer {
                interval_ms: Some(50),
                delay_ms: Some(0),
                ..Default::default()
            },
            pruner: PrunerLayer {
                interval_ms: Some(50),
                delay_ms: Some(0),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    pub fn finish(mut self) -> IndexerConfig {
        check_extra("top-level", mem::take(&mut self.extra));
        self
    }
}

impl IngestionLayer {
    pub fn finish(self, base: IngestionConfig) -> IngestionConfig {
        check_extra("ingestion", self.extra);
        IngestionConfig {
            checkpoint_buffer_size: self
                .checkpoint_buffer_size
                .unwrap_or(base.checkpoint_buffer_size),
            ingest_concurrency: self.ingest_concurrency.unwrap_or(base.ingest_concurrency),
            retry_interval_ms: self.retry_interval_ms.unwrap_or(base.retry_interval_ms),
        }
    }
}

impl SequentialLayer {
    pub fn finish(self, base: SequentialConfig) -> SequentialConfig {
        check_extra("sequential pipeline", self.extra);
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
    /// Unlike other parameters, `pruner` will appear in the finished configuration only if they
    /// appear in the layer *and* in the base.
    pub fn finish(self, base: ConcurrentConfig) -> ConcurrentConfig {
        check_extra("concurrent pipeline", self.extra);
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
    pub fn finish(self, base: CommitterConfig) -> CommitterConfig {
        check_extra("committer", self.extra);
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
    pub fn finish(self, base: PrunerConfig) -> PrunerConfig {
        check_extra("pruner", self.extra);
        PrunerConfig {
            interval_ms: self.interval_ms.unwrap_or(base.interval_ms),
            delay_ms: self.delay_ms.unwrap_or(base.delay_ms),
            retention: self.retention.unwrap_or(base.retention),
            max_chunk_size: self.max_chunk_size.unwrap_or(base.max_chunk_size),
            prune_concurrency: self.prune_concurrency.unwrap_or(base.prune_concurrency),
        }
    }
}

impl PipelineLayer {
    /// Generate an example configuration, suitable for demonstrating the fields available to
    /// configure.
    pub fn example() -> Self {
        PipelineLayer {
            coin_balance_buckets: Some(Default::default()),
            obj_info: Some(Default::default()),
            sum_displays: Some(Default::default()),
            sum_packages: Some(Default::default()),
            cp_sequence_numbers: Some(Default::default()),
            ev_emit_mod: Some(Default::default()),
            ev_struct_inst: Some(Default::default()),
            kv_checkpoints: Some(Default::default()),
            kv_epoch_ends: Some(Default::default()),
            kv_epoch_starts: Some(Default::default()),
            kv_feature_flags: Some(Default::default()),
            kv_objects: Some(Default::default()),
            kv_protocol_configs: Some(Default::default()),
            kv_transactions: Some(Default::default()),
            obj_versions: Some(Default::default()),
            tx_affected_addresses: Some(Default::default()),
            tx_affected_objects: Some(Default::default()),
            tx_balance_changes: Some(Default::default()),
            tx_calls: Some(Default::default()),
            tx_digests: Some(Default::default()),
            tx_kinds: Some(Default::default()),
            extra: Default::default(),
        }
    }

    pub fn finish(mut self) -> PipelineLayer {
        check_extra("pipeline", mem::take(&mut self.extra));
        self
    }
}

impl Merge for IndexerConfig {
    fn merge(self, other: IndexerConfig) -> IndexerConfig {
        check_extra("top-level", self.extra);
        check_extra("top-level", other.extra);
        IndexerConfig {
            ingestion: self.ingestion.merge(other.ingestion),
            consistency: self.consistency.merge(other.consistency),
            committer: self.committer.merge(other.committer),
            pruner: self.pruner.merge(other.pruner),
            pipeline: self.pipeline.merge(other.pipeline),
            extra: Default::default(),
        }
    }
}

impl Merge for IngestionLayer {
    fn merge(self, other: IngestionLayer) -> IngestionLayer {
        check_extra("ingestion", self.extra);
        check_extra("ingestion", other.extra);
        IngestionLayer {
            checkpoint_buffer_size: other.checkpoint_buffer_size.or(self.checkpoint_buffer_size),
            ingest_concurrency: other.ingest_concurrency.or(self.ingest_concurrency),
            retry_interval_ms: other.retry_interval_ms.or(self.retry_interval_ms),
            extra: Default::default(),
        }
    }
}

impl Merge for SequentialLayer {
    fn merge(self, other: SequentialLayer) -> SequentialLayer {
        check_extra("sequential pipeline", self.extra);
        check_extra("sequential pipeline", other.extra);
        SequentialLayer {
            committer: self.committer.merge(other.committer),
            checkpoint_lag: other.checkpoint_lag.or(self.checkpoint_lag),
            extra: Default::default(),
        }
    }
}

impl Merge for ConcurrentLayer {
    fn merge(self, other: ConcurrentLayer) -> ConcurrentLayer {
        check_extra("concurrent pipeline", self.extra);
        check_extra("concurrent pipeline", other.extra);
        ConcurrentLayer {
            committer: self.committer.merge(other.committer),
            pruner: self.pruner.merge(other.pruner),
            extra: Default::default(),
        }
    }
}

impl Merge for CommitterLayer {
    fn merge(self, other: CommitterLayer) -> CommitterLayer {
        check_extra("committer", self.extra);
        check_extra("committer", other.extra);
        CommitterLayer {
            write_concurrency: other.write_concurrency.or(self.write_concurrency),
            collect_interval_ms: other.collect_interval_ms.or(self.collect_interval_ms),
            watermark_interval_ms: other.watermark_interval_ms.or(self.watermark_interval_ms),
            extra: Default::default(),
        }
    }
}

impl Merge for PrunerLayer {
    /// Last write takes precedence for all fields except the `retention`, which takes the max of
    /// all available values.
    fn merge(self, other: PrunerLayer) -> PrunerLayer {
        check_extra("pruner", self.extra);
        check_extra("pruner", other.extra);
        PrunerLayer {
            interval_ms: other.interval_ms.or(self.interval_ms),
            delay_ms: other.delay_ms.or(self.delay_ms),
            retention: match (other.retention, self.retention) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (Some(a), _) | (_, Some(a)) => Some(a),
                (None, None) => None,
            },
            max_chunk_size: other.max_chunk_size.or(self.max_chunk_size),
            prune_concurrency: other.prune_concurrency.or(self.prune_concurrency),
            extra: Default::default(),
        }
    }
}

impl Merge for PipelineLayer {
    fn merge(self, other: PipelineLayer) -> PipelineLayer {
        check_extra("pipeline", self.extra);
        check_extra("pipeline", other.extra);
        PipelineLayer {
            coin_balance_buckets: self.coin_balance_buckets.merge(other.coin_balance_buckets),
            obj_info: self.obj_info.merge(other.obj_info),
            sum_displays: self.sum_displays.merge(other.sum_displays),
            sum_packages: self.sum_packages.merge(other.sum_packages),
            cp_sequence_numbers: self.cp_sequence_numbers.merge(other.cp_sequence_numbers),
            ev_emit_mod: self.ev_emit_mod.merge(other.ev_emit_mod),
            ev_struct_inst: self.ev_struct_inst.merge(other.ev_struct_inst),
            kv_checkpoints: self.kv_checkpoints.merge(other.kv_checkpoints),
            kv_epoch_ends: self.kv_epoch_ends.merge(other.kv_epoch_ends),
            kv_epoch_starts: self.kv_epoch_starts.merge(other.kv_epoch_starts),
            kv_feature_flags: self.kv_feature_flags.merge(other.kv_feature_flags),
            kv_objects: self.kv_objects.merge(other.kv_objects),
            kv_protocol_configs: self.kv_protocol_configs.merge(other.kv_protocol_configs),
            kv_transactions: self.kv_transactions.merge(other.kv_transactions),
            obj_versions: self.obj_versions.merge(other.obj_versions),
            tx_affected_addresses: self
                .tx_affected_addresses
                .merge(other.tx_affected_addresses),
            tx_affected_objects: self.tx_affected_objects.merge(other.tx_affected_objects),
            tx_balance_changes: self.tx_balance_changes.merge(other.tx_balance_changes),
            tx_calls: self.tx_calls.merge(other.tx_calls),
            tx_digests: self.tx_digests.merge(other.tx_digests),
            tx_kinds: self.tx_kinds.merge(other.tx_kinds),
            extra: Default::default(),
        }
    }
}

impl<T: Merge> Merge for Option<T> {
    fn merge(self, other: Option<T>) -> Option<T> {
        match (self, other) {
            (Some(a), Some(b)) => Some(a.merge(b)),
            (Some(a), _) | (_, Some(a)) => Some(a),
            (None, None) => None,
        }
    }
}

impl From<IngestionConfig> for IngestionLayer {
    fn from(config: IngestionConfig) -> Self {
        Self {
            checkpoint_buffer_size: Some(config.checkpoint_buffer_size),
            ingest_concurrency: Some(config.ingest_concurrency),
            retry_interval_ms: Some(config.retry_interval_ms),
            extra: Default::default(),
        }
    }
}

impl From<SequentialConfig> for SequentialLayer {
    fn from(config: SequentialConfig) -> Self {
        Self {
            committer: Some(config.committer.into()),
            checkpoint_lag: Some(config.checkpoint_lag),
            extra: Default::default(),
        }
    }
}

impl From<ConcurrentConfig> for ConcurrentLayer {
    fn from(config: ConcurrentConfig) -> Self {
        Self {
            committer: Some(config.committer.into()),
            pruner: config.pruner.map(Into::into),
            extra: Default::default(),
        }
    }
}

impl From<CommitterConfig> for CommitterLayer {
    fn from(config: CommitterConfig) -> Self {
        Self {
            write_concurrency: Some(config.write_concurrency),
            collect_interval_ms: Some(config.collect_interval_ms),
            watermark_interval_ms: Some(config.watermark_interval_ms),
            extra: Default::default(),
        }
    }
}

impl From<PrunerConfig> for PrunerLayer {
    fn from(config: PrunerConfig) -> Self {
        Self {
            interval_ms: Some(config.interval_ms),
            delay_ms: Some(config.delay_ms),
            retention: Some(config.retention),
            max_chunk_size: Some(config.max_chunk_size),
            prune_concurrency: Some(config.prune_concurrency),
            extra: Default::default(),
        }
    }
}

/// Check whether there are any unrecognized extra fields and if so, warn about them.
fn check_extra(pos: &str, extra: toml::Table) {
    if !extra.is_empty() {
        warn!(
            "Found unrecognized {pos} field{} which will be ignored by this indexer. This could be \
             because of a typo, or because it was introduced in a newer version of the indexer:\n{}",
            if extra.len() != 1 { "s" } else { "" },
            extra,
        )
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
    fn merge_recursive() {
        let this = PipelineLayer {
            sum_displays: Some(SequentialLayer {
                committer: Some(CommitterLayer {
                    write_concurrency: Some(10),
                    collect_interval_ms: Some(1000),
                    watermark_interval_ms: None,
                    extra: Default::default(),
                }),
                checkpoint_lag: Some(100),
                extra: Default::default(),
            }),
            sum_packages: None,
            ev_emit_mod: Some(ConcurrentLayer {
                committer: Some(CommitterLayer {
                    write_concurrency: Some(5),
                    collect_interval_ms: Some(500),
                    watermark_interval_ms: None,
                    extra: Default::default(),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let that = PipelineLayer {
            sum_displays: Some(SequentialLayer {
                committer: Some(CommitterLayer {
                    write_concurrency: Some(5),
                    collect_interval_ms: None,
                    watermark_interval_ms: Some(500),
                    extra: Default::default(),
                }),
                checkpoint_lag: Some(200),
                extra: Default::default(),
            }),
            sum_packages: Some(SequentialLayer {
                committer: Some(CommitterLayer {
                    write_concurrency: Some(10),
                    collect_interval_ms: None,
                    watermark_interval_ms: Some(1000),
                    extra: Default::default(),
                }),
                ..Default::default()
            }),
            ev_emit_mod: None,
            ..Default::default()
        };

        let this_then_that = this.clone().merge(that.clone());
        let that_then_this = that.clone().merge(this.clone());

        assert_matches!(
            this_then_that,
            PipelineLayer {
                sum_displays: Some(SequentialLayer {
                    committer: Some(CommitterLayer {
                        write_concurrency: Some(5),
                        collect_interval_ms: Some(1000),
                        watermark_interval_ms: Some(500),
                        extra: _,
                    }),
                    checkpoint_lag: Some(200),
                    extra: _,
                }),
                sum_packages: Some(SequentialLayer {
                    committer: Some(CommitterLayer {
                        write_concurrency: Some(10),
                        collect_interval_ms: None,
                        watermark_interval_ms: Some(1000),
                        extra: _,
                    }),
                    checkpoint_lag: None,
                    extra: _,
                }),
                ev_emit_mod: Some(ConcurrentLayer {
                    committer: Some(CommitterLayer {
                        write_concurrency: Some(5),
                        collect_interval_ms: Some(500),
                        watermark_interval_ms: None,
                        extra: _,
                    }),
                    pruner: None,
                    extra: _,
                }),
                ..
            },
        );

        assert_matches!(
            that_then_this,
            PipelineLayer {
                sum_displays: Some(SequentialLayer {
                    committer: Some(CommitterLayer {
                        write_concurrency: Some(10),
                        collect_interval_ms: Some(1000),
                        watermark_interval_ms: Some(500),
                        extra: _,
                    }),
                    checkpoint_lag: Some(100),
                    extra: _,
                }),
                sum_packages: Some(SequentialLayer {
                    committer: Some(CommitterLayer {
                        write_concurrency: Some(10),
                        collect_interval_ms: None,
                        watermark_interval_ms: Some(1000),
                        extra: _,
                    }),
                    checkpoint_lag: None,
                    extra: _,
                }),
                ev_emit_mod: Some(ConcurrentLayer {
                    committer: Some(CommitterLayer {
                        write_concurrency: Some(5),
                        collect_interval_ms: Some(500),
                        watermark_interval_ms: None,
                        extra: _,
                    }),
                    pruner: None,
                    extra: _,
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
            prune_concurrency: Some(1),
            extra: Default::default(),
        };

        let that = PrunerLayer {
            interval_ms: Some(400),
            delay_ms: None,
            retention: Some(500),
            max_chunk_size: Some(600),
            prune_concurrency: Some(2),
            extra: Default::default(),
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
                prune_concurrency: Some(2),
                extra: _,
            },
        );

        assert_matches!(
            that_then_this,
            PrunerLayer {
                interval_ms: Some(400),
                delay_ms: Some(100),
                retention: Some(500),
                max_chunk_size: Some(300),
                prune_concurrency: Some(1),
                extra: _,
            },
        );
    }

    #[test]
    fn finish_concurrent_unpruned_override() {
        let layer = ConcurrentLayer {
            committer: None,
            pruner: None,
            extra: Default::default(),
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
            extra: Default::default(),
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
            extra: Default::default(),
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
                prune_concurrency: 1,
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
                    prune_concurrency: 1,
                }),
            },
        );
    }
}
