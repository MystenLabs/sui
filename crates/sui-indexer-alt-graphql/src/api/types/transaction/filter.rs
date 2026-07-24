// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::CustomValidator;
use async_graphql::Enum;
use async_graphql::InputObject;
use async_graphql::InputValueError;

use sui_indexer_alt_reader::kv_loader::TransactionContents;
use sui_rpc::proto::sui::rpc::v2::AffectedAddressFilter;
use sui_rpc::proto::sui::rpc::v2::AffectedObjectFilter;
use sui_rpc::proto::sui::rpc::v2::MoveCallFilter;
use sui_rpc::proto::sui::rpc::v2::SenderFilter;
use sui_rpc::proto::sui::rpc::v2::TransactionFilter as GrpcTransactionFilter;
use sui_rpc::proto::sui::rpc::v2::TransactionLiteral;
use sui_rpc::proto::sui::rpc::v2::TransactionTerm;
use sui_rpc::proto::sui::rpc::v2::transaction_literal::Predicate;
use sui_types::transaction::TransactionDataAPI;

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

        let result = Self {
            after_checkpoint: intersect!(after_checkpoint, intersect::by_max)?,
            at_checkpoint: intersect!(at_checkpoint, intersect::by_eq)?,
            before_checkpoint: intersect!(before_checkpoint, intersect::by_min)?,
            function: intersect!(function, intersect::by_eq)?,
            kind: intersect!(kind, intersect::by_eq)?,
            affected_address: intersect!(affected_address, intersect::by_eq)?,
            affected_object: intersect!(affected_object, intersect::by_eq)?,
            sent_address: intersect!(sent_address, intersect::by_eq)?,
        };

        result.checkpoint_bounds_are_valid().then_some(result)
    }

    /// Check that checkpoint bounds don't contradict each other across fields.
    /// e.g. `afterCheckpoint: 100` with `atCheckpoint: 5` is inconsistent.
    fn checkpoint_bounds_are_valid(&self) -> bool {
        if let (Some(after), Some(at)) = (self.after_checkpoint, self.at_checkpoint)
            && after >= at
        {
            return false;
        }
        if let (Some(before), Some(at)) = (self.before_checkpoint, self.at_checkpoint)
            && before <= at
        {
            return false;
        }
        if let (Some(after), Some(before)) = (self.after_checkpoint, self.before_checkpoint)
            && after >= before
        {
            return false;
        }
        true
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

    pub(crate) fn to_grpc_filter(&self) -> Option<GrpcTransactionFilter> {
        let mut literals = Vec::new();

        if let Some(sent) = &self.sent_address {
            literals.push(include_literal(sender_predicate(sent)));
        }
        if let Some(address) = &self.affected_address {
            literals.push(include_literal(affected_address_predicate(address)));
        }
        if let Some(object) = &self.affected_object {
            literals.push(include_literal(affected_object_predicate(object)));
        }
        if let Some(function) = &self.function {
            literals.push(include_literal(move_call_predicate(function)));
        }
        if let Some(kind) = &self.kind {
            // Every system transaction (Genesis, ConsensusCommitPrologue, ChangeEpoch,
            // RandomnessStateUpdate, AuthenticatorStateUpdate, EndOfEpoch) is sent from 0x0;
            // programmable transactions never are. So ProgrammableTx maps to an unanchored
            // `Exclude(Sender = 0x0)` and the bitmap layer synthesizes the TxUniverse anchor for us
            // (sui-rpc-api/src/ledger_history/filter.rs).
            let zero_sender = sender_predicate(&SuiAddress::ZERO);
            literals.push(match kind {
                TransactionKindInput::SystemTx => include_literal(zero_sender),
                TransactionKindInput::ProgrammableTx => exclude_literal(zero_sender),
            });
        }

        if literals.is_empty() {
            return None;
        }

        let filter = GrpcTransactionFilter::default()
            .with_terms(vec![TransactionTerm::default().with_literals(literals)]);

        Some(filter)
    }

    /// Check if a transaction's contents matches this filter's non-checkpoint conditions.
    ///
    /// Checkpoint bounds (after/at/before) are not checked here — they are handled by the
    /// caller since streamed transactions are already scoped to a specific checkpoint.
    ///
    // Adapted from TransactionFilter::matches in PR #25717 (scan APIs). When #25717 merges,
    // we will unify them.
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

        // TODO: Verify consistency with the indexed path (tx_affected_addresses).
        // The indexed path includes sender, gas payer, and recipients from
        // all_changed_objects(). Confirm this matches for sponsored transactions
        // and edge cases (deleted/wrapped object owners).
        if let Some(affected_address) = &self.affected_address {
            let in_changed_objects = effects.all_changed_objects().iter().any(|(_, owner, _)| {
                owner
                    .get_address_owner_address()
                    .is_ok_and(|addr| SuiAddress::from(addr) == *affected_address)
            });
            let is_sender = SuiAddress::from(data.sender()) == *affected_address;
            let is_payer = SuiAddress::from(data.gas_data().owner) == *affected_address;
            if !in_changed_objects && !is_sender && !is_payer {
                return false;
            }
        }

        // TODO: Verify consistency with the indexed path (tx_affected_objects).
        // The indexed path uses object_changes() which includes deleted/wrapped
        // objects, while all_changed_objects() excludes them.
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

fn include_literal(predicate: Predicate) -> TransactionLiteral {
    let mut literal = TransactionLiteral::default();
    literal.predicate = Some(predicate);
    literal
}

fn exclude_literal(predicate: Predicate) -> TransactionLiteral {
    let mut literal = TransactionLiteral::default();
    literal.predicate = Some(predicate);
    literal.negated = true;
    literal
}

fn sender_predicate(address: &SuiAddress) -> Predicate {
    let mut f = SenderFilter::default();
    f.address = Some(address.to_string());
    Predicate::Sender(f)
}

fn affected_address_predicate(address: &SuiAddress) -> Predicate {
    let mut f = AffectedAddressFilter::default();
    f.address = Some(address.to_string());
    Predicate::AffectedAddress(f)
}

fn affected_object_predicate(object: &SuiAddress) -> Predicate {
    let mut f = AffectedObjectFilter::default();
    f.object_id = Some(object.to_string());
    Predicate::AffectedObject(f)
}

fn move_call_predicate(function: &FqNameFilter) -> Predicate {
    let mut f = MoveCallFilter::default();
    f.function = Some(function.to_string());
    Predicate::MoveCall(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(s: &str) -> SuiAddress {
        s.parse().expect("valid address")
    }

    /// Extract the predicates from a single-term filter, asserting no literal is
    /// negated (the parity shape).
    fn term_includes(filter: &GrpcTransactionFilter) -> Vec<&Predicate> {
        assert_eq!(filter.terms.len(), 1, "parity filter is a single term");
        filter.terms[0]
            .literals
            .iter()
            .map(|literal| {
                assert!(!literal.negated, "expected non-negated literal");
                literal.predicate.as_ref().expect("predicate set")
            })
            .collect()
    }

    /// Extract literals from a single-term filter, preserving negation. Used by tests
    /// that mix negated and non-negated literals (e.g. `kind`).
    fn term_literals(filter: &GrpcTransactionFilter) -> Vec<(bool, &Predicate)> {
        assert_eq!(filter.terms.len(), 1, "expected a single term");
        filter.terms[0]
            .literals
            .iter()
            .map(|literal| {
                (
                    literal.negated,
                    literal.predicate.as_ref().expect("predicate set"),
                )
            })
            .collect()
    }

    fn assert_sender_literal(literal: &(bool, &Predicate), include: bool, expected_address: &str) {
        assert_eq!(literal.0, !include);
        match literal.1 {
            Predicate::Sender(f) => {
                assert_eq!(f.address.as_deref(), Some(expected_address));
            }
            other => panic!("expected Sender predicate, got {other:?}"),
        }
    }

    #[test]
    fn unfiltered_yields_no_proto_filter() {
        let filter = TransactionFilter::default();
        assert!(filter.to_grpc_filter().is_none());
    }

    #[test]
    fn checkpoint_bounds_alone_are_not_a_predicate() {
        let filter = TransactionFilter {
            after_checkpoint: Some(UInt53::from(10)),
            before_checkpoint: Some(UInt53::from(20)),
            ..Default::default()
        };
        // Checkpoint bounds map to the request's checkpoint range, not the filter.
        assert!(filter.to_grpc_filter().is_none());
    }

    #[test]
    fn sent_address_maps_to_sender_include() {
        let sender = addr("0x2");
        let filter = TransactionFilter {
            sent_address: Some(sender),
            ..Default::default()
        };

        let proto = filter.to_grpc_filter().expect("serviceable");
        let predicates = term_includes(&proto);
        assert_eq!(predicates.len(), 1);
        match predicates[0] {
            Predicate::Sender(f) => {
                assert_eq!(f.address.as_deref(), Some(sender.to_string().as_str()));
            }
            other => panic!("expected Sender, got {other:?}"),
        }
    }

    #[test]
    fn affected_object_maps_to_affected_object_include() {
        let object = addr("0xabc");
        let filter = TransactionFilter {
            affected_object: Some(object),
            ..Default::default()
        };

        let proto = filter.to_grpc_filter().expect("serviceable");
        let predicates = term_includes(&proto);
        assert_eq!(predicates.len(), 1);
        match predicates[0] {
            Predicate::AffectedObject(f) => {
                assert_eq!(f.object_id.as_deref(), Some(object.to_string().as_str()));
            }
            other => panic!("expected AffectedObject, got {other:?}"),
        }
    }

    #[test]
    fn function_maps_to_move_call_include() {
        let function = FqNameFilter::FqName(addr("0x2"), "coin".to_string(), "join".to_string());
        let expected = function.to_string();
        let filter = TransactionFilter {
            function: Some(function),
            ..Default::default()
        };

        let proto = filter.to_grpc_filter().expect("serviceable");
        let predicates = term_includes(&proto);
        assert_eq!(predicates.len(), 1);
        match predicates[0] {
            Predicate::MoveCall(f) => {
                assert_eq!(f.function.as_deref(), Some(expected.as_str()));
            }
            other => panic!("expected MoveCall, got {other:?}"),
        }
    }

    #[test]
    fn sender_combines_with_each_other_field_in_one_term() {
        let sender = addr("0x2");

        // sender + affected_address: both predicates land as Include literals in the same
        // term (an AND).
        let with_address = TransactionFilter {
            sent_address: Some(sender),
            affected_address: Some(addr("0xdef")),
            ..Default::default()
        }
        .to_grpc_filter()
        .expect("valid bitmap filter");
        let preds = term_includes(&with_address);
        assert_eq!(preds.len(), 2);
        assert!(matches!(preds[0], Predicate::Sender(_)));
        assert!(matches!(preds[1], Predicate::AffectedAddress(_)));

        // sender + affected_object
        let with_object = TransactionFilter {
            sent_address: Some(sender),
            affected_object: Some(addr("0xabc")),
            ..Default::default()
        }
        .to_grpc_filter()
        .expect("valid bitmap filter");
        let preds = term_includes(&with_object);
        assert_eq!(preds.len(), 2);
        assert!(matches!(preds[0], Predicate::Sender(_)));
        assert!(matches!(preds[1], Predicate::AffectedObject(_)));

        // sender + function
        let with_function = TransactionFilter {
            sent_address: Some(sender),
            function: Some(FqNameFilter::FqName(
                addr("0x2"),
                "coin".to_string(),
                "join".to_string(),
            )),
            ..Default::default()
        }
        .to_grpc_filter()
        .expect("valid bitmap filter");
        let preds = term_includes(&with_function);
        assert_eq!(preds.len(), 2);
        assert!(matches!(preds[0], Predicate::Sender(_)));
        assert!(matches!(preds[1], Predicate::MoveCall(_)));
    }

    #[test]
    fn system_kind_maps_to_sender_zero_include() {
        let filter = TransactionFilter {
            kind: Some(TransactionKindInput::SystemTx),
            ..Default::default()
        };

        let proto = filter.to_grpc_filter().expect("serviceable");
        let literals = term_literals(&proto);
        assert_eq!(literals.len(), 1);
        assert_sender_literal(&literals[0], true, &SuiAddress::ZERO.to_string());
    }

    #[test]
    fn programmable_kind_maps_to_sender_zero_negated() {
        let filter = TransactionFilter {
            kind: Some(TransactionKindInput::ProgrammableTx),
            ..Default::default()
        };

        // Unanchored negation: a single negated literal. The bitmap layer synthesizes
        // the TxUniverse anchor when converting the term, so this term resolves as
        // `range \ {tx : sender == 0x0}`.
        let proto = filter.to_grpc_filter().expect("serviceable");
        let literals = term_literals(&proto);
        assert_eq!(literals.len(), 1);
        assert_sender_literal(&literals[0], false, &SuiAddress::ZERO.to_string());
    }

    #[test]
    fn programmable_kind_combines_with_sent_address() {
        let sender = addr("0x2");
        let filter = TransactionFilter {
            sent_address: Some(sender),
            kind: Some(TransactionKindInput::ProgrammableTx),
            ..Default::default()
        };

        let proto = filter.to_grpc_filter().expect("serviceable");
        let literals = term_literals(&proto);
        assert_eq!(literals.len(), 2);
        assert_sender_literal(&literals[0], true, &sender.to_string());
        assert_sender_literal(&literals[1], false, &SuiAddress::ZERO.to_string());
    }
}
