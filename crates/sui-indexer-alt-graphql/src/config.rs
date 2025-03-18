// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use sui_default_config::DefaultConfig;

use crate::extensions::timeout::TimeoutConfig;

#[DefaultConfig]
#[derive(Default)]
pub struct RpcConfig {
    pub limits: Limits,
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

impl Limits {
    pub(crate) fn timeouts(&self) -> TimeoutConfig {
        TimeoutConfig {
            query: Duration::from_millis(self.query_timeout_ms as u64),
            mutation: Duration::from_millis(self.mutation_timeout_ms as u64),
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
