// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//! This module implements the [Rosetta Account API](https://www.rosetta-api.org/docs/AccountApi.html)

use std::sync::Arc;

use axum::{Extension, Json};

use sui_core::authority::AuthorityState;
use sui_types::base_types::SuiAddress;
use sui_types::gas_coin::GasCoin;
use sui_types::object::Owner;

use crate::errors::Error;
use crate::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountCoinsRequest, AccountCoinsResponse,
    Amount, Coin, CoinID, CoinIdentifier, SignedValue,
};
use crate::{ErrorType, OnlineServerContext, SuiEnv, SUI};

/// Get an array of all AccountBalances for an AccountIdentifier and the BlockIdentifier
/// at which the balance lookup was performed.
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/AccountApi.html#accountbalance)
pub async fn balance(
    Json(request): Json<AccountBalanceRequest>,
    Extension(context): Extension<Arc<OnlineServerContext>>,
    Extension(env): Extension<SuiEnv>,
) -> Result<AccountBalanceResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;

    let block_id = if let Some(index) = request.block_identifier.index {
        context.blocks().get_block_by_index(index).await.ok()
    } else if let Some(hash) = request.block_identifier.hash {
        context.blocks().get_block_by_hash(hash).await.ok()
    } else {
        None
    }
    .map(|b| b.block.block_identifier);

    if let Some(block_identifier) = block_id {
        context
            .blocks()
            .get_balance_at_block(request.account_identifier.address, block_identifier.index)
            .await
            .map(|balance| AccountBalanceResponse {
                block_identifier,
                balances: vec![Amount::new(balance.into())],
            })
    } else {
        let gas_coins = get_coins(&context.state, request.account_identifier.address).await?;
        let amount: u128 = gas_coins.iter().map(|coin| coin.amount.value.abs()).sum();
        Ok(AccountBalanceResponse {
            block_identifier: context.blocks().current_block_identifier().await?,
            balances: vec![Amount::new(amount.into())],
        })
    }
}

/// Get an array of all unspent coins for an AccountIdentifier and the BlockIdentifier at which the lookup was performed. .
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/AccountApi.html#accountcoins)
pub async fn coins(
    Json(request): Json<AccountCoinsRequest>,
    Extension(context): Extension<Arc<OnlineServerContext>>,
    Extension(env): Extension<SuiEnv>,
) -> Result<AccountCoinsResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let coins = get_coins(&context.state, request.account_identifier.address).await?;
    Ok(AccountCoinsResponse {
        block_identifier: context.blocks().current_block_identifier().await?,
        coins,
    })
}

async fn get_coins(state: &AuthorityState, address: SuiAddress) -> Result<Vec<Coin>, Error> {
    let object_infos = state.get_owner_objects(Owner::AddressOwner(address))?;
    let coin_infos = object_infos
        .iter()
        .filter(|o| o.type_.is_gas_coin())
        .map(|info| info.object_id)
        .collect::<Vec<_>>();

    let objects = state.get_objects(&coin_infos).await?;
    objects
        .iter()
        .flatten()
        .map(|o| {
            let coin = GasCoin::try_from(o)?;
            Ok(Coin {
                coin_identifier: CoinIdentifier {
                    identifier: CoinID {
                        id: o.id(),
                        version: o.version(),
                    },
                },
                amount: Amount {
                    value: SignedValue::from(coin.value()),
                    currency: SUI.clone(),
                },
            })
        })
        .collect::<Result<Vec<_>, anyhow::Error>>()
        .map_err(|e| Error::new_with_cause(ErrorType::InternalError, e))
}
