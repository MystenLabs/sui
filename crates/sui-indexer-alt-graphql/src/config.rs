// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    mem,
    time::Duration,
};

use sui_default_config::DefaultConfig;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use tracing::warn;

use crate::{
    extensions::{query_limits::QueryLimitsConfig, timeout::TimeoutConfig},
    pagination::PaginationConfig,
};

#[derive(Default)]
pub struct RpcConfig {
    /// Constraints that the service will impose on requests.
    pub limits: Limits,
}

#[DefaultConfig]
#[derive(Clone, Default, Debug)]
pub struct RpcLayer {
    pub limits: LimitsLayer,

    #[serde(flatten)]
    pub extra: toml::Table,
}

#[DefaultConfig]
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
}

#[DefaultConfig]
#[derive(Default, Clone, Debug)]
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

    #[serde(flatten)]
    pub extra: toml::Table,
}

impl RpcLayer {
    pub fn example() -> Self {
        Self {
            limits: Limits::default().into(),
            extra: Default::default(),
        }
    }

    pub fn finish(mut self) -> RpcConfig {
        check_extra("top-level", mem::take(&mut self.extra));
        RpcConfig {
            limits: self.limits.finish(Limits::default()),
        }
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
            tx_payload_args: BTreeSet::from_iter([
                ("Mutation", "executeTransaction", "txBytes"),
                ("Mutation", "executeTransaction", "signatures"),
                ("Query", "simulateTransaction", "txBytes"),
                ("Query", "verifyZkloginSignature", "bytes"),
                ("Query", "verifyZkloginSignature", "signature"),
            ]),
        }
    }

    pub(crate) fn pagination(&self) -> PaginationConfig {
        PaginationConfig::new(self.default_page_size, BTreeMap::new())
    }
}

impl LimitsLayer {
    pub(crate) fn finish(mut self, base: Limits) -> Limits {
        check_extra("limits", mem::take(&mut self.extra));
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
            extra: Default::default(),
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        let max_tx_size_bytes = max_across_protocol(ProtocolConfig::max_tx_size_bytes_as_option)
            .unwrap_or(u32::MAX as u64) as u32;

        Self {
            // This default was picked as the sum of pre- and post- quorum timeouts from
            // [sui_core::authority_aggregator::TimeoutConfig], with a 10% buffer.
            //
            // <https://github.com/MystenLabs/sui/blob/eaf05fe5d293c06e3a2dfc22c87ba2aef419d8ea/crates/sui-core/src/authority_aggregator.rs#L84-L85>
            mutation_timeout_ms: 74_000,
            query_timeout_ms: 40_000,
            max_query_depth: 20,
            max_query_nodes: 300,
            max_output_nodes: 100_000,
            // Add a 30% buffer to the protocol limit, rounded up to account Base64 overhead.
            max_tx_payload_size: (max_tx_size_bytes * 4).div_ceil(3) as u32,
            max_query_payload_size: 5_000,
            default_page_size: 20,
            max_page_size: 50,
        }
    }
}

/// Check whether there are any unrecognized extra fields and if so, warn about them.
fn check_extra(pos: &str, extra: toml::Table) {
    if !extra.is_empty() {
        warn!(
            "Found unrecognized {pos} field{} which will be ignored. This could be \
             because of a typo, or because it was introduced in a newer version of the indexer:\n{}",
            if extra.len() != 1 { "s" } else { "" },
            extra,
        )
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
