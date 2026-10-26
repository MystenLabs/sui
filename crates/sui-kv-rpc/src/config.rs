// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Archival (BigTable) KV RPC configuration.

use std::num::NonZeroU32;
use std::num::NonZeroU64;
use std::path::Path;
use std::time::Duration;

use anyhow::Context;
use serde::Deserialize;
use serde::Serialize;
use sui_inverted_index::SkipPolicy;
use sui_kvstore::PoolConfig;
use sui_kvstore::ReadRowsRetryConfig;

const DEFAULT_LEDGER_HISTORY_METHOD_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_RENDER_AHEAD: usize = 4;
const DEFAULT_BITMAP_BUCKET_BUDGET_TX: u64 = 4_000;
const DEFAULT_BITMAP_BUCKET_BUDGET_EVENT: u64 = 4_000;
const DEFAULT_MAX_BITMAP_FILTER_LITERALS: usize = 10;
// Comfortably above the 10ms watermark refresh: a replica slower than this is genuinely behind,
// and the client is better off retrying elsewhere.
const DEFAULT_MONOTONIC_READ_WAIT_TIMEOUT_MS: u64 = 100;
const DEFAULT_BITMAP_DRAIN_PROBE_ROWS: u32 = 50;
const DEFAULT_WATERMARK_BEACON_BUCKETS: NonZeroU64 = NonZeroU64::new(256).unwrap();
const DEFAULT_REQUEST_BIGTABLE_CONCURRENCY: usize = 50;
// Largest `tx_seq_digest` read size measured: reads of up to 100 keys within
// one or two buckets cost about one seek.
const DEFAULT_MAX_KEYS_PER_READ: usize = 100;
// Largest flow-control window HTTP/2 allows (RFC 9113 §6.9.1); h2 panics on a larger one.
const MAX_HTTP2_WINDOW_BYTES: u32 = (1 << 31) - 1;

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
    max_limit_items: 50,
    render_ahead: DEFAULT_RENDER_AHEAD,
};

/// Per-endpoint tunables for one ledger-history list API. Every field is
/// optional and falls back to a built-in default; see
/// [`ResolvedLedgerHistoryMethodConfig`].
#[derive(Clone, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct LedgerHistoryMethodConfig {
    /// Wall-clock budget covering computation and response delivery. Defaults
    /// to `30000`
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

/// One read pipeline stage. Most stages are identified by the BigTable table
/// they read and are shared by every method that reads that table, so their
/// cost is a property of the table (e.g. object rows are large,
/// `tx_seq_digest` rows tiny) rather than the calling method. `TxCpResolve`
/// also reads `tx_seq_digest`, but has its own settings because its reads run
/// ahead of a limit they cannot see (see [`StagesConfig::tx_cp_resolve`]).
#[derive(Clone, Copy, Debug)]
pub enum PipelineStage {
    TxSeqDigest,
    TxCpResolve,
    Transactions,
    Objects,
    Checkpoints,
}

impl PipelineStage {
    /// Whether a seek on this stage is a bucket of 1,000 consecutive
    /// transactions rather than a key. Both stages read `tx_seq_digest`, whose
    /// rows for one bucket sit next to each other, so a read pays about one
    /// seek per bucket, however many of its keys it carries; they cap keys
    /// separately (`max-keys-per-read`).
    pub fn counts_buckets(self) -> bool {
        matches!(
            self,
            PipelineStage::TxSeqDigest | PipelineStage::TxCpResolve
        )
    }
}

