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

    pub(crate) fn checkpoint_bounds(
        &self,
        checkpoint_viewed_at: u64,
    ) -> Option<RangeInclusive<u64>> {
        let cp_after = self.after_checkpoint.map(u64::from);
        let cp_at = self.at_checkpoint.map(u64::from);
        let cp_before = self.before_checkpoint.map(u64::from);

        // Calculate the lower bound checkpoint
        let cp_lo = max_option([cp_after.map(|x| x.saturating_add(1)), cp_at])?;

        // Handle the before_checkpoint filter
        let cp_before_exclusive = match cp_before {
            // There are no results strictly before checkpoint 0.
            Some(0) => return None,
            Some(x) => Some(x - 1),
            None => None,
        };

        // Calculate the upper bound checkpoint
        let cp_hi = min_option([cp_before_exclusive, cp_at, Some(checkpoint_viewed_at)])?;

        Some(RangeInclusive::new(cp_lo, cp_hi))
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
