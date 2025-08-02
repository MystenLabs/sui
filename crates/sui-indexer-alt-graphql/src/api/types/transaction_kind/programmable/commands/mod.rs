// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod move_call;
mod transaction_argument;

use async_graphql::*;
use sui_types::transaction::Command as NativeCommand;

pub use move_call::MoveCallCommand;
pub use transaction_argument::TransactionArgument;

use crate::scope::Scope;

/// A single command in the programmable transaction.
#[derive(Union, Clone)]
pub enum Command {
    MoveCall(MoveCallCommand),
    Other(OtherCommand),
}

/// Placeholder for unimplemented command types
#[derive(SimpleObject, Clone)]
pub struct OtherCommand {
    /// Placeholder field for unimplemented commands
    #[graphql(name = "_")]
    pub dummy: Option<bool>,
}

impl Command {
    pub fn from(_scope: Scope, command: NativeCommand) -> Self {
        match command {
            NativeCommand::MoveCall(call) => Command::MoveCall(MoveCallCommand { native: *call }),
            _ => Command::Other(OtherCommand { dummy: None }),
        }
    }
}
