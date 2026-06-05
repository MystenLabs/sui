// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Runtime configuration for the `sui-rpc-store` indexer.
//!
//! The indexer is driven by [`ServiceConfig`], which groups the
//! ingestion, consistency, RocksDB, committer, and per-pipeline
//! settings the orchestrator needs.
//!
//! Per-pipeline enable/disable is expressed through
//! [`PipelineLayer`]: every pipeline maps to an
//! `Option<CommitterLayer>` field; `Some(_)` means the pipeline is
//! registered (with the supplied committer overrides), `None` means
//! it is skipped. The standalone binary populates the layer from
//! its TOML config; the embedded-fullnode caller builds it
//! programmatically via [`PipelineLayer::indexes_only`] so the raw
//! chain CFs (populated by the fullnode itself) are not double-
//! written by this indexer.

use sui_default_config::DefaultConfig;
use sui_indexer_alt_framework::pipeline::CommitterConfig;

/// Top-level configuration for the `sui-rpc-store` indexer
/// service. Parses from TOML; every field has a sensible default
/// for tests and for the embedded use case where most knobs are
/// supplied programmatically.
#[DefaultConfig]
#[derive(Default)]
#[serde(deny_unknown_fields)]
pub struct ServiceConfig {
    /// Cross-pipeline consistency knobs: how often to take
    /// snapshots and how deep the per-pipeline write buffer is.
    pub consistency: ConsistencyConfig,

    /// Default committer settings shared by all pipelines.
    /// Per-pipeline entries in [`PipelineLayer`] can override
    /// individual fields.
    pub committer: CommitterLayer,

    /// Per-pipeline enable/disable plus optional committer
    /// overrides.
    pub pipeline: PipelineLayer,
}

/// Cross-pipeline consistency knobs surfaced to operators. The
/// indexer threads these into the [`Synchronizer`] at startup.
///
/// Snapshot *retention* (how many in-memory snapshots are kept, and
/// thus how far back consistent reads can reach) is not configured
/// here: it is an open-time property of the database, set via
/// [`DbOptions::snapshot_capacity`]. The effective consistent-read
/// window is roughly `stride * snapshot_capacity` checkpoints.
///
/// [`Synchronizer`]: sui_consistent_store::Synchronizer
/// [`DbOptions::snapshot_capacity`]: sui_consistent_store::DbOptions::snapshot_capacity
#[DefaultConfig]
#[derive(Clone)]
#[serde(deny_unknown_fields)]
pub struct ConsistencyConfig {
    /// Number of checkpoints between cross-pipeline snapshots. A
    /// stride of `1` snapshots after every checkpoint; higher
    /// strides reduce snapshot frequency (and the load it puts on
    /// RocksDB compaction) at the cost of read-side staleness.
    pub stride: u64,

    /// Per-pipeline mpsc capacity for batches waiting to be
    /// committed. The synchronizer's slowest pipeline gates
    /// progress; this buffer absorbs short bursts of slack between
    /// peer pipelines before back-pressure kicks in.
    pub buffer_size: usize,
}

/// Per-pipeline registration + override map. Every pipeline that
/// writes to a CF in [`RpcStoreSchema`] has a corresponding
/// `Option<CommitterLayer>` field here.
///
/// `Some(layer)` registers the pipeline with the supplied committer
/// overrides folded onto the shared [`CommitterLayer`] default;
/// `None` skips the pipeline entirely (e.g. the raw chain CFs in
/// the embedded-fullnode case, where the fullnode populates them
/// through a separate path).
///
/// Grouped in the struct for documentation only — serde sees each
/// field as a top-level key.
///
/// [`RpcStoreSchema`]: crate::RpcStoreSchema
#[DefaultConfig]
#[derive(Default)]
pub struct PipelineLayer {
    // --- Raw chain data ---
    pub epochs: Option<CommitterLayer>,
    pub checkpoint_summary: Option<CommitterLayer>,
    pub checkpoint_contents: Option<CommitterLayer>,
    pub checkpoint_seq_by_digest: Option<CommitterLayer>,
    pub transactions: Option<CommitterLayer>,
    pub tx_seq_by_digest: Option<CommitterLayer>,
    pub tx_metadata_by_seq: Option<CommitterLayer>,
    pub effects: Option<CommitterLayer>,
    pub events: Option<CommitterLayer>,
    pub objects: Option<CommitterLayer>,
    pub live_objects: Option<CommitterLayer>,

