// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Context, Object, Result};

use crate::{
    config::Limits,
    error::RpcError,
    pagination::{is_connection, PaginationConfig},
};

pub(crate) struct ServiceConfig;

#[Object]
impl ServiceConfig {
    /// Maximum time in milliseconds spent waiting for a response from fullnode after issuing a transaction to execute. Note that the transaction may still succeed even in the case of a timeout. Transactions are idempotent, so a transaction that times out should be re-submitted until the network returns a definite response (success or failure, not timeout).
    async fn mutation_timeout_ms(&self, ctx: &Context<'_>) -> Result<Option<u32>, RpcError> {
        let limits: &Limits = ctx.data()?;
        Ok(Some(limits.mutation_timeout_ms))
    }

    /// Maximum time in milliseconds that will be spent to serve one query request.
    async fn query_timeout_ms(&self, ctx: &Context<'_>) -> Result<Option<u32>, RpcError> {
        let limits: &Limits = ctx.data()?;
        Ok(Some(limits.query_timeout_ms))
    }

    /// Maximum depth of a GraphQL query that can be accepted by this service.
    async fn max_query_depth(&self, ctx: &Context<'_>) -> Result<Option<u32>, RpcError> {
        let limits: &Limits = ctx.data()?;
        Ok(Some(limits.max_query_depth))
    }

    /// The maximum number of nodes (field names) the service will accept in a single query.
    async fn max_query_nodes(&self, ctx: &Context<'_>) -> Result<Option<u32>, RpcError> {
        let limits: &Limits = ctx.data()?;
        Ok(Some(limits.max_query_nodes))
    }

    /// Maximum number of estimated output nodes in a GraphQL response.
    ///
    /// The estimate is an upperbound of how many nodes there would be in the output assuming every requested field is present, paginated requests return full page sizes, and multi-get queries find all requested keys. Below is a worked example query:
    ///
    /// ```graphql
    /// |  0: query {                            # 514 = total
    /// |  1:   checkpoint {                     # 1
    /// |  2:     sequenceNumber                 # 1
    /// |  3:   }
    /// |  4:
    /// |  5:   multiGetObjects([$a, $b, $c]) {  # 1 (* 3)
    /// |  6:     address                        # 3
    /// |  7:     digest                         # 3
    /// |  8:   }
    /// |  9:
    /// | 10:   # default page size is 20
    /// | 11:   transactions {                   # 1 (* 20)
    /// | 12:     pageInfo {                     # 1
    /// | 13:       hasNextPage                  # 1
    /// | 14:       endCursor                    # 1
    /// | 15:     }
    /// | 16:
    /// | 17:     nodes                          # 1
    /// | 18:     {                              # 20
    /// | 19:       digest                       # 20
    /// | 20:       effects {                    # 20
    /// | 21:         objectChanges(first: 10) { # 20 (* 10)
    /// | 22:           nodes                    # 20
    /// | 23:           {                        # 200
    /// | 24:             address                # 200
    /// | 25:           }
    /// | 26:         }
    /// | 27:       }
    /// | 28:     }
    /// | 29:   }
    /// | 30: }
    /// ```
    async fn max_output_nodes(&self, ctx: &Context<'_>) -> Result<Option<u32>, RpcError> {
        let limits: &Limits = ctx.data()?;
        Ok(Some(limits.max_output_nodes))
    }

    /// Maximum size in bytes allowed for the `txBytes` and `signatures` parameters of an `executeTransaction` or `simulateTransaction` field, or the `bytes` and `signature` parameters of a `verifyZkLoginSignature` field.
    ///
    /// This is cumulative across all matching fields in a single GraphQL request.
    async fn max_transaction_payload_size(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<u32>, RpcError> {
        let limits: &Limits = ctx.data()?;
        Ok(Some(limits.max_tx_payload_size))
    }

    /// Maximum size in bytes of a single GraphQL request, excluding the elements covered by `maxTransactionPayloadSize`.
    async fn max_query_payload_size(&self, ctx: &Context<'_>) -> Result<Option<u32>, RpcError> {
        let limits: &Limits = ctx.data()?;
        Ok(Some(limits.max_query_payload_size))
    }

    /// Number of elements a paginated connection will return if a page size is not supplied.
    ///
    /// Accepts `type` and `field` arguments which identify the connection that is being queried. If the field in question is paginated, its default page size is returned. If it does not exist or is not paginated, `null` is returned.
    async fn default_page_size(
        &self,
        ctx: &Context<'_>,
        type_: String,
        field: String,
    ) -> Result<Option<u32>, RpcError> {
        let registry = &ctx.schema_env.registry;

        if !registry
            .concrete_type_by_name(&type_)
            .and_then(|t| t.field_by_name(&field))
            .is_some_and(is_connection)
        {
            return Ok(None);
        }

        let config: &PaginationConfig = ctx.data()?;
        Ok(Some(config.limits(&type_, &field).default))
    }

    /// Maximum number of elements that can be requested from a paginated connection. A request to fetch more elements will result in an error.
    ///
    /// Accepts `type` and `field` arguments which identify the connection that is being queried. If the field in question is paginated, its max page size is returned. If it does not exist or is not paginated, `null` is returned.
    async fn max_page_size(
        &self,
        ctx: &Context<'_>,
        type_: String,
        field: String,
    ) -> Result<Option<u32>, RpcError> {
        let registry = &ctx.schema_env.registry;

        if !registry
            .concrete_type_by_name(&type_)
            .and_then(|t| t.field_by_name(&field))
            .is_some_and(is_connection)
        {
            return Ok(None);
        }

        let config: &PaginationConfig = ctx.data()?;
        Ok(Some(config.limits(&type_, &field).max))
    }

    /// Maximum number of elements that can be requested from a multi-get query. A request to fetch more keys will result in an error.
    async fn max_multi_get_size(&self, ctx: &Context<'_>) -> Result<Option<u32>, RpcError> {
        let limits: &Limits = ctx.data()?;
        Ok(Some(limits.max_multi_get_size))
    }

    /// Maximum amount of nesting among type arguments (type arguments nest when a type argument is itself generic and has arguments).
    async fn max_type_argument_depth(&self, ctx: &Context<'_>) -> Result<Option<usize>, RpcError> {
        let limits: &Limits = ctx.data()?;
        Ok(Some(limits.max_type_argument_depth))
    }

    /// Maximum number of type parameters a type can have.
    async fn max_type_argument_width(&self, ctx: &Context<'_>) -> Result<Option<usize>, RpcError> {
        let limits: &Limits = ctx.data()?;
        Ok(Some(limits.max_type_argument_width))
    }

    /// Maximum number of datatypes that need to be processed when calculating the layout of a single type.
    async fn max_type_nodes(&self, ctx: &Context<'_>) -> Result<Option<usize>, RpcError> {
        let limits: &Limits = ctx.data()?;
        Ok(Some(limits.max_type_nodes))
    }

    /// Maximum nesting allowed in datatype fields when calculating the layout of a single type.
    async fn max_move_value_depth(&self, ctx: &Context<'_>) -> Result<Option<usize>, RpcError> {
        let limits: &Limits = ctx.data()?;
        Ok(Some(limits.max_move_value_depth))
    }

    /// Maximum budget in bytes to spend when outputting a structured `MoveValue`.
    async fn max_move_value_bound(&self, ctx: &Context<'_>) -> Result<Option<usize>, RpcError> {
        let limits: &Limits = ctx.data()?;
        Ok(Some(limits.max_move_value_bound))
    }
}
