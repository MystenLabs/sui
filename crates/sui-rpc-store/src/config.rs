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
//! `Option<PipelineConfig>` field; `Some(_)` means the pipeline is
//! registered (with the supplied committer overrides and optional
//! availability policy), `None` means it is skipped. The standalone
//! binary populates the layer from its TOML config; the
//! embedded-fullnode caller builds it programmatically via
//! [`PipelineLayer::embedded`] so the raw chain CFs (served by the
//! fullnode's perpetual store) are not double-written by this
//! indexer.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;
use sui_indexer_alt_framework::pipeline::CommitterConfig;

/// Top-level configuration for the `sui-rpc-store` indexer
/// service. Parses from TOML; every field has a sensible default
/// for tests and for the embedded use case where most knobs are
/// supplied programmatically.
#[derive(Default, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct ServiceConfig {
    /// Cross-pipeline consistency knobs: how often to take
    /// snapshots and how deep the per-pipeline write buffer is.
    pub consistency: ConsistencyConfig,

    /// Default committer settings shared by all pipelines.
    /// Per-pipeline entries in [`PipelineLayer`] can override
    /// individual fields.
    pub committer: CommitterLayer,

    /// Default availability policy applied to every pipeline
    /// without a `[pipeline.<name>.availability]` override. Absent
    /// (the default) means every pipeline is always served. See
    /// [`PipelineAvailability`].
    pub availability: Option<PipelineAvailability>,

    /// Per-pipeline enable/disable plus optional committer
    /// overrides and availability policies.
    pub pipeline: PipelineLayer,

    /// Pruning policy for the historical CFs. Absent (the default)
    /// disables pruning entirely — the store retains all history.
    pub pruner: Option<PrunerConfig>,
}

/// Cross-pipeline consistency knobs surfaced to operators. The
/// indexer threads these into the [`Synchronizer`] at startup.
///
/// Snapshot *retention* (how many in-memory snapshots are kept, and
/// thus how far back consistent reads can reach) is not configured
/// here: it is an open-time property of the database, set via
/// [`DbOptions::snapshot_capacity`]. Because a snapshot is taken at
/// every checkpoint boundary, the effective consistent-read window
/// is roughly `snapshot_capacity` checkpoints.
///
/// [`Synchronizer`]: sui_consistent_store::Synchronizer
/// [`DbOptions::snapshot_capacity`]: sui_consistent_store::DbOptions::snapshot_capacity
#[derive(Clone, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct ConsistencyConfig {
    /// Per-pipeline mpsc capacity for batches waiting to be
    /// committed. The synchronizer's slowest pipeline gates
    /// progress; this buffer absorbs short bursts of slack between
    /// peer pipelines before back-pressure kicks in.
    pub buffer_size: usize,
}

