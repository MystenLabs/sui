// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{mem, time::Duration};

use sui_default_config::DefaultConfig;
use tracing::warn;

use crate::extensions::timeout::TimeoutConfig;

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
}

#[DefaultConfig]
#[derive(Default, Clone, Debug)]
pub struct LimitsLayer {
    pub mutation_timeout_ms: Option<u32>,
    pub query_timeout_ms: Option<u32>,

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
}

impl LimitsLayer {
    pub(crate) fn finish(mut self, base: Limits) -> Limits {
        check_extra("limits", mem::take(&mut self.extra));
        Limits {
            mutation_timeout_ms: self.mutation_timeout_ms.unwrap_or(base.mutation_timeout_ms),
            query_timeout_ms: self.query_timeout_ms.unwrap_or(base.query_timeout_ms),
        }
    }
}

impl From<Limits> for LimitsLayer {
    fn from(value: Limits) -> Self {
        Self {
            mutation_timeout_ms: Some(value.mutation_timeout_ms),
            query_timeout_ms: Some(value.query_timeout_ms),
            extra: Default::default(),
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            // This default was picked as the sum of pre- and post- quorum timeouts from
            // [sui_core::authority_aggregator::TimeoutConfig], with a 10% buffer.
            //
            // <https://github.com/MystenLabs/sui/blob/eaf05fe5d293c06e3a2dfc22c87ba2aef419d8ea/crates/sui-core/src/authority_aggregator.rs#L84-L85>
            mutation_timeout_ms: 74_000,
            query_timeout_ms: 40_000,
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
