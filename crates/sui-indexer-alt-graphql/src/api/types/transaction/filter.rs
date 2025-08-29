// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use async_graphql::{Context, InputObject};
use diesel::{
    sql_types::{BigInt, Binary},
    QueryableByName,
};

use crate::{
    api::{
        scalars::{sui_address::SuiAddress, uint53::UInt53},
        types::{checkpoint::filter::checkpoint_bounds, transaction::CTransaction},
    },
    error::RpcError,
    intersect,
    pagination::Page,
    scope::Scope,
    task::watermark::Watermarks,
};

use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_sql_macro::query;

#[derive(InputObject, Debug, Default, Clone)]
pub(crate) struct TransactionFilter {
    /// Limit to transactions that occured strictly after the given checkpoint.
    pub after_checkpoint: Option<UInt53>,

    /// Limit to transactions in the given checkpoint.
    pub at_checkpoint: Option<UInt53>,

    /// Limit to transaction that occured strictly before the given checkpoint.
    pub before_checkpoint: Option<UInt53>,

    /// Limit to transactions that interacted with the given address. The address could be a
    /// sender, sponsor, or recipient of the transaction.
    pub affected_address: Option<SuiAddress>,
}

#[derive(QueryableByName)]
pub(crate) struct TxSequenceNumberDigest {
    #[diesel(sql_type = BigInt)]
    pub tx_sequence_number: i64,
    #[diesel(sql_type = Binary)]
    pub tx_digest: Vec<u8>,
}

impl TransactionFilter {
    /// Try to create a filter whose results are the intersection of transaction blocks in `self`'s
    /// results and transaction blocks in `other`'s results. This may not be possible if the
    /// resulting filter is inconsistent in some way (e.g. a filter that requires one field to be
    /// two different values simultaneously).
    pub(crate) fn intersect(self, other: Self) -> Option<Self> {
        macro_rules! intersect {
            ($field:ident, $body:expr) => {
                intersect::field(self.$field, other.$field, $body)
            };
        }

        Some(Self {
            after_checkpoint: intersect!(after_checkpoint, intersect::by_max)?,
            at_checkpoint: intersect!(at_checkpoint, intersect::by_eq)?,
            before_checkpoint: intersect!(before_checkpoint, intersect::by_min)?,
            affected_address: intersect!(affected_address, intersect::by_eq)?,
        })
    }
}

/// The tx_sequence_numbers within checkpoint bounds
/// The checkpoint lower and upper bounds are used to determine the inclusive lower (tx_lo) and exclusive
/// upper (tx_hi) bounds of the sequence of tx_sequence_numbers to use in queries.
///
/// tx_lo: The cp_sequence_number of the checkpoint at the start of the bounds.
/// tx_hi: The tx_lo of the checkpoint directly after the cp_bounds.end(). If it does not exist
///      at cp_bounds.end(), fallback to the maximum tx_sequence_number in the context's watermark
///      (global_tx_hi).
///
/// NOTE: for consistency, assume that lowerbounds are inclusive and upperbounds are exclusive.
/// Bounds that do not follow this convention will be annotated explicitly (e.g. `lo_exclusive` or
/// `hi_inclusive`).
pub(crate) async fn fetch_tx_sequence_number_digests(
    ctx: &Context<'_>,
    scope: &Scope,
    watermarks: &Watermarks,
    page: &Page<CTransaction>,
    filter: TransactionFilter,
) -> Result<Vec<TxSequenceNumberDigest>, RpcError> {
    let reader_lo = watermarks.pipeline_lo_watermark("tx_digests")?.checkpoint();
    let global_tx_hi = watermarks.high_watermark().transaction();

    let Some(cp_bounds) = checkpoint_bounds(
        filter.after_checkpoint.map(u64::from),
        filter.at_checkpoint.map(u64::from),
        filter.before_checkpoint.map(u64::from),
        reader_lo,
        scope.checkpoint_viewed_at(),
    ) else {
        return Ok(vec![]);
    };

    let pg_reader: &PgReader = ctx.data()?;
    let is_asc = page.is_from_front();
    let mut query = query!(
        r#"
WITH
-- MAX(tx_lo of the checkpoint directly after the cp_bounds.end(), page_tx_lo)
tx_lo AS (
    SELECT
        GREATEST(tx_lo, {BigInt} /* page_tx_lo */) AS tx_sequence_number
    FROM
        cp_sequence_numbers
    WHERE
        cp_sequence_number = {BigInt} /* cp_lo */
    LIMIT 1
),

-- MIN(tx_hi is the tx_lo of the checkpoint directly after the cp_bounds.end(), page_tx_hi)
tx_hi AS (
    SELECT
        -- If we cannot get the tx_hi from the checkpoint directly after the cp_bounds.end() we use global tx_hi.
        LEAST(COALESCE(MAX(tx_lo), {BigInt} /* global_tx_hi */), {BigInt} /* page_tx_hi */) AS tx_sequence_number
    FROM
        cp_sequence_numbers
    WHERE
        cp_sequence_number = {BigInt} /* cp_hi */
    LIMIT 1
)

SELECT
    dig.tx_sequence_number, dig.tx_digest
FROM
    tx_digests dig
INNER JOIN
    tx_lo lo ON lo.tx_sequence_number <= dig.tx_sequence_number
INNER JOIN
    tx_hi hi ON dig.tx_sequence_number < hi.tx_sequence_number
"#,
        page.after().map_or(i64::MIN, |cursor| **cursor as i64), // page_tx_lo
        *cp_bounds.start() as i64,                               // cp_lo
        global_tx_hi as i64,                                     // global_tx_hi,
        page.before().map_or(i64::MAX, |cursor| **cursor as i64 + 1), // page_tx_hi
        *cp_bounds.end() as i64 + 1,                             // cp_hi
    );

    if let Some(SuiAddress(affected_address)) = filter.affected_address {
        query += query!(
            r#"
INNER JOIN
    tx_affected_addresses aff ON aff.tx_sequence_number = dig.tx_sequence_number AND aff.affected = {Bytea} /* affected_address */
"#,
            affected_address.to_vec(), /* affected_address */
        )
    };

    // todo conditionally add join filters here

    query += query!(
        r#"
ORDER BY
    CASE WHEN {Bool} /* is_asc */ = true THEN dig.tx_sequence_number ELSE NULL END ASC,
    CASE WHEN {Bool} /* is_asc */ = false THEN dig.tx_sequence_number ELSE NULL END DESC
LIMIT
    {BigInt} /* page_limit */
"#,
        is_asc,
        is_asc,
        page.limit_with_overhead() as i64, // page_limit
    );

    let mut conn = pg_reader
        .connect()
        .await
        .context("Failed to connect to database")?;

    let mut tx_sequence_number_digests: Vec<TxSequenceNumberDigest> = conn
        .results(query)
        .await
        .context("Failed to execute query")?;

    if !is_asc {
        // Graphql "last" queries are in DESC order to apply LIMIT, but results need to be in ASC order.
        tx_sequence_number_digests.reverse()
    }

    Ok(tx_sequence_number_digests)
}
