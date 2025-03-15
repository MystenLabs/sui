// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::ExecutedTransaction;
use super::Object;
use super::Transaction;
use super::TransactionEffects;
use super::TransactionEvents;
use crate::message::{MessageField, MessageFields};

impl ExecutedTransaction {
    const DIGEST_FIELD: &'static MessageField = &MessageField::new("digest");
    const TRANSACTION_FIELD: &'static MessageField =
        &MessageField::new("transaction").with_message_fields(Transaction::FIELDS);
    const SIGNATURES_FIELD: &'static MessageField = &MessageField::new("signatures"); //.with_message_fields(UserSignature::FIELDS);
    const EFFECTS_FIELD: &'static MessageField =
        &MessageField::new("effects").with_message_fields(TransactionEffects::FIELDS);
    const EVENTS_FIELD: &'static MessageField =
        &MessageField::new("events").with_message_fields(TransactionEvents::FIELDS);
    const CHECKPOINT_FIELD: &'static MessageField = &MessageField::new("checkpoint");
    const TIMESTAMP_FIELD: &'static MessageField = &MessageField::new("timestamp");
    const BALANCE_CHANGES_FIELD: &'static MessageField = &MessageField::new("balance_changes");
    const INPUT_OBJECTS_FIELD: &'static MessageField =
        &MessageField::new("input_objects").with_message_fields(Object::FIELDS);
    const OUTPUT_OBJECTS_FIELD: &'static MessageField =
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
