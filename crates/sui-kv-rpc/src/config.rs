// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Archival (BigTable) KV RPC configuration.

use std::num::NonZeroU32;
use std::path::Path;
use std::time::Duration;

use anyhow::Context;
use serde::Deserialize;
use serde::Serialize;
use sui_inverted_index::SkipPolicy;
use sui_kvstore::PoolConfig;
use sui_kvstore::validate_pipeline_name;

use crate::default_service_info_watermark_pipelines;

const DEFAULT_LEDGER_HISTORY_METHOD_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_RENDER_AHEAD: usize = 4;
const DEFAULT_BITMAP_BUCKET_BUDGET_TX: u64 = 1_024;
const DEFAULT_BITMAP_BUCKET_BUDGET_EVENT: u64 = 1_024;
const DEFAULT_MAX_BITMAP_FILTER_LITERALS: usize = 10;
const DEFAULT_BITMAP_DRAIN_PROBE_ROWS: u32 = 50;
const DEFAULT_REQUEST_BIGTABLE_CONCURRENCY: usize = 10;
const DEFAULT_STAGE_CHUNK_SIZE: usize = 100;
const DEFAULT_STAGE_CONCURRENCY: usize = 10;

/// Built-in per-endpoint defaults. These differ per endpoint (e.g. checkpoints
/// page smaller than transactions).
struct LedgerHistoryMethodDefaults {
    default_limit_items: u32,
    max_limit_items: u32,
    render_ahead: usize,
}

const LIST_TRANSACTIONS_DEFAULTS: LedgerHistoryMethodDefaults = LedgerHistoryMethodDefaults {
    default_limit_items: 50,
    max_limit_items: 500,
    render_ahead: DEFAULT_RENDER_AHEAD,
};
const LIST_EVENTS_DEFAULTS: LedgerHistoryMethodDefaults = LedgerHistoryMethodDefaults {
    default_limit_items: 50,
    max_limit_items: 1_000,
    render_ahead: DEFAULT_RENDER_AHEAD,
};
const LIST_CHECKPOINTS_DEFAULTS: LedgerHistoryMethodDefaults = LedgerHistoryMethodDefaults {
    default_limit_items: 10,
    max_limit_items: 100,
    render_ahead: DEFAULT_RENDER_AHEAD,
};

/// Per-endpoint tunables for one ledger-history list API. Every field is
/// optional and falls back to a built-in default; see
/// [`ResolvedLedgerHistoryMethodConfig`].
#[derive(Clone, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct LedgerHistoryMethodConfig {
    /// Per-request wall-clock timeout, in milliseconds. Defaults to `5000`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,

    /// Page size used when a request omits `limit_items`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_limit_items: Option<u32>,

    /// Upper bound a request's `limit_items` is clamped to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_limit_items: Option<u32>,

    /// Bounds admitted render tasks and retained rendered responses for this
    /// endpoint's render-ahead stage. Defaults to `4`; `1` serializes rendering
    /// with emission.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render_ahead: Option<usize>,
}

/// A [`LedgerHistoryMethodConfig`] with all defaults applied.
#[derive(Clone, Copy, Debug)]
pub struct ResolvedLedgerHistoryMethodConfig {
    pub timeout: Duration,
    pub default_limit_items: u32,
    pub max_limit_items: u32,
    pub render_ahead: usize,
}

impl LedgerHistoryMethodConfig {
    fn resolve(
        this: Option<&LedgerHistoryMethodConfig>,
        defaults: LedgerHistoryMethodDefaults,
    ) -> ResolvedLedgerHistoryMethodConfig {
        ResolvedLedgerHistoryMethodConfig {
            timeout: Duration::from_millis(
                this.and_then(|c| c.timeout_ms)
                    .unwrap_or(DEFAULT_LEDGER_HISTORY_METHOD_TIMEOUT_MS),
            ),
            default_limit_items: this
                .and_then(|c| c.default_limit_items)
                .unwrap_or(defaults.default_limit_items),
            max_limit_items: this
                .and_then(|c| c.max_limit_items)
                .unwrap_or(defaults.max_limit_items),
            render_ahead: this
                .and_then(|c| c.render_ahead)
                .unwrap_or(defaults.render_ahead),
        }
    }
}

