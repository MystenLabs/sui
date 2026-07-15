// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::time::Duration;

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RpcConfig {
    /// Enable indexing of transactions and objects
    ///
    /// This enables indexing of transactions and objects which allows for a slightly richer rpc
    /// api. There are some APIs which will be disabled/enabled based on this config while others
    /// (eg GetTransaction) will still be enabled regardless of this config but may return slight
    /// less data (eg GetTransaction won't return the checkpoint that includes the requested
    /// transaction).
    ///
    /// Defaults to `false`, with indexing and APIs which require indexes being disabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_indexing: Option<bool>,

    /// Configure the address to listen on for https
    ///
    /// Defaults to `0.0.0.0:9443` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub https_address: Option<SocketAddr>,

    /// TLS configuration to use for https.
    ///
    /// If not provided then the node will not create an https service.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<RpcTlsConfig>,

    /// Maxumum budget for rendering a Move value into JSON.
    ///
    /// This sets the numbers of bytes that we are willing to spend on rendering field names and
    /// values when rendering a Move value into a JSON value.
    ///
    /// Defaults to `1MiB` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_json_move_value_size: Option<usize>,

    /// Aggregate budget for Move-value JSON rendering across a single response.
    ///
    /// Endpoints that render many Move values in one response (e.g. `GetCheckpoint`
    /// with a `read_mask` that selects every event's `json` field) share this
    /// budget across all per-item renders, so the response cannot multiply one
    /// request into hundreds of MiB of materialized `prost_types::Value`.
    ///
    /// Defaults to `16 MiB` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_json_move_value_response_size: Option<usize>,

    /// Configuration for RPC index initialization and bulk loading
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_initialization: Option<RpcIndexInitConfig>,

    /// Tunables for the ledger-history list APIs (`list_transactions`,
    /// `list_events`, `list_checkpoints`). These scan the historical inverted
    /// indexes, unlike the live object-set listings (`list_owned_objects`,
    /// `list_dynamic_fields`), so they carry their own time and scan-cost bounds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ledger_history: Option<LedgerHistoryConfig>,

    /// Number of consecutive checkpoints a filtered subscription may go without
    /// producing an item before the server emits a progress-only frame,
    /// so sparse subscribers always learn their resume point. Defaults to 25
    /// (~5 seconds at mainnet checkpoint cadence).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_watermark_interval: Option<u32>,

    /// Maximum number of concurrent RPC subscriptions. Defaults to 1024.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_max_subscribers: Option<usize>,

    /// Number of parallel shard tasks that evaluate subscription filters and
    /// deliver updates. Each subscriber lives on one shard; per-checkpoint
    /// filter evaluation parallelizes across shards. Defaults to the host's
    /// available parallelism.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_shards: Option<u32>,

    /// Configuration for rendering Objects based on the Display standard
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<DisplayConfig>,
}

impl RpcConfig {
    pub fn enable_indexing(&self) -> bool {
        self.enable_indexing.unwrap_or(false)
    }

    pub fn https_address(&self) -> SocketAddr {
        self.https_address
            .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 9443)))
    }

    pub fn tls_config(&self) -> Option<&RpcTlsConfig> {
        self.tls.as_ref()
    }

    pub fn max_json_move_value_size(&self) -> usize {
        self.max_json_move_value_size.unwrap_or(1024 * 1024)
    }

    pub fn max_json_move_value_response_size(&self) -> usize {
        self.max_json_move_value_response_size
            .unwrap_or(16 * 1024 * 1024)
    }

    pub fn index_initialization_config(&self) -> Option<&RpcIndexInitConfig> {
        self.index_initialization.as_ref()
    }

    pub fn ledger_history(&self) -> &LedgerHistoryConfig {
        const DEFAULT_LEDGER_HISTORY_CONFIG: LedgerHistoryConfig = LedgerHistoryConfig {
            list_transactions: None,
            list_events: None,
            list_checkpoints: None,
            bitmap_bucket_scan_budget: None,
            chunk_bucket_scan_budget: None,
            max_bitmap_filter_literals: None,
        };

        self.ledger_history
            .as_ref()
            .unwrap_or(&DEFAULT_LEDGER_HISTORY_CONFIG)
    }

    /// Validate cross-field invariants. Call once at startup to fail fast on a
    /// misconfiguration rather than surfacing it per-request.
    pub fn validate(&self) -> anyhow::Result<()> {
        self.ledger_history().validate()
    }

    pub fn display(&self) -> &DisplayConfig {
        const DEFAULT_DISPLAY_CONFIG: DisplayConfig = DisplayConfig {
            max_field_depth: None,
            max_format_nodes: None,
            max_object_loads: None,
            max_move_value_depth: None,
            max_output_size: None,
        };

        self.display.as_ref().unwrap_or(&DEFAULT_DISPLAY_CONFIG)
    }
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RpcTlsConfig {
    /// File path to a PEM formatted TLS certificate chain
    cert: String,
    /// File path to a PEM formatted TLS private key
    key: String,
}

