// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::SimpleObject;
use sui_rpc::proto::sui::rpc::v2 as proto;

use crate::{
    api::types::{
        move_type::MoveType, move_value::MoveValue,
        transaction_kind::programmable::commands::TransactionArgument,
    },
    scope::Scope,
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
    /// The structured Move value, if available.
    pub value: Option<MoveValue>,
}

impl CommandResult {
    pub(crate) fn from_proto(result: proto::CommandResult, scope: Scope) -> Self {
        Self {
            return_values: Some(
                result
                    .return_values
                    .into_iter()
                    .map(|output| CommandOutput::from_proto(output, scope.clone()))
                    .collect(),
            ),
            mutated_references: Some(
                result
                    .mutated_by_ref
                    .into_iter()
                    .map(|output| CommandOutput::from_proto(output, scope.clone()))
                    .collect(),
            ),
        }
    }
}

impl CommandOutput {
    pub(crate) fn from_proto(output: proto::CommandOutput, scope: Scope) -> Self {
        Self {
            argument: output
                .argument
                .and_then(|arg| TransactionArgument::try_from(arg).ok()),
            value: output.value.and_then(|bcs| {
                let type_name = bcs.name?;
                let value_bytes = bcs.value?.to_vec();

                // Parse the type name string into a MoveType
                let type_tag = type_name.parse().ok()?;
                let move_type = MoveType::from_native(type_tag, scope.clone());

                Some(MoveValue::new(move_type, value_bytes))
            }),
        }
    }
}
