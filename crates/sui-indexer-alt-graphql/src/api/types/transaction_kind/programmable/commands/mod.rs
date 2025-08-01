// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod move_call;

use async_graphql::*;

use crate::scope::Scope;
pub use move_call::MoveCallCommand;

/// A single command in the programmable transaction.
#[derive(Union, Clone)]
pub enum Command {
    MoveCall(MoveCallCommand),
}

impl Command {
    pub fn from(command: sui_types::transaction::Command, _scope: Scope) -> Self {
        use sui_types::transaction::Command as NativeCommand;

        match command {
            NativeCommand::MoveCall(_) => Self::MoveCall(MoveCallCommand { dummy: None }),
            // TODO: Handle other command types, for now just use MoveCall as placeholder
            _ => Self::MoveCall(MoveCallCommand { dummy: None }),
        }
    }
}
