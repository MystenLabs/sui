// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//! This module implements the [Rosetta Account API](https://www.rosetta-api.org/docs/AccountApi.html)

use std::sync::Arc;

use axum::{Extension, Json};
use futures::StreamExt;

use sui_sdk::SUI_COIN_TYPE;

use crate::errors::Error;
use crate::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountCoinsRequest, AccountCoinsResponse,
    Amount, Coin,
};
use crate::{OnlineServerContext, SuiEnv};

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
        let amount = context
            .client
            .coin_read_api()
            .get_coins_stream(
                request.account_identifier.address,
                Some(SUI_COIN_TYPE.to_string()),
            )
            .fold(0u128, |acc, coin| async move { acc + coin.balance as u128 })
            .await;

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
    let coins = context
        .client
        .coin_read_api()
        .get_coins_stream(
            request.account_identifier.address,
            Some(SUI_COIN_TYPE.to_string()),
        )
        .map(Coin::from)
        .collect()
        .await;

    Ok(AccountCoinsResponse {
        block_identifier: context.blocks().current_block_identifier().await?,
        coins,
    })
}
