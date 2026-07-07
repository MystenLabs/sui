// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;
use sui_name_service::NameServiceConfig;
use sui_protocol_config::Chain;
use sui_protocol_config::ProtocolConfig;
use sui_protocol_config::ProtocolVersion;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;

use crate::extensions::query_limits::QueryLimitsConfig;
use crate::extensions::timeout::TimeoutConfig;
use crate::pagination::PageLimits;
use crate::pagination::PaginationConfig;

pub use fastcrypto_zkp::bn254::zk_login_api::ZkLoginEnv;

#[derive(Default)]
pub struct RpcConfig {
    /// Constraints that the service will impose on requests.
    pub limits: Limits,

    /// Configuration for health checks.
    pub health: HealthConfig,

    /// Configure for SuiNS related RPC methods.
    pub name_service: NameServiceConfig,

    /// Configuration for the watermark task.
    pub watermark: WatermarkConfig,

    /// Configuration for zkLogin verification.
    pub zklogin: ZkLoginConfig,

    /// Configuration for streaming subscriptions.
    pub subscription: SubscriptionConfig,

    /// Configuration for the request-logging extension.
    pub logging: LoggingConfig,

    /// Configuration for which pipelines this service can expect to find populated.
    pub pipeline: PipelineConfig,

    /// Which pipelines to serve, based on how far their data lags behind the network tip.
    pub availability: AvailabilityConfig,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct RpcLayer {
    pub limits: LimitsLayer,
    pub health: HealthLayer,
    pub name_service: NameServiceLayer,
    pub watermark: WatermarkLayer,
    pub zklogin: ZkLoginLayer,
    pub subscription: SubscriptionLayer,
    pub logging: LoggingLayer,
    pub pipeline_defaults: PipelineLayer,
    pub pipeline: BTreeMap<String, PipelineLayer>,
}

#[derive(Clone)]
pub struct HealthConfig {
    /// How long to wait for a health check to complete before timing out.
    pub max_checkpoint_lag: Duration,
}

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct HealthLayer {
    pub max_checkpoint_lag_ms: Option<u64>,
}

#[derive(Clone, Default, Debug)]
pub struct PipelineConfig {
    /// Resolved availability for every pipeline explicitly mentioned in config.
    pub availability: BTreeMap<String, PipelineAvailability>,

    /// Availability for any pipeline not present in `availability` above -- e.g. one that starts
    /// producing watermarks after startup, without ever being explicitly listed.
    pub default_availability: PipelineAvailability,
}

/// Availability policy for a pipeline: serve it unconditionally (`enabled = true`), never serve
/// it (`enabled = false`), or serve it only while its high-watermark is within
/// `max-checkpoint-lag` checkpoints of the network tip. A policy section sets exactly one of the
/// two keys. Queries that require a pipeline that is not served return a `FeatureUnavailable`
/// error, and such a pipeline stops pinning the server's consistency boundary.
///
/// The shared `[pipeline-defaults].availability` section sets a default policy for every tracked
/// pipeline, and a `[pipeline.<name>].availability` section overrides it for a single pipeline
/// (e.g. `enabled = true` exempts one pipeline from a configured default). A pipeline with neither
/// is always served, so this feature is opt-in and does not change behaviour unless configured:
///
/// ```toml
/// [pipeline-defaults.availability]
/// max-checkpoint-lag = 100          # default for every tracked pipeline
///
/// [pipeline.kv_objects.availability]
/// enabled = true                    # always serve, exempt from the default
///
/// [pipeline.tx_kinds.availability]
/// enabled = false                   # never serve
///
/// [pipeline.tx_calls.availability]
/// max-checkpoint-lag = 1000         # lag-gated override
/// ```
///
/// The lag distance is measured against the network tip, approximated as the highest checkpoint
/// high-watermark across all tracked pipelines (equal to the true network tip when a Bigtable or
/// Ledger gRPC KV source is configured, and the fastest local pipeline otherwise). The default
/// also covers the virtual `bigtable`/`ledger_grpc`/`consistent` pipelines tracked from those
/// stores; the KV ones define the tip, so in practice a lag default never gates them. Gating a
/// `kv_*` content pipeline only takes effect in a pure-Postgres deployment; when content is served
/// from an external KV store those pipelines are not tracked, so the policy is inert.
///
/// Also doubles as the resolved value in [`PipelineConfig::availability`]/`default_availability`,
/// where only the `Enabled`/`Disabled` variants are ever produced (from a pipeline's plain
/// `enabled` setting, not an explicit availability policy).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(
    try_from = "PipelineAvailabilityLayer",
    into = "PipelineAvailabilityLayer"
)]
pub enum PipelineAvailability {
    /// Always serve the pipeline.
    #[default]
    Enabled,

    /// Never serve the pipeline.
    Disabled,

    /// Serve the pipeline only while its high-watermark is within this many checkpoints of the
    /// network tip.
    MaxCheckpointLag(u64),
}

/// Availability policies assembled from `[pipeline-defaults].availability` and each pipeline's own
/// `[pipeline.<name>].availability` override. See [`PipelineAvailability`] for the policy
/// semantics.
#[derive(Clone, Default)]
pub struct AvailabilityConfig {
    /// Default policy applied to every tracked pipeline without an override.
    pub default: Option<PipelineAvailability>,

