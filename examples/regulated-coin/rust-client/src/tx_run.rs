// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod coin;
mod deny;

use anyhow::Result;
use move_core_types::language_storage::TypeTag;
use sui_keys::keystore::AccountKeystore;
use sui_sdk::SuiClient;
use sui_sdk::rpc_types::SuiTransactionBlockResponse;
use sui_sdk::types::base_types::{SuiAddress, ObjectID};
use sui_sdk::wallet_context::WalletContext;

#[derive(Debug)]
pub enum AppCommand {
    DenyListAdd(SuiAddress),
    DenyListRemove(SuiAddress),
    MintAndTransfer(u64, SuiAddress),
    Transfer(ObjectID, SuiAddress),
    Burn(ObjectID)
}

pub struct AppConfig {
    pub client: SuiClient,
    pub wallet_context: WalletContext,
    pub type_tag: TypeTag,
}

pub async fn execute_command(
    command: AppCommand,
    config: AppConfig,
) -> Result<SuiTransactionBlockResponse> {
    let AppConfig {
        client,
        mut wallet_context,
        type_tag,
    } = config;
    let active_addr = wallet_context.active_address()?;
    let signer = wallet_context.config.keystore.get_key(&active_addr)?;

    match command {
        AppCommand::DenyListAdd(address) => {
            let deny_list = deny::get_deny_list(&client).await?;
            let deny_cap = deny::get_deny_cap(&client, active_addr, type_tag.clone()).await?;
            deny::deny_list_add(&client, signer, type_tag, deny_list, deny_cap, address).await
        }
        AppCommand::DenyListRemove(address) => {
            let deny_list = deny::get_deny_list(&client).await?;
            let deny_cap = deny::get_deny_cap(&client, active_addr, type_tag.clone()).await?;
            deny::deny_list_remove(&client, signer, type_tag, deny_list, deny_cap, address).await
        }
        AppCommand::MintAndTransfer(balance, to_address) => {
            let treasury_cap =
                coin::get_treasury_cap(&client, active_addr, type_tag.clone()).await?;
            coin::mint_and_transfer(&client, signer, type_tag, treasury_cap, to_address, balance)
                .await
        }
        AppCommand::Transfer(coin_id, to_address) => {
            let coin = coin::get_coin(&client, coin_id).await?;
            coin::transfer(&client, signer, coin, to_address).await
        }
        AppCommand::Burn(coin_id) => {
            let treasury_cap =
                coin::get_treasury_cap(&client, active_addr, type_tag.clone()).await?;
            let coin = coin::get_coin(&client, coin_id).await?;
            coin::burn(&client, signer, type_tag, treasury_cap, coin).await
        }
    }
}
