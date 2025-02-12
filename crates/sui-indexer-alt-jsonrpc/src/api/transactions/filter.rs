// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Context as _};
use diesel::{
    dsl::sql,
    expression::{
        is_aggregate::{Never, No},
        MixedAggregates, ValidGrouping,
    },
    pg::Pg,
    query_builder::{BoxedSelectStatement, FromClause, QueryFragment},
    sql_types::BigInt as SqlBigInt,
    AppearsOnTable, Column, Expression, ExpressionMethods, QueryDsl, QuerySource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sui_indexer_alt_schema::schema::{
    tx_affected_addresses, tx_affected_objects, tx_calls, tx_digests,
};
use sui_json_rpc_types::{Page as PageResponse, SuiTransactionBlockResponseOptions};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    digests::TransactionDigest,
    messages_checkpoint::{CheckpointContents, CheckpointSequenceNumber, CheckpointSummary},
    sui_serde::{BigInt, Readable},
};

use crate::{
    data::{checkpoints::CheckpointKey, tx_digests::TxDigestKey},
    error::{invalid_params, RpcError},
    paginate::{Cursor as _, JsonCursor, Page},
};

use super::{error::Error, Context, TransactionsConfig};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
#[serde(
    rename_all = "camelCase",
    rename = "TransactionBlockResponseQuery",
    default
)]
pub(crate) struct SuiTransactionBlockResponseQuery {
    /// If None, no filter will be applied.
    pub filter: Option<TransactionFilter>,
    /// Configures which fields to include in the response, by default only digest is included.
    pub options: Option<SuiTransactionBlockResponseOptions>,
}

#[serde_as]
#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize)]
pub(crate) enum TransactionFilter {
    /// Query by checkpoint.
    Checkpoint(
        #[schemars(with = "BigInt<u64>")]
        #[serde_as(as = "Readable<BigInt<u64>, _>")]
        CheckpointSequenceNumber,
    ),
    /// Query by move function.
    MoveFunction {
        package: ObjectID,
        module: Option<String>,
        function: Option<String>,
    },
    /// Query for transactions that touch this object.
    AffectedObject(ObjectID),
    /// Query by sender address.
    FromAddress(SuiAddress),
    /// Query by sender and recipient address.
    FromAndToAddress { from: SuiAddress, to: SuiAddress },
    /// Query transactions that have a given address as sender or recipient.
    FromOrToAddress { addr: SuiAddress },
}

type Cursor = JsonCursor<u64>;
type Digests = PageResponse<TransactionDigest, String>;

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
) -> Result<Digests, RpcError<Error>> {
    let page: Page<Cursor> = Page::from_params(
        config.default_page_size,
        config.max_page_size,
        cursor,
        limit,
        descending_order,
    )?;

    use TransactionFilter as F;
    match filter {
        None => all_transactions(ctx, &page).await,

        Some(F::Checkpoint(seq)) => by_checkpoint(ctx, &page, *seq).await,

        Some(F::MoveFunction {
            package,
            module,
            function,
        }) => tx_calls(ctx, &page, package, module.as_ref(), function.as_ref()).await,

        Some(F::AffectedObject(object)) => tx_affected_objects(ctx, &page, *object).await,

        Some(F::FromAddress(from)) => tx_affected_addresses(ctx, &page, Some(*from), *from).await,

        Some(F::FromAndToAddress { from, to }) => {
            tx_affected_addresses(ctx, &page, Some(*from), *to).await
        }

        Some(F::FromOrToAddress { addr }) => tx_affected_addresses(ctx, &page, None, *addr).await,
    }
}

/// Fetch a page of transaction digests without filtering them.
async fn all_transactions(ctx: &Context, page: &Page<Cursor>) -> Result<Digests, RpcError<Error>> {
    use tx_digests::dsl as d;

    let query = d::tx_digests
        .select((d::tx_sequence_number, d::tx_digest))
        .into_boxed();

    let results: Vec<(i64, Vec<u8>)> = ctx
        .reader()
        .connect()
        .await
        .context("Failed to connect to the database")?
        .results(paginate(page, "tx_digests", d::tx_sequence_number, query))
        .await
        .context("Failed to fetch transaction sequence numbers")?;

    from_digests(page.limit, results)
}

