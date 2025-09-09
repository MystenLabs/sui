// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::SimpleObject;
use sui_rpc::proto::sui::rpc::v2beta2 as proto;

use crate::api::{
    scalars::base64::Base64, types::transaction_kind::programmable::commands::TransactionArgument,
};

/// The intermediate results for each command of a transaction simulation.
#[derive(Clone, SimpleObject)]
pub struct CommandResult {
    /// Return results of each command in this transaction.
    pub return_values: Option<Vec<CommandOutput>>,
    /// Changes made to arguments that were mutably borrowed by each command in this transaction.
    pub mutated_references: Option<Vec<CommandOutput>>,
}

/// A value produced or modified during command execution.
///
/// This can represent either a return value from a command or an argument that was mutated by reference.
#[derive(Clone, SimpleObject)]
pub struct CommandOutput {
    /// The transaction argument that this value corresponds to (if any).
    pub argument: Option<TransactionArgument>,
    /// BCS-encoded representation of the value.
    pub value: Option<Base64>,
}

impl From<proto::CommandResult> for CommandResult {
    fn from(result: proto::CommandResult) -> Self {
        Self {
            return_values: Some(
                result
                    .return_values
                    .into_iter()
                    .map(CommandOutput::from)
                    .collect(),
            ),
            mutated_references: Some(
                result
                    .mutated_by_ref
                    .into_iter()
                    .map(CommandOutput::from)
                    .collect(),
            ),
        }
    }
}

impl From<proto::CommandOutput> for CommandOutput {
    fn from(output: proto::CommandOutput) -> Self {
        Self {
            argument: output.argument.map(TransactionArgument::from),
            value: output
                .value
                .map(|bcs| Base64(bcs.value.unwrap_or_default().to_vec())),
        }
    }
}
