// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use diesel::{ExpressionMethods, QueryDsl};
use sui_indexer_alt_schema::schema::tx_digests;
use sui_json_rpc_types::{Page as PageResponse, TransactionFilter};
use sui_types::digests::TransactionDigest;

use crate::{
    error::{invalid_params, rpc_bail, RpcError},
    paginate::{Cursor, Page},
};

use super::{error::Error, Context, TransactionsConfig};

/// Fetch the digests for a page of transactions that satisfy the given `filter` and pagination
/// parameters. Returns the digests and a cursor pointing to the last result (if there are any
/// results).
pub(super) async fn transactions(
    ctx: &Context,
    config: &TransactionsConfig,
    filter: &Option<TransactionFilter>,
    cursor: Option<String>,
    limit: Option<usize>,
    descending_order: Option<bool>,
) -> Result<PageResponse<TransactionDigest, String>, RpcError<Error>> {
    let page: Page<u64> = Page::from_params(
        config.default_page_size,
        config.max_page_size,
        cursor,
        limit,
        descending_order,
    )?;

    use TransactionFilter as F;
    let mut refs = match filter {
        None => all_transactions(ctx, &page).await?,

        Some(F::TransactionKind(_) | F::TransactionKindIn(_)) => {
            return unsupported("TransactionKind filter is not supported")
        }

        Some(F::InputObject(_)) => {
            return unsupported(
                "InputObject filter is not supported, please use AffectedObject instead.",
            )
        }

        Some(F::ChangedObject(_)) => {
            return unsupported(
                "ChangedObject filter is not supported, please use AffectedObject instead.",
            )
        }

        Some(F::ToAddress(_)) => {
            return unsupported(
                "ToAddress filter is not supported, please use FromOrToAddress instead.",
            )
        }

        _ => rpc_bail!("Not implemented yet"),
    };

    let has_next_page = refs.len() > page.limit as usize;
    if has_next_page {
        refs.truncate(page.limit as usize);
    }

    let digests = refs
        .iter()
        .map(|(_, digest)| TransactionDigest::try_from(digest.as_slice()))
        .collect::<Result<Vec<TransactionDigest>, _>>()
        .context("Failed to deserialize transaction digests")?;

    let cursor = refs
        .last()
        .map(|(last, _)| Cursor(*last).encode())
        .transpose()
        .context("Failed to encode next cursor")?;

    Ok(PageResponse {
        data: digests,
        next_cursor: cursor,
        has_next_page,
    })
}

/// Fetch a page of transaction digests without filtering them. Fetches one more result than was
/// requested to detect a next page.
async fn all_transactions(
    ctx: &Context,
    page: &Page<u64>,
) -> Result<Vec<(i64, Vec<u8>)>, RpcError<Error>> {
    use tx_digests::dsl as d;

    let mut query = d::tx_digests
        .select((d::tx_sequence_number, d::tx_digest))
        .limit(page.limit + 1)
        .into_boxed();

    if let Some(Cursor(tx)) = page.cursor {
        if page.descending {
            query = query.filter(d::tx_sequence_number.lt(tx as i64));
        } else {
            query = query.filter(d::tx_sequence_number.gt(tx as i64));
        }
    }

    if page.descending {
        query = query.order(d::tx_sequence_number.desc());
    } else {
        query = query.order(d::tx_sequence_number.asc());
    }

    let mut conn = ctx
        .reader()
        .connect()
        .await
        .context("Failed to connect to database")?;

    let refs: Vec<(i64, Vec<u8>)> = conn
        .results(query)
        .await
        .context("Failed to fetch matching transaction digests")?;

    Ok(refs)
}

fn unsupported<T>(msg: &'static str) -> Result<T, RpcError<Error>> {
    Err(invalid_params(Error::Unsupported(msg)))
}
