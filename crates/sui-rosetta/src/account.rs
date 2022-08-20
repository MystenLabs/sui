// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use axum::{Extension, Json};
use serde_json::json;

use crate::errors::Error;
use crate::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountCoinsRequest, AccountCoinsResponse,
    Amount, BlockIdentifier, Coin, CoinIdentifier,
};
use crate::{ApiState, ErrorType};
use sui_sdk::rpc_types::SuiData;
use sui_sdk::types::base_types::SuiAddress;
use sui_sdk::types::gas_coin::GasCoin;
use sui_sdk::SuiClient;

pub async fn balance(
    Json(payload): Json<AccountBalanceRequest>,
    Extension(state): Extension<Arc<ApiState>>,
) -> Result<AccountBalanceResponse, Error> {
    state.checks_network_identifier(&payload.network_identifier)?;
    let gas_coins = get_gas_coins(
        state.get_client(payload.network_identifier.network).await?,
        payload.account_identifier.address,
    )
    .await?;
    let amount: u64 = gas_coins.iter().map(|coin| coin.value()).sum();
    Ok(AccountBalanceResponse {
        block_identifier: BlockIdentifier {
            index: 0,
            hash: "".to_string(),
        },
        balances: vec![Amount::new(amount.try_into()?)],
    })
}

pub async fn coins(
    Json(payload): Json<AccountCoinsRequest>,
    Extension(state): Extension<Arc<ApiState>>,
) -> Result<AccountCoinsResponse, Error> {
    state.checks_network_identifier(&payload.network_identifier)?;
    let coins = get_gas_coins(
        state.get_client(payload.network_identifier.network).await?,
        payload.account_identifier.address,
    )
    .await?;
    let coins = coins
        .iter()
        .map(|coin| {
            Ok(Coin {
                coin_identifier: CoinIdentifier {
                    identifier: *coin.id(),
                },
                amount: Amount::new(coin.value().try_into()?),
            })
        })
        .collect::<Result<_, anyhow::Error>>()?;

    Ok(AccountCoinsResponse {
        block_identifier: BlockIdentifier {
            index: 0,
            hash: "".to_string(),
        },
        coins,
    })
}

async fn get_gas_coins(client: &SuiClient, address: SuiAddress) -> Result<Vec<GasCoin>, Error> {
    let object_infos = client
        .read_api()
        .get_objects_owned_by_address(address)
        .await
        .unwrap();
    let coin_infos = object_infos
        .iter()
        .filter(|o| o.type_ == GasCoin::type_().to_string());

    let mut coins = Vec::new();
    for coin in coin_infos {
        let response = client.read_api().get_object(coin.object_id).await.unwrap();
        let coin = response
            .object()?
            .data
            .try_as_move()
            .ok_or_else(|| {
                Error::new_with_detail(
                    ErrorType::DataError,
                    json!({
                        "cause": format!("Object [{}] is not a Move object.", coin.object_id)
                    }),
                )
            })?
            .deserialize()?;
        coins.push(coin);
    }
    Ok(coins)
}
