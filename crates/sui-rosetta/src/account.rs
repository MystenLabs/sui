// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
use crate::{ErrorType, ServerContext, SUI};

pub async fn balance(
    Json(payload): Json<AccountBalanceRequest>,
    Extension(context): Extension<Arc<ServerContext>>,
) -> Result<AccountBalanceResponse, Error> {
    context.checks_network_identifier(&payload.network_identifier)?;
    let gas_coins = get_coins(&context.state, payload.account_identifier.address).await?;
    let amount: u64 = gas_coins.iter().map(|coin| coin.amount.value.abs()).sum();

    Ok(AccountBalanceResponse {
        block_identifier: context.blocks().current_block_identifier().await?,
        balances: vec![Amount::new(amount.into())],
    })
}

pub async fn coins(
    Json(payload): Json<AccountCoinsRequest>,
    Extension(context): Extension<Arc<ServerContext>>,
) -> Result<AccountCoinsResponse, Error> {
    context.checks_network_identifier(&payload.network_identifier)?;
    let coins = get_coins(&context.state, payload.account_identifier.address).await?;
    Ok(AccountCoinsResponse {
        block_identifier: context.blocks().current_block_identifier().await?,
        coins,
    })
}

async fn get_coins(state: &AuthorityState, address: SuiAddress) -> Result<Vec<Coin>, Error> {
    let object_infos = state.get_owner_objects(Owner::AddressOwner(address))?;
    let coin_infos = object_infos
        .iter()
        .filter(|o| o.type_ == GasCoin::type_().to_string())
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
                        version: Some(o.version()),
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