/// Per-stage tunables. Every field is optional and falls back to a built-in
/// default; see [`ResolvedStageConfig`].
#[derive(Clone, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct StageConfig {
    /// Most seeks one read of this stage may cost. A seek is one jump to a
    /// place in the table: one per key looked up, except on `tx-seq-digest`
    /// and `tx-cp-resolve`, where the `tx_seq_digest` rows of a bucket of
    /// 1,000 consecutive transactions sit next to each other, so one seek
    /// covers a bucket however many of its keys the read carries (those
    /// stages cap keys with `max-keys-per-read`). Defaults to `25` on
    /// `tx-seq-digest`, `tx-cp-resolve` and `transactions`, `100` elsewhere.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_seeks_per_read: Option<usize>,

    /// Max in-flight reads for this stage. Independent of
    /// `request_bigtable_concurrency`, which still caps total concurrent reads
    /// across all stages within a request. Defaults are stage-specific; see
    /// [`StagesConfig`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<usize>,

    /// When set, the work this stage has ready at once (a burst) is split over
    /// its free `concurrency` slots: up to one read per free slot, each
    /// costing at least this many seeks (unless the whole burst costs less)
    /// and at most `max-seeks-per-read`, so a small burst is fetched by
    /// parallel reads instead of one long multi-get. Unset, a burst goes out as
    /// the fewest reads of at most `max-seeks-per-read` seeks, except where
    /// `max-keys-per-read` ends reads early. Defaults to `4` on
    /// `tx-seq-digest`, `tx-cp-resolve` and `transactions`, unset elsewhere;
    /// on a stage with a default, set it to at least `max-seeks-per-read` to
    /// stop splitting. Applies to every stage. On `objects`, a seek is an
    /// object key the request has not already read or started reading, and
    /// one transaction's or checkpoint's objects can be divided between reads.
    /// On `transactions` it also covers the transaction reads of full-mask
    /// `list_checkpoints` and `get_checkpoint`, one key per transaction, so a
    /// checkpoint's transactions are split like any other burst, and those of
    /// `list_events`, where a seek is a transaction the request has not
    /// already read or started reading. On `checkpoints` it applies to
    /// filtered `list_checkpoints`. A read never spans a watermark, and a read
    /// that finds no free slot waits for one with its planned size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_seeks_per_split_read: Option<usize>,

    /// Most keys one read may carry, on `tx-seq-digest` and `tx-cp-resolve`
    /// only; rejected on other stages, where every key is its own seek and
    /// `max-seeks-per-read` already caps keys. A key is one `tx_seq_digest`
    /// row: one transaction, however many of its events `list_events` returns.
    /// A read ends at the last bucket boundary before this cap; a bucket is
    /// divided between reads only when it alone holds more keys, and then only
    /// between transactions. Reads aim at near-equal shares of a burst's
    /// seeks, so when this cap ends reads early, a burst can take more reads
    /// than the fewest possible, some of them costing fewer seeks than
    /// `min-seeks-per-split-read`. Defaults to `100`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_keys_per_read: Option<usize>,
}

/// A [`StageConfig`] with all defaults applied.
#[derive(Clone, Copy, Debug)]
pub struct ResolvedStageConfig {
    pub max_seeks_per_read: usize,
    pub concurrency: usize,
    pub min_seeks_per_split_read: Option<usize>,
    /// Set only on the stages whose seeks are buckets
    /// ([`PipelineStage::counts_buckets`]); elsewhere every key is a seek.
    pub max_keys_per_read: Option<usize>,
}

/// Built-in settings of one stage, used for every field its config leaves
/// unset.
struct StageDefaults {
    max_seeks_per_read: usize,
    concurrency: usize,
    min_seeks_per_split_read: Option<usize>,
}

impl StageConfig {
    fn resolve(
        this: Option<&StageConfig>,
        defaults: StageDefaults,
        counts_buckets: bool,
    ) -> ResolvedStageConfig {
        ResolvedStageConfig {
            max_seeks_per_read: this
                .and_then(|c| c.max_seeks_per_read)
                .unwrap_or(defaults.max_seeks_per_read),
            concurrency: this
                .and_then(|c| c.concurrency)
                .unwrap_or(defaults.concurrency),
            min_seeks_per_split_read: this
                .and_then(|c| c.min_seeks_per_split_read)
                .or(defaults.min_seeks_per_split_read),
            max_keys_per_read: counts_buckets.then(|| {
                this.and_then(|c| c.max_keys_per_read)
                    .unwrap_or(DEFAULT_MAX_KEYS_PER_READ)
            }),
        }
    }
}

