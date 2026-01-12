// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::sync::Arc;

use async_graphql::Context;
use diesel::sql_types::BigInt;
use sui_pg_db::query::Query;
use sui_sql_macro::query;

use crate::api::scalars::cursor::JsonCursor;
use crate::api::scalars::uint53::UInt53;
use crate::api::types::checkpoint::filter::checkpoint_bounds;
use crate::error::RpcError;
use crate::pagination::Page;
use crate::scope::Scope;
use crate::task::watermark::Watermarks;

pub(crate) trait CheckpointBounds {
    fn after_checkpoint(&self) -> Option<UInt53>;
    fn at_checkpoint(&self) -> Option<UInt53>;
    fn before_checkpoint(&self) -> Option<UInt53>;

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
    async fn tx_bounds<'a>(
        &self,
        ctx: &Context<'_>,
        scope: &Scope,
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
            self.after_checkpoint().map(u64::from),
            self.at_checkpoint().map(u64::from),
            self.before_checkpoint().map(u64::from),
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
                .map(|c| c.tx_sequence_number() as i64)
                // convert cursor inclusive bounds to exclusive bounds
                .and_then(|c| c.checked_add(1)), /* page_tx_hi */
            global_tx_hi as i64,
            *cp_bounds.end() as i64, /* cp_hi */
        );

        Ok(Some(query))
    }
}

pub(crate) trait TxBoundsCursor {
    fn tx_sequence_number(&self) -> u64;
}

/// Trait for cursors used in checkpoint-based scanning operations.
/// Provides access to checkpoint and transaction sequence numbers for bounds computation.
pub(crate) trait ScanCursor {
    /// The checkpoint sequence number for this cursor position.
    fn cp_sequence_number(&self) -> u64;

    /// The transaction index within the checkpoint for this cursor position.
    fn tx_sequence_number(&self) -> u64;
}

/// Extension trait for scan cursors that also track event position within a transaction.
pub(crate) trait ScanCursorWithEvent: ScanCursor {
    /// The event index within the transaction for this cursor position.
    fn ev_sequence_number(&self) -> u64;
}

/// Computes the transaction index bounds within a checkpoint based on cursor positions.
///
/// Returns a range of transaction indices `[tx_lo, tx_hi)` that should be iterated
/// for this checkpoint, respecting the page's after/before cursors.
pub(crate) fn cp_tx_bounds<C: ScanCursor>(
    page: &Page<JsonCursor<C>>,
    cp_sequence_number: u64,
    tx_count: usize,
) -> Range<usize> {
    let tx_lo = page
        .after()
        .filter(|c| c.cp_sequence_number() == cp_sequence_number)
        .map(|c| c.tx_sequence_number() as usize)
        .unwrap_or(0)
        .min(tx_count);

    let tx_hi = page
        .before()
        .filter(|c| c.cp_sequence_number() == cp_sequence_number)
        .map(|c| (c.tx_sequence_number() as usize).saturating_add(1))
        .unwrap_or(tx_count)
        .max(tx_lo)
        .min(tx_count);

    tx_lo..tx_hi
}

/// Computes the event index bounds within a transaction for scan operations.
///
/// Returns a range of event indices `[ev_lo, ev_hi)` that should be iterated
/// for this transaction within the checkpoint, respecting the page's after/before cursors.
pub(crate) fn cp_ev_bounds<C: ScanCursorWithEvent>(
    page: &Page<JsonCursor<C>>,
    cp_sequence_number: u64,
    tx_idx: usize,
    ev_count: usize,
) -> Range<usize> {
    let ev_lo = page
        .after()
        .filter(|c| {
            c.cp_sequence_number() == cp_sequence_number && c.tx_sequence_number() == tx_idx as u64
        })
        .map(|c| c.ev_sequence_number() as usize)
        .unwrap_or(0)
        .min(ev_count);

    let ev_hi = page
        .before()
        .filter(|c| {
            c.cp_sequence_number() == cp_sequence_number && c.tx_sequence_number() == tx_idx as u64
        })
        .map(|c| (c.ev_sequence_number() as usize).saturating_add(1))
        .unwrap_or(ev_count)
        .max(ev_lo)
        .min(ev_count);

    ev_lo..ev_hi
}