    /// Per-pipeline overrides, keyed by pipeline name.
    pub pipelines: BTreeMap<String, PipelineAvailability>,
}

/// TOML mirror of [`PipelineAvailability`]: a policy section sets exactly one of these keys,
/// validated when converting to the enum.
#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct PipelineAvailabilityLayer {
    pub enabled: Option<bool>,
    pub max_checkpoint_lag: Option<u64>,
}

/// Configuration for a single pipeline's `enabled` and `availability` settings -- used both for
/// the shared default (`[pipeline-defaults]`) and for per-pipeline overrides (`[pipeline.<name>]`).
/// Kept as its own type/section rather than flattened together with the per-pipeline map: TOML
/// forbids redefining the same key as both a scalar and a table, so if the default lived directly
/// in the `[pipeline]` table, a pipeline named `enabled` could never be expressed, regardless of
/// how the Rust side deserialized it.
#[derive(Clone, Default, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct PipelineLayer {
    pub enabled: Option<bool>,

    /// Overrides the default availability policy for this pipeline. An override exists only to
    /// set a policy, so its section must set `enabled` or `max-checkpoint-lag`.
    pub availability: Option<PipelineAvailability>,
}

pub struct Limits {
    /// Time (in milliseconds) to wait for a transaction to be executed and the results returned
    /// from GraphQL. If the transaction takes longer than this time to execute, the request will
    /// return a timeout error, but the transaction may continue executing.
    pub mutation_timeout_ms: u32,

    /// Time (in milliseconds) to wait for a read request from the GraphQL service. Requests that
    /// take longer than this time to return a result will return a timeout error.
    pub query_timeout_ms: u32,

    /// Maximum depth of a GraphQL query that can be accepted by this service.
    pub max_query_depth: u32,

    /// The maximum number of nodes (field names) the service will accept in a single query.
    pub max_query_nodes: u32,

    /// Maximum number of estimated output nodes in a GraphQL response.
    pub max_output_nodes: u32,

    /// Maximum size in bytes allowed for the `txBytes` and `signatures` parameters of an
    /// `executeTransaction` or `simulateTransaction` field, the `message` and `signature`
    /// parameters of a `verifySignature` field, or the `bytes` and `signature` parameters of a
    /// `verifyZkLoginSignature` field.
    ///
    /// This is cumulative across all matching fields in a single GraphQL request.
    pub max_tx_payload_size: u32,

    /// Maximum size in bytes of a single GraphQL request, excluding the elements covered by
    /// `max_transaction_payload_size`.
    pub max_query_payload_size: u32,

    /// By default, paginated queries will return this many elements if a page size is not
    /// provided. This may be overridden for paginated queries that are limited by the protocol.
    pub default_page_size: u32,

    /// By default, paginated queries can return at most this many elements. A request to fetch
    /// more elements will result in an error. This limit may be superseded when the field being
    /// paginated is limited by the protocol (e.g. object changes for a transaction).
    pub max_page_size: u32,

    /// Maximum number of keys that can be passed to a multi-get query. A request to fetch more
    /// keys will result in an error.
    pub max_multi_get_size: u32,

    /// Maximum (and default) number of object changes that can be returned in a single page of
    /// `TransactionEffects.objectChanges`.
    pub page_size_override_fx_object_changes: u32,

    /// Maximum (and default) number of packages that can be returned in a single page of
    /// `Query.packages`.
    pub page_size_override_packages: u32,

    /// Maximum amount of nesting among type arguments (type arguments nest when a type argument is
    /// itself generic and has arguments).
    pub max_type_argument_depth: usize,

    /// Maximum number of type parameters a type can have.
    pub max_type_argument_width: usize,

    /// Maximum number of datatypes that need to be processed when calculating the layout of a
    /// single type.
    pub max_type_nodes: usize,

    /// Maximum nesting allowed in datatype fields when calculating the layout of a single type.
    pub max_move_value_depth: usize,

    /// Maximum budget in bytes to spend when outputting a structured Move value.
    pub max_move_value_bound: usize,

    /// Maximum depth of nested field access supported in display outputs.
    pub max_display_field_depth: usize,

    /// Maximum number of components in a Display v2 format string.
    pub max_display_format_nodes: usize,

    /// Maximum number of objects that can be loaded while evaluating a display.
    pub max_display_object_loads: usize,

    /// Maximum output size of a display output.
    pub max_display_output_size: usize,

    /// Maximum output size of a disassembled Move module, in bytes.
    pub max_disassembled_module_size: usize,

