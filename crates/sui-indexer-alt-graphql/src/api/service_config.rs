// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Context, Object, Result};

use crate::{config::RpcConfig, error::RpcError};

pub(crate) struct ServiceConfig;

#[Object]
impl ServiceConfig {
    /// Maximum time in milliseconds spent waiting for a response from fullnode after issuing a transaction to execute. Note that the transaction may still succeed even in the case of a timeout. Transactions are idempotent, so a transaction that times out should be re-submitted until the network returns a definite response (success or failure, not timeout).
    async fn mutation_timeout_ms(&self, ctx: &Context<'_>) -> Result<u32, RpcError> {
        let config: &RpcConfig = ctx.data()?;
        Ok(config.limits.mutation_timeout_ms)
    }

    /// Maximum time in milliseconds that will be spent to serve one query request.
    async fn query_timeout_ms(&self, ctx: &Context<'_>) -> Result<u32, RpcError> {
        let config: &RpcConfig = ctx.data()?;
        Ok(config.limits.query_timeout_ms)
    }
}
