// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_json_rpc_types::{Page, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::digests::TransactionDigest;

use self::{error::Error, filter::SuiTransactionBlockResponseQuery};

use crate::{
    context::Context,
    error::{rpc_bail, InternalContext, RpcError},
};

use super::rpc_module::RpcModule;

mod error;
mod filter;
mod response;

#[open_rpc(namespace = "sui", tag = "Transactions API")]
#[rpc(server, namespace = "sui")]
trait TransactionsApi {
    /// Fetch a transaction by its transaction digest.
    #[method(name = "getTransactionBlock")]
    async fn get_transaction_block(
        &self,
        /// The digest of the queried transaction.
        digest: TransactionDigest,
        /// Options controlling the output format.
        options: Option<SuiTransactionBlockResponseOptions>,
    ) -> RpcResult<SuiTransactionBlockResponse>;
}

#[open_rpc(namespace = "suix", tag = "Query Transactions API")]
#[rpc(server, namespace = "suix")]
trait QueryTransactionsApi {
    /// Query transactions based on their properties (sender, affected addresses, function calls,
    /// etc). Returns a paginated list of transactions.
    ///
    /// If a cursor is provided, the query will start from the transaction after the one pointed to
    /// by this cursor, otherwise pagination starts from the first transaction that meets the query
    /// criteria.
    ///
    /// The definition of "first" transaction is changed by the `descending_order` parameter, which
    /// is optional, and defaults to false, meaning that the oldest transaction is shown first.
    ///
    /// The size of each page is controlled by the `limit` parameter.
    #[method(name = "queryTransactionBlocks")]
    async fn query_transaction_blocks(
        &self,
        /// The query criteria, and the output options.
        query: SuiTransactionBlockResponseQuery,
        /// Cursor to start paginating from.
        cursor: Option<String>,
        /// Maximum number of transactions to return per page.
        limit: Option<usize>,
        /// Order of results, defaulting to ascending order (false), by sequence on-chain.
        descending_order: Option<bool>,
    ) -> RpcResult<Page<SuiTransactionBlockResponse, String>>;
}

pub(crate) struct Transactions(pub Context);

pub(crate) struct QueryTransactions(pub Context);

#[async_trait::async_trait]
impl TransactionsApiServer for Transactions {
    async fn get_transaction_block(
        &self,
        digest: TransactionDigest,
        options: Option<SuiTransactionBlockResponseOptions>,
    ) -> RpcResult<SuiTransactionBlockResponse> {
        let Self(ctx) = self;
        Ok(
            response::transaction(ctx, digest, &options.unwrap_or_default())
                .await
                .with_internal_context(|| format!("Failed to get transaction {digest}"))?,
        )
    }
}

#[async_trait::async_trait]
impl QueryTransactionsApiServer for QueryTransactions {
    async fn query_transaction_blocks(
        &self,
        query: SuiTransactionBlockResponseQuery,
        cursor: Option<String>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<Page<SuiTransactionBlockResponse, String>> {
        let Self(ctx) = self;

        let Page {
            data: digests,
            next_cursor,
            has_next_page,
        } = filter::transactions(ctx, &query.filter, cursor.clone(), limit, descending_order)
            .await?;

        let options = query.options.unwrap_or_default();

        let tx_futures = digests.iter().map(|d| {
            async {
                let mut tx = response::transaction(ctx, *d, &options).await;

                let config = &ctx.config().transactions;
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(
                    config.tx_retry_interval_ms,
                ));

                let mut retries = 0;
                for _ in 0..config.tx_retry_count {
                    // Retry only if the error is an invalid params error, which can only be due to
                    // the transaction not being found in the kv store or tx balance changes table.
                    if let Err(RpcError::InvalidParams(
                        _e @ (Error::BalanceChangesNotFound(_) | Error::NotFound(_)),
                    )) = tx
                    {
                        interval.tick().await;
                        retries += 1;
                        tx = response::transaction(ctx, *d, &options).await;
                        ctx.metrics()
                            .read_retries
                            .with_label_values(&["tx_response"])
                            .inc();
                    } else {
                        break;
                    }
                }

                ctx.metrics()
                    .read_retries_per_request
                    .with_label_values(&["tx_response"])
                    .observe(retries as f64);
                tx
            }
        });

        let data = future::join_all(tx_futures)
            .await
            .into_iter()
            .zip(digests)
            .map(|(r, d)| {
                if let Err(RpcError::InvalidParams(e @ Error::NotFound(_))) = r {
                    rpc_bail!(e)
                } else {
                    r.with_internal_context(|| format!("Failed to get transaction {d}"))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Page {
            data,
            next_cursor: next_cursor.or(cursor),
            has_next_page,
        })
    }
}

impl RpcModule for Transactions {
    fn schema(&self) -> Module {
        TransactionsApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}

impl RpcModule for QueryTransactions {
    fn schema(&self) -> Module {
        QueryTransactionsApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}
