// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::base_types::SuiAddress;
use crate::ObjectID;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize)]
pub enum TransactionQuery {
    // All transaction hashes.
    All,
    // Query by move function.
    MoveFunction {
        package: ObjectID,
        module: Option<String>,
        function: Option<String>,
    },
    // Query by input object.
    InputObject(ObjectID),
    // Query by mutated object.
    MutatedObject(ObjectID),
    // Query by sender address.
    FromAddress(SuiAddress),
    // Query by recipient address.
    ToAddress(SuiAddress),
}

#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize, Eq, PartialEq)]
pub enum Ordering {
    // Ascending order (oldest transaction first), transactions are causal ordered.
    Ascending,
    // Descending order (latest transaction first), transactions are causal ordered.
    Descending,
}