/// Pruning policy for the historical column families.
///
/// Retention is expressed in epochs, mirroring the validator's
/// perpetual-store pruner: the `retention_epochs` most-recent
/// epochs (including the current one) are retained in full, and
/// everything in older epochs becomes eligible for deletion. The
/// resulting floor is additionally clamped so it never advances past
/// the oldest in-memory snapshot, keeping point-in-time reads
/// coherent even under an aggressively small retention.
///
/// The pruner advances the floor toward its target in chunks of at
/// most `max_chunk_checkpoints` checkpoints, persisting the new
/// watermark after each chunk so progress survives a restart. Each
/// tick advances the floor by at most `max_checkpoints_per_tick`
/// checkpoints so a large backlog drains across many ticks rather
/// than one long blocking pass.
///
/// Only the historical CFs are pruned: the per-transaction
/// (`transactions`, `effects`, `events`, `tx_metadata_by_seq`),
/// per-checkpoint (`checkpoint_summary`, `checkpoint_contents`),
/// digest-reverse-index (`tx_seq_by_digest`,
/// `checkpoint_seq_by_digest`), superseded-`objects`-version,
/// checkpoint-pinned `object_version_by_checkpoint`, and
/// ledger-history bitmap CFs. The live-set-bounded indexes
/// (`object_by_owner`, `object_by_type`, `balance`,
/// `package_versions`) and the tiny `epochs` CF are never pruned.
#[derive(Clone, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct PrunerConfig {
    /// Number of most-recent epochs to retain in full. Data in
    /// epochs older than this is eligible for pruning. Must be at
    /// least `1`; the pruner refuses to start otherwise, since a
    /// value of `0` would prune the current epoch.
    pub retention_epochs: u64,

    /// How often the pruner wakes to recompute the target floor and
    /// advance toward it, in milliseconds.
    pub interval_ms: u64,

    /// Maximum number of checkpoints whose data is deleted in a
    /// single write batch. Bounds the per-batch work (and the number
    /// of effects rows scanned for object/digest deletes) when a
    /// whole epoch ages out at once.
    pub max_chunk_checkpoints: u64,

    /// Maximum number of checkpoints whose history is pruned in a
    /// single tick. Bounds the per-tick (blocking) work so that a
    /// large backlog — for example when pruning is first enabled on
    /// an old database — drains across many ticks rather than one
    /// long pass that occupies a blocking thread for minutes. The
    /// floor still converges to its retention target over subsequent
    /// ticks; `interval_ms` and this bound together set the drain
    /// rate. Must be at least `1`; the pruner refuses to start
    /// otherwise, since a value of `0` would never make progress.
    pub max_checkpoints_per_tick: u64,
}

impl Default for PrunerConfig {
    fn default() -> Self {
        Self {
            retention_epochs: 30,
            interval_ms: 300_000,
            max_chunk_checkpoints: 100,
            // 100 chunks per tick at the default chunk size. Far above
            // the steady-state rate at which a single epoch ages out,
            // so retention is honored without intervention, while a
            // first-run backlog on an old database is still bounded
            // per tick rather than drained in one blocking pass.
            max_checkpoints_per_tick: 10_000,
        }
    }
}

impl PrunerConfig {
    /// The pruner's wake interval as a [`Duration`].
    pub fn interval(&self) -> Duration {
        Duration::from_millis(self.interval_ms)
    }
}

/// Read-availability policy for a pipeline: serve it
/// unconditionally (`enabled = true`), never serve it
/// (`enabled = false`), or serve it only while its committed
/// watermark is within `max-checkpoint-lag` checkpoints of the tip
/// (the highest committed watermark across all pipelines). A policy
/// section sets exactly one of the two keys. Reads that need a
/// pipeline that is not served fail as unavailable, and such a
/// pipeline is excluded from the reader's cross-pipeline watermark
/// bounds so it stops pinning the reported tip.
///
/// The top-level `[availability]` key sets a default policy for
/// every pipeline, and `[pipeline.<name>.availability]` overrides
/// it for a single pipeline (e.g. `enabled = true` exempts one
/// pipeline from a configured default). A pipeline with neither is
/// always served, so this feature is opt-in:
///
/// ```toml
/// [availability]
/// max-checkpoint-lag = 100          # default for every pipeline
///
/// [pipeline.balance.availability]
/// enabled = false                   # never serve
///
/// [pipeline.object-by-owner.availability]
/// enabled = true                    # always serve, exempt from the default
/// ```
///
/// The policy is read-side only: it never affects whether a
/// pipeline indexes. Registration (`Some`/`None` in
/// [`PipelineLayer`]) remains the write-side switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "AvailabilityLayer", into = "AvailabilityLayer")]
pub enum PipelineAvailability {
    /// Always serve the pipeline.
    Enabled,

    /// Never serve the pipeline.
    Disabled,

    /// Serve the pipeline only while its committed watermark is
    /// within this many checkpoints of the tip.
    MaxCheckpointLag(u64),
}

/// TOML mirror of [`PipelineAvailability`]: a policy section sets
/// exactly one of these keys, validated when converting to the
/// enum.
#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct AvailabilityLayer {
    pub enabled: Option<bool>,
    pub max_checkpoint_lag: Option<u64>,
}

