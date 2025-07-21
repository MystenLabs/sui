// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::InputObject;

use crate::api::scalars::uint53::UInt53;
use crate::api::types::intersect;

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

    pub(crate) fn is_empty(&self) -> bool {
        self.before_checkpoint == Some(UInt53::from(0))
            || matches!(
                (self.after_checkpoint, self.before_checkpoint),
                (Some(after), Some(before)) if after >= before
            )
            || matches!(
                (self.after_checkpoint, self.at_checkpoint),
                (Some(after), Some(at)) if after >= at
            )
            || matches!(
                (self.at_checkpoint, self.before_checkpoint),
                (Some(at), Some(before)) if at >= before
            )
        // If SystemTx, sender if specified must be 0x0. Conversely, if sender is 0x0, kind must be SystemTx.
    }
}
