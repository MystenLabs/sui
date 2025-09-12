// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::Context;
use diesel::sql_types::BigInt;
use sui_pg_db::query::Query;
use sui_sql_macro::query;

use crate::api::types::checkpoint::filter::checkpoint_bounds;
use crate::scope::Scope;
use crate::task::watermark::Watermarks;
use crate::{api::scalars::uint53::UInt53, error::RpcError, pagination::Page};

pub(crate) trait TxBoundsFilter {
    fn after_checkpoint(&self) -> Option<UInt53>;
    fn at_checkpoint(&self) -> Option<UInt53>;
    fn before_checkpoint(&self) -> Option<UInt53>;
}

pub(crate) trait TxBoundsCursor {
    fn tx_sequence_number(&self) -> u64;
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
pub(crate) async fn tx_bounds_query<'a>(
    ctx: &Context<'_>,
    scope: &Scope,
    filter: &impl TxBoundsFilter,
    reader_lo: u64,
    page: &Page<impl TxBoundsCursor>,
) -> Result<Option<Query<'a>>, RpcError> {
    if page.limit() == 0 {
        return Ok(None);
    }

    let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
        return Ok(None);
    };

    let Some(cp_bounds) = checkpoint_bounds(
        filter.after_checkpoint().map(u64::from),
        filter.at_checkpoint().map(u64::from),
        filter.before_checkpoint().map(u64::from),
        reader_lo,
        checkpoint_viewed_at,
    ) else {
        return Ok(None);
    };

    let watermarks: &Arc<Watermarks> = ctx.data()?;
    let global_tx_hi = watermarks.high_watermark().transaction();

    let query = query!(
        r#"
WITH
-- tx_lo is the tx_lo of the checkpoint at cp_lo
tx_lo AS MATERIALIZED (
    SELECT
        -- MAX returns NULL if there are no rows
        -- GREATEST ignores nulls
        GREATEST(MAX(tx_lo), {Nullable<BigInt>} /* page_tx_lo */) AS tx_lo
    FROM
        cp_sequence_numbers
    WHERE
        cp_sequence_number = {BigInt} /* cp_lo */
),

-- tx_hi is the tx_lo of the checkpoint after cp_hi
tx_hi AS MATERIALIZED (
    SELECT
        -- MAX returns NULL if there are no rows
        -- LEAST ignores nulls
        LEAST(MAX(tx_lo), {Nullable<BigInt>} /* page_tx_hi */, {BigInt} /* global_tx_hi */) AS tx_hi
    FROM
        cp_sequence_numbers
    WHERE
        cp_sequence_number = {BigInt} /* cp_hi */ + 1
)
"#,
        page.after().map(|c| c.tx_sequence_number() as i64), /* page_tx_lo */
        *cp_bounds.start() as i64,                           /* cp_lo */
        page.before()
            // convert cursor inclusive bounds to exclusive bounds
            .map(|c| (c.tx_sequence_number() as i64).saturating_add(1)), /* page_tx_hi */
        global_tx_hi as i64,
        *cp_bounds.end() as i64, /* cp_hi */
    );

    Ok(Some(query))
}