/// One read pipeline stage, identified by the BigTable table it scans. The same
/// stage is shared by every list method that reads that table, so its cost is a
/// property of the table (e.g. object rows are large, `tx_seq_digest` rows tiny)
/// rather than the calling method.
#[derive(Clone, Copy, Debug)]
pub enum PipelineStage {
    TxSeqDigest,
    Transactions,
    Objects,
    Checkpoints,
}

/// Per-stage tunables. Every field is optional and falls back to a built-in
/// default; see [`ResolvedStageConfig`].
#[derive(Clone, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct StageConfig {
    /// Items per multi-get batch issued to this stage's table. Defaults to `100`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_size: Option<usize>,

    /// Max in-flight chunk fetches for this stage. Independent of
    /// `request_bigtable_concurrency`, which still caps total concurrent reads
    /// across all stages within a request. Defaults to `10`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<usize>,
}

/// A [`StageConfig`] with all defaults applied.
#[derive(Clone, Copy, Debug)]
pub struct ResolvedStageConfig {
    pub chunk_size: usize,
    pub concurrency: usize,
}

impl StageConfig {
    fn resolve(this: Option<&StageConfig>) -> ResolvedStageConfig {
        ResolvedStageConfig {
            chunk_size: this
                .and_then(|c| c.chunk_size)
                .unwrap_or(DEFAULT_STAGE_CHUNK_SIZE),
            concurrency: this
                .and_then(|c| c.concurrency)
                .unwrap_or(DEFAULT_STAGE_CONCURRENCY),
        }
    }
}

/// Per-table-stage pipeline tunables, shared across the list methods that read
/// each table.
#[derive(Clone, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct StagesConfig {
    /// Reads of the `tx_seq_digest` table (sequence number -> digest/checkpoint).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_seq_digest: Option<StageConfig>,

    /// Reads of the `transactions` table (digest -> transaction body).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transactions: Option<StageConfig>,

    /// Multi-get reads of the `objects` table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub objects: Option<StageConfig>,

    /// Reads of the `checkpoints` table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoints: Option<StageConfig>,
}

impl StagesConfig {
    fn get(&self, stage: PipelineStage) -> Option<&StageConfig> {
        match stage {
            PipelineStage::TxSeqDigest => self.tx_seq_digest.as_ref(),
            PipelineStage::Transactions => self.transactions.as_ref(),
            PipelineStage::Objects => self.objects.as_ref(),
            PipelineStage::Checkpoints => self.checkpoints.as_ref(),
        }
    }

    /// Resolved tunables for one pipeline stage. Unset stages fall back to the
    /// built-in chunk size (`100`) and stage concurrency (`10`); the latter is
    /// independent of `request_bigtable_concurrency`.
    pub fn stage(&self, stage: PipelineStage) -> ResolvedStageConfig {
        StageConfig::resolve(self.get(stage))
    }
}

/// Tunables for the ledger-history list APIs. Per-endpoint knobs live in
/// the three [`LedgerHistoryMethodConfig`] fields; the remaining knobs are
/// global across all three. Every field is optional and falls back to a built-in
/// default.
#[derive(Clone, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct LedgerHistoryConfig {
    /// Per-endpoint tunables for `list_transactions`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_transactions: Option<LedgerHistoryMethodConfig>,

    /// Per-endpoint tunables for `list_events`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_events: Option<LedgerHistoryMethodConfig>,

    /// Per-endpoint tunables for `list_checkpoints`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_checkpoints: Option<LedgerHistoryMethodConfig>,

    /// Per-request evaluated-bucket budget for filtered tx-bitmap scans, shared
    /// across all DNF dimensions of one query. Caps how many fetched buckets the
    /// eval evaluates, NOT how many bucket reads BigTable receives — at
    /// exhaustion each leaf stream may have fetched one additional bucket that is
    /// discarded rather than evaluated, so observed reads can exceed this by up
    /// to `max_bitmap_filter_literals`. Exhausting it ends the query with
    /// `SCAN_LIMIT` and a resume cursor.
    ///
    /// Defaults to `1024` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitmap_bucket_budget_tx: Option<u64>,

    /// Per-request evaluated-bucket budget for filtered event-bitmap scans. Event
    /// buckets cover far fewer source-domain positions than tx buckets, so this
    /// is tuned separately even though both default to the same number. Same
    /// fetched-vs-evaluated semantics as `bitmap_bucket_budget_tx`.
    ///
    /// Defaults to `1024` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitmap_bucket_budget_event: Option<u64>,

    /// Dead bucket rows drained from an open leaf scan per lag episode before
    /// abandoning the stream and reopening it past the gap. A fresh reopen is
    /// disabled by `0`; provably dead gaps still advance the resume cursor.
    ///
    /// Defaults to `50` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitmap_drain_probe_rows: Option<u32>,

    /// Maximum total filter literals (bitmap dimensions) accepted in one filtered
    /// request, across all DNF terms. Each literal becomes one bitmap leaf, so
    /// this bounds a single filter's scan fanout. Must not exceed either bitmap
    /// budget (see [`LedgerHistoryConfig::validate`]).
    ///
    /// Defaults to `10` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_bitmap_filter_literals: Option<usize>,
}