    /// Maximum number of "rich" queries that can be performed in a single request. Rich queries are
    /// queries that require dedicated requests to the backing store.
    pub max_rich_queries: usize,
}

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct LimitsLayer {
    pub mutation_timeout_ms: Option<u32>,
    pub query_timeout_ms: Option<u32>,
    pub max_query_depth: Option<u32>,
    pub max_query_nodes: Option<u32>,
    pub max_output_nodes: Option<u32>,
    pub max_tx_payload_size: Option<u32>,
    pub max_query_payload_size: Option<u32>,
    pub default_page_size: Option<u32>,
    pub max_page_size: Option<u32>,
    pub max_multi_get_size: Option<u32>,
    pub page_size_override_fx_object_changes: Option<u32>,
    pub page_size_override_packages: Option<u32>,
    pub max_type_argument_depth: Option<usize>,
    pub max_type_argument_width: Option<usize>,
    pub max_type_nodes: Option<usize>,
    pub max_move_value_depth: Option<usize>,
    pub max_move_value_bound: Option<usize>,
    pub max_display_field_depth: Option<usize>,
    pub max_display_format_nodes: Option<usize>,
    pub max_display_object_loads: Option<usize>,
    pub max_display_output_size: Option<usize>,
    pub max_disassembled_module_size: Option<usize>,
    pub max_rich_queries: Option<usize>,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct NameServiceLayer {
    pub package_address: Option<SuiAddress>,
    pub registry_id: Option<ObjectID>,
    pub reverse_registry_id: Option<ObjectID>,
}

pub struct WatermarkConfig {
    /// How long to wait between updating the watermark.
    pub watermark_polling_interval: Duration,
}

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct WatermarkLayer {
    pub watermark_polling_interval_ms: Option<u64>,
}

pub struct ZkLoginConfig {
    pub env: ZkLoginEnv,
    pub max_epoch_upper_bound_delta: Option<u64>,
}

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct ZkLoginLayer {
    pub env: Option<ZkLoginEnv>,
    pub max_epoch_upper_bound_delta: Option<Option<u64>>,
}

#[derive(Clone)]
pub struct SubscriptionConfig {
    /// Number of checkpoints the broadcast channel can buffer before slow subscribers are
    /// dropped. Higher values give subscribers more time to catch up but use more memory,
    /// as each buffered checkpoint's data is kept alive until it leaves the buffer.
    /// Subscribers that fall behind by this many checkpoints receive a lagged error.
    pub broadcast_buffer: usize,

    /// How often (in milliseconds) the eviction task checks the `kv_packages` watermark
    /// and evicts indexed packages from the streaming index.
    pub package_eviction_interval_ms: u64,

    /// Number of checkpoints fetched concurrently per chunk during upstream gap recovery.
    pub gap_recovery_chunk_size: usize,

    /// Upper bound on the rate (queries per second) at which a single subscriber's catch-up
    /// scan issues kv-rpc fetches. Prevents a single client with a large backfill from
    /// monopolising shared kv-rpc throughput.
    ///
    /// This is a per-subscriber cap, so aggregate kv-rpc QPS scales linearly with subscriber
    /// count; aggregate protection belongs on the kv-rpc server itself.
    ///
    /// Increasing this value lets each subscriber catch up faster, at the cost of more
    /// kv-rpc QPS per subscriber. The aggregate (subscribers * this) shares capacity with
    /// the main query API, so if kv-rpc saturates, fetch latency rises and the effective
    /// rate falls below this cap for everyone.
    pub per_subscriber_scan_max_qps: u32,

    /// Maximum in-flight kv-rpc fetches per subscriber during the catch-up scan. Must be
    /// large enough to keep the pipeline full at your kv-rpc's fetch latency; otherwise
    /// actual throughput is limited by concurrency rather than the QPS cap.
    ///
    /// Increasing this value keeps the QPS cap saturated under higher-latency kv-rpc, at
    /// the cost of more per-subscriber memory held.
    pub per_subscriber_scan_max_concurrent_fetches: usize,
}

impl Default for SubscriptionConfig {
    fn default() -> Self {
        Self {
            broadcast_buffer: 256,
            package_eviction_interval_ms: 300_000,
            gap_recovery_chunk_size: 50,
            per_subscriber_scan_max_qps: 500,
            per_subscriber_scan_max_concurrent_fetches: 50,
        }
    }
}

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct SubscriptionLayer {
    pub broadcast_buffer: Option<usize>,
    pub package_eviction_interval_ms: Option<u64>,
    pub gap_recovery_chunk_size: Option<usize>,
    pub per_subscriber_scan_max_qps: Option<u32>,
    pub per_subscriber_scan_max_concurrent_fetches: Option<usize>,
}

impl SubscriptionLayer {
    pub(crate) fn finish(self, base: SubscriptionConfig) -> SubscriptionConfig {
        SubscriptionConfig {
            broadcast_buffer: self.broadcast_buffer.unwrap_or(base.broadcast_buffer),
            package_eviction_interval_ms: self
                .package_eviction_interval_ms
                .unwrap_or(base.package_eviction_interval_ms),
            gap_recovery_chunk_size: self
                .gap_recovery_chunk_size
                .unwrap_or(base.gap_recovery_chunk_size),
            per_subscriber_scan_max_qps: self
                .per_subscriber_scan_max_qps
                .unwrap_or(base.per_subscriber_scan_max_qps),
            per_subscriber_scan_max_concurrent_fetches: self
                .per_subscriber_scan_max_concurrent_fetches
                .unwrap_or(base.per_subscriber_scan_max_concurrent_fetches),
        }
    }
}

#[derive(Clone, Default, Debug)]
pub struct LoggingConfig {
    /// Per-SDK list of versions emitted verbatim as the `client_sdk_version` Prometheus label.
    /// Versions outside this list map to `"other"`. Add an entry only when explicitly tracking
    /// adoption or retention of a specific SDK version.
    pub sdk_version_allowlist: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct LoggingLayer {
    pub sdk_version_allowlist: Option<BTreeMap<String, BTreeSet<String>>>,
}

impl LoggingLayer {
    pub(crate) fn finish(self, base: LoggingConfig) -> LoggingConfig {
        LoggingConfig {
            sdk_version_allowlist: self
                .sdk_version_allowlist
                .unwrap_or(base.sdk_version_allowlist),
        }
    }
}

impl From<LoggingConfig> for LoggingLayer {
    fn from(value: LoggingConfig) -> Self {
        Self {
            sdk_version_allowlist: Some(value.sdk_version_allowlist),
        }
    }
}

impl RpcLayer {
    pub fn example() -> Self {
        Self {
            limits: Limits::default().into(),
            health: HealthConfig::default().into(),
            name_service: NameServiceConfig::default().into(),
            watermark: WatermarkConfig::default().into(),
            zklogin: ZkLoginConfig::default().into(),
            subscription: SubscriptionConfig::default().into(),
            logging: LoggingConfig::default().into(),
            pipeline_defaults: PipelineLayer {
                enabled: Some(true),
                availability: None,
            },
            pipeline: BTreeMap::new(),
        }
    }

    pub fn finish(self) -> RpcConfig {
        let availability = AvailabilityConfig {
            default: self.pipeline_defaults.availability,
            pipelines: self
                .pipeline
                .iter()
                .filter_map(|(name, layer)| Some((name.clone(), layer.availability?)))
                .collect(),
        };

        RpcConfig {
            limits: self.limits.finish(Limits::default()),
            health: self.health.finish(HealthConfig::default()),
            name_service: self.name_service.finish(NameServiceConfig::default()),
            watermark: self.watermark.finish(WatermarkConfig::default()),
            zklogin: self.zklogin.finish(ZkLoginConfig::default()),
            subscription: self.subscription.finish(SubscriptionConfig::default()),
            logging: self.logging.finish(LoggingConfig::default()),
            pipeline: finish_pipelines(
                self.pipeline_defaults,
                self.pipeline,
                PipelineConfig::default(),
            ),
            availability,
        }
    }
}

impl HealthLayer {
    pub(crate) fn finish(self, base: HealthConfig) -> HealthConfig {
        HealthConfig {
            max_checkpoint_lag: self
                .max_checkpoint_lag_ms
                .map(Duration::from_millis)
                .unwrap_or(base.max_checkpoint_lag),
        }
    }
}

impl PipelineAvailability {
    /// Whether a pipeline whose high-watermark is at `checkpoint` should be served, given the
    /// current `network_tip` (the highest checkpoint high-watermark across all tracked pipelines).
    pub(crate) fn is_available(&self, checkpoint: u64, network_tip: u64) -> bool {
        match self {
            Self::Enabled => true,
            Self::Disabled => false,
            Self::MaxCheckpointLag(lag) => network_tip.saturating_sub(checkpoint) <= *lag,
        }
    }
}

impl AvailabilityConfig {
    /// The policy gating `pipeline`, if any: its own override when configured, otherwise the
    /// default.
    pub(crate) fn policy_for(&self, pipeline: &str) -> Option<&PipelineAvailability> {
        self.pipelines.get(pipeline).or(self.default.as_ref())
    }
}

impl Limits {
    pub(crate) fn timeouts(&self) -> TimeoutConfig {
        TimeoutConfig {
            query: Duration::from_millis(self.query_timeout_ms as u64),
            mutation: Duration::from_millis(self.mutation_timeout_ms as u64),
        }
    }

    pub(crate) fn query_limits(&self) -> QueryLimitsConfig {
        QueryLimitsConfig {
            max_output_nodes: self.max_output_nodes,
            max_query_nodes: self.max_query_nodes,
            max_query_depth: self.max_query_depth,
            max_query_payload_size: self.max_query_payload_size,
            max_tx_payload_size: self.max_tx_payload_size,
            tx_payload_args: BTreeSet::from([
                ("Mutation", "executeTransaction", "transactionDataBcs"),
                ("Mutation", "executeTransaction", "signatures"),
                ("Query", "simulateTransaction", "transaction"),
                ("Query", "verifySignature", "message"),
                ("Query", "verifySignature", "signature"),
                ("Query", "verifyZkLoginSignature", "bytes"),
                ("Query", "verifyZkLoginSignature", "signature"),
            ]),
        }
    }

    pub(crate) fn pagination(&self) -> PaginationConfig {
        PaginationConfig::new(
            self.max_multi_get_size,
            PageLimits {
                default: self.default_page_size,
                max: self.max_page_size,
            },
            BTreeMap::from([
                (
                    ("TransactionEffects", "objectChanges"),
                    PageLimits {
                        default: self.page_size_override_fx_object_changes,
                        max: self.page_size_override_fx_object_changes,
                    },
                ),
                (
                    ("Query", "packages"),
                    PageLimits {
                        default: self.page_size_override_packages,
                        max: self.page_size_override_packages,
                    },
                ),
            ]),
        )
    }

    pub(crate) fn package_resolver(&self) -> sui_package_resolver::Limits {
        sui_package_resolver::Limits {
            max_type_argument_depth: self.max_type_argument_depth,
            max_type_argument_width: self.max_type_argument_width,
            max_type_nodes: self.max_type_nodes,
            max_move_value_depth: self.max_move_value_depth,
        }
    }

    pub(crate) fn display(&self) -> sui_display::v2::Limits {
        sui_display::v2::Limits {
            max_depth: self.max_display_field_depth,
            max_nodes: self.max_display_format_nodes,
            max_loads: self.max_display_object_loads,
        }
    }
}

impl LimitsLayer {
    pub(crate) fn finish(self, base: Limits) -> Limits {
        Limits {
            mutation_timeout_ms: self.mutation_timeout_ms.unwrap_or(base.mutation_timeout_ms),
            query_timeout_ms: self.query_timeout_ms.unwrap_or(base.query_timeout_ms),
            max_query_depth: self.max_query_depth.unwrap_or(base.max_query_depth),
            max_query_nodes: self.max_query_nodes.unwrap_or(base.max_query_nodes),
            max_output_nodes: self.max_output_nodes.unwrap_or(base.max_output_nodes),
            max_tx_payload_size: self.max_tx_payload_size.unwrap_or(base.max_tx_payload_size),
            max_query_payload_size: self
                .max_query_payload_size
                .unwrap_or(base.max_query_payload_size),
            default_page_size: self.default_page_size.unwrap_or(base.default_page_size),
            max_page_size: self.max_page_size.unwrap_or(base.max_page_size),
            max_multi_get_size: self.max_multi_get_size.unwrap_or(base.max_multi_get_size),
            page_size_override_fx_object_changes: self
                .page_size_override_fx_object_changes
                .unwrap_or(base.page_size_override_fx_object_changes),
            page_size_override_packages: self
                .page_size_override_packages
                .unwrap_or(base.page_size_override_packages),
            max_type_argument_depth: self
                .max_type_argument_depth
                .unwrap_or(base.max_type_argument_depth),
            max_type_argument_width: self
                .max_type_argument_width
                .unwrap_or(base.max_type_argument_width),
            max_type_nodes: self.max_type_nodes.unwrap_or(base.max_type_nodes),
            max_move_value_depth: self
                .max_move_value_depth
                .unwrap_or(base.max_move_value_depth),
            max_move_value_bound: self
                .max_move_value_bound
                .unwrap_or(base.max_move_value_bound),
            max_display_field_depth: self
                .max_display_field_depth
                .unwrap_or(base.max_display_field_depth),
            max_display_format_nodes: self
                .max_display_format_nodes
                .unwrap_or(base.max_display_format_nodes),
            max_display_object_loads: self
                .max_display_object_loads
                .unwrap_or(base.max_display_object_loads),
            max_display_output_size: self
                .max_display_output_size
                .unwrap_or(base.max_display_output_size),
            max_disassembled_module_size: self
                .max_disassembled_module_size
                .unwrap_or(base.max_disassembled_module_size),
            max_rich_queries: self.max_rich_queries.unwrap_or(base.max_rich_queries),
        }
    }
}

impl NameServiceLayer {
    pub(crate) fn finish(self, base: NameServiceConfig) -> NameServiceConfig {
        NameServiceConfig {
            package_address: self.package_address.unwrap_or(base.package_address),
            registry_id: self.registry_id.unwrap_or(base.registry_id),
            reverse_registry_id: self.reverse_registry_id.unwrap_or(base.reverse_registry_id),
        }
    }
}

impl WatermarkLayer {
    pub(crate) fn finish(self, base: WatermarkConfig) -> WatermarkConfig {
        WatermarkConfig {
            watermark_polling_interval: self
                .watermark_polling_interval_ms
                .map(Duration::from_millis)
                .unwrap_or(base.watermark_polling_interval),
        }
    }
}

impl ZkLoginLayer {
    pub(crate) fn finish(self, base: ZkLoginConfig) -> ZkLoginConfig {
        ZkLoginConfig {
            env: self.env.unwrap_or(base.env),
            max_epoch_upper_bound_delta: self
                .max_epoch_upper_bound_delta
                .unwrap_or(base.max_epoch_upper_bound_delta),
        }
    }
}

impl From<PipelineLayer> for PipelineAvailability {
    fn from(layer: PipelineLayer) -> Self {
        if layer.enabled.unwrap_or(true) {
            PipelineAvailability::Enabled
        } else {
            PipelineAvailability::Disabled
        }
    }
}

impl TryFrom<PipelineAvailabilityLayer> for PipelineAvailability {
    type Error = String;

    fn try_from(layer: PipelineAvailabilityLayer) -> Result<Self, Self::Error> {
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

impl From<PipelineAvailability> for PipelineAvailabilityLayer {
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

impl From<HealthConfig> for HealthLayer {
    fn from(value: HealthConfig) -> Self {
        Self {
            max_checkpoint_lag_ms: Some(value.max_checkpoint_lag.as_millis() as u64),
        }
    }
}

impl From<Limits> for LimitsLayer {
    fn from(value: Limits) -> Self {
        Self {
            mutation_timeout_ms: Some(value.mutation_timeout_ms),
            query_timeout_ms: Some(value.query_timeout_ms),
            max_query_depth: Some(value.max_query_depth),
            max_query_nodes: Some(value.max_query_nodes),
            max_output_nodes: Some(value.max_output_nodes),
            max_tx_payload_size: Some(value.max_tx_payload_size),
            max_query_payload_size: Some(value.max_query_payload_size),
            default_page_size: Some(value.default_page_size),
            max_page_size: Some(value.max_page_size),
            max_multi_get_size: Some(value.max_multi_get_size),
            page_size_override_fx_object_changes: Some(value.page_size_override_fx_object_changes),
            page_size_override_packages: Some(value.page_size_override_packages),
            max_type_argument_depth: Some(value.max_type_argument_depth),
            max_type_argument_width: Some(value.max_type_argument_width),
            max_type_nodes: Some(value.max_type_nodes),
            max_move_value_depth: Some(value.max_move_value_depth),
            max_move_value_bound: Some(value.max_move_value_bound),
            max_display_field_depth: Some(value.max_display_field_depth),
            max_display_format_nodes: Some(value.max_display_format_nodes),
            max_display_object_loads: Some(value.max_display_object_loads),
            max_display_output_size: Some(value.max_display_output_size),
            max_disassembled_module_size: Some(value.max_disassembled_module_size),
            max_rich_queries: Some(value.max_rich_queries),
        }
    }
}

impl From<NameServiceConfig> for NameServiceLayer {
    fn from(config: NameServiceConfig) -> Self {
        Self {
            package_address: Some(config.package_address),
            registry_id: Some(config.registry_id),
            reverse_registry_id: Some(config.reverse_registry_id),
        }
    }
}

impl From<WatermarkConfig> for WatermarkLayer {
    fn from(value: WatermarkConfig) -> Self {
        Self {
            watermark_polling_interval_ms: Some(value.watermark_polling_interval.as_millis() as u64),
        }
    }
}

impl From<ZkLoginConfig> for ZkLoginLayer {
    fn from(value: ZkLoginConfig) -> Self {
        Self {
            env: Some(value.env),
            max_epoch_upper_bound_delta: Some(value.max_epoch_upper_bound_delta),
        }
    }
}

impl From<SubscriptionConfig> for SubscriptionLayer {
    fn from(value: SubscriptionConfig) -> Self {
        Self {
            broadcast_buffer: Some(value.broadcast_buffer),
            package_eviction_interval_ms: Some(value.package_eviction_interval_ms),
            gap_recovery_chunk_size: Some(value.gap_recovery_chunk_size),
            per_subscriber_scan_max_qps: Some(value.per_subscriber_scan_max_qps),
            per_subscriber_scan_max_concurrent_fetches: Some(
                value.per_subscriber_scan_max_concurrent_fetches,
            ),
        }
    }
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            max_checkpoint_lag: Duration::from_secs(300),
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        let max_tx_size_bytes = max_across_protocol(ProtocolConfig::max_tx_size_bytes_as_option)
            .unwrap_or(u32::MAX as u64) as u32;

        let max_type_argument_depth =
            max_across_protocol(ProtocolConfig::max_type_argument_depth_as_option)
                .unwrap_or(u32::MAX) as usize;

        let max_type_argument_width =
            max_across_protocol(ProtocolConfig::max_generic_instantiation_length_as_option)
                .unwrap_or(u32::MAX as u64) as usize;

        let max_type_nodes = max_across_protocol(ProtocolConfig::max_type_nodes_as_option)
            .unwrap_or(u32::MAX as u64) as usize;

        let max_move_value_depth =
            max_across_protocol(ProtocolConfig::max_move_value_depth_as_option)
                .unwrap_or(u32::MAX as u64) as usize;

        let display_limits = sui_display::v2::Limits::default();

        Self {
            // This default was picked as the sum of pre- and post- quorum timeouts from
            // [sui_core::authority_aggregator::TimeoutConfig], with a 10% buffer.
            //
            // <https://github.com/MystenLabs/sui/blob/eaf05fe5d293c06e3a2dfc22c87ba2aef419d8ea/crates/sui-core/src/authority_aggregator.rs#L84-L85>
            mutation_timeout_ms: 74_000,
            query_timeout_ms: 40_000,
            max_query_depth: 20,
            max_query_nodes: 300,
            max_output_nodes: 1_000_000,
            // Add a 30% buffer to the protocol limit, rounded up to account Base64 overhead.
            max_tx_payload_size: (max_tx_size_bytes * 4).div_ceil(3),
            max_query_payload_size: 5_000,
            default_page_size: 20,
            max_page_size: 50,
            max_multi_get_size: 200,
            // A much larger page size than the default, to make it unlikely that users need to
            // fetch a second page.
            page_size_override_fx_object_changes: 1024,
            page_size_override_packages: 200,
            max_type_argument_depth,
            max_type_argument_width,
            max_type_nodes,
            max_move_value_depth,
            max_move_value_bound: 1024 * 1024,
            max_display_field_depth: display_limits.max_depth,
            max_display_format_nodes: display_limits.max_nodes,
            max_display_object_loads: display_limits.max_loads,
            max_display_output_size: 1024 * 1024,
            max_disassembled_module_size: 1024 * 1024,
            max_rich_queries: 21,
        }
    }
}

impl Default for WatermarkConfig {
    fn default() -> Self {
        Self {
            watermark_polling_interval: Duration::from_millis(500),
        }
    }
}

impl Default for ZkLoginConfig {
    fn default() -> Self {
        Self {
            env: ZkLoginEnv::Prod,
            max_epoch_upper_bound_delta: max_across_protocol(
                ProtocolConfig::zklogin_max_epoch_upper_bound_delta,
            ),
        }
    }
}

/// Fetch the maximum value of a protocol config across all chains and versions.
fn max_across_protocol<T: Ord>(f: impl Fn(&ProtocolConfig) -> Option<T>) -> Option<T> {
    let mut x = None;
    let mut v = ProtocolVersion::MIN;
    while v <= ProtocolVersion::MAX {
        x = x.max(f(&ProtocolConfig::get_for_version(v, Chain::Unknown)));
        x = x.max(f(&ProtocolConfig::get_for_version(v, Chain::Mainnet)));
        x = x.max(f(&ProtocolConfig::get_for_version(v, Chain::Testnet)));
        v = v + 1;
    }

    x
}

/// Resolve availability for every pipeline mentioned in config, given a shared default and
/// per-pipeline overrides.
fn finish_pipelines(
    defaults: PipelineLayer,
    pipeline: BTreeMap<String, PipelineLayer>,
    base: PipelineConfig,
) -> PipelineConfig {
    let default_availability = PipelineAvailability::from(defaults);

    let mut availability = base.availability;
    availability.extend(pipeline.into_iter().map(|(name, entry)| {
        let status = if entry.enabled.is_some() {
            PipelineAvailability::from(entry)
        } else {
            default_availability
        };
        (name, status)
    }));

    PipelineConfig {
        availability,
        default_availability,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_pipelines(config: &PipelineConfig) -> BTreeSet<&str> {
        config
            .availability
            .iter()
            .filter(|(_, status)| matches!(status, PipelineAvailability::Enabled))
            .map(|(name, _)| name.as_str())
            .collect()
    }

    #[test]
    fn pipeline_default_enabled_applies_to_unset_entries() {
        let layer: RpcLayer = toml::from_str(
            r#"
            [pipeline-defaults]
            enabled = true

            [pipeline.tx_calls]

            [pipeline.kv_objects]
            enabled = false

            [pipeline.obj_versions]
            "#,
        )
        .unwrap();

        let config = layer.finish();
        assert_eq!(
            enabled_pipelines(&config.pipeline),
            BTreeSet::from(["tx_calls", "obj_versions"]),
        );
    }

    #[test]
    fn pipeline_unlisted_is_never_enabled() {
        let layer: RpcLayer = toml::from_str(
            r#"
            [pipeline-defaults]
            enabled = true

            [pipeline.tx_calls]
            "#,
        )
        .unwrap();

        let config = layer.finish();
        assert_eq!(
            enabled_pipelines(&config.pipeline),
            BTreeSet::from(["tx_calls"]),
        );
        assert!(!enabled_pipelines(&config.pipeline).contains("kv_objects"));
    }

    #[test]
    fn pipeline_missing_defaults_section_enables_listed_pipelines() {
        let layer: RpcLayer = toml::from_str(
            r#"
            [pipeline.tx_calls]

            [pipeline.kv_objects]
            enabled = false
            "#,
        )
        .unwrap();

        let config = layer.finish();
        assert_eq!(
            enabled_pipelines(&config.pipeline),
            BTreeSet::from(["tx_calls"]),
        );
    }

    #[test]
    fn pipeline_enabled_override_applies_despite_false_default() {
        let layer: RpcLayer = toml::from_str(
            r#"
            [pipeline-defaults]
            enabled = false

            [pipeline.tx_calls]
            enabled = true

            [pipeline.kv_objects]
            "#,
        )
        .unwrap();

        let config = layer.finish();
        assert_eq!(
            enabled_pipelines(&config.pipeline),
            BTreeSet::from(["tx_calls"]),
        );
    }

    #[test]
    fn availability_within_tip_respects_lag() {
        let a = PipelineAvailability::MaxCheckpointLag(100);
        // At the tip, and exactly at the lag boundary (inclusive), are available.
        assert!(a.is_available(1_000_000, 1_000_000));
        assert!(a.is_available(999_900, 1_000_000));
        // One checkpoint beyond the lag budget is unavailable.
        assert!(!a.is_available(999_899, 1_000_000));
        // A watermark momentarily ahead of the recorded tip saturates to zero lag.
        assert!(a.is_available(1_000_050, 1_000_000));
    }

    #[test]
    fn enabled_and_disabled_ignore_lag() {
        // Force-on serves regardless of distance from the tip; force-off never serves, even at it.
        assert!(PipelineAvailability::Enabled.is_available(0, 1_000_000));
        assert!(!PipelineAvailability::Disabled.is_available(1_000_000, 1_000_000));
    }

    #[test]
    fn default_and_overrides_parse_from_toml() {
        let layer: RpcLayer = toml::from_str(
            r#"
            [pipeline-defaults.availability]
            max-checkpoint-lag = 100

            [pipeline.tx_calls.availability]
            max-checkpoint-lag = 1000

            [pipeline.tx_kinds.availability]
            max-checkpoint-lag = 0
            "#,
        )
        .unwrap();

        let config = layer.finish().availability;
        assert_eq!(
            config.policy_for("tx_calls"),
            Some(&PipelineAvailability::MaxCheckpointLag(1000))
        );
        assert_eq!(
            config.policy_for("tx_kinds"),
            Some(&PipelineAvailability::MaxCheckpointLag(0))
        );
        // A pipeline with no override falls back to the default.
        assert_eq!(
            config.policy_for("unset"),
            Some(&PipelineAvailability::MaxCheckpointLag(100))
        );
    }

    #[test]
    fn enabled_and_disabled_parse_from_toml() {
        let layer: RpcLayer = toml::from_str(
            r#"
            [pipeline-defaults.availability]
            max-checkpoint-lag = 100

            [pipeline.kv_objects.availability]
            enabled = true

            [pipeline.tx_kinds.availability]
            enabled = false
            "#,
        )
        .unwrap();

        let config = layer.finish().availability;
        assert_eq!(
            config.policy_for("kv_objects"),
            Some(&PipelineAvailability::Enabled)
        );
        assert_eq!(
            config.policy_for("tx_kinds"),
            Some(&PipelineAvailability::Disabled)
        );
    }

    #[test]
    fn enabled_and_lag_are_mutually_exclusive() {
        let err = toml::from_str::<RpcLayer>(
            r#"
            [pipeline.tx_calls.availability]
            enabled = true
            max-checkpoint-lag = 100
            "#,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("mutually exclusive"),
            "unexpected error: {err}",
        );
    }

    #[test]
    fn overrides_without_a_default_gate_only_themselves() {
        let layer: RpcLayer = toml::from_str(
            r#"
            [pipeline.tx_calls.availability]
            max-checkpoint-lag = 100
            "#,
        )
        .unwrap();

        let config = layer.finish().availability;
        assert_eq!(
            config.policy_for("tx_calls"),
            Some(&PipelineAvailability::MaxCheckpointLag(100))
        );
        assert_eq!(config.policy_for("unset"), None);
    }

    #[test]
    fn empty_availability_sections_are_rejected() {
        // A policy section exists only to set a policy, so one of its keys is mandatory.
        for toml in [
            "[pipeline-defaults.availability]",
            "[pipeline.tx_calls.availability]",
        ] {
            let err = toml::from_str::<RpcLayer>(toml).unwrap_err();
            assert!(
                err.to_string()
                    .contains("expected 'enabled' or 'max-checkpoint-lag'"),
                "unexpected error for {toml:?}: {err}",
            );
        }
    }

    #[test]
    fn pipeline_section_without_availability_has_no_override() {
        // An enablement-only pipeline section (no `.availability` sub-table) implies no
        // availability override, regardless of whether it enables the pipeline.
        let layer: RpcLayer = toml::from_str("[pipeline.tx_calls]").unwrap();
        let config = layer.finish().availability;
        assert_eq!(config.policy_for("tx_calls"), None);
    }

    #[test]
    fn availability_sections_reject_unknown_fields() {
        for toml in [
            "[pipeline-defaults.availability]\nmode = \"disabled\"",
            "[pipeline.tx_calls]\nmode = \"disabled\"",
            "[pipeline.tx_calls.availability]\nmax-checkpoint-lag = 100\nmode = \"disabled\"",
        ] {
            let result: Result<RpcLayer, _> = toml::from_str(toml);
            assert!(result.is_err(), "expected {toml:?} to be rejected");
        }
    }

    #[test]
    fn empty_config_has_no_availability_policies() {
        let layer: RpcLayer = toml::from_str("").unwrap();
        let config = layer.finish().availability;
        assert!(config.default.is_none());
        assert!(config.pipelines.is_empty());
    }

    #[test]
    fn example_config_roundtrips() {
        let example = RpcLayer::example();
        let serialized = toml::to_string_pretty(&example).unwrap();
        let parsed: RpcLayer = toml::from_str(&serialized).unwrap();
        assert_eq!(parsed.pipeline_defaults, example.pipeline_defaults);
        assert_eq!(parsed.pipeline, example.pipeline);
    }

    #[test]
    fn availability_policies_roundtrip() {
        for policy in [
            PipelineAvailability::Enabled,
            PipelineAvailability::Disabled,
            PipelineAvailability::MaxCheckpointLag(100),
        ] {
            let layer = RpcLayer {
                pipeline_defaults: PipelineLayer {
                    availability: Some(policy),
                    ..PipelineLayer::default()
                },
                ..RpcLayer::default()
            };
            let serialized = toml::to_string_pretty(&layer).unwrap();
            let parsed: RpcLayer = toml::from_str(&serialized).unwrap();
            assert_eq!(
                parsed.pipeline_defaults.availability,
                Some(policy),
                "roundtrip failed for {policy:?}:\n{serialized}",
            );
        }
    }

    #[test]
    fn unknown_top_level_key_is_rejected() {
        let result: Result<RpcLayer, _> = toml::from_str("nonexistent-key = 3");
        assert!(result.is_err());
    }
}
