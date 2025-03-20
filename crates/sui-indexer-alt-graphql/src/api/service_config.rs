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

    /// Maximum depth of a GraphQL query that can be accepted by this service.
    async fn max_query_depth(&self, ctx: &Context<'_>) -> Result<u32, RpcError> {
        let config: &RpcConfig = ctx.data()?;
        Ok(config.limits.max_query_depth)
    }

    /// The maximum number of nodes (field names) the service will accept in a single query.
    async fn max_query_nodes(&self, ctx: &Context<'_>) -> Result<u32, RpcError> {
        let config: &RpcConfig = ctx.data()?;
        Ok(config.limits.max_query_nodes)
    }

    /// Maximum number of estimated output nodes in a GraphQL response.
    ///
    /// The estimate is an upperbound of how many nodes there would be in the output assuming every requested field is present, paginated requests return full page sizes, and multi-get queries find all requested keys. Below is a worked example query:
    ///
    /// ```graphql
    /// |  0: query {                            # 273 = 2 + 6 + 265
    /// |  1:   checkpoint {                     #   2 = 1 + 1
    /// |  2:     sequenceNumber                 #     1
    /// |  3:   }
    /// |  4:
    /// |  5:   multiGetObjects([$a, $b, $c]) {  #   6 = 1 + 3 * (1 + 1)
    /// |  6:     address                        #     1
    /// |  7:     digest                         #     1
    /// |  8:   }
    /// |  9:
    /// | 10:   # default page size is 20
    /// | 11:   transactions {                   #   265 = 1 + 3 + 1 + 20 * (1 + 12)
    /// | 12:     pageInfo {                     #     3 = 1 + 1 + 1
    /// | 13:       hasNextPage                  #       1
    /// | 14:       endCursor                    #       1
    /// | 15:     }
    /// | 16:
    /// | 17:     nodes {                        #     1
    /// | 18:       digest                       #       1
    /// | 19:       effects {                    #       12 = 1 + 11
    /// | 20:         objectChanges(first: 10) { #         11 = 1 + 10 * (1)
    /// | 21:           nodes {                  #         1
    /// | 22:             address                #           1
    /// | 23:           }
    /// | 24:         }
    /// | 25:       }
    /// | 26:     }
    /// | 27:   }
    /// | 28: }
    /// ```
    async fn max_output_nodes(&self, ctx: &Context<'_>) -> Result<u32, RpcError> {
        let config: &RpcConfig = ctx.data()?;
        Ok(config.limits.max_output_nodes)
    }

    /// Maximum size in bytes allowed for the `txBytes` and `signatures` parameters of an `executeTransaction` or `simulateTransaction` field, or the `bytes` and `signature` parameters of a `verifyZkLoginSignature` field.
    ///
    /// This is cumulative across all matching fields in a single GraphQL request.
    async fn max_transaction_payload_size(&self, ctx: &Context<'_>) -> Result<u32, RpcError> {
        let config: &RpcConfig = ctx.data()?;
        Ok(config.limits.max_tx_payload_size)
    }

    /// Maximum size in bytes of a single GraphQL request, excluding the elements covered by `maxTransactionPayloadSize`.
    async fn max_query_payload_size(&self, ctx: &Context<'_>) -> Result<u32, RpcError> {
        let config: &RpcConfig = ctx.data()?;
        Ok(config.limits.max_query_payload_size)
    }

    /// By default, paginated queries will return this many elements if a page size is not provided. This may be overridden for paginated queries that are limited by the protocol.
    async fn default_page_size(&self, ctx: &Context<'_>) -> Result<u32, RpcError> {
        let config: &RpcConfig = ctx.data()?;
        Ok(config.limits.default_page_size)
    }

    /// By default, paginated queries can return at most this many elements. A request to fetch more elements will result in an error. This limit may be superseded when the field being paginated is limited by the protocol (e.g. object changes for a transaction).
    async fn max_page_size(&self, ctx: &Context<'_>) -> Result<u32, RpcError> {
        let config: &RpcConfig = ctx.data()?;
        Ok(config.limits.max_page_size)
    }
}