/// Fetch a page of transaction digests from the given `checkpoint` (by sequence number).
async fn by_checkpoint(
    ctx: &Context,
    page: &Page<Cursor>,
    checkpoint: u64,
) -> Result<Digests, RpcError<Error>> {
    let Some(checkpoint) = ctx
        .loader()
        .load_one(CheckpointKey(checkpoint))
        .await
        .context("Failed to load checkpoint")?
    else {
        return Ok(PageResponse::empty());
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

    from_digests(*limit, results)
}

/// Fetch a page of transaction digests that called the described function(s). Functions can be
/// selected by just their package, their module, or their fully-qualified name. It is an error to
/// supply a package and function, but no module.
async fn tx_calls(
    ctx: &Context,
    page: &Page<Cursor>,
    package: &ObjectID,
    module: Option<&String>,
    function: Option<&String>,
) -> Result<Digests, RpcError<Error>> {
    use tx_calls::dsl as c;

    if let (None, Some(function)) = (module, function) {
        return Err(invalid_params(Error::MissingModule {
            function: function.clone(),
        }));
    }

    let mut query = c::tx_calls
        .select(c::tx_sequence_number)
        .filter(c::package.eq(package.as_slice()))
        .into_boxed();

    if let Some(module) = module {
        query = query.filter(c::module.eq(module.as_str()));
    }

    if let Some(function) = function {
        query = query.filter(c::function.eq(function.as_str()));
    }

    let results: Vec<i64> = ctx
        .reader()
        .connect()
        .await
        .context("Failed to connect to the database")?
        .results(paginate(page, "tx_calls", c::tx_sequence_number, query))
        .await
        .context("Failed to fetch transaction sequence numbers")?;

    from_sequence_numbers(ctx, page.limit, results).await
}

/// Fetch a page of transaction digests that touched `object` (created it, modified it, deleted it
/// or wrapped it).
async fn tx_affected_objects(
    ctx: &Context,
    page: &Page<Cursor>,
    object: ObjectID,
) -> Result<Digests, RpcError<Error>> {
    use tx_affected_objects::dsl as o;

    let query = o::tx_affected_objects
        .select(o::tx_sequence_number)
        .filter(o::affected.eq(object.as_slice()))
        .into_boxed();

    let results: Vec<i64> = ctx
        .reader()
        .connect()
        .await
        .context("Failed to connect to the database")?
        .results(paginate(
            page,
            "tx_affected_objects",
            o::tx_sequence_number,
            query,
        ))
        .await
        .context("Failed to fetch transaction sequence numbers")?;

    from_sequence_numbers(ctx, page.limit, results).await
}

/// Fetch a page of transaction digests that touched the provided addresses (`from` and `to`).
///
/// - If both are supplied, then returns transactions that were sent by `from` and affected `to`.
/// - If `from == to` this is equivalent to filtering down to the transactions that were sent by `from`.
/// - If `from` is not supplied, this finds the transactions that `to` was affected by in some way
///   (either it is the sender, or it is the recipient of one of the output objects).
async fn tx_affected_addresses(
    ctx: &Context,
    page: &Page<Cursor>,
    from: Option<SuiAddress>,
    to: SuiAddress,
) -> Result<Digests, RpcError<Error>> {
    use tx_affected_addresses::dsl as a;

    let mut query = a::tx_affected_addresses
        .select(a::tx_sequence_number)
        .filter(a::affected.eq(to.to_inner()))
        .into_boxed();

    if let Some(from) = from {
        query = query.filter(a::sender.eq(from.to_inner()));
    }

    let results: Vec<i64> = ctx
        .reader()
        .connect()
        .await
        .context("Failed to connect to the database")?
        .results(paginate(
            page,
            "tx_affected_addresses",
            a::tx_sequence_number,
            query,
        ))
        .await
        .context("Failed to fetch transaction sequence numbers")?;

    from_sequence_numbers(ctx, page.limit, results).await
}

/// Modify `query` to be paginated according to `page`, using `tx_sequence_number` as the column
/// containing the sequence number. The query is also modified to limit results returned by the
/// reader low-watermark from the `watermarks` table (using the `cp_sequence_numbers` table to
/// translate a checkpoint bound into a transaction sequence number bound). This helps the database
/// avoid scanning dead tuples due to pruning.
///
/// The query fetches one more element than the limit, to determine if there is a next page.
fn paginate<'q, TX, ST, QS>(
    page: &Page<Cursor>,
    pipeline: &'static str,
    tx_sequence_number: TX,
    mut query: BoxedSelectStatement<'q, ST, FromClause<QS>, Pg>,
) -> BoxedSelectStatement<'q, ST, FromClause<QS>, Pg>
where
    QS: QuerySource,
    TX: Copy + Send + Sync + 'q,
    TX: ValidGrouping<()> + QueryFragment<Pg>,
    TX: Column<Table = QS> + AppearsOnTable<QS>,
    TX: ExpressionMethods + Expression<SqlType = SqlBigInt>,
    TX::IsAggregate: MixedAggregates<Never, Output = No>,
{
    query = query.filter(tx_sequence_number.ge(sql::<SqlBigInt>(&format!(
        r#"COALESCE(
            (
                SELECT
                    MAX(tx_lo)
                FROM
                    watermarks w
                INNER JOIN
                    cp_sequence_numbers c
                ON
                    w.reader_lo = c.cp_sequence_number
                WHERE
                    w.pipeline IN ('{pipeline}', 'tx_digests')
            ),
            0
        )"#
    ))));

    if let Some(JsonCursor(tx)) = page.cursor {
        if page.descending {
            query = query.filter(tx_sequence_number.lt(tx as i64));
        } else {
            query = query.filter(tx_sequence_number.gt(tx as i64));
        }
    }

    if page.descending {
        query = query.order(tx_sequence_number.desc());
    } else {
        query = query.order(tx_sequence_number.asc());
    }

    query.limit(page.limit + 1)
}