/// Resolved availability policies: the top-level `[availability]`
/// default plus the `[pipeline.<name>.availability]` overrides.
/// Handed to the reader; see
/// [`RpcStoreReader::with_availability`](crate::RpcStoreReader::with_availability).
#[derive(Clone, Debug, Default)]
pub struct AvailabilityConfig {
    /// Default policy applied to every pipeline without an override.
    pub default: Option<PipelineAvailability>,

    /// Per-pipeline overrides, keyed by pipeline name.
    pub pipelines: BTreeMap<String, PipelineAvailability>,
}

/// Per-pipeline registration + override map. Every pipeline that
/// writes to a CF in [`RpcStoreSchema`] has a corresponding
/// `Option<PipelineConfig>` field here.
///
/// `Some(entry)` registers the pipeline with the supplied committer
/// overrides folded onto the shared [`CommitterLayer`] default;
/// `None` skips the pipeline entirely (e.g. the raw chain CFs in
/// the embedded-fullnode case, where the fullnode populates them
/// through a separate path).
///
/// Grouped in the struct for documentation only — serde sees each
/// field as a top-level key.
///
/// [`RpcStoreSchema`]: crate::RpcStoreSchema
#[derive(Default, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct PipelineLayer {
    // --- Raw chain data ---
    pub epochs: Option<PipelineConfig>,
    pub checkpoint_summary: Option<PipelineConfig>,
    pub checkpoint_contents: Option<PipelineConfig>,
    pub checkpoint_seq_by_digest: Option<PipelineConfig>,
    pub transactions: Option<PipelineConfig>,
    pub tx_seq_by_digest: Option<PipelineConfig>,
    pub tx_metadata_by_seq: Option<PipelineConfig>,
    pub effects: Option<PipelineConfig>,
    pub events: Option<PipelineConfig>,
    pub objects: Option<PipelineConfig>,
    pub object_version_by_checkpoint: Option<PipelineConfig>,

    // --- Indexes ---
    pub object_by_owner: Option<PipelineConfig>,
    pub object_by_type: Option<PipelineConfig>,
    pub balance: Option<PipelineConfig>,
    pub package_versions: Option<PipelineConfig>,
    pub transaction_bitmap: Option<PipelineConfig>,
    pub event_bitmap: Option<PipelineConfig>,
}

/// Per-pipeline registration entry, under `[pipeline.<name>]`. An
/// empty entry registers the pipeline with all-default settings.
#[derive(Default, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct PipelineConfig {
    /// Committer overrides folded onto the shared `[committer]`
    /// default.
    pub committer: CommitterLayer,

    /// Overrides the top-level `[availability]` default for this
    /// pipeline. Read-side only: it does not affect whether the
    /// pipeline indexes.
    pub availability: Option<PipelineAvailability>,
}

/// Per-pipeline committer overrides. Every field is optional; an
/// unset field inherits from the shared committer default the
/// orchestrator passes through to
/// [`CommitterLayer::finish`](Self::finish).
#[derive(Default, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
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
            availability: None,
            pipeline: PipelineLayer::all(),
            pruner: Some(PrunerConfig::default()),
        }
    }

    /// The resolved per-pipeline availability policies, for handing
    /// to [`RpcStoreReader::with_availability`](crate::RpcStoreReader::with_availability).
    pub fn availability_config(&self) -> AvailabilityConfig {
        AvailabilityConfig {
            default: self.availability,
            pipelines: self.pipeline.availability_overrides(),
        }
    }
}

impl PipelineAvailability {
    /// Whether a pipeline whose committed watermark is at
    /// `committed` (`None` = no watermark yet) should be served,
    /// given the current `tip` (the highest committed watermark
    /// across all pipelines).
    pub fn is_available(&self, committed: Option<u64>, tip: u64) -> bool {
        match self {
            Self::Enabled => true,
            Self::Disabled => false,
            Self::MaxCheckpointLag(lag) => committed.is_some_and(|c| tip.saturating_sub(c) <= *lag),
        }
    }
}

impl AvailabilityConfig {
    /// The policy gating `pipeline`, if any: its own override when
    /// configured, otherwise the default.
    pub fn policy_for(&self, pipeline: &str) -> Option<PipelineAvailability> {
        self.pipelines.get(pipeline).copied().or(self.default)
    }

