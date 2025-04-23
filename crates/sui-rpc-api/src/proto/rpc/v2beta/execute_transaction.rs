// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{ExecuteTransactionRequest, ExecuteTransactionResponse, ExecutedTransaction};
use crate::message::{MessageField, MessageFields};

impl ExecuteTransactionResponse {
    pub const FINALITY_FIELD: &'static MessageField = &MessageField::new("finality");
    pub const TRANSACTION_FIELD: &'static MessageField =
        &MessageField::new("transaction").with_message_fields(ExecutedTransaction::FIELDS);
}

impl MessageFields for ExecuteTransactionResponse {
    const FIELDS: &'static [&'static MessageField] =
        &[Self::FINALITY_FIELD, Self::TRANSACTION_FIELD];
}

impl ExecuteTransactionRequest {
    pub const READ_MASK_DEFAULT: &str = "finality";
}
