// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod merge_coins;
mod move_call;
mod publish;
mod split_coins;
mod transaction_argument;
mod transfer_objects;
mod upgrade;

use async_graphql::*;
use sui_types::transaction::Command as NativeCommand;

use crate::api::scalars::{base64::Base64, sui_address::SuiAddress};

pub use merge_coins::MergeCoinsCommand;
pub use move_call::MoveCallCommand;
pub use publish::PublishCommand;
pub use split_coins::SplitCoinsCommand;
pub use transaction_argument::TransactionArgument;
pub use transfer_objects::TransferObjectsCommand;
pub use upgrade::UpgradeCommand;

use crate::scope::Scope;

/// A single command in the programmable transaction.
#[derive(Union, Clone)]
pub enum Command {
    MergeCoins(MergeCoinsCommand),
    MoveCall(MoveCallCommand),
    Publish(PublishCommand),
    SplitCoins(SplitCoinsCommand),
    TransferObjects(TransferObjectsCommand),
    Upgrade(UpgradeCommand),
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
            NativeCommand::MergeCoins(coin, coins) => Command::MergeCoins(MergeCoinsCommand {
                coin: Some(TransactionArgument::from(coin)),
                coins: coins.into_iter().map(TransactionArgument::from).collect(),
            }),
            NativeCommand::TransferObjects(objects, address) => {
                Command::TransferObjects(TransferObjectsCommand {
                    inputs: objects.into_iter().map(TransactionArgument::from).collect(),
                    address: Some(TransactionArgument::from(address)),
                })
            }
            NativeCommand::Publish(modules, dependencies) => Command::Publish(PublishCommand {
                modules: Some(modules.into_iter().map(Base64::from).collect()),
                dependencies: Some(dependencies.into_iter().map(SuiAddress::from).collect()),
            }),
            NativeCommand::Upgrade(modules, dependencies, current_package, upgrade_ticket) => {
                Command::Upgrade(UpgradeCommand {
                    modules: Some(modules.into_iter().map(Base64::from).collect()),
                    dependencies: Some(dependencies.into_iter().map(SuiAddress::from).collect()),
                    current_package: Some(SuiAddress::from(current_package)),
                    upgrade_ticket: Some(TransactionArgument::from(upgrade_ticket)),
                })
            }
            _ => Command::Other(OtherCommand { dummy: None }),
        }
    }
}