    /// No policy configured anywhere — the reader's zero-cost fast
    /// path.
    pub fn is_trivial(&self) -> bool {
        self.default.is_none() && self.pipelines.is_empty()
    }
}

impl PipelineLayer {
    /// Every pipeline enabled with default settings
    /// (`Some(PipelineConfig::default())`). The standalone-binary
    /// default.
    pub fn all() -> Self {
        Self {
            epochs: Some(PipelineConfig::default()),
            checkpoint_summary: Some(PipelineConfig::default()),
            checkpoint_contents: Some(PipelineConfig::default()),
            checkpoint_seq_by_digest: Some(PipelineConfig::default()),
            transactions: Some(PipelineConfig::default()),
            tx_seq_by_digest: Some(PipelineConfig::default()),
            tx_metadata_by_seq: Some(PipelineConfig::default()),
            effects: Some(PipelineConfig::default()),
            events: Some(PipelineConfig::default()),
            objects: Some(PipelineConfig::default()),
            object_version_by_checkpoint: Some(PipelineConfig::default()),
            object_by_owner: Some(PipelineConfig::default()),
            object_by_type: Some(PipelineConfig::default()),
            balance: Some(PipelineConfig::default()),
            package_versions: Some(PipelineConfig::default()),
            transaction_bitmap: Some(PipelineConfig::default()),
            event_bitmap: Some(PipelineConfig::default()),
        }
    }

    /// Per-pipeline availability overrides, keyed by pipeline name.
    /// Field idents match `Processor::NAME` for every pipeline
    /// (pinned by `availability_override_keys_match_pipeline_names`).
    fn availability_overrides(&self) -> BTreeMap<String, PipelineAvailability> {
        // Exhaustive destructure so adding a pipeline field forces a
        // decision here.
        let Self {
            epochs,
            checkpoint_summary,
            checkpoint_contents,
            checkpoint_seq_by_digest,
            transactions,
            tx_seq_by_digest,
            tx_metadata_by_seq,
            effects,
            events,
            objects,
            object_version_by_checkpoint,
            object_by_owner,
            object_by_type,
            balance,
            package_versions,
            transaction_bitmap,
            event_bitmap,
        } = self;

        [
            ("epochs", epochs),
            ("checkpoint_summary", checkpoint_summary),
            ("checkpoint_contents", checkpoint_contents),
            ("checkpoint_seq_by_digest", checkpoint_seq_by_digest),
            ("transactions", transactions),
            ("tx_seq_by_digest", tx_seq_by_digest),
            ("tx_metadata_by_seq", tx_metadata_by_seq),
            ("effects", effects),
            ("events", events),
            ("objects", objects),
            ("object_version_by_checkpoint", object_version_by_checkpoint),
            ("object_by_owner", object_by_owner),
            ("object_by_type", object_by_type),
            ("balance", balance),
            ("package_versions", package_versions),
            ("transaction_bitmap", transaction_bitmap),
            ("event_bitmap", event_bitmap),
        ]
        .into_iter()
        .filter_map(|(name, entry)| Some((name.to_string(), entry.as_ref()?.availability?)))
        .collect()
    }