impl LedgerHistoryConfig {
    pub fn list_transactions(&self) -> ResolvedLedgerHistoryMethodConfig {
        LedgerHistoryMethodConfig::resolve(
            self.list_transactions.as_ref(),
            LIST_TRANSACTIONS_DEFAULTS,
        )
    }

    pub fn list_events(&self) -> ResolvedLedgerHistoryMethodConfig {
        LedgerHistoryMethodConfig::resolve(self.list_events.as_ref(), LIST_EVENTS_DEFAULTS)
    }

    pub fn list_checkpoints(&self) -> ResolvedLedgerHistoryMethodConfig {
        LedgerHistoryMethodConfig::resolve(
            self.list_checkpoints.as_ref(),
            LIST_CHECKPOINTS_DEFAULTS,
        )
    }

    pub fn bitmap_bucket_budget_tx(&self) -> u64 {
        self.bitmap_bucket_budget_tx
            .unwrap_or(DEFAULT_BITMAP_BUCKET_BUDGET_TX)
    }

    pub fn bitmap_bucket_budget_event(&self) -> u64 {
        self.bitmap_bucket_budget_event
            .unwrap_or(DEFAULT_BITMAP_BUCKET_BUDGET_EVENT)
    }
    pub fn bitmap_skip_policy(&self) -> SkipPolicy {
        SkipPolicy {
            drain_probe_rows: NonZeroU32::new(
                self.bitmap_drain_probe_rows
                    .unwrap_or(DEFAULT_BITMAP_DRAIN_PROBE_ROWS),
            ),
        }
    }

    pub fn max_bitmap_filter_literals(&self) -> usize {
        self.max_bitmap_filter_literals
            .unwrap_or(DEFAULT_MAX_BITMAP_FILTER_LITERALS)
    }

    /// Reject configurations that cannot make forward progress. Each filter
    /// literal becomes one bitmap leaf that must fetch at least one bucket to
    /// emit its first watermark; if a per-request budget is below the literal
    /// cap a `SCAN_LIMIT` can fire before any merged watermark reaches the wire,
    /// leaving the client a cursorless `QueryEnd` it cannot resume from. Mirrors
    /// the fullnode side's `LedgerHistoryConfig::validate`.
    pub fn validate(&self) -> anyhow::Result<()> {
        for (name, endpoint) in [
            ("list_transactions", self.list_transactions()),
            ("list_events", self.list_events()),
            ("list_checkpoints", self.list_checkpoints()),
        ] {
            anyhow::ensure!(
                endpoint.render_ahead > 0,
                "ledger_history.{name}.render_ahead must be greater than zero",
            );
        }
        anyhow::ensure!(
            self.max_bitmap_filter_literals() > 0,
            "ledger_history.max_bitmap_filter_literals must be greater than zero",
        );
        anyhow::ensure!(
            self.bitmap_bucket_budget_tx() >= self.max_bitmap_filter_literals() as u64,
            "ledger_history.bitmap_bucket_budget_tx ({}) must be >= \
             max_bitmap_filter_literals ({}) so every leaf stream gets at least \
             one bucket before SCAN_LIMIT",
            self.bitmap_bucket_budget_tx(),
            self.max_bitmap_filter_literals(),
        );
        anyhow::ensure!(
            self.bitmap_bucket_budget_event() >= self.max_bitmap_filter_literals() as u64,
            "ledger_history.bitmap_bucket_budget_event ({}) must be >= \
             max_bitmap_filter_literals ({}) so every leaf stream gets at least \
             one bucket before SCAN_LIMIT",
            self.bitmap_bucket_budget_event(),
            self.max_bitmap_filter_literals(),
        );
        Ok(())
    }
}

