// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//! This module implements the [Rosetta Account API](https://www.rosetta-api.org/docs/AccountApi.html)
use axum::extract::State;
use axum::{Extension, Json};
use axum_extra::extract::WithRejection;
use futures::future::join_all;

use prost_types::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2beta2::{
    Checkpoint, CheckpointSummary, GetBalanceRequest, GetCheckpointRequest, GetEpochRequest,
    ListOwnedObjectsRequest, Object,
};
use sui_sdk::SUI_COIN_TYPE;
use sui_sdk_types::Address;
use sui_types::base_types::SuiAddress;
use tracing::info;

use crate::errors::Error;
use crate::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountCoinsRequest, AccountCoinsResponse,
    Amount, Coin, CoinID, CoinIdentifier, Currencies, Currency, SubAccountType, SubBalance,
};
use crate::{OnlineServerContext, SuiEnv};
use std::time::Duration;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

/// Get an array of all AccountBalances for an AccountIdentifier and the BlockIdentifier
/// at which the balance lookup was performed.
/// [Rosetta API Spec](https://docs.cdp.coinbase.com/api-reference/mesh/account/get-an-account-balance)
pub async fn balance(
    State(mut ctx): State<OnlineServerContext>,
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<AccountBalanceRequest>, Error>,
) -> Result<AccountBalanceResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let address = request.account_identifier.address;
    let currencies = &request.currencies;
    let mut retry_attempts = 5;
    //TODO this retry logic should probably be ripped out.
    while retry_attempts > 0 {
        let balances_first = get_balances(&mut ctx, &request, address, currencies.clone()).await?;
        let checkpoint1 = get_checkpoint(&mut ctx).await?;
        let mut checkpoint2 = get_checkpoint(&mut ctx).await?;
        while checkpoint2 <= checkpoint1 {
            checkpoint2 = get_checkpoint(&mut ctx).await?;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        let balances_second = get_balances(&mut ctx, &request, address, currencies.clone()).await?;
        if balances_first.eq(&balances_second) {
            info!(
                "same balance for account {} at checkpoint {}",
                address, checkpoint2
            );
            return Ok(AccountBalanceResponse {
                block_identifier: ctx.blocks().create_block_identifier(checkpoint2).await?,
                balances: balances_first,
            });
        } else {
            info!(
                "different balance for account {} at checkpoint {}",
                address, checkpoint2
            );
            retry_attempts -= 1;
        }
    }
    Err(Error::RetryExhausted(String::from("retry")))
}

//TODO this can be a convenience method in the SDK.
async fn get_checkpoint(ctx: &mut OnlineServerContext) -> Result<CheckpointSequenceNumber, Error> {
    let request = GetCheckpointRequest {
        checkpoint_id: None, // None means get latest checkpoint
        read_mask: Some(FieldMask::from_paths([Checkpoint::SEQUENCE_NUMBER_FIELD])),
    };

    let response = ctx
        .grpc_client
        .ledger_client()
        .get_checkpoint(request)
        .await?
        .into_inner();

    let checkpoint = response
        .checkpoint
        .ok_or_else(|| Error::DataError("No checkpoint in GetCheckpoint response".to_string()))?;

    checkpoint
        .sequence_number
        .ok_or_else(|| Error::DataError("No sequence_number for checkpoint".to_string()))
}

//TODO this can be a convenience method in the SDK.
async fn get_current_epoch(grpc_client: &mut sui_rpc::client::Client) -> Result<u64, Error> {
    let request = GetEpochRequest {
        epoch: None, // None means get current epoch
        read_mask: Some(FieldMask::from_paths([CheckpointSummary::EPOCH_FIELD.name])),
    };

    let response = grpc_client
        .ledger_client()
        .get_epoch(request)
        .await?
        .into_inner();

    let epoch_info = response
        .epoch
        .ok_or_else(|| Error::DataError("No epoch in GetEpoch response".to_string()))?;

    epoch_info
        .epoch
        .ok_or_else(|| Error::DataError("No epoch number in epoch response".to_string()))
}

async fn get_balances(
    ctx: &mut OnlineServerContext,
    request: &AccountBalanceRequest,
    address: SuiAddress,
    currencies: Currencies,
) -> Result<Vec<Amount>, Error> {
    if let Some(sub_account) = &request.account_identifier.sub_account {
        let account_type = sub_account.account_type.clone();
        get_sub_account_balances(account_type, &mut ctx.grpc_client, address).await
    } else if !currencies.0.is_empty() {
        let balance_futures = currencies.0.iter().map(|currency| {
            let coin_type = currency.metadata.clone().coin_type.clone();
            let mut grpc_client = ctx.grpc_client.clone();
            async move {
                (
                    currency.clone(),
                    get_account_balances(&mut grpc_client, address, &coin_type).await,
                )
            }
        });
        let balances: Vec<(Currency, Result<i128, Error>)> = join_all(balance_futures).await;
        let mut amounts = Vec::new();
        for (currency, balance_result) in balances {
            match balance_result {
                Ok(value) => amounts.push(Amount::new(value, Some(currency))),
                Err(_e) => {
                    return Err(Error::InvalidInput(format!(
                        "{:?}",
                        currency.metadata.coin_type
                    )))
                }
            }
        }
        Ok(amounts)
    } else {
        Err(Error::InvalidInput(
            "Coin type is required for this request".to_string(),
        ))
    }
}