    /// The embedded-fullnode cohort: every pipeline this indexer owns
    /// when it runs inside a Sui fullnode beside the validator's
    /// perpetual store.
    ///
    /// The raw chain-data CFs (`transactions`, `effects`, `events`,
    /// `objects`, `checkpoint_summary`, `checkpoint_contents`,
    /// `checkpoint_seq_by_digest`) are left `None`: the perpetual store
    /// already holds that data and serves it directly, so this indexer
    /// must not double-write it.
    ///
    /// The enabled pipelines form two cohorts. The
    /// [`Synchronizer`](sui_consistent_store::Synchronizer)
    /// distinguishes them by their persisted watermark at startup,
    /// not by this layer, so both are simply registered here:
    ///
    /// - **Live cohort** — restored to the fullnode's tip and
    ///   following live from there: `object_by_owner`,
    ///   `object_by_type`, `balance`.
    /// - **History cohort** — seeded to the lowest available
    ///   checkpoint and backfilling upward: `epochs`,
    ///   `object_version_by_checkpoint`, `package_versions`,
    ///   `tx_seq_by_digest`, `tx_metadata_by_seq`, `transaction_bitmap`,
    ///   `event_bitmap`. These back the ledger-history list APIs (the
    ///   bitmaps plus the `tx_seq` <-> digest maps needed to interpret
    ///   bitmap results) and the per-epoch protocol/committee reads
    ///   (`epochs`). `object_version_by_checkpoint` and
    ///   `package_versions` are additionally restored at the tip for
    ///   their floor rows, then backfill the per-checkpoint detail over
    ///   `(L, T]` (see the cohort docs in
    ///   [`restore`](crate::indexer::restore)).
    pub fn embedded() -> Self {
        Self {
            // Live cohort: restored to the tip, follows live.
            object_by_owner: Some(PipelineConfig::default()),
            object_by_type: Some(PipelineConfig::default()),
            balance: Some(PipelineConfig::default()),
            // History cohort: seeded to L, backfills upward.
            // `object_version_by_checkpoint` and `package_versions` are
            // additionally restored at the tip for their floor rows (see
            // the cohort docs in `restore.rs`).
            epochs: Some(PipelineConfig::default()),
            object_version_by_checkpoint: Some(PipelineConfig::default()),
            package_versions: Some(PipelineConfig::default()),
            tx_seq_by_digest: Some(PipelineConfig::default()),
            tx_metadata_by_seq: Some(PipelineConfig::default()),
            transaction_bitmap: Some(PipelineConfig::default()),
            event_bitmap: Some(PipelineConfig::default()),
            ..Self::default()
        }
    }
}

/// Per-pipeline registration toggles for
/// [`restore_indexes`](crate::restore_indexes).
///
/// The derived-index pipelines (`object_by_owner`, `object_by_type`,
/// `balance`, `package_versions`) are always
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

impl TryFrom<AvailabilityLayer> for PipelineAvailability {
    type Error = String;

    fn try_from(layer: AvailabilityLayer) -> Result<Self, Self::Error> {
        match (layer.enabled, layer.max_checkpoint_lag) {
            (Some(_), Some(_)) => {
                Err("'enabled' and 'max-checkpoint-lag' are mutually exclusive".to_string())
            }
            (Some(true), None) => Ok(Self::Enabled),
            (Some(false), None) => Ok(Self::Disabled),
            (None, Some(lag)) => Ok(Self::MaxCheckpointLag(lag)),
            (None, None) => Err("expected 'enabled' or 'max-checkpoint-lag'".to_string()),
        }
    }
}

impl From<PipelineAvailability> for AvailabilityLayer {
    fn from(value: PipelineAvailability) -> Self {
        match value {
            PipelineAvailability::Enabled => Self {
                enabled: Some(true),
                max_checkpoint_lag: None,
            },
            PipelineAvailability::Disabled => Self {
                enabled: Some(false),
                max_checkpoint_lag: None,
            },
            PipelineAvailability::MaxCheckpointLag(lag) => Self {
                enabled: None,
                max_checkpoint_lag: Some(lag),
            },
        }
    }
}

