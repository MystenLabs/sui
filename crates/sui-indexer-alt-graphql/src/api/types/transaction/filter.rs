// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::InputObject;
use std::ops::RangeInclusive;

use crate::api::scalars::uint53::UInt53;
use crate::intersect;

#[derive(InputObject, Debug, Default, Clone)]
pub(crate) struct TransactionFilter {
    /// Limit to transactions that occured strictly after the given checkpoint.
    pub after_checkpoint: Option<UInt53>,

    /// Limit to transactions in the given checkpoint.
    pub at_checkpoint: Option<UInt53>,

    /// Limit to transaction that occured strictly before the given checkpoint.
    pub before_checkpoint: Option<UInt53>,
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
        })
    }

    /// The bounds on checkpoint sequence number, imposed by filters, and transaction checkpoint filters. The
    /// outermost bounds are determined by the lowest checkpoint that is safe to read from (reader_lo) and
    /// the highest checkpoint that has been processed based on the context's watermark(checkpoint_viewed_at).
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
    ///
    /// The bounds can be used to derive transaction sequence numbers to query for.
    pub(crate) fn checkpoint_bounds(
        &self,
        reader_lo: u64,
        checkpoint_viewed_at: u64,
    ) -> Option<RangeInclusive<u64>> {
        let cp_after = self.after_checkpoint.map(u64::from);
        let cp_at = self.at_checkpoint.map(u64::from);
        let cp_before = self.before_checkpoint.map(u64::from);

        let cp_after_inclusive = match cp_after.map(|x| x.checked_add(1)) {
            Some(Some(after)) => Some(after),
            Some(None) => return None,
            None => None,
        };
        // Inclusive checkpoint sequence number lower bound. If are no upper lower bound filters,
        // we will use the smallest checkpoint available from the database, retrieved from
        // the watermark.
        //
        // SAFETY: we can unwrap because of`Some(reader_lo)`
        let cp_lo = max_option([cp_after_inclusive, cp_at, Some(reader_lo)]).unwrap();

        let cp_before_inclusive = match cp_before.map(|x| x.checked_sub(1)) {
            Some(Some(before)) => Some(before),
            Some(None) => return None,
            None => None,
        };

        // Inclusive checkpoint sequence upperbound. If are no upper bound filters,
        // we will use `checkpoint_viewed_at`.
        //
        // SAFETY: we can unwrap because of the `Some(checkpoint_viewed_at)`
        let cp_hi = min_option([cp_before_inclusive, cp_at, Some(checkpoint_viewed_at)]).unwrap();

        cp_hi
            .checked_sub(cp_lo)
            .map(|_| RangeInclusive::new(cp_lo, cp_hi))
    }
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
    use super::*;

    #[test]
    fn test_checkpoint_bounds_no_filters() {
        assert_eq!(
            TransactionFilter::default()
                .checkpoint_bounds(5, 200)
                .unwrap(),
            5..=200
        );
    }

    #[test]
    fn test_checkpoint_bounds_after_checkpoint() {
        assert_eq!(
            TransactionFilter {
                after_checkpoint: Some(UInt53::from(10)),
                ..Default::default()
            }
            .checkpoint_bounds(5, 200)
            .unwrap(),
            11..=200
        );
    }

    #[test]
    fn test_checkpoint_bounds_before_checkpoint() {
        assert_eq!(
            TransactionFilter {
                before_checkpoint: Some(UInt53::from(100)),
                ..Default::default()
            }
            .checkpoint_bounds(5, 200)
            .unwrap(),
            5..=99
        );
    }

    #[test]
    fn test_checkpoint_bounds_at_checkpoint() {
        assert_eq!(
            TransactionFilter {
                at_checkpoint: Some(UInt53::from(50)),
                ..Default::default()
            }
            .checkpoint_bounds(5, 200)
            .unwrap(),
            50..=50
        );
    }

    #[test]
    fn test_checkpoint_bounds_combined_filters() {
        assert_eq!(
            TransactionFilter {
                after_checkpoint: Some(UInt53::from(10)),
                before_checkpoint: Some(UInt53::from(100)),
                ..Default::default()
            }
            .checkpoint_bounds(5, 200)
            .unwrap(),
            11..=99
        );
    }

    #[test]
    fn test_checkpoint_bounds_at_checkpoint_precedence() {
        assert_eq!(
            TransactionFilter {
                after_checkpoint: Some(UInt53::from(10)),
                at_checkpoint: Some(UInt53::from(50)),
                before_checkpoint: Some(UInt53::from(100)),
            }
            .checkpoint_bounds(5, 200)
            .unwrap(),
            50..=50
        );
    }

    #[test]
    fn test_checkpoint_bounds_boundary_equal() {
        assert_eq!(
            TransactionFilter::default()
                .checkpoint_bounds(100, 100)
                .unwrap(),
            100..=100
        );
    }

    #[test]
    fn test_checkpoint_bounds_reader_lo_override() {
        assert_eq!(
            TransactionFilter {
                after_checkpoint: Some(UInt53::from(10)),
                ..Default::default()
            }
            .checkpoint_bounds(50, 200)
            .unwrap(),
            50..=200
        );
    }

    #[test]
    fn test_checkpoint_bounds_viewed_at_override() {
        assert_eq!(
            TransactionFilter {
                before_checkpoint: Some(UInt53::from(100)),
                ..Default::default()
            }
            .checkpoint_bounds(5, 80)
            .unwrap(),
            5..=80
        );
    }

    #[test]
    fn test_checkpoint_bounds_overflow() {
        assert!(TransactionFilter {
            after_checkpoint: Some(UInt53::from(u64::MAX)),
            ..Default::default()
        }
        .checkpoint_bounds(5, 200)
        .is_none());
    }

    #[test]
    fn test_checkpoint_bounds_underflow() {
        assert!(TransactionFilter {
            before_checkpoint: Some(UInt53::from(0)),
            ..Default::default()
        }
        .checkpoint_bounds(5, 200)
        .is_none());
    }

    #[test]
    fn test_checkpoint_bounds_invalid_range() {
        assert!(TransactionFilter {
            after_checkpoint: Some(UInt53::from(100)),
            before_checkpoint: Some(UInt53::from(50)),
            ..Default::default()
        }
        .checkpoint_bounds(5, 200)
        .is_none());
    }

    #[test]
    fn test_checkpoint_bounds_reader_lo_greater() {
        assert!(TransactionFilter::default()
            .checkpoint_bounds(100, 50)
            .is_none());
    }
}