const DEFAULT_ADDRESS: &str = "[::1]:8000";
const DEFAULT_METRICS_HOST: &str = "127.0.0.1";
const DEFAULT_METRICS_PORT: u16 = 9184;

/// Root archival KV RPC config, deserialized from a YAML file (`--config-path`).
///
/// Every field is optional and falls back to a built-in default via the
/// accessors below; `instance_id` is the sole required field and is resolved by
/// the binary (it may also still be supplied by the deprecated CLI flag).
#[derive(Clone, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct KvRpcConfig {
    /// BigTable instance id to read from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,

    /// GCP project id for the BigTable instance (defaults to the token
    /// provider's project).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bigtable_project: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_profile_id: Option<String>,

    /// Path to a GCP service account JSON key file. If unset, Application
    /// Default Credentials are used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credentials: Option<String>,

    /// Channel-level timeout in milliseconds for BigTable gRPC calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bigtable_channel_timeout_ms: Option<u64>,

    /// Address the gRPC server listens on. Defaults to `[::1]:8000`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,

    /// Host the Prometheus metrics server binds to. Defaults to `127.0.0.1`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics_host: Option<String>,

    /// Port the Prometheus metrics server binds to. Defaults to `9184`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics_port: Option<u16>,

    /// PEM TLS certificate path. TLS is enabled only when both cert and key are
    /// set and non-empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_cert: Option<String>,

    /// PEM TLS private key path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_key: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bigtable_initial_pool_size: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bigtable_min_pool_size: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bigtable_max_pool_size: Option<usize>,

    /// Pipeline watermarks to include when reporting GetServiceInfo checkpoint
    /// height. When unset, derived from `enable_experimental_query_apis`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watermark_pipeline: Option<Vec<String>>,

    /// Enable the List APIs (and their alpha service-info pipelines).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_experimental_query_apis: Option<bool>,

    /// Tunables for the ledger-history list APIs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ledger_history: Option<LedgerHistoryConfig>,

    /// Per-request semaphore capacity gating downstream BigTable reads, shared by
    /// the point-get and list APIs. Bitmap scans are not gated by this; their
    /// fanout is bounded by `ledger_history.max_bitmap_filter_literals`.
    ///
    /// Defaults to `10` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_bigtable_concurrency: Option<usize>,

    /// Per-table-stage read pipeline tunables (chunk size and fan-out
    /// concurrency), shared by the point-get and list APIs. Bounded by
    /// `request_bigtable_concurrency` at runtime, which remains the request-wide
    /// read ceiling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stages: Option<StagesConfig>,
}

