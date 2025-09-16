// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::{Range, RangeInclusive};

use anyhow::Context as _;
use async_graphql::Context;
use diesel::{sql_types::BigInt, QueryableByName};
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_sql_macro::query;

use crate::{error::RpcError, pagination::Page};

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
pub(crate) async fn tx_bounds<C>(
    ctx: &Context<'_>,
    cp_bounds: &RangeInclusive<u64>,
    global_tx_hi: u64,
    page: &Page<C>,
    f: fn(&C) -> u64,
) -> Result<Range<u64>, RpcError> {
    let pg_reader: &PgReader = ctx.data()?;
    let query = query!(
        r#"
WITH
tx_lo AS (
    SELECT
        tx_lo
    FROM
        cp_sequence_numbers
    WHERE
        cp_sequence_number = {BigInt}
    LIMIT 1
),

-- tx_hi is the tx_lo of the checkpoint directly after the cp_bounds.end()
tx_hi AS (
    SELECT
        tx_lo AS tx_hi
    FROM
        cp_sequence_numbers
    WHERE
        cp_sequence_number = {BigInt} + 1
    LIMIT 1
)

SELECT
    (SELECT tx_lo FROM tx_lo) AS "tx_lo",
    -- If we cannot get the tx_hi from the checkpoint directly after the cp_bounds.end() we use global tx_hi.
    COALESCE((SELECT tx_hi FROM tx_hi), {BigInt}) AS "tx_hi";
"#,
        *cp_bounds.start() as i64,
        *cp_bounds.end() as i64,
        global_tx_hi as i64
    );

    let mut conn = pg_reader
        .connect()
        .await
        .context("Failed to connect to database")?;

    #[derive(QueryableByName)]
    struct TxBounds {
        #[diesel(sql_type = BigInt, column_name = "tx_lo")]
        tx_lo: i64,
        #[diesel(sql_type = BigInt, column_name = "tx_hi")]
        tx_hi: i64,
    }

    let results: Vec<TxBounds> = conn
        .results(query)
        .await
        .context("Failed to execute query")?;

    let (tx_lo, tx_hi) = results
        .first()
        .context("No valid checkpoints found")
        .map(|bounds| (bounds.tx_lo as u64, bounds.tx_hi as u64))?;

    // Inclusive cursor bounds
    let tx_lo = page.after().map_or(tx_lo, |cursor| f(cursor).max(tx_lo));

    let tx_hi = page
        .before()
        .map_or(tx_hi, |cursor| f(cursor).saturating_add(1).min(tx_hi));

    Ok(tx_lo..tx_hi)
}