/// Convert a list of raw transaction sequence numbers from the database into a page of parsed
/// transaction digests, ready to be loaded. This requires loading digests from the data loader.
async fn from_sequence_numbers(
    ctx: &Context,
    limit: i64,
    mut rows: Vec<i64>,
) -> Result<Digests, RpcError<Error>> {
    let has_next_page = rows.len() > limit as usize;
    if has_next_page {
        rows.truncate(limit as usize);
    }

    let next_cursor = rows
        .last()
        .map(|last| JsonCursor(*last).encode())
        .transpose()
        .context("Failed to encode next cursor")?;

    let digests = ctx
        .loader()
        .load_many(rows.iter().map(|&seq| TxDigestKey(seq as u64)))
        .await
        .context("Failed to load transaction digests")?;

    let mut data = Vec::with_capacity(rows.len());
    for seq in rows {
        let bytes = digests
            .get(&TxDigestKey(seq as u64))
            .ok_or_else(|| anyhow!("Missing transaction digest for transaction {seq}"))?
            .tx_digest
            .as_slice();

        let digest = TransactionDigest::try_from(bytes)
            .context("Failed to deserialize transaction digests")?;

        data.push(digest);
    }

    Ok(PageResponse {
        data,
        next_cursor,
        has_next_page,
    })
}

/// Convert a list of raw transaction sequence numbers and digests from the database into a page of
/// parsed transaction digests, ready to be loaded.
fn from_digests(limit: i64, mut rows: Vec<(i64, Vec<u8>)>) -> Result<Digests, RpcError<Error>> {
    let has_next_page = rows.len() > limit as usize;
    if has_next_page {
        rows.truncate(limit as usize);
    }

    let data = rows
        .iter()
        .map(|(_, digest)| TransactionDigest::try_from(digest.as_slice()))
        .collect::<Result<Vec<TransactionDigest>, _>>()
        .context("Failed to deserialize transaction digests")?;

    let next_cursor = rows
        .last()
        .map(|(last, _)| JsonCursor(*last).encode())
        .transpose()
        .context("Failed to encode next cursor")?;

    Ok(PageResponse {
        data,
        next_cursor,
        has_next_page,
    })
}
