// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::ExecutedTransaction;
use super::Object;
use super::Transaction;
use super::TransactionEffects;
use super::TransactionEvents;
use super::UserSignature;
use crate::message::MessageMerge;
use crate::message::MessageMergeFrom;
use crate::message::{MessageField, MessageFields};

impl ExecutedTransaction {
    pub const DIGEST_FIELD: &'static MessageField = &MessageField::new("digest");
    pub const TRANSACTION_FIELD: &'static MessageField =
        &MessageField::new("transaction").with_message_fields(Transaction::FIELDS);
    pub const SIGNATURES_FIELD: &'static MessageField = &MessageField::new("signatures"); //.with_message_fields(UserSignature::FIELDS);
    pub const EFFECTS_FIELD: &'static MessageField =
        &MessageField::new("effects").with_message_fields(TransactionEffects::FIELDS);
    pub const EVENTS_FIELD: &'static MessageField =
        &MessageField::new("events").with_message_fields(TransactionEvents::FIELDS);
    pub const CHECKPOINT_FIELD: &'static MessageField = &MessageField::new("checkpoint");
    pub const TIMESTAMP_FIELD: &'static MessageField = &MessageField::new("timestamp");
    pub const BALANCE_CHANGES_FIELD: &'static MessageField = &MessageField::new("balance_changes");
    pub const INPUT_OBJECTS_FIELD: &'static MessageField =
        &MessageField::new("input_objects").with_message_fields(Object::FIELDS);
    pub const OUTPUT_OBJECTS_FIELD: &'static MessageField =
        &MessageField::new("output_objects").with_message_fields(Object::FIELDS);
}

impl MessageFields for ExecutedTransaction {
    const FIELDS: &'static [&'static MessageField] = &[
        Self::DIGEST_FIELD,
        Self::TRANSACTION_FIELD,
        Self::SIGNATURES_FIELD,
        Self::EFFECTS_FIELD,
        Self::EVENTS_FIELD,
        Self::CHECKPOINT_FIELD,
        Self::TIMESTAMP_FIELD,
        Self::BALANCE_CHANGES_FIELD,
        Self::INPUT_OBJECTS_FIELD,
        Self::OUTPUT_OBJECTS_FIELD,
    ];
}

impl MessageMerge<&ExecutedTransaction> for ExecutedTransaction {
    fn merge(&mut self, source: &ExecutedTransaction, mask: &crate::field_mask::FieldMaskTree) {
        let ExecutedTransaction {
            digest,
            transaction,
            signatures,
            effects,
            events,
            checkpoint,
            timestamp,
            balance_changes,
            input_objects,
            output_objects,
        } = source;

        if mask.contains(Self::DIGEST_FIELD.name) {
            self.digest = digest.clone();
        }

        if let Some(submask) = mask.subtree(Self::TRANSACTION_FIELD.name) {
            self.transaction = transaction
                .as_ref()
                .map(|t| Transaction::merge_from(t, &submask));
        }

        if let Some(submask) = mask.subtree(Self::SIGNATURES_FIELD.name) {
            self.signatures = signatures
                .iter()
                .map(|s| UserSignature::merge_from(s, &submask))
                .collect();
        }

        if let Some(submask) = mask.subtree(Self::EFFECTS_FIELD.name) {
            self.effects = effects
                .as_ref()
                .map(|e| TransactionEffects::merge_from(e, &submask));
        }

        if let Some(submask) = mask.subtree(Self::EVENTS_FIELD.name) {
            self.events = events
                .as_ref()
                .map(|events| TransactionEvents::merge_from(events, &submask));
        }

        if mask.contains(Self::CHECKPOINT_FIELD.name) {
            self.checkpoint = *checkpoint;
        }

        if mask.contains(Self::TIMESTAMP_FIELD.name) {
            self.timestamp = *timestamp;
        }

        if mask.contains(Self::BALANCE_CHANGES_FIELD.name) {
            self.balance_changes = balance_changes.clone();
        }

        if let Some(submask) = mask.subtree(Self::INPUT_OBJECTS_FIELD.name) {
            self.input_objects = input_objects
                .iter()
                .map(|object| Object::merge_from(object, &submask))
                .collect();
        }

        if let Some(submask) = mask.subtree(Self::OUTPUT_OBJECTS_FIELD.name) {
            self.output_objects = output_objects
                .iter()
                .map(|object| Object::merge_from(object, &submask))
                .collect();
        }
    }
}