    // --- Indexes ---
    pub object_by_owner: Option<CommitterLayer>,
    pub object_by_type: Option<CommitterLayer>,
    pub balance: Option<CommitterLayer>,
    pub package_versions: Option<CommitterLayer>,
    pub transaction_bitmap: Option<CommitterLayer>,
    pub event_bitmap: Option<CommitterLayer>,
}

/// Per-pipeline committer overrides. Every field is optional; an
/// unset field inherits from the shared committer default the
/// orchestrator passes through to
/// [`CommitterLayer::finish`](Self::finish).
#[DefaultConfig]
#[derive(Default)]
#[serde(deny_unknown_fields)]
pub struct CommitterLayer {
    pub write_concurrency: Option<usize>,
    pub collect_interval_ms: Option<u64>,
    pub watermark_interval_ms: Option<u64>,
}

impl ServiceConfig {
    /// Configuration matching [`Self::default()`] but with every
    /// pipeline explicitly enabled and the committer layer
    /// initialised from [`CommitterConfig::default()`]. Suitable
    /// for surfacing as a TOML example.
    pub fn example() -> Self {
        Self {
            consistency: ConsistencyConfig::default(),
            committer: CommitterConfig::default().into(),
            pipeline: PipelineLayer::all(),
        }
    }
}

impl PipelineLayer {
    /// Every pipeline enabled with default committer overrides
    /// (`Some(CommitterLayer::default())`). The standalone-binary
    /// default.
    pub fn all() -> Self {
        Self {
            epochs: Some(CommitterLayer::default()),
            checkpoint_summary: Some(CommitterLayer::default()),
            checkpoint_contents: Some(CommitterLayer::default()),
            checkpoint_seq_by_digest: Some(CommitterLayer::default()),
            transactions: Some(CommitterLayer::default()),
            tx_seq_by_digest: Some(CommitterLayer::default()),
            tx_metadata_by_seq: Some(CommitterLayer::default()),
            effects: Some(CommitterLayer::default()),
            events: Some(CommitterLayer::default()),
            objects: Some(CommitterLayer::default()),
            live_objects: Some(CommitterLayer::default()),
            object_by_owner: Some(CommitterLayer::default()),
            object_by_type: Some(CommitterLayer::default()),
            balance: Some(CommitterLayer::default()),
            package_versions: Some(CommitterLayer::default()),
            transaction_bitmap: Some(CommitterLayer::default()),
            event_bitmap: Some(CommitterLayer::default()),
        }
    }

    /// Only the derived-index pipelines enabled. The raw chain CFs
    /// (`epochs`, `checkpoint_*`, `transactions`, `effects`,
    /// `events`, `objects`, `live_objects`, `tx_*`) are left `None`
    /// because, in the embedded-fullnode case, the fullnode
    /// populates those CFs through its own write path.
    pub fn indexes_only() -> Self {
        Self {
            object_by_owner: Some(CommitterLayer::default()),
            object_by_type: Some(CommitterLayer::default()),
            balance: Some(CommitterLayer::default()),
            package_versions: Some(CommitterLayer::default()),
            transaction_bitmap: Some(CommitterLayer::default()),
            event_bitmap: Some(CommitterLayer::default()),
            ..Self::default()
        }
    }
}

