// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Context as _};
use diesel::{ExpressionMethods, JoinOnDsl, QueryDsl};
use sui_indexer_alt_schema::schema::{tx_affected_objects, tx_calls, tx_digests};
use sui_json_rpc_types::{Page as PageResponse, TransactionFilter};
use sui_types::{
    base_types::ObjectID,
    digests::TransactionDigest,
    messages_checkpoint::{CheckpointContents, CheckpointSummary},
};

use crate::{
    data::checkpoints::CheckpointKey,
    error::{invalid_params, rpc_bail, RpcError},
    paginate::{Cursor, Page},
};

use super::{error::Error, Context, TransactionsConfig};

/// A list of transaction sequence numbers and their digests, as they appear in the database.
type Keys = Vec<(i64, Vec<u8>)>;

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

        Some(F::Checkpoint(seq)) => by_checkpoint(ctx, &page, *seq).await?,

        Some(F::MoveFunction {
            package,
            module,
            function,
        }) => tx_calls(ctx, &page, package, module.as_ref(), function.as_ref()).await?,

        Some(F::AffectedObject(object)) => tx_affected_objects(ctx, &page, *object).await?,

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
async fn all_transactions(ctx: &Context, page: &Page<u64>) -> Result<Keys, RpcError<Error>> {
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

/// Fetch a page of transaction digests from the given `checkpoint` (by sequence number). Fetches
/// one more result than was requested to detect a next page.
async fn by_checkpoint(
    ctx: &Context,
    page: &Page<u64>,
    checkpoint: u64,
) -> Result<Keys, RpcError<Error>> {
    let Some(checkpoint) = ctx
        .loader()
        .load_one(CheckpointKey(checkpoint))
        .await
        .context("Failed to load checkpoint")?
    else {
        return Ok(vec![]);
    };

    let summary: CheckpointSummary = bcs::from_bytes(&checkpoint.checkpoint_summary)
        .context("Failed to deserialize checkpoint summary")?;

    let contents: CheckpointContents = bcs::from_bytes(&checkpoint.checkpoint_contents)
        .context("Failed to deserialize checkpoint contents")?;

    // Transaction sequence number bounds from the checkpoint
    let cp_hi = summary.network_total_transactions;
    let cp_lo = summary.network_total_transactions - contents.inner().len() as u64;

    let Page {
        cursor,
        limit,
        descending,
    } = page;

    // Transaction sequence number bounds from the page
    let pg_lo: u64;
    let pg_hi: u64;
    if *descending {
        pg_hi = cursor.as_ref().map(|c| c.0).map_or(cp_hi, |c| c.min(cp_hi));
        pg_lo = pg_hi.saturating_sub(1 + *limit as u64).max(cp_lo);
    } else {
        pg_lo = cursor
            .as_ref()
            .map(|c| c.0.saturating_add(1))
            .map_or(cp_lo, |c| c.max(cp_lo));

        pg_hi = pg_lo.saturating_add(1 + *limit as u64).min(cp_hi);
    }

    let digests = contents.inner();
    let mut results = Vec::with_capacity(pg_hi.saturating_sub(pg_lo) as usize);
    for tx in pg_lo..pg_hi {
        let ix = (tx - cp_lo) as usize;
        let digest = digests
            .get(ix)
            .ok_or_else(|| anyhow!("Transaction out of bounds in checkpoint"))?
            .transaction
            .inner()
            .to_vec();

        results.push((tx as i64, digest));
    }

    if *descending {
        results.reverse();
    }

    Ok(results)
}

/// Fetch a page of transaction digests that called the described function(s). Functions can be
/// selected by just their package, their module, or their fully-qualified name. It is an error to
/// supply a package and function, but no module.
///
/// Fetches one more result than was requested to detect a next page.
async fn tx_calls(
    ctx: &Context,
    page: &Page<u64>,
    package: &ObjectID,
    module: Option<&String>,
    function: Option<&String>,
) -> Result<Keys, RpcError<Error>> {
    use tx_calls::dsl as c;
    use tx_digests::dsl as d;

    if let (None, Some(function)) = (module, function) {
        return Err(invalid_params(Error::MissingModule {
            function: function.clone(),
        }));
    }

    let mut query = d::tx_digests
        .inner_join(c::tx_calls.on(d::tx_sequence_number.eq(c::tx_sequence_number)))
        .select((d::tx_sequence_number, d::tx_digest))
        .limit(page.limit + 1)
        .into_boxed();

    query = query.filter(c::package.eq(package.as_slice()));

    if let Some(module) = module {
        query = query.filter(c::module.eq(module.as_str()));
    }

    if let Some(function) = function {
        query = query.filter(c::function.eq(function.as_str()));
    }

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

/// Fetch a page of transaction digests that touched `object` (created it, modified it, deleted it
/// or wrapped it). Fetches one more result than was requested to detect a next page.
async fn tx_affected_objects(
    ctx: &Context,
    page: &Page<u64>,
    object: ObjectID,
) -> Result<Keys, RpcError<Error>> {
    use tx_affected_objects::dsl as o;
    use tx_digests::dsl as d;

    let mut query = d::tx_digests
        .inner_join(o::tx_affected_objects.on(d::tx_sequence_number.eq(o::tx_sequence_number)))
        .select((d::tx_sequence_number, d::tx_digest))
        .limit(page.limit + 1)
        .into_boxed();

    query = query.filter(o::affected.eq(object.as_slice()));

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