impl RpcTlsConfig {
    pub fn cert(&self) -> &str {
        &self.cert
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}

/// Configuration for RPC index initialization and bulk loading
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RpcIndexInitConfig {
    /// Override for RocksDB's set_db_write_buffer_size during bulk indexing.
    /// This is the total memory budget for all column families' memtables.
    ///
    /// Defaults to 90% of system RAM if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_write_buffer_size: Option<usize>,

    /// Override for each column family's write buffer size during bulk indexing.
    ///
    /// Defaults to 25% of system RAM divided by max_write_buffer_number if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cf_write_buffer_size: Option<usize>,

    /// Override for the maximum number of write buffers per column family during bulk indexing.
    /// This value is capped at 32 as an upper bound.
    ///
    /// Defaults to a dynamic value based on system RAM if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cf_max_write_buffer_number: Option<i32>,

    /// Override for the number of background jobs during bulk indexing.
    ///
    /// Defaults to the number of CPU cores if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_background_jobs: Option<i32>,

    /// Override for the batch size limit during bulk indexing.
    /// This controls how much data is accumulated in memory before flushing to disk.
    ///
    /// Defaults to half the write buffer size or 128MB, whichever is smaller.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size_limit: Option<usize>,
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DisplayConfig {
    /// Maximum number of times the parser can recurse into nested structures. Depth does not
    /// account for all nodes, only nodes that can be contained within themselves.
    ///
    /// Defaults to `32` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_field_depth: Option<usize>,

    /// Maximum number of AST nodes that can be allocated during parsing. This counts all values
    /// that are instances of AST types (but not, for example, `Vec<T>`).
    ///
    /// Defaults to `32768` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_format_nodes: Option<usize>,

    /// Maximum number of objects that can be loaded during formatting.
    ///
    /// Defaults to `8` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_object_loads: Option<usize>,

    /// Maximum depth to use when converting a rendered Display value to JSON.
    ///
    /// Defaults to `32` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_move_value_depth: Option<usize>,

    /// Maxumum budget for rendering an object based on its Display template.
    ///
    /// This sets the numbers of bytes that we are willing to spend on rendering field names and
    /// values when rendering an object based on its Display template.
    ///
    /// Defaults to `1MiB` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_size: Option<usize>,
}

impl DisplayConfig {
    pub fn max_field_depth(&self) -> usize {
        self.max_field_depth.unwrap_or(32)
    }

    pub fn max_format_nodes(&self) -> usize {
        self.max_format_nodes.unwrap_or(32768)
    }

    pub fn max_object_loads(&self) -> usize {
        self.max_object_loads.unwrap_or(8)
    }

    pub fn max_move_value_depth(&self) -> usize {
        self.max_move_value_depth.unwrap_or(32)
    }

    pub fn max_output_size(&self) -> usize {
        self.max_output_size.unwrap_or(1024 * 1024)
    }
}

const DEFAULT_LEDGER_HISTORY_METHOD_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_BITMAP_BUCKET_SCAN_BUDGET: usize = 1_024;
const DEFAULT_CHUNK_BUCKET_SCAN_BUDGET: usize = 256;
const DEFAULT_MAX_BITMAP_FILTER_LITERALS: usize = 10;
// A chunk never evaluates more buckets than the whole request is allowed, so the
// per-chunk cap must not exceed the per-request budget. Enforced for the
// defaults here; the accessors clamp configured values the same way.
const _: () = assert!(DEFAULT_CHUNK_BUCKET_SCAN_BUDGET <= DEFAULT_BITMAP_BUCKET_SCAN_BUDGET);

/// Built-in per-endpoint defaults. These differ per endpoint (e.g. checkpoints
/// page smaller than transactions, and scan a narrower chunk).
struct LedgerHistoryMethodDefaults {
    default_limit_items: u32,
    max_limit_items: u32,
    chunk_max: usize,
}

const LIST_TRANSACTIONS_DEFAULTS: LedgerHistoryMethodDefaults = LedgerHistoryMethodDefaults {
    default_limit_items: 50,
    max_limit_items: 500,
    chunk_max: 32,
};
const LIST_EVENTS_DEFAULTS: LedgerHistoryMethodDefaults = LedgerHistoryMethodDefaults {
    default_limit_items: 50,
    max_limit_items: 1_000,
    chunk_max: 32,
};
const LIST_CHECKPOINTS_DEFAULTS: LedgerHistoryMethodDefaults = LedgerHistoryMethodDefaults {
    default_limit_items: 10,
    max_limit_items: 100,
    chunk_max: 16,
};

/// Per-endpoint tunables for one ledger-history list API. Every field is optional
/// and falls back to a built-in default; see [`ResolvedLedgerHistoryMethodConfig`].
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
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

    /// Maximum items materialized per internal scan chunk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_max: Option<usize>,
}

/// A [`LedgerHistoryMethodConfig`] with all defaults applied.
#[derive(Clone, Copy, Debug)]
pub struct ResolvedLedgerHistoryMethodConfig {
    pub timeout: Duration,
    pub default_limit_items: u32,
    pub max_limit_items: u32,
    pub chunk_max: usize,
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
            chunk_max: this.and_then(|c| c.chunk_max).unwrap_or(defaults.chunk_max),
        }
    }
}