/// Per-stage pipeline tunables, shared across the methods that run each stage.
#[derive(Clone, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct StagesConfig {
    /// Batched reads of the `tx_seq_digest` table by filtered `list_transactions`
    /// and `list_events` (sequence number -> digest). Defaults:
    /// `max-seeks-per-read` 25 (buckets), `max-keys-per-read` 100,
    /// `concurrency` 20, `min-seeks-per-split-read` 4.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_seq_digest: Option<StageConfig>,

    /// Batched transaction -> checkpoint lookups of filtered `list_checkpoints`,
    /// which read the `tx_seq_digest` table (metric stage label
    /// `tx_cp_resolve`). The page limit counts checkpoints, not the
    /// transactions looked up, so lookups run ahead of it and those still in
    /// flight when the page fills are cancelled; this stage's concurrency
    /// bounds that overshoot independently of `tx_seq_digest`. The
    /// per-watermark lookups of `list_events`, `list_transactions` and filtered
    /// `list_checkpoints` share the metric label but are issued one at a time
    /// and do not use this stage.
    /// Defaults: `max-seeks-per-read` 25 (buckets), `max-keys-per-read` 100,
    /// `concurrency` 4, `min-seeks-per-split-read` 4.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_cp_resolve: Option<StageConfig>,

    /// Reads of the `transactions` table (digest -> transaction body).
    /// `list_events` reads a transaction once however many of its events it
    /// returns, so its reads are sized on the transactions still to read.
    /// Defaults: `max-seeks-per-read` 25, `concurrency` 25,
    /// `min-seeks-per-split-read` 4.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transactions: Option<StageConfig>,

    /// Multi-get reads of the `objects` table. A key the request has already
    /// read, or is reading, is not read again, so reads are sized on the object
    /// keys still to read. Defaults: `max-seeks-per-read` 100, `concurrency` 50.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub objects: Option<StageConfig>,

    /// Reads of the `checkpoints` table. Defaults: `max-seeks-per-read` 100,
    /// `concurrency` 10.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoints: Option<StageConfig>,
}

impl StagesConfig {
    fn get(&self, stage: PipelineStage) -> Option<&StageConfig> {
        match stage {
            PipelineStage::TxSeqDigest => self.tx_seq_digest.as_ref(),
            PipelineStage::TxCpResolve => self.tx_cp_resolve.as_ref(),
            PipelineStage::Transactions => self.transactions.as_ref(),
            PipelineStage::Objects => self.objects.as_ref(),
            PipelineStage::Checkpoints => self.checkpoints.as_ref(),
        }
    }

