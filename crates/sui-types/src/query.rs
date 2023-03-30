// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::SuiAddress;
use crate::messages_checkpoint::CheckpointSequenceNumber;
use crate::sui_serde::BigInt;
use crate::ObjectID;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_with::serde_as;

#[serde_as]
#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize)]
pub enum TransactionFilter {
    /// Query by checkpoint.
    Checkpoint(
        #[schemars(with = "BigInt<u64>")]
        #[serde_as(as = "BigInt<u64>")]
        CheckpointSequenceNumber,
    ),
    /// Query by move function.
    MoveFunction {
        package: ObjectID,
        module: Option<String>,
        function: Option<String>,
    },
    /// Query by input object.
    InputObject(ObjectID),
    /// Query by changed object, including created, mutated and unwrapped objects.
    ChangedObject(ObjectID),
    /// Query by sender address.
    FromAddress(SuiAddress),
    /// Query by recipient address.
    ToAddress(SuiAddress),
    /// Query by sender and recipient address.
    FromAndToAddress { from: SuiAddress, to: SuiAddress },
    /// Query by transaction kind
    TransactionKind(String),
}