impl Default for ConsistencyConfig {
    fn default() -> Self {
        Self { buffer_size: 5_000 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_enables_only_cohort_pipelines() {
        let layer = PipelineLayer::embedded();
        // Live cohort.
        assert!(layer.object_by_owner.is_some());
        assert!(layer.object_by_type.is_some());
        assert!(layer.balance.is_some());
        // History cohort (object_version_by_checkpoint and
        // package_versions are also restored).
        assert!(layer.epochs.is_some());
        assert!(layer.object_version_by_checkpoint.is_some());
        assert!(layer.package_versions.is_some());
        assert!(layer.tx_seq_by_digest.is_some());
        assert!(layer.tx_metadata_by_seq.is_some());
        assert!(layer.transaction_bitmap.is_some());
        assert!(layer.event_bitmap.is_some());
        // Deactivated: served directly by the perpetual store.
        assert!(layer.objects.is_none());
        assert!(layer.transactions.is_none());
        assert!(layer.effects.is_none());
        assert!(layer.events.is_none());
        assert!(layer.checkpoint_summary.is_none());
        assert!(layer.checkpoint_contents.is_none());
        assert!(layer.checkpoint_seq_by_digest.is_none());
    }

    #[test]
    fn all_enables_every_pipeline() {
        let layer = PipelineLayer::all();
        assert!(layer.epochs.is_some());
        assert!(layer.checkpoint_summary.is_some());
        assert!(layer.transactions.is_some());
        assert!(layer.objects.is_some());
        assert!(layer.object_by_owner.is_some());
        assert!(layer.balance.is_some());
        assert!(layer.event_bitmap.is_some());
    }

    #[test]
    fn pruning_disabled_by_default() {
        // A default ServiceConfig (the embedded-fullnode shape)
        // leaves pruning off; `example()` surfaces it populated.
        assert!(ServiceConfig::default().pruner.is_none());
        assert!(ServiceConfig::example().pruner.is_some());
    }

    #[test]
    fn pruner_config_interval_round_trips() {
        let cfg = PrunerConfig {
            interval_ms: 1_500,
            ..PrunerConfig::default()
        };
        assert_eq!(cfg.interval(), std::time::Duration::from_millis(1_500));
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

    #[test]
    fn availability_within_tip_respects_lag() {
        let a = PipelineAvailability::MaxCheckpointLag(100);
        // At the tip, and exactly at the lag boundary (inclusive), are available.
        assert!(a.is_available(Some(1_000_000), 1_000_000));
        assert!(a.is_available(Some(999_900), 1_000_000));
        // One checkpoint beyond the lag budget is unavailable.
        assert!(!a.is_available(Some(999_899), 1_000_000));
        // A watermark momentarily ahead of the recorded tip saturates to zero lag.
        assert!(a.is_available(Some(1_000_050), 1_000_000));
        // No watermark yet ⇒ not caught up ⇒ unavailable.
        assert!(!a.is_available(None, 1_000_000));
    }

    #[test]
    fn enabled_and_disabled_ignore_lag() {
        assert!(PipelineAvailability::Enabled.is_available(None, 1_000_000));
        assert!(!PipelineAvailability::Disabled.is_available(Some(1_000_000), 1_000_000));
    }

    #[test]
    fn availability_default_and_overrides_parse_from_toml() {
        let config: ServiceConfig = toml::from_str(
            r#"
            [availability]
            max-checkpoint-lag = 100

            [pipeline.balance.availability]
            enabled = false

            [pipeline.object-by-owner.availability]
            enabled = true

            [pipeline.epochs.availability]
            max-checkpoint-lag = 1000
            "#,
        )
        .unwrap();

        let availability = config.availability_config();
        assert_eq!(
            availability.policy_for("balance"),
            Some(PipelineAvailability::Disabled)
        );
        assert_eq!(
            availability.policy_for("object_by_owner"),
            Some(PipelineAvailability::Enabled)
        );
        assert_eq!(
            availability.policy_for("epochs"),
            Some(PipelineAvailability::MaxCheckpointLag(1000))
        );
        // A pipeline with no override falls back to the default.
        assert_eq!(
            availability.policy_for("object_by_type"),
            Some(PipelineAvailability::MaxCheckpointLag(100))
        );
    }

    #[test]
    fn overrides_without_a_default_gate_only_themselves() {
        let config: ServiceConfig = toml::from_str(
            r#"
            [pipeline.balance.availability]
            enabled = false
            "#,
        )
        .unwrap();

        let availability = config.availability_config();
        assert_eq!(
            availability.policy_for("balance"),
            Some(PipelineAvailability::Disabled)
        );
        assert_eq!(availability.policy_for("object_by_owner"), None);
        assert!(!availability.is_trivial());
        assert!(ServiceConfig::default().availability_config().is_trivial());
    }

    #[test]
    fn availability_enabled_and_lag_are_mutually_exclusive() {
        let err = toml::from_str::<ServiceConfig>(
            r#"
            [pipeline.balance.availability]
            enabled = true
            max-checkpoint-lag = 100
            "#,
        )
        .err()
        .unwrap();
        assert!(
            err.to_string().contains("mutually exclusive"),
            "unexpected error: {err}",
        );
    }

    #[test]
    fn empty_availability_sections_are_rejected() {
        // A policy section exists only to set a policy, so one of its keys is mandatory.
        for toml in ["[availability]", "[pipeline.balance.availability]"] {
            let err = toml::from_str::<ServiceConfig>(toml).err().unwrap();
            assert!(
                err.to_string()
                    .contains("expected 'enabled' or 'max-checkpoint-lag'"),
                "unexpected error for {toml:?}: {err}",
            );
        }
    }

    #[test]
    fn availability_sections_reject_unknown_fields() {
        for toml in [
            "[availability]\nmode = \"off\"",
            "[pipeline.balance]\nmode = \"off\"",
            "[pipeline.balance.availability]\nenabled = true\nmode = \"off\"",
            // Old flat committer keys must fail loudly now that they nest
            // under `[pipeline.<name>.committer]`.
            "[pipeline.balance]\nwrite-concurrency = 4",
            // Misspelled pipeline names are no longer silently ignored.
            "[pipeline.balanec]\n",
        ] {
            let result = toml::from_str::<ServiceConfig>(toml);
            assert!(result.is_err(), "expected {toml:?} to be rejected");
        }
    }

    #[test]
    fn nested_committer_overrides_still_merge() {
        let config: ServiceConfig = toml::from_str(
            r#"
            [pipeline.balance.committer]
            write-concurrency = 8
            "#,
        )
        .unwrap();
        let merged = config
            .pipeline
            .balance
            .unwrap()
            .committer
            .finish(CommitterConfig::default());
        assert_eq!(merged.write_concurrency, 8);
    }

    #[test]
    fn empty_pipeline_section_registers_with_defaults_and_no_policy() {
        let config: ServiceConfig = toml::from_str("[pipeline.balance]").unwrap();
        assert!(config.pipeline.balance.is_some());
        assert!(config.availability_config().is_trivial());
    }

    #[test]
    fn availability_policies_roundtrip_through_toml() {
        for policy in [
            PipelineAvailability::Enabled,
            PipelineAvailability::Disabled,
            PipelineAvailability::MaxCheckpointLag(100),
        ] {
            let config = ServiceConfig {
                availability: Some(policy),
                ..ServiceConfig::default()
            };
            let serialized = toml::to_string_pretty(&config).unwrap();
            let parsed: ServiceConfig = toml::from_str(&serialized).unwrap();
            assert_eq!(
                parsed.availability,
                Some(policy),
                "roundtrip failed for {policy:?}:\n{serialized}",
            );
        }
    }

    #[test]
    fn availability_override_keys_match_pipeline_names() {
        let mut layer = PipelineLayer::all();
        for entry in [
            &mut layer.epochs,
            &mut layer.checkpoint_summary,
            &mut layer.checkpoint_contents,
            &mut layer.checkpoint_seq_by_digest,
            &mut layer.transactions,
            &mut layer.tx_seq_by_digest,
            &mut layer.tx_metadata_by_seq,
            &mut layer.effects,
            &mut layer.events,
            &mut layer.objects,
            &mut layer.object_version_by_checkpoint,
            &mut layer.object_by_owner,
            &mut layer.object_by_type,
            &mut layer.balance,
            &mut layer.package_versions,
            &mut layer.transaction_bitmap,
            &mut layer.event_bitmap,
        ] {
            entry.as_mut().unwrap().availability = Some(PipelineAvailability::Enabled);
        }

        let keys: std::collections::BTreeSet<_> =
            layer.availability_overrides().into_keys().collect();
        let names: std::collections::BTreeSet<_> = crate::indexer::restore::ALL_PIPELINES
            .iter()
            .map(|n| n.to_string())
            .collect();
        assert_eq!(keys, names);
    }
}
