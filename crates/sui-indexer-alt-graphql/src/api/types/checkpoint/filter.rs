// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::convert::Infallible;
use std::ops::RangeInclusive;

use anyhow::Context as _;
use async_graphql::Context;
use async_graphql::Error as RpcError;
use async_graphql::InputObject;
use diesel::prelude::QueryableByName;
use diesel::sql_types::BigInt;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_sql_macro::query;

use crate::api::scalars::uint53::UInt53;
use crate::api::types::checkpoint::CCheckpoint;
use crate::extensions::query_limits;
use crate::intersect;
use crate::pagination::Page;

// Filter for checkpoint-based queries across checkpoints, packages, and epochs.
#[derive(InputObject, Debug, Default, Clone)]
pub(crate) struct CheckpointFilter {
    /// Limit query results to checkpoints at this epoch.
    pub at_epoch: Option<UInt53>,

    /// Limit query results to checkpoints that occured strictly after the given checkpoint.
    pub after_checkpoint: Option<UInt53>,

    /// Limit query results to checkpoints that occured at the given checkpoint.
    pub at_checkpoint: Option<UInt53>,

    /// Limit query results to checkpoints that occured strictly before the given checkpoint.
    pub before_checkpoint: Option<UInt53>,
}

#[derive(QueryableByName, Debug)]
pub(crate) struct CpBounds {
    #[diesel(sql_type = BigInt, column_name = "cp_lo")]
    cp_lo: i64,
    #[diesel(sql_type = BigInt, column_name = "cp_hi_inclusive")]
    cp_hi_inclusive: i64,
}

impl CheckpointFilter {
    pub(crate) fn intersect(self, other: Self) -> Option<Self> {
        macro_rules! intersect {
            ($field:ident, $body:expr) => {
                intersect::field(self.$field, other.$field, $body)
            };
        }

        Some(Self {
            at_epoch: intersect!(at_epoch, intersect::by_eq)?,
            after_checkpoint: intersect!(after_checkpoint, intersect::by_max)?,
            at_checkpoint: intersect!(at_checkpoint, intersect::by_eq)?,
            before_checkpoint: intersect!(before_checkpoint, intersect::by_min)?,
        })
    }

    /// The active filters in CheckpointFilter. Used to find the pipelines that are available to serve queries with these filters applied.
    pub(crate) fn active_filters(&self) -> Vec<String> {
        let mut filters = vec![];
        if self.at_epoch.is_some() {
            filters.push("atEpoch".to_string());
        }
        if self.at_checkpoint.is_some() {
            filters.push("atCheckpoint".to_string());
        }
        if self.after_checkpoint.is_some() {
            filters.push("afterCheckpoint".to_string());
        }
        if self.before_checkpoint.is_some() {
            filters.push("beforeCheckpoint".to_string());
        }
        filters
    }
}

/// The bounds on checkpoint sequence number, imposed by filters. The outermost bounds are
/// determined by the lowest checkpoint that is safe to read from (reader_lo) and the highest
/// checkpoint that has been processed based on the context's watermark(checkpoint_viewed_at).
///
/// ```ignore
///     reader_lo                                                     checkpoint_viewed_at
///     [-----------------------------------------------------------------]
/// ```
///
/// The bounds are further tightened by the filters if they are present.
///
/// ```ignore
///    filter.after_checkpoint                         filter.before_checkpoint
///         [------------[--------------------------]------------]
///                         filter.at_checkpoint
/// ```
pub(crate) fn checkpoint_bounds(
    cp_after: Option<u64>,
    cp_at: Option<u64>,
    cp_before: Option<u64>,
    reader_lo: u64,
    checkpoint_viewed_at: u64,
) -> Option<RangeInclusive<u64>> {
    let cp_after_inclusive = match cp_after.map(|x| x.checked_add(1)) {
        Some(Some(after)) => Some(after),
        Some(None) => return None,
        None => None,
    };
    // Inclusive checkpoint sequence number lower bound. If there are no lower bound filters,
    // we will use the smallest checkpoint available from the database, retrieved from
    // the watermark.
    //
    // SAFETY: we can unwrap because of `Some(reader_lo)`
    let cp_lo = max_option([cp_after_inclusive, cp_at, Some(reader_lo)]).unwrap();

    let cp_before_inclusive = match cp_before.map(|x| x.checked_sub(1)) {
        Some(Some(before)) => Some(before),
        Some(None) => return None,
        None => None,
    };

    // Inclusive checkpoint sequence upper bound. If there are no upper bound filters,
    // we will use `checkpoint_viewed_at`.
    //
    // SAFETY: we can unwrap because of the `Some(checkpoint_viewed_at)`
    let cp_hi = min_option([cp_before_inclusive, cp_at, Some(checkpoint_viewed_at)]).unwrap();

    cp_hi
        .checked_sub(cp_lo)
        .map(|_| RangeInclusive::new(cp_lo, cp_hi))
}

