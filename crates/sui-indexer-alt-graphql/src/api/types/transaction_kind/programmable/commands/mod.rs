// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod move_call;
mod split_coins;
mod transaction_argument;
mod transfer_objects;

use async_graphql::*;
use sui_types::transaction::Command as NativeCommand;

pub use move_call::MoveCallCommand;
pub use split_coins::SplitCoinsCommand;
pub use transaction_argument::TransactionArgument;
pub use transfer_objects::TransferObjectsCommand;

use crate::scope::Scope;

/// A single command in the programmable transaction.
#[derive(Union, Clone)]
pub enum Command {
    MoveCall(MoveCallCommand),
    SplitCoins(SplitCoinsCommand),
    TransferObjects(TransferObjectsCommand),
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
            NativeCommand::SplitCoins(coin, amounts) => Command::SplitCoins(SplitCoinsCommand {
                coin: Some(TransactionArgument::from(coin)),
                amounts: amounts.into_iter().map(TransactionArgument::from).collect(),
            }),
            NativeCommand::TransferObjects(objects, address) => {
                Command::TransferObjects(TransferObjectsCommand {
                    inputs: objects.into_iter().map(TransactionArgument::from).collect(),
                    address: Some(TransactionArgument::from(address)),
                })
            }
            _ => Command::Other(OtherCommand { dummy: None }),
        }
    }
}
