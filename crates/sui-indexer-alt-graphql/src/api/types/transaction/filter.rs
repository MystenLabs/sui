// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::CustomValidator;
use async_graphql::Enum;
use async_graphql::InputObject;
use async_graphql::InputValueError;
use sui_indexer_alt_reader::kv_loader::TransactionContents;
use sui_types::transaction::TransactionDataAPI as _;

use sui_indexer_alt_schema::blooms::should_skip_for_bloom;

use crate::api::scalars::fq_name_filter::FqNameFilter;
use crate::api::scalars::sui_address::SuiAddress;
use crate::api::scalars::uint53::UInt53;
use crate::api::types::lookups::CheckpointBounds;
use crate::intersect;

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

/// Validator for transactions pagination - allows at most one primary filter.
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

    /// The active filters in TransactionFilter. Used to find the pipelines that are available to serve queries with these filters applied.
    pub(crate) fn active_filters(&self) -> Vec<String> {
        let mut filters = vec![];

        if self.affected_address.is_some() {
            filters.push("affectedAddress".to_string());
        }
        if self.sent_address.is_some() {
            filters.push("sentAddress".to_string());
        }
        if self.kind.is_some() {
            filters.push("kind".to_string());
        }
        if self.function.is_some() {
            filters.push("function".to_string());
        }
        if self.affected_object.is_some() {
            filters.push("affectedObjects".to_string());
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

    /// Values to probe in bloom filters.
    pub(crate) fn bloom_probe_values(&self) -> Vec<[u8; 32]> {
        [
            self.function.as_ref().map(|f| f.package().into_bytes()),
            self.affected_address.map(|a| a.into_bytes()),
            self.affected_object.map(|o| o.into_bytes()),
            self.sent_address.map(|s| s.into_bytes()),
        ]
        .into_iter()
        .flatten()
        .filter(|v| !should_skip_for_bloom(v))
        .collect()
    }
}

impl TransactionFilter {
    pub(crate) fn matches(&self, transaction: &TransactionContents) -> bool {
        let Ok(data) = transaction.data() else {
            return false;
        };
        let Ok(effects) = transaction.effects() else {
            return false;
        };

        if let Some(function) = &self.function {
            let has_match = data.move_calls().into_iter().any(|(_, p, m, f)| {
                SuiAddress::from(*p) == function.package()
                    && function.module().is_none_or(|module| m == module)
                    && function.name().is_none_or(|name| f == name)
            });
            if !has_match {
                return false;
            }
        }

        if let Some(sent_address) = &self.sent_address
            && SuiAddress::from(data.sender()) != *sent_address
        {
            return false;
        }

        if let Some(affected_address) = &self.affected_address {
            let in_changed_objects = effects.all_changed_objects().iter().any(|(_, owner, _)| {
                owner
                    .get_address_owner_address()
                    .is_ok_and(|addr| SuiAddress::from(addr) == *affected_address)
            });
            let is_sender = SuiAddress::from(data.sender()) == *affected_address;
            if !in_changed_objects && !is_sender {
                return false;
            }
        }

        if let Some(affected_object) = &self.affected_object {
            let has_match = effects
                .all_changed_objects()
                .iter()
                .any(|((object_id, _, _), _, _)| SuiAddress::from(*object_id) == *affected_object);
            if !has_match {
                return false;
            }
        }

        if let Some(kind) = &self.kind {
            let is_programmable = matches!(
                data.kind(),
                sui_types::transaction::TransactionKind::ProgrammableTransaction(_)
            );
            let matches_kind = match kind {
                TransactionKindInput::ProgrammableTx => is_programmable,
                TransactionKindInput::SystemTx => !is_programmable,
            };
            if !matches_kind {
                return false;
            }
        }

        true
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_bloom_probe_values_skips_zero_address() {
        let zero = SuiAddress::from_str("0x0").unwrap();
        let filter = TransactionFilter {
            sent_address: Some(zero),
            ..Default::default()
        };
        assert!(filter.bloom_probe_values().is_empty());
    }

    #[test]
    fn test_bloom_probe_values_skips_clock_address() {
        let clock = SuiAddress::from_str("0x6").unwrap();
        let filter = TransactionFilter {
            affected_object: Some(clock),
            ..Default::default()
        };
        assert!(filter.bloom_probe_values().is_empty());
    }

    #[test]
    fn test_bloom_probe_values_keeps_normal_address() {
        let addr = SuiAddress::from_str("0x42").unwrap();
        let filter = TransactionFilter {
            sent_address: Some(addr),
            ..Default::default()
        };
        let values = filter.bloom_probe_values();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0], addr.into_bytes());
    }

    #[test]
    fn test_bloom_probe_values_empty_filter() {
        let filter = TransactionFilter::default();
        assert!(filter.bloom_probe_values().is_empty());
    }
}
