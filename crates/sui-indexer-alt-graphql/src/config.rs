// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use sui_default_config::DefaultConfig;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};

use crate::{
    extensions::{query_limits::QueryLimitsConfig, timeout::TimeoutConfig},
    pagination::{PageLimits, PaginationConfig},
};

#[derive(Default)]
pub struct RpcConfig {
    /// Constraints that the service will impose on requests.
    pub limits: Limits,

    /// Configuration for health checks.
    pub health: HealthConfig,

    /// Configuration for the watermark task.
    pub watermark: WatermarkConfig,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
#[serde(deny_unknown_fields)]
pub struct RpcLayer {
    pub limits: LimitsLayer,
    pub health: HealthLayer,
    pub watermark: WatermarkLayer,
}

#[derive(Clone)]
pub struct HealthConfig {
    /// How long to wait for a health check to complete before timing out.
    pub max_checkpoint_lag: Duration,
}

#[DefaultConfig]
#[derive(Default, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct HealthLayer {
    pub max_checkpoint_lag_ms: Option<u64>,
}

/// Config for an indexer writing to a database used by this RPC service. It is simplified w.r.t.
/// to the actual indexer config to focus on extracting the names of pipelines enabled on that
/// indexer.
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct IndexerConfig {
    pub pipeline: toml::Table,
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
    /// `executeTransaction` or `simulateTransaction` field, or the `bytes` and `signature`
    /// parameters of a `verifyZkLoginSignature` field.
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

    /// Maximumm output size of a display output.
    pub max_display_output_size: usize,
}

#[DefaultConfig]
#[derive(Default, Clone, Debug)]
#[serde(deny_unknown_fields)]
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
    pub max_display_output_size: Option<usize>,
}

pub struct WatermarkConfig {
    /// How long to wait between updating the watermark.
    pub watermark_polling_interval: Duration,
}

#[DefaultConfig]
#[derive(Default, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct WatermarkLayer {
    pub watermark_polling_interval_ms: Option<u64>,
}

impl RpcLayer {
    pub fn example() -> Self {
        Self {
            limits: Limits::default().into(),
            health: HealthConfig::default().into(),
            watermark: WatermarkConfig::default().into(),
        }
    }

    pub fn finish(self) -> RpcConfig {
        RpcConfig {
            limits: self.limits.finish(Limits::default()),
            health: self.health.finish(HealthConfig::default()),
            watermark: self.watermark.finish(WatermarkConfig::default()),
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

impl IndexerConfig {
    /// Pipelines detected as enabled in this indexer configuration.
    pub fn pipelines(&self) -> impl Iterator<Item = &str> {
        self.pipeline.iter().map(|(k, _)| k.as_str())
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
                ("Mutation", "executeTransaction", "txBytes"),
                ("Mutation", "executeTransaction", "signatures"),
                ("Query", "simulateTransaction", "txBytes"),
                ("Query", "verifyZkloginSignature", "bytes"),
                ("Query", "verifyZkloginSignature", "signature"),
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
            max_display_output_size: self
                .max_display_output_size
                .unwrap_or(base.max_display_output_size),
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
            max_display_output_size: Some(value.max_display_output_size),
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
            max_tx_payload_size: (max_tx_size_bytes * 4).div_ceil(3) as u32,
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
            max_display_field_depth: 10,
            max_display_output_size: 1024 * 1024,
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