    /// Resolved tunables for one pipeline stage: each unset field takes the
    /// stage's default (see the fields of [`StagesConfig`]), and
    /// `max-keys-per-read` defaults to `100` on the two stages whose seeks are
    /// buckets.
    pub fn stage(&self, stage: PipelineStage) -> ResolvedStageConfig {
        let defaults = match stage {
            PipelineStage::TxSeqDigest => StageDefaults {
                max_seeks_per_read: 25,
                concurrency: 20,
                min_seeks_per_split_read: Some(4),
            },
            PipelineStage::TxCpResolve => StageDefaults {
                max_seeks_per_read: 25,
                concurrency: 4,
                min_seeks_per_split_read: Some(4),
            },
            PipelineStage::Transactions => StageDefaults {
                max_seeks_per_read: 25,
                concurrency: 25,
                min_seeks_per_split_read: Some(4),
            },
            PipelineStage::Objects => StageDefaults {
                max_seeks_per_read: 100,
                concurrency: 50,
                min_seeks_per_split_read: None,
            },
            PipelineStage::Checkpoints => StageDefaults {
                max_seeks_per_read: 100,
                concurrency: 10,
                min_seeks_per_split_read: None,
            },
        };
        StageConfig::resolve(self.get(stage), defaults, stage.counts_buckets())
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
    /// Defaults to `4000` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitmap_bucket_budget_tx: Option<u64>,

    /// Per-request evaluated-bucket budget for filtered event-bitmap scans. Event
    /// buckets cover far fewer source-domain positions than tx buckets, so this
    /// is tuned separately even though both default to the same number. Same
    /// fetched-vs-evaluated semantics as `bitmap_bucket_budget_tx`.
    ///
    /// Defaults to `4000` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitmap_bucket_budget_event: Option<u64>,

    /// Dead bucket rows drained from an open leaf scan per lag episode before
    /// abandoning the stream and reopening it past the gap. A fresh reopen is
    /// disabled by `0`; provably dead gaps still advance the resume cursor.
    /// This is also what a reopen counts as toward `watermark-beacon-buckets`.
    ///
    /// Defaults to `50` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitmap_drain_probe_rows: Option<u32>,

    /// How much a filtered List scan may scan without finding an item before
    /// the bitmap evaluator emits its newest scan watermark, which becomes a
    /// progress beacon. Scan work is counted in bucket rows: the rows charged
    /// to the scan budget (`bitmap-bucket-budget-*`), plus
    /// `bitmap-drain-probe-rows` for every gap catch-up step that reopened a
    /// leaf scan, since each reopen is a new BigTable read. Each beacon costs
    /// one BigTable read (`tx_cp_resolve`); items carry their own cursors, so
    /// beacons only flow while a scan crosses stretches without matches. The
    /// evaluator emits a watermark once this much work has been done since
    /// its last item or watermark; the scan's first watermark is not held
    /// back. A client cut off mid-scan can rescan more than this when it
    /// resumes: later stages merge watermarks that arrive while they still
    /// have frames ready, and items leave in chunks, so frames can trail the
    /// scan.
    ///
    /// Defaults to `256` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watermark_beacon_buckets: Option<NonZeroU64>,

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

    pub fn watermark_beacon_buckets(&self) -> NonZeroU64 {
        self.watermark_beacon_buckets
            .unwrap_or(DEFAULT_WATERMARK_BEACON_BUCKETS)
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

/// Root archival KV RPC config, deserialized from TOML by `--config`.
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

    /// Address a second, unencrypted gRPC listener binds to. Serves the same
    /// LedgerService as `address`, without TLS — for trusted internal callers
    /// (e.g. other in-cluster services) that should not need to negotiate
    /// TLS. A full socket address, so it can be bound to a narrower, private
    /// network interface than `address`. Unset disables this listener.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plaintext_address: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bigtable_initial_pool_size: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bigtable_min_pool_size: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bigtable_max_pool_size: Option<usize>,

    /// Detect stalled BigTable channels, quarantine them (cancelling their open
    /// requests so reads retry on other channels), and replace ones that stay
    /// unhealthy. Defaults to `true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bigtable_channel_health_check: Option<bool>,

    /// Longest a BigTable ReadRows attempt may wait for its response headers
    /// or its next response message before it is retried on another channel,
    /// in milliseconds. Defaults to `5000`. The final attempt has no gap
    /// timeout. A stall before the headers is retried only while
    /// `bigtable-channel-timeout-ms`, which ends that wait on every attempt, is
    /// longer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bigtable_read_rows_gap_timeout_ms: Option<u64>,

    /// Attempts per BigTable ReadRows call, including the first. Defaults to
    /// `3`; `1` turns off the gap timeout and retries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bigtable_read_rows_max_attempts: Option<u32>,

    /// HTTP/2 receive window of each BigTable response stream, in bytes: the
    /// most unread data one read can hold. Defaults to hyper's 2 MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bigtable_http2_stream_window_bytes: Option<u32>,

    /// HTTP/2 receive window of each BigTable connection, in bytes: the most
    /// unread data all reads on one channel can hold together. While unread
    /// data fills it, no read on that channel receives anything, so it should
    /// be at least `bigtable-http2-stream-window-bytes` times the most reads a
    /// channel carries at once. Defaults to hyper's 5 MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bigtable_http2_connection_window_bytes: Option<u32>,

    /// Deprecated and ignored: the `GetServiceInfo` watermark set is derived
    /// from `enable-list-apis` and can no longer be chosen per pipeline.
    /// Still parsed so existing config files keep loading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watermark_pipeline: Option<Vec<String>>,

    /// Serve the List APIs (`ListCheckpoints`, `ListTransactions`,
    /// `ListEvents`) and fold the pipelines backing them into the
    /// `GetServiceInfo` checkpoint height. Defaults to `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_list_apis: Option<bool>,

    /// Deprecated name for `enable-list-apis`. Used only when
    /// `enable-list-apis` is unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_experimental_query_apis: Option<bool>,

    /// Tunables for the ledger-history list APIs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ledger_history: Option<LedgerHistoryConfig>,

    /// Per-request semaphore capacity gating downstream BigTable reads, shared by
    /// the point-get and list APIs. Bitmap scans are not gated by this; their
    /// fanout is bounded by `ledger_history.max_bitmap_filter_literals`.
    ///
    /// Defaults to `50` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_bigtable_concurrency: Option<usize>,

    /// How long a request carrying `x-sui-min-checkpoint` waits for this replica's watermark to
    /// reach the requested checkpoint before being told to retry elsewhere. Applies to every method;
    /// requests without that header never wait.
    ///
    /// Defaults to `100` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monotonic_read_wait_timeout_ms: Option<u64>,

    /// Per-table-stage read pipeline tunables (read sizes and fan-out
    /// concurrency), shared by the point-get and list APIs. Bounded by
    /// `request_bigtable_concurrency` at runtime, which remains the request-wide
    /// read ceiling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stages: Option<StagesConfig>,
}

impl KvRpcConfig {
    /// Deserialize a [`KvRpcConfig`] from a TOML file.
    pub fn load_toml(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        toml::from_str(&contents)
            .with_context(|| format!("failed to parse TOML config file {}", path.display()))
    }

    /// Deserialize a [`KvRpcConfig`] from a YAML file accepted by the deprecated
    /// `--config-path` flag.
    pub fn load_yaml(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        serde_yaml::from_str(&contents)
            .with_context(|| format!("failed to parse YAML config file {}", path.display()))
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

    pub fn plaintext_address(&self) -> Option<&str> {
        self.plaintext_address.as_deref()
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

    pub fn enable_list_apis(&self) -> bool {
        self.enable_list_apis
            .or(self.enable_experimental_query_apis)
            .unwrap_or(false)
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
            health_check: base
                .health_check
                .filter(|_| self.bigtable_channel_health_check.unwrap_or(true)),
            http2_stream_window_bytes: self.bigtable_http2_stream_window_bytes,
            http2_connection_window_bytes: self.bigtable_http2_connection_window_bytes,
            ..base
        }
    }

    pub fn read_rows_retry(&self) -> ReadRowsRetryConfig {
        let base = ReadRowsRetryConfig::default();
        ReadRowsRetryConfig {
            gap_timeout: self
                .bigtable_read_rows_gap_timeout_ms
                .map_or(base.gap_timeout, Duration::from_millis),
            max_attempts: self
                .bigtable_read_rows_max_attempts
                .unwrap_or(base.max_attempts),
        }
    }

    pub fn ledger_history(&self) -> LedgerHistoryConfig {
        self.ledger_history.clone().unwrap_or_default()
    }

    pub fn request_bigtable_concurrency(&self) -> usize {
        self.request_bigtable_concurrency
            .unwrap_or(DEFAULT_REQUEST_BIGTABLE_CONCURRENCY)
    }

    pub fn monotonic_read_wait_timeout(&self) -> Duration {
        Duration::from_millis(
            self.monotonic_read_wait_timeout_ms
                .unwrap_or(DEFAULT_MONOTONIC_READ_WAIT_TIMEOUT_MS),
        )
    }

    pub fn stages(&self) -> StagesConfig {
        self.stages.clone().unwrap_or_default()
    }

    /// Reject configurations that cannot make forward progress. Validates the
    /// shared read-pipeline knobs (concurrency and per-stage chunk/fan-out) and
    /// delegates to [`LedgerHistoryConfig::validate`] for the list-API knobs.
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(plaintext) = self.plaintext_address() {
            anyhow::ensure!(
                plaintext != self.address(),
                "plaintext_address must differ from address",
            );
        }
        anyhow::ensure!(
            self.request_bigtable_concurrency() > 0,
            "request_bigtable_concurrency must be greater than zero",
        );
        self.read_rows_retry().validate(self.channel_timeout())?;
        for (key, bytes) in [
            (
                "bigtable-http2-stream-window-bytes",
                self.bigtable_http2_stream_window_bytes,
            ),
            (
                "bigtable-http2-connection-window-bytes",
                self.bigtable_http2_connection_window_bytes,
            ),
        ] {
            if let Some(bytes) = bytes {
                anyhow::ensure!(
                    (1..=MAX_HTTP2_WINDOW_BYTES).contains(&bytes),
                    "{key} must be between 1 and {MAX_HTTP2_WINDOW_BYTES}",
                );
            }
        }
        let stages = self.stages();
        for stage in [
            PipelineStage::TxSeqDigest,
            PipelineStage::TxCpResolve,
            PipelineStage::Transactions,
            PipelineStage::Objects,
            PipelineStage::Checkpoints,
        ] {
            let resolved = stages.stage(stage);
            anyhow::ensure!(
                resolved.max_seeks_per_read > 0,
                "stages.{stage:?} max_seeks_per_read must be greater than zero",
            );
            anyhow::ensure!(
                resolved.concurrency > 0,
                "stages.{stage:?} concurrency must be greater than zero",
            );
            anyhow::ensure!(
                resolved.min_seeks_per_split_read != Some(0),
                "stages.{stage:?} min_seeks_per_split_read must be greater than zero",
            );
            anyhow::ensure!(
                resolved.max_keys_per_read != Some(0),
                "stages.{stage:?} max_keys_per_read must be greater than zero",
            );
            anyhow::ensure!(
                stage.counts_buckets()
                    || stages
                        .get(stage)
                        .is_none_or(|c| c.max_keys_per_read.is_none()),
                "stages.{stage:?} max_keys_per_read applies only to stages whose seeks are \
                 buckets (TxSeqDigest, TxCpResolve)",
            );
        }
        self.ledger_history().validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEFAULT_SERVICE_INFO_WATERMARK_PIPELINES;
    use crate::LIST_API_SERVICE_INFO_WATERMARK_PIPELINES;
    use crate::service_info_watermark_pipelines;

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
    max-seeks-per-read: 50
  tx-cp-resolve:
    concurrency: 3
  tx-seq-digest:
    max-keys-per-read: 40
"#;
        let cfg: KvRpcConfig = serde_yaml::from_str(yaml).unwrap();
        let stages = cfg.stages();

        let objects = stages.stage(PipelineStage::Objects);
        assert_eq!(objects.concurrency, 4);
        // Unset sibling fields on the same stage fall back to its defaults.
        assert_eq!(objects.max_seeks_per_read, 100);
        assert_eq!(objects.min_seeks_per_split_read, None);

        let transactions = stages.stage(PipelineStage::Transactions);
        assert_eq!(transactions.max_seeks_per_read, 50);
        assert_eq!(transactions.concurrency, 25);
        assert_eq!(transactions.min_seeks_per_split_read, Some(4));

        // Untouched stage is fully default.
        let checkpoints = stages.stage(PipelineStage::Checkpoints);
        assert_eq!(checkpoints.max_seeks_per_read, 100);
        assert_eq!(checkpoints.concurrency, 10);

        // ListCheckpoints' lookup stage is configured apart from the
        // `tx_seq_digest` stage that reads the same table.
        let tx_cp_resolve = stages.stage(PipelineStage::TxCpResolve);
        assert_eq!(tx_cp_resolve.concurrency, 3);
        assert_eq!(tx_cp_resolve.max_seeks_per_read, 25);
        assert_eq!(tx_cp_resolve.min_seeks_per_split_read, Some(4));
        assert_eq!(stages.stage(PipelineStage::TxSeqDigest).concurrency, 20);

        // Stages whose seeks are buckets cap the keys per read (default 100);
        // on the others every key is a seek, so there is no separate key cap.
        assert_eq!(tx_cp_resolve.max_keys_per_read, Some(100));
        assert_eq!(
            stages.stage(PipelineStage::TxSeqDigest).max_keys_per_read,
            Some(40)
        );
        assert_eq!(transactions.max_keys_per_read, None);

        cfg.validate().unwrap();
    }

    #[test]
    fn validate_rejects_zero_stage_knob() {
        let cfg = KvRpcConfig {
            stages: Some(StagesConfig {
                objects: Some(StageConfig {
                    max_seeks_per_read: Some(0),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let err = cfg
            .validate()
            .expect_err("zero max_seeks_per_read must fail");
        assert!(err.to_string().contains("max_seeks_per_read"), "{err}");

        let cfg = KvRpcConfig {
            stages: Some(StagesConfig {
                transactions: Some(StageConfig {
                    concurrency: Some(0),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let err = cfg.validate().expect_err("zero concurrency must fail");
        assert!(err.to_string().contains("concurrency"), "{err}");

        let cfg = KvRpcConfig {
            stages: Some(StagesConfig {
                tx_seq_digest: Some(StageConfig {
                    min_seeks_per_split_read: Some(0),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let err = cfg
            .validate()
            .expect_err("zero min_seeks_per_split_read must fail");
        assert!(
            err.to_string().contains("min_seeks_per_split_read"),
            "{err}"
        );

        let cfg = KvRpcConfig {
            stages: Some(StagesConfig {
                tx_cp_resolve: Some(StageConfig {
                    max_keys_per_read: Some(0),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let err = cfg
            .validate()
            .expect_err("zero max_keys_per_read must fail");
        assert!(err.to_string().contains("max_keys_per_read"), "{err}");
    }

    #[test]
    fn validate_rejects_max_keys_per_read_on_stage_counting_keys() {
        let cfg = KvRpcConfig {
            stages: Some(StagesConfig {
                transactions: Some(StageConfig {
                    max_keys_per_read: Some(50),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let err = cfg
            .validate()
            .expect_err("max_keys_per_read on a stage whose seeks are keys must fail");
        assert!(err.to_string().contains("max_keys_per_read"), "{err}");
    }

    #[test]
    fn validate_accepts_min_seeks_per_split_read_on_objects() {
        let cfg = KvRpcConfig {
            stages: Some(StagesConfig {
                objects: Some(StageConfig {
                    min_seeks_per_split_read: Some(4),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        cfg.validate().unwrap();
        assert_eq!(
            cfg.stages()
                .stage(PipelineStage::Objects)
                .min_seeks_per_split_read,
            Some(4)
        );
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
    fn plaintext_address_disabled_by_default() {
        let cfg = KvRpcConfig::default();
        assert_eq!(cfg.plaintext_address(), None);
        cfg.validate().unwrap();
    }

    #[test]
    fn plaintext_address_parses_from_toml() {
        let config_toml = r#"
instance-id = "my-instance"
address = "[::]:8000"
plaintext-address = "127.0.0.1:8001"
"#;
        let cfg: KvRpcConfig = toml::from_str(config_toml).unwrap();
        assert_eq!(cfg.plaintext_address(), Some("127.0.0.1:8001"));
        cfg.validate().unwrap();
    }

    #[test]
    fn validate_rejects_plaintext_address_equal_to_address() {
        let cfg = KvRpcConfig {
            address: Some("[::]:8000".to_string()),
            plaintext_address: Some("[::]:8000".to_string()),
            ..Default::default()
        };
        let err = cfg
            .validate()
            .expect_err("plaintext_address == address must fail");
        assert!(err.to_string().contains("plaintext_address"), "{err}");
    }

    #[test]
    fn partial_toml_falls_back_to_defaults() {
        // Only one endpoint is customized; everything else must resolve to defaults.
        let config_toml = r#"
instance-id = "my-instance"

[ledger-history]
bitmap-bucket-budget-tx = 2048

[ledger-history.list-transactions]
max-limit-items = 7
render-ahead = 3
"#;
        let cfg: KvRpcConfig = toml::from_str(config_toml).unwrap();
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
        assert_eq!(lh.bitmap_bucket_budget_event(), 4000);
        lh.validate().unwrap();
    }

    #[test]
    fn list_apis_off_by_default_and_bound_the_watermark_set() {
        let cfg = KvRpcConfig::default();
        assert!(!cfg.enable_list_apis());
        assert_eq!(
            service_info_watermark_pipelines(cfg.enable_list_apis()),
            DEFAULT_SERVICE_INFO_WATERMARK_PIPELINES.to_vec(),
        );

        let yaml = "enable-list-apis: true\n";
        let cfg: KvRpcConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.enable_list_apis());
        let pipelines = service_info_watermark_pipelines(cfg.enable_list_apis());
        for name in LIST_API_SERVICE_INFO_WATERMARK_PIPELINES {
            assert!(pipelines.contains(&name), "{name} must bound the height");
        }
    }

    #[test]
    fn deprecated_experimental_key_still_enables_list_apis() {
        // Configs written before the rename must keep serving the List APIs.
        let yaml = "enable-experimental-query-apis: true\n";
        let cfg: KvRpcConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.enable_list_apis());

        // The current name wins when both are present.
        let yaml = "enable-list-apis: false\nenable-experimental-query-apis: true\n";
        let cfg: KvRpcConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!cfg.enable_list_apis());
    }

    #[test]
    fn deprecated_watermark_pipeline_loads_but_is_ignored() {
        // Existing configs pinning a pipeline set must still load, and must not
        // change the watermark set the server reports.
        let yaml = r#"
watermark-pipeline:
  - kvstore_checkpoints
  - a_pipeline_that_no_longer_exists
enable-list-apis: true
"#;
        let cfg: KvRpcConfig = serde_yaml::from_str(yaml).expect("stale key must not hard-fail");
        cfg.validate().unwrap();
        assert_eq!(
            service_info_watermark_pipelines(cfg.enable_list_apis()).len(),
            DEFAULT_SERVICE_INFO_WATERMARK_PIPELINES.len()
                + LIST_API_SERVICE_INFO_WATERMARK_PIPELINES.len(),
        );
    }
}
