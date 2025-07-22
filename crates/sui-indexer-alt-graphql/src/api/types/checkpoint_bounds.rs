// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::types::transaction_filter::TransactionFilter;
use crate::error::{bad_user_input, RpcError};

pub(crate) struct CheckpointBounds {
    cp_lo: u64,
    cp_hi: u64,
}

#[derive(thiserror::Error, Debug, Clone)]
pub(crate) enum Error {
    #[error("No valid lower checkpoint bound found")]
    LowerBound,

    #[error("No valid upper checkpoint bound found")]
    UpperBound,

    #[error("No results before checkpoint 0")]
    Past,

    #[error("Invalid checkpoint bounds: lower bound ({0}) is greater than upper bound ({1})")]
    InvalidBounds(u64, u64),
}

impl CheckpointBounds {
    /// Constructs CheckpointBounds from filters by:
    ///  - Converting filter parameters to checkpoint sequence numbers
    ///  - Computing the lower bound (cp_lo) as the maximum of after_checkpoint + 1 or at_checkpoint
    ///  - Computing the upper bound (cp_hi) as the minimum of`before_checkpoint - 1, at_checkpoint,
    ///    or the current checkpoint_viewed_at
    pub(crate) fn from_transaction_filter(
        filter: &TransactionFilter,
        checkpoint_viewed_at: u64,
    ) -> Result<CheckpointBounds, RpcError<Error>> {
        let cp_after = filter.after_checkpoint.map(u64::from);
        let cp_at = filter.at_checkpoint.map(u64::from);
        let cp_before = filter.before_checkpoint.map(u64::from);

        // Calculate the lower bound checkpoint
        let cp_lo = max_option([cp_after.map(|x| x.saturating_add(1)), cp_at])
            .ok_or_else(|| bad_user_input(Error::LowerBound))?;

        // Handle the before_checkpoint filter
        let cp_before_exclusive = match cp_before {
            // There are no results strictly before checkpoint 0.
            Some(0) => {
                return Err(bad_user_input(Error::Past));
            }
            Some(x) => Some(x - 1),
            None => None,
        };

        // Calculate the upper bound checkpoint
        let cp_hi = min_option([cp_before_exclusive, cp_at, Some(checkpoint_viewed_at)])
            .ok_or_else(|| bad_user_input(Error::UpperBound))?;

        // Validate that the bounds make sense
        if cp_lo > cp_hi {
            return Err(bad_user_input(Error::InvalidBounds(cp_lo, cp_hi)));
        }

        Ok(Self { cp_lo, cp_hi })
    }

    /// Get the lower checkpoint bound
    pub(crate) fn lower(&self) -> u64 {
        self.cp_lo
    }

    /// Get the upper checkpoint bound
    pub(crate) fn upper(&self) -> u64 {
        self.cp_hi
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
