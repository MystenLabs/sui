// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::RangeInclusive;

use async_graphql::{CustomValidator, Enum, InputObject, InputValueError};
use sui_pg_db::query::Query;
use sui_sql_macro::query;

use crate::{
    api::scalars::{fq_name_filter::FqNameFilter, sui_address::SuiAddress, uint53::UInt53},
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

/// The tx_sequence_numbers within checkpoint bounds, further filtered by the after and before cursors.
/// The checkpoint lower and upper bounds are used to determine the inclusive lower (tx_lo) and exclusive
/// upper (tx_hi) bounds of the sequence of tx_sequence_numbers.
///
/// tx_lo: The greatest of the after cursor tx_sequence_number and the tx_lo of the checkpoint at the start of the bounds.
/// tx_hi: The least of the before cursor tx_sequence_number and the tx_hi of the checkpoint directly after the cp_bounds.end(),
///        or the tx_hi of the context's watermark (global_tx_hi) if the checkpoint directly after the cp_bounds.end() does not exist.
///
/// NOTE: for consistency, assume that lowerbounds are inclusive and upperbounds are exclusive.
/// Bounds that do not follow this convention will be annotated explicitly (e.g. `lo_exclusive` or
/// `hi_inclusive`).
/// TODO: (henry) merge this with lookups::tx_bounds
pub(crate) fn tx_bounds_query(
    cp_bounds: &RangeInclusive<u64>,
    global_tx_hi: u64,
    cursor_lo: u64,
    cursor_hi: u64,
) -> Query<'static> {
    query!(
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
            (
            SELECT
            -- tx_hi is the greatest of the after cursor, cp_bounds.start()
            GREATEST(tx_lo, {BigInt})
            FROM tx_lo
            ) AS tx_lo,
            -- tx_hi is the least of the before cursor and cp_bounds.end()
            LEAST(
                -- If we cannot get the tx_hi from the checkpoint directly after the cp_bounds.end() we use global_tx_hi
                COALESCE((SELECT tx_hi FROM tx_hi), {BigInt}),
                {BigInt}
            ) AS tx_hi"#,
        *cp_bounds.start() as i64,
        *cp_bounds.end() as i64,
        cursor_lo as i64,
        global_tx_hi as i64,
        cursor_hi as i64,
    )
}