/// The cp_sequence_numbers within checkpoint bounds with cursors applied inclusively.
///
/// pg_lo: The maximum of the cursor and the start of the checkpoint bound.
/// pg_hi_inclusive: The minimum of the cursor and the end of the checkpoint bound.
///
/// A cursor outside the checkpoint bound on the side the page is drawn from (`after` below it going
/// forward, `before` above it going backward) is kept as the page's edge element, so
/// `paginate_results` trims it as redundant instead of treating the page as out of bounds.
pub(super) fn cp_unfiltered(cp_bounds: &RangeInclusive<u64>, page: &Page<CCheckpoint>) -> Vec<u64> {
    let cp_lo = *cp_bounds.start();
    let cp_hi = *cp_bounds.end();

    // Inclusive cursor bounds
    let pg_lo = page
        .after()
        .map_or(cp_lo, |cursor| cursor.sequence_number().max(cp_lo));
    let pg_hi_inclusive = page
        .before()
        .map_or(cp_hi, |cursor| cursor.sequence_number().min(cp_hi));

    if page.is_from_front() {
        let redundant_after = page
            .after()
            .map(|cursor| cursor.sequence_number())
            .filter(|after| *after < cp_lo);
        redundant_after
            .into_iter()
            .chain(pg_lo..=pg_hi_inclusive)
            .take(page.limit_with_overhead())
            .collect()
    } else {
        let redundant_before = page
            .before()
            .map(|cursor| cursor.sequence_number())
            .filter(|before| *before > cp_hi);
        // Graphql last syntax expects results to be in ascending order. If we are paginating backwards,
        // we reverse the results after applying limits.
        let mut results: Vec<_> = redundant_before
            .into_iter()
            .chain((pg_lo..=pg_hi_inclusive).rev())
            .take(page.limit_with_overhead())
            .collect();
        results.reverse();
        results
    }
}

/// The checkpoint sequence numbers in a range bounded by checkpoints in an epoch.
/// The range is further tightened by bounds derived from filters if they are present.
pub(super) async fn cp_by_epoch(
    ctx: &Context<'_>,
    page: &Page<CCheckpoint>,
    cp_bounds: &RangeInclusive<u64>,
    epoch: u64,
) -> Result<Vec<u64>, RpcError> {
    query_limits::rich::debit::<Infallible>(ctx)?;
    let pg_reader: &PgReader = ctx.data()?;

    let query = query!(
        r#"
        SELECT
            GREATEST(es.cp_lo, {BigInt}) AS "cp_lo",
            LEAST(COALESCE(ee.cp_hi - 1, {BigInt}), {BigInt}) AS "cp_hi_inclusive"
        FROM
            kv_epoch_starts es
        LEFT JOIN kv_epoch_ends ee ON es.epoch = ee.epoch
        WHERE
            es.epoch = {BigInt}
        "#,
        *cp_bounds.start() as i64,
        *cp_bounds.end() as i64,
        *cp_bounds.end() as i64,
        epoch as i64,
    );

    let mut conn = pg_reader
        .connect()
        .await
        .context("Failed to connect to database")?;

    let results: Vec<CpBounds> = conn
        .results(query)
        .await
        .context("Failed to execute epoch checkpoint query")?;

    let (cp_lo, cp_hi_inclusive) = match results.first() {
        Some(bounds) => (bounds.cp_lo as u64, bounds.cp_hi_inclusive as u64),
        None => return Ok(vec![]),
    };

    Ok(cp_unfiltered(&(cp_lo..=cp_hi_inclusive), page))
}

/// Determines the maximum value in an arbitrary number of Option<impl Ord>.
fn max_option<T: Ord>(xs: impl IntoIterator<Item = Option<T>>) -> Option<T> {
    xs.into_iter().flatten().max()
}

