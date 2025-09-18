// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{CustomValidator, Enum, InputObject, InputValueError};

use crate::{
    api::{
        scalars::{fq_name_filter::FqNameFilter, sui_address::SuiAddress, uint53::UInt53},
        types::lookups::CheckpointBounds,
    },
    intersect,
};

#[derive(InputObject, Debug, Default, Clone)]
pub(crate) struct TransactionFilter {
    /// Filter to transactions that occurred strictly after the given checkpoint.
    pub after_checkpoint: Option<UInt53>,

    /// Filter to transactions in the given checkpoint.
    pub at_checkpoint: Option<UInt53>,

    /// Filter to transaction that occurred strictly before the given checkpoint.
    pub before_checkpoint: Option<UInt53>,

    /// Filter transactions by move function called. Calls can be filtered by the `package`, `package::module`, or the `package::module::name` of their function.
    pub function: Option<FqNameFilter>,

    /// An input filter selecting for either system or programmable transactions.
    pub kind: Option<TransactionKindInput>,

    /// Limit to transactions that interacted with the given address.
    /// The address could be a sender, sponsor, or recipient of the transaction.
    pub affected_address: Option<SuiAddress>,

    /// Limit to transactions that interacted with the given object.
    /// The object could have been created, read, modified, deleted, wrapped, or unwrapped by the transaction.
    /// Objects that were passed as a `Receiving` input are not considered to have been affected by a transaction unless they were actually received.
    pub affected_object: Option<SuiAddress>,

    /// Limit to transactions that were sent by the given address.
    pub sent_address: Option<SuiAddress>,
}

/// An input filter selecting for either system or programmable transactions.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum TransactionKindInput {
    /// A system transaction can be one of several types of transactions.
    /// See [unions/transaction-block-kind] for more details.
    SystemTx = 0,
    /// A user submitted transaction block.
    ProgrammableTx = 1,
}

pub(crate) struct TransactionFilterValidator;

impl CustomValidator<TransactionFilter> for TransactionFilterValidator {
    fn check(&self, filter: &TransactionFilter) -> Result<(), InputValueError<TransactionFilter>> {
        let filters = filter.affected_address.is_some() as u8
            + filter.affected_object.is_some() as u8
            + filter.function.is_some() as u8
            + filter.kind.is_some() as u8;
        if filters > 1 {
            return Err(InputValueError::custom(
                "At most one of [affectedAddress, affectedObject, function, kind] can be specified",
            ));
        }

        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Invalid filter, expected: {0}")]
    InvalidFormat(&'static str),
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
            function: intersect!(function, intersect::by_eq)?,
            kind: intersect!(kind, intersect::by_eq)?,
            affected_address: intersect!(affected_address, intersect::by_eq)?,
            affected_object: intersect!(affected_object, intersect::by_eq)?,
            sent_address: intersect!(sent_address, intersect::by_eq)?,
        })
    }
}

impl CheckpointBounds for TransactionFilter {
    fn after_checkpoint(&self) -> Option<UInt53> {
        self.after_checkpoint
    }

    fn at_checkpoint(&self) -> Option<UInt53> {
        self.at_checkpoint
    }

    fn before_checkpoint(&self) -> Option<UInt53> {
        self.before_checkpoint
    }
}