impl KvRpcConfig {
    /// Deserialize a [`KvRpcConfig`] from a YAML file. Kept inline (rather than
    /// via `sui_config::Config`) so the archival binary stays decoupled from the
    /// fullnode config crate.
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        serde_yaml::from_str(&contents)
            .with_context(|| format!("failed to parse config file {}", path.display()))
    }

    /// Render the JSON Schema for the config file (backs the `--config-schema`
    /// flag). Field descriptions are pulled from the `///` doc comments above,
    /// so this is the single source of truth for the file format.
    pub fn schema_json() -> anyhow::Result<String> {
        let schema = schemars::schema_for!(KvRpcConfig);
        Ok(serde_json::to_string_pretty(&schema)?)
    }

    pub fn address(&self) -> &str {
        self.address.as_deref().unwrap_or(DEFAULT_ADDRESS)
    }

    pub fn metrics_host(&self) -> &str {
        self.metrics_host.as_deref().unwrap_or(DEFAULT_METRICS_HOST)
    }

    pub fn metrics_port(&self) -> u16 {
        self.metrics_port.unwrap_or(DEFAULT_METRICS_PORT)
    }

    pub fn channel_timeout(&self) -> Option<Duration> {
        self.bigtable_channel_timeout_ms.map(Duration::from_millis)
    }

    pub fn enable_experimental_query_apis(&self) -> bool {
        self.enable_experimental_query_apis.unwrap_or(false)
    }

    /// TLS identity, when both cert and key are set and non-empty.
    pub fn tls_identity(&self) -> anyhow::Result<Option<tonic::transport::Identity>> {
        let (Some(cert), Some(key)) = (self.tls_cert.as_deref(), self.tls_key.as_deref()) else {
            return Ok(None);
        };
        if cert.is_empty() || key.is_empty() {
            return Ok(None);
        }
        Ok(Some(tonic::transport::Identity::from_pem(
            std::fs::read(cert)?,
            std::fs::read(key)?,
        )))
    }

    pub fn pool_config(&self) -> PoolConfig {
        let base = PoolConfig::default();
        PoolConfig {
            initial_pool_size: self
                .bigtable_initial_pool_size
                .unwrap_or(base.initial_pool_size),
            min_pool_size: self.bigtable_min_pool_size.unwrap_or(base.min_pool_size),
            max_pool_size: self.bigtable_max_pool_size.unwrap_or(base.max_pool_size),
            ..base
        }
    }

    /// Resolve the service-info watermark pipelines: an explicit non-empty
    /// `watermark_pipeline` (validated against the known pipeline names) takes
    /// precedence, otherwise the set is derived from
    /// `enable_experimental_query_apis`.
    pub fn service_info_watermark_pipelines(&self) -> anyhow::Result<Vec<&'static str>> {
        match self.watermark_pipeline.as_deref() {
            Some(pipelines) if !pipelines.is_empty() => pipelines
                .iter()
                .map(|name| validate_pipeline_name(name).map_err(anyhow::Error::msg))
                .collect(),
            _ => Ok(default_service_info_watermark_pipelines(
                self.enable_experimental_query_apis(),
            )),
        }
    }

    pub fn ledger_history(&self) -> LedgerHistoryConfig {
        self.ledger_history.clone().unwrap_or_default()
    }

    pub fn request_bigtable_concurrency(&self) -> usize {
        self.request_bigtable_concurrency
            .unwrap_or(DEFAULT_REQUEST_BIGTABLE_CONCURRENCY)
    }

    pub fn stages(&self) -> StagesConfig {
        self.stages.clone().unwrap_or_default()
    }

    /// Reject configurations that cannot make forward progress. Validates the
    /// shared read-pipeline knobs (concurrency and per-stage chunk/fan-out) and
    /// delegates to [`LedgerHistoryConfig::validate`] for the list-API knobs.
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.request_bigtable_concurrency() > 0,
            "request_bigtable_concurrency must be greater than zero",
        );
        let stages = self.stages();
        for stage in [
            PipelineStage::TxSeqDigest,
            PipelineStage::Transactions,
            PipelineStage::Objects,
            PipelineStage::Checkpoints,
        ] {
            let resolved = stages.stage(stage);
            anyhow::ensure!(
                resolved.chunk_size > 0,
                "stages.{stage:?} chunk_size must be greater than zero",
            );
            anyhow::ensure!(
                resolved.concurrency > 0,
                "stages.{stage:?} concurrency must be greater than zero",
            );
        }
        self.ledger_history().validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_budget_below_literal_cap() {
        // Per-request budgets must be at least the accepted-literal cap, so
        // every leaf bitmap stream gets a bucket fetch before SCAN_LIMIT.
        let mut cfg = LedgerHistoryConfig {
            max_bitmap_filter_literals: Some(10),
            bitmap_bucket_budget_tx: Some(5),
            bitmap_bucket_budget_event: Some(10),
            ..Default::default()
        };
        let err = cfg.validate().expect_err("budget < literal cap must fail");
        assert!(
            err.to_string().contains("bitmap_bucket_budget_tx"),
            "error mentions the tx budget: {err}"
        );

        cfg.bitmap_bucket_budget_tx = Some(10);
        cfg.bitmap_bucket_budget_event = Some(9);
        let err = cfg.validate().expect_err("budget < literal cap must fail");
        assert!(
            err.to_string().contains("bitmap_bucket_budget_event"),
            "error mentions the event budget: {err}"
        );

        cfg.bitmap_bucket_budget_event = Some(10);
        cfg.validate()
            .expect("equal budget and literal cap is valid");
    }

    #[test]
    fn validate_accepts_defaults() {
        LedgerHistoryConfig::default().validate().unwrap();
        assert_eq!(
            LedgerHistoryConfig::default()
                .list_checkpoints()
                .render_ahead,
            4
        );
        assert_eq!(
            LedgerHistoryConfig::default()
                .list_transactions()
                .render_ahead,
            4
        );
        assert_eq!(LedgerHistoryConfig::default().list_events().render_ahead, 4);
    }

    #[test]
    fn stage_override_wins_and_siblings_default() {
        let yaml = r#"
stages:
  objects:
    concurrency: 4
  transactions:
    chunk-size: 25
"#;
        let cfg: KvRpcConfig = serde_yaml::from_str(yaml).unwrap();
        let stages = cfg.stages();

        let objects = stages.stage(PipelineStage::Objects);
        assert_eq!(objects.concurrency, 4);
        // Unset sibling field on the same stage falls back to its default.
        assert_eq!(objects.chunk_size, 100);

        let transactions = stages.stage(PipelineStage::Transactions);
        assert_eq!(transactions.chunk_size, 25);
        assert_eq!(transactions.concurrency, 10);

        // Untouched stage is fully default.
        let checkpoints = stages.stage(PipelineStage::Checkpoints);
        assert_eq!(checkpoints.chunk_size, 100);
        assert_eq!(checkpoints.concurrency, 10);

        cfg.validate().unwrap();
    }

    #[test]
    fn validate_rejects_zero_stage_knob() {
        let cfg = KvRpcConfig {
            stages: Some(StagesConfig {
                objects: Some(StageConfig {
                    chunk_size: Some(0),
                    concurrency: None,
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let err = cfg.validate().expect_err("zero chunk_size must fail");
        assert!(err.to_string().contains("chunk_size"), "{err}");

        let cfg = KvRpcConfig {
            stages: Some(StagesConfig {
                transactions: Some(StageConfig {
                    chunk_size: None,
                    concurrency: Some(0),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let err = cfg.validate().expect_err("zero concurrency must fail");
        assert!(err.to_string().contains("concurrency"), "{err}");
    }

    #[test]
    fn validate_rejects_zero_render_ahead() {
        let cfg = LedgerHistoryConfig {
            list_events: Some(LedgerHistoryMethodConfig {
                render_ahead: Some(0),
                ..Default::default()
            }),
            ..Default::default()
        };
        let err = cfg
            .validate()
            .expect_err("zero render_ahead must fail validation");
        assert!(
            err.to_string()
                .contains("ledger_history.list_events.render_ahead"),
            "{err}"
        );
    }

    #[test]
    fn partial_yaml_falls_back_to_defaults() {
        // Only one endpoint is customized; everything else must resolve to defaults.
        let yaml = r#"
instance-id: my-instance
ledger-history:
  list-transactions:
    max-limit-items: 7
    render-ahead: 3
  bitmap-bucket-budget-tx: 2048
"#;
        let cfg: KvRpcConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.instance_id.as_deref(), Some("my-instance"));
        assert_eq!(cfg.address(), DEFAULT_ADDRESS);
        assert_eq!(cfg.metrics_port(), DEFAULT_METRICS_PORT);

        let lh = cfg.ledger_history();
        assert_eq!(lh.list_transactions().max_limit_items, 7);
        // Untouched sibling falls back to the per-endpoint default.
        assert_eq!(lh.list_transactions().default_limit_items, 50);
        assert_eq!(lh.bitmap_bucket_budget_tx(), 2048);
        assert_eq!(lh.list_transactions().render_ahead, 3);
        assert_eq!(lh.list_events().render_ahead, 4);
        assert_eq!(lh.bitmap_bucket_budget_event(), 1024);
        lh.validate().unwrap();
    }
}