/// Per-pipeline registration toggles for
/// [`restore_indexes`](crate::restore_indexes).
///
/// The derived-index pipelines (`live_objects`, `object_by_owner`,
/// `object_by_type`, `balance`, `package_versions`) are always
/// restored — they cannot be reconstructed from anywhere else. The
/// raw `objects` CF is conditional: the standalone deployment
/// needs it so version-keyed reads are served by the restored
/// snapshot, while the embedded-fullnode deployment already has
/// every object version in the validator's perpetual store and
/// can skip the duplicate write.
#[derive(Default, Clone, Debug)]
pub struct RestoreLayer {
    /// If true, register the `objects` pipeline with the restore
    /// driver so each live object lands as an
    /// `(ObjectID, version) → StoredObject` row.
    pub objects: bool,
}

impl RestoreLayer {
    /// Restore every pipeline, including the raw `objects` CF.
    /// The standalone-binary default.
    pub fn all() -> Self {
        Self { objects: true }
    }

    /// Restore only the derived-index pipelines. The embedded-
    /// fullnode default — the fullnode's perpetual store already
    /// holds every object version, so the `objects` CF is left
    /// untouched here.
    pub fn indexes_only() -> Self {
        Self { objects: false }
    }
}

impl CommitterLayer {
    /// Fold the override layer onto a shared default
    /// [`CommitterConfig`]. Unset fields inherit from `base`.
    pub fn finish(self, base: CommitterConfig) -> CommitterConfig {
        CommitterConfig {
            write_concurrency: self.write_concurrency.unwrap_or(base.write_concurrency),
            collect_interval_ms: self.collect_interval_ms.unwrap_or(base.collect_interval_ms),
            watermark_interval_ms: self
                .watermark_interval_ms
                .unwrap_or(base.watermark_interval_ms),
            ..Default::default()
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

impl Default for ConsistencyConfig {
    fn default() -> Self {
        Self {
            stride: 1,
            buffer_size: 5_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_only_disables_raw_chain_pipelines() {
        let layer = PipelineLayer::indexes_only();
        // Indexes are enabled.
        assert!(layer.object_by_owner.is_some());
        assert!(layer.object_by_type.is_some());
        assert!(layer.balance.is_some());
        assert!(layer.package_versions.is_some());
        assert!(layer.transaction_bitmap.is_some());
        assert!(layer.event_bitmap.is_some());
        // Raw chain CFs are disabled.
        assert!(layer.epochs.is_none());
        assert!(layer.checkpoint_summary.is_none());
        assert!(layer.checkpoint_contents.is_none());
        assert!(layer.checkpoint_seq_by_digest.is_none());
        assert!(layer.transactions.is_none());
        assert!(layer.tx_seq_by_digest.is_none());
        assert!(layer.tx_metadata_by_seq.is_none());
        assert!(layer.effects.is_none());
        assert!(layer.events.is_none());
        assert!(layer.objects.is_none());
        assert!(layer.live_objects.is_none());
    }

    #[test]
    fn all_enables_every_pipeline() {
        let layer = PipelineLayer::all();
        assert!(layer.epochs.is_some());
        assert!(layer.checkpoint_summary.is_some());
        assert!(layer.transactions.is_some());
        assert!(layer.objects.is_some());
        assert!(layer.live_objects.is_some());
        assert!(layer.object_by_owner.is_some());
        assert!(layer.balance.is_some());
        assert!(layer.event_bitmap.is_some());
    }

    #[test]
    fn committer_layer_overrides_base() {
        let base = CommitterConfig {
            write_concurrency: 4,
            collect_interval_ms: 200,
            watermark_interval_ms: 200,
            ..Default::default()
        };
        let layer = CommitterLayer {
            write_concurrency: Some(8),
            collect_interval_ms: None,
            watermark_interval_ms: Some(500),
        };
        let merged = layer.finish(base);
        assert_eq!(merged.write_concurrency, 8);
        // Unset fields inherit from `base`.
        assert_eq!(merged.collect_interval_ms, 200);
        assert_eq!(merged.watermark_interval_ms, 500);
    }
}