async fn get_account_balances(
    grpc_client: &mut sui_rpc::client::Client,
    address: SuiAddress,
    coin_type: &str,
) -> Result<i128, Error> {
    let request = GetBalanceRequest {
        owner: Some(address.to_string()),
        coin_type: Some(coin_type.to_string()),
    };

    let response = grpc_client.live_data_client().get_balance(request).await?;

    let balance = response
        .into_inner()
        .balance
        .and_then(|b| b.balance)
        .unwrap_or(0);

    Ok(balance as i128)
}

async fn get_sub_account_balances(
    account_type: SubAccountType,
    grpc_client: &mut sui_rpc::client::Client,
    address: SuiAddress,
) -> Result<Vec<Amount>, Error> {
    let current_epoch = get_current_epoch(grpc_client).await?;
    let address = Address::from(address);
    let delegated_stakes = grpc_client.list_delegated_stake(&address).await?;

    let amounts: Vec<SubBalance> = match account_type {
        SubAccountType::Stake => delegated_stakes
            .into_iter()
            .filter(|stake| current_epoch >= stake.activation_epoch)
            .map(|stake| SubBalance {
                stake_id: stake.staked_sui_id,
                validator: stake.validator_address,
                value: stake.principal as i128,
            })
            .collect(),
        SubAccountType::PendingStake => delegated_stakes
            .into_iter()
            .filter(|stake| current_epoch < stake.activation_epoch)
            .map(|stake| SubBalance {
                stake_id: stake.staked_sui_id,
                validator: stake.validator_address,
                value: stake.principal as i128,
            })
            .collect(),

        SubAccountType::EstimatedReward => delegated_stakes
            .into_iter()
            .filter(|stake| current_epoch >= stake.activation_epoch)
            .map(|stake| SubBalance {
                stake_id: stake.staked_sui_id,
                validator: stake.validator_address,
                value: stake.rewards as i128,
            })
            .collect(),
    };

    // Make sure there are always one amount returned
    Ok(if amounts.is_empty() {
        vec![Amount::new(0, None)]
    } else {
        vec![Amount::new_from_sub_balances(amounts)]
    })
}

/// Get an array of all unspent coins for an AccountIdentifier and the BlockIdentifier at which the lookup was performed. .
/// [Rosetta API Spec](https://docs.cdp.coinbase.com/api-reference/mesh/account/get-an-account-unspent-coins)
/// TODO This API is supposed to return coins of all types, not just SUI. It also has a 'currencies' parameter that we
/// are igorning which can be used to filter the type of coins that are returned.
pub async fn coins(
    State(mut context): State<OnlineServerContext>,
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<AccountCoinsRequest>, Error>,
) -> Result<AccountCoinsResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;

    //TODO bound this list.
    let mut coins = Vec::new();
    let mut page_token = None;

    loop {
        let list_request = ListOwnedObjectsRequest {
            owner: Some(request.account_identifier.address.to_string()),
            object_type: Some(SUI_COIN_TYPE.to_string()),
            page_size: Some(5000),
            page_token,
            read_mask: Some(FieldMask::from_paths([
                Object::OBJECT_ID_FIELD.name,
                Object::VERSION_FIELD.name,
                Object::BALANCE_FIELD.name,
            ])),
        };

        let response = context
            .grpc_client
            .live_data_client()
            .list_owned_objects(list_request)
            .await?
            .into_inner();

        for object in response.objects {
            if let (Some(object_id), Some(version), Some(balance)) =
                (object.object_id, object.version, object.balance)
            {
                let coin = Coin {
                    coin_identifier: CoinIdentifier {
                        identifier: CoinID {
                            id: ObjectID::from_hex_literal(&object_id).map_err(|e| {
                                Error::DataError(format!("Invalid object_id: {}", e))
                            })?,
                            version: SequenceNumber::from(version),
                        },
                    },
                    amount: Amount::new(balance as i128, None),
                };
                coins.push(coin);
            }
        }

        page_token = response.next_page_token;
        if page_token.is_none() {
            break;
        }
    }

    Ok(AccountCoinsResponse {
        block_identifier: context.blocks().current_block_identifier().await?,
        coins,
    })
}