/// Determines the minimum value in an arbitrary number of Option<impl Ord>.
fn min_option<T: Ord>(xs: impl IntoIterator<Item = Option<T>>) -> Option<T> {
    xs.into_iter().flatten().min()
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use crate::api::types::checkpoint::CheckpointToken;
    use crate::pagination::PageLimits;

    use super::*;

    fn page(
        first: Option<u64>,
        after: Option<u64>,
        last: Option<u64>,
        before: Option<u64>,
    ) -> Page<CCheckpoint> {
        let limits = PageLimits {
            default: 3,
            max: 10,
        };
        Page::from_params(
            &limits,
            first,
            after.map(CheckpointToken::cursor),
            last,
            before.map(CheckpointToken::cursor),
        )
        .unwrap()
    }

    /// Paginate `cp_unfiltered`'s output the way `Checkpoint::paginate` does, returning the
    /// checkpoints on the page.
    fn paginate(cp_bounds: RangeInclusive<u64>, page: &Page<CCheckpoint>) -> Vec<u64> {
        let conn = page
            .paginate_results(
                cp_unfiltered(&cp_bounds, page),
                |c| CheckpointToken::cursor(*c),
                Ok::<_, Infallible>,
            )
            .unwrap();
        conn.edges.into_iter().map(|e| e.node).collect()
    }

    #[test]
    fn test_cp_unfiltered_after_cursor_at_after_checkpoint() {
        // `after: cursor(10)` with `afterCheckpoint: 10`: the cursor is redundant with the bound.
        let page = page(Some(3), Some(10), None, None);
        assert_eq!(paginate(11..=100, &page), vec![11, 12, 13]);
    }

    #[test]
    fn test_cp_unfiltered_after_cursor_below_after_checkpoint() {
        let page = page(Some(3), Some(4), None, None);
        assert_eq!(paginate(11..=100, &page), vec![11, 12, 13]);
    }

    /// Backward pages never required the `after` cursor to be the first element.
    #[test]
    fn test_cp_unfiltered_after_cursor_below_bound_from_back() {
        let page = page(None, Some(10), Some(3), None);
        assert_eq!(paginate(11..=12, &page), vec![11, 12]);
        assert_eq!(paginate(11..=100, &page), vec![98, 99, 100]);
    }

    #[test]
    fn test_cp_unfiltered_after_cursor_within_bound_unchanged() {
        let page = page(Some(3), Some(20), None, None);
        assert_eq!(paginate(11..=100, &page), vec![21, 22, 23]);
    }

    #[test]
    fn test_cp_unfiltered_before_cursor_at_before_checkpoint() {
        // `before: cursor(20)` with `beforeCheckpoint: 20`: the cursor is redundant with the bound.
        let page = page(None, None, Some(3), Some(20));
        assert_eq!(paginate(0..=19, &page), vec![17, 18, 19]);
    }

    #[test]
    fn test_cp_unfiltered_before_cursor_above_bound_from_front() {
        let page = page(Some(3), None, None, Some(20));
        assert_eq!(paginate(10..=11, &page), vec![10, 11]);
        assert_eq!(paginate(0..=19, &page), vec![0, 1, 2]);
    }

    #[test]
    fn test_checkpoint_bounds_no_filters() {
        assert_eq!(
            checkpoint_bounds(None, None, None, 5, 200).unwrap(),
            5..=200
        );
    }

    #[test]
    fn test_checkpoint_bounds_after_checkpoint() {
        assert_eq!(
            checkpoint_bounds(Some(10), None, None, 5, 200).unwrap(),
            11..=200
        );
    }

    #[test]
    fn test_checkpoint_bounds_before_checkpoint() {
        assert_eq!(
            checkpoint_bounds(None, None, Some(100), 5, 200).unwrap(),
            5..=99
        );
    }

    #[test]
    fn test_checkpoint_bounds_at_checkpoint() {
        assert_eq!(
            checkpoint_bounds(None, Some(50), None, 5, 200).unwrap(),
            50..=50
        );
    }

    #[test]
    fn test_checkpoint_bounds_combined_filters() {
        assert_eq!(
            checkpoint_bounds(Some(10), None, Some(100), 5, 200).unwrap(),
            11..=99
        );
    }

    #[test]
    fn test_checkpoint_bounds_at_checkpoint_precedence() {
        assert_eq!(
            checkpoint_bounds(Some(10), Some(50), Some(100), 5, 200).unwrap(),
            50..=50
        );
    }

    #[test]
    fn test_checkpoint_bounds_boundary_equal() {
        assert_eq!(
            checkpoint_bounds(None, None, None, 100, 100).unwrap(),
            100..=100
        );
    }

    #[test]
    fn test_checkpoint_bounds_reader_lo_override() {
        assert_eq!(
            checkpoint_bounds(Some(10), None, None, 50, 200).unwrap(),
            50..=200
        );
    }

    #[test]
    fn test_checkpoint_bounds_viewed_at_override() {
        assert_eq!(
            checkpoint_bounds(None, None, Some(100), 5, 80).unwrap(),
            5..=80
        );
    }

    #[test]
    fn test_checkpoint_bounds_overflow() {
        assert!(checkpoint_bounds(Some(u64::MAX), None, None, 5, 200).is_none());
    }

    #[test]
    fn test_checkpoint_bounds_underflow() {
        assert!(checkpoint_bounds(None, None, Some(0), 5, 200).is_none());
    }

    #[test]
    fn test_checkpoint_bounds_invalid_range() {
        assert!(checkpoint_bounds(Some(100), None, Some(50), 5, 200).is_none());
    }

    #[test]
    fn test_checkpoint_bounds_reader_lo_greater() {
        assert!(checkpoint_bounds(None, None, None, 100, 50).is_none());
    }
}
