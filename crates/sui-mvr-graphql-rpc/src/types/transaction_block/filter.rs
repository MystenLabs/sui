// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::TransactionBlockKindInput;
use crate::types::{digest::Digest, sui_address::SuiAddress, type_filter::FqNameFilter};
use crate::types::{intersect, uint53::UInt53};
use async_graphql::InputObject;
use std::collections::BTreeSet;
use sui_types::base_types::SuiAddress as NativeSuiAddress;

#[derive(InputObject, Debug, Default, Clone)]
pub(crate) struct TransactionBlockFilter {
    /// Filter transactions by move function called. Calls can be filtered by the `package`,
    /// `package::module`, or the `package::module::name` of their function.
    pub function: Option<FqNameFilter>,

    /// An input filter selecting for either system or programmable transactions.
    pub kind: Option<TransactionBlockKindInput>,

    /// Limit to transactions that occured strictly after the given checkpoint.
    pub after_checkpoint: Option<UInt53>,

    /// Limit to transactions in the given checkpoint.
    pub at_checkpoint: Option<UInt53>,

    /// Limit to transaction that occured strictly before the given checkpoint.
    pub before_checkpoint: Option<UInt53>,

    /// Limit to transactions that interacted with the given address. The address could be a
    /// sender, sponsor, or recipient of the transaction.
    pub affected_address: Option<SuiAddress>,

    /// Limit to transactions that interacted with the given object. The object could have been
    /// created, read, modified, deleted, wrapped, or unwrapped by the transaction. Objects that
    /// were passed as a `Receiving` input are not considered to have been affected by a
    /// transaction unless they were actually received.
    #[cfg(feature = "staging")]
    pub affected_object: Option<SuiAddress>,

    /// Limit to transactions that were sent by the given address.
    pub sent_address: Option<SuiAddress>,

    /// Limit to transactions that accepted the given object as an input. NOTE: this input filter
    /// has been deprecated in favor of `affectedObject` which offers an easier to under behavior.
    ///
    /// This filter will be removed with 1.36.0 (2024-10-14), or at least one release after
    /// `affectedObject` is introduced, whichever is later.
    pub input_object: Option<SuiAddress>,

    /// Limit to transactions that output a versioon of this object. NOTE: this input filter has
    /// been deprecated in favor of `affectedObject` which offers an easier to understand behavor.
    ///
    /// This filter will be removed with 1.36.0 (2024-10-14), or at least one release after
    /// `affectedObject` is introduced, whichever is later.
    pub changed_object: Option<SuiAddress>,

    /// Select transactions by their digest.
    pub transaction_ids: Option<Vec<Digest>>,
}

impl TransactionBlockFilter {
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
            function: intersect!(function, FqNameFilter::intersect)?,
            kind: intersect!(kind, intersect::by_eq)?,

            after_checkpoint: intersect!(after_checkpoint, intersect::by_max)?,
            at_checkpoint: intersect!(at_checkpoint, intersect::by_eq)?,
            before_checkpoint: intersect!(before_checkpoint, intersect::by_min)?,

            affected_address: intersect!(affected_address, intersect::by_eq)?,
            #[cfg(feature = "staging")]
            affected_object: intersect!(affected_object, intersect::by_eq)?,
            sent_address: intersect!(sent_address, intersect::by_eq)?,
            input_object: intersect!(input_object, intersect::by_eq)?,
            changed_object: intersect!(changed_object, intersect::by_eq)?,

            transaction_ids: intersect!(transaction_ids, |a, b| {
                let a = BTreeSet::from_iter(a.into_iter());
                let b = BTreeSet::from_iter(b.into_iter());
                Some(a.intersection(&b).cloned().collect())
            })?,
        })
    }

    /// Most filter conditions require a scan limit if used in tandem with other filters. The
    /// exception to this is sender and checkpoint, since sender is denormalized on all tables, and
    /// the corresponding tx range can be determined for a checkpoint.
    pub(crate) fn requires_scan_limit(&self) -> bool {
        [
            self.function.is_some(),
            self.kind.is_some(),
            self.affected_address.is_some(),
            #[cfg(feature = "staging")]
            self.affected_object.is_some(),
            self.input_object.is_some(),
            self.changed_object.is_some(),
            self.transaction_ids.is_some(),
        ]
        .into_iter()
        .filter(|is_set| *is_set)
        .count()
            > 1
    }

    /// If we don't query a lookup table that has a denormalized sender column, we need to
    /// explicitly specify the sender with a query on `tx_sender`. This function returns the sender
    /// we need to add an explicit query for if one is required, or `None` otherwise.
    pub(crate) fn explicit_sender(&self) -> Option<SuiAddress> {
        let missing_implicit_sender = self.function.is_none()
            && self.kind.is_none()
            && self.affected_address.is_none()
            && self.input_object.is_none()
            && self.changed_object.is_none();

        #[cfg(feature = "staging")]
        let missing_implicit_sender = missing_implicit_sender && self.affected_object.is_none();

        missing_implicit_sender
            .then_some(self.sent_address)
            .flatten()
    }

    /// A TransactionBlockFilter is considered not to have any filters if no filters are specified,
    /// or if the only filters are on `checkpoint`.
    pub(crate) fn has_filters(&self) -> bool {
        let has_filters = self.function.is_some()
            || self.kind.is_some()
            || self.sent_address.is_some()
            || self.affected_address.is_some()
            || self.input_object.is_some()
            || self.changed_object.is_some()
            || self.transaction_ids.is_some();

        #[cfg(feature = "staging")]
        let has_filters = has_filters || self.affected_object.is_some();

        has_filters
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
            || matches!(
                (self.kind, self.sent_address),
                (Some(kind), Some(signer))
                    if (kind == TransactionBlockKindInput::SystemTx)
                        != (signer == SuiAddress::from(NativeSuiAddress::ZERO))
            )
    }
}
