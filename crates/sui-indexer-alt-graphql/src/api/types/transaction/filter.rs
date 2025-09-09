// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{CustomValidator, InputObject, InputValueError};

use crate::{api::scalars::sui_address::SuiAddress, api::scalars::uint53::UInt53, intersect};

#[derive(InputObject, Debug, Default, Clone)]
pub(crate) struct TransactionFilter {
    /// Filter to transactions that occurred strictly after the given checkpoint.
    pub after_checkpoint: Option<UInt53>,

    /// Filter to transactions in the given checkpoint.
    pub at_checkpoint: Option<UInt53>,

    /// Filter to transaction that occurred strictly before the given checkpoint.
    pub before_checkpoint: Option<UInt53>,

    /// Limit to transactions that interacted with the given address.
    /// The address could be a sender, sponsor, or recipient of the transaction.
    pub affected_address: Option<SuiAddress>,

    /// Limit to transactions that were sent by the given address.
    pub sent_address: Option<SuiAddress>,
}

pub(crate) struct TransactionFilterValidator;

impl CustomValidator<TransactionFilter> for TransactionFilterValidator {
    fn check(&self, filter: &TransactionFilter) -> Result<(), InputValueError<TransactionFilter>> {
        let filters = filter.affected_address.is_some() as u8;
        if filters > 1 {
            return Err(InputValueError::custom(
                "Only one of affectedAddress can be specified",
            ));
        }

        Ok(())
    }
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
            sent_address: intersect!(sent_address, intersect::by_eq)?,
        })
    }
}