/// Tunables for the ledger-history list APIs. Per-endpoint knobs live in
/// the three [`LedgerHistoryMethodConfig`] fields; the remaining knobs are global across
/// all three. Every field is optional and falls back to a built-in default.
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
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

    /// Total evaluated-bucket budget for one filtered request, shared by all
    /// three list APIs. Exhausting it ends the query with `SCAN_LIMIT` and a
    /// resume cursor, bounding the worst-case scan cost of a sparse filter.
    ///
    /// Defaults to `1024` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitmap_bucket_scan_budget: Option<usize>,

    /// Per-chunk evaluated-bucket cap. A chunk that hits this while the request
    /// budget remains emits a progress watermark and resumes in the next chunk,
    /// so a long sparse scan reports incremental progress. Clamped to
    /// `bitmap_bucket_scan_budget`.
    ///
    /// Defaults to `256` if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_bucket_scan_budget: Option<usize>,

    /// Maximum total filter literals (bitmap dimensions) accepted in one filtered
    /// request, across all DNF terms. Each literal becomes one bitmap leaf, so
    /// this bounds a single filter's scan fanout. Must not exceed
    /// `bitmap_bucket_scan_budget` (see [`LedgerHistoryConfig::validate`]).
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

    pub fn bitmap_bucket_scan_budget(&self) -> usize {
        self.bitmap_bucket_scan_budget
            .unwrap_or(DEFAULT_BITMAP_BUCKET_SCAN_BUDGET)
    }

    pub fn chunk_bucket_scan_budget(&self) -> usize {
        self.chunk_bucket_scan_budget
            .unwrap_or(DEFAULT_CHUNK_BUCKET_SCAN_BUDGET)
            .min(self.bitmap_bucket_scan_budget())
    }

    pub fn max_bitmap_filter_literals(&self) -> usize {
        self.max_bitmap_filter_literals
            .unwrap_or(DEFAULT_MAX_BITMAP_FILTER_LITERALS)
    }

    /// Reject configurations that cannot make forward progress. Each filter
    /// literal becomes one bitmap leaf that must fetch at least one bucket to
    /// emit its first watermark; if the per-request budget is below the literal
    /// cap a `SCAN_LIMIT` can fire before any merged watermark reaches the wire,
    /// leaving the client a cursorless `QueryEnd` it cannot resume from. Mirrors
    /// the archival/BigTable side's `LedgerHistoryConfig::validate`.
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.bitmap_bucket_scan_budget() >= self.max_bitmap_filter_literals(),
            "ledger_history.bitmap_bucket_scan_budget ({}) must be >= \
             max_bitmap_filter_literals ({}) so every filter leaf gets at least one \
             bucket before SCAN_LIMIT",
            self.bitmap_bucket_scan_budget(),
            self.max_bitmap_filter_literals(),
        );
        Ok(())
    }
}
