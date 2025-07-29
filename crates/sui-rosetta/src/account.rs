// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//! This module implements the [Rosetta Account API](https://www.rosetta-api.org/docs/AccountApi.html)
use axum::extract::State;
use axum::{Extension, Json};
use axum_extra::extract::WithRejection;
use futures::future::join_all;

use sui_sdk::SUI_COIN_TYPE;
use sui_types::base_types::SuiAddress;
use tracing::info;

use crate::errors::Error;
use crate::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountCoinsRequest, AccountCoinsResponse,
    Amount, Coin, Currencies, Currency, SubAccountType, SubBalance,
};
use crate::{OnlineServerContext, SuiEnv};
use std::time::Duration;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

/// Get an array of all AccountBalances for an AccountIdentifier and the BlockIdentifier
/// at which the balance lookup was performed.
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/AccountApi.html#accountbalance)
pub async fn balance(
    State(ctx): State<OnlineServerContext>,
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<AccountBalanceRequest>, Error>,
) -> Result<AccountBalanceResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let address = request.account_identifier.address;
    let currencies = &request.currencies;
    let mut retry_attempts = 5;
    while retry_attempts > 0 {
        let balances_first = get_balances(&ctx, &request, address, currencies.clone()).await?;
        let checkpoint1 = get_checkpoint(&ctx).await?;
        let mut checkpoint2 = get_checkpoint(&ctx).await?;
        while checkpoint2 <= checkpoint1 {
            checkpoint2 = get_checkpoint(&ctx).await?;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        let balances_second = get_balances(&ctx, &request, address, currencies.clone()).await?;
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

async fn get_checkpoint(ctx: &OnlineServerContext) -> Result<CheckpointSequenceNumber, Error> {
    ctx.client.get_latest_checkpoint().await
}

async fn get_balances(
    ctx: &OnlineServerContext,
    request: &AccountBalanceRequest,
    address: SuiAddress,
    currencies: Currencies,
) -> Result<Vec<Amount>, Error> {
    if let Some(sub_account) = &request.account_identifier.sub_account {
        let account_type = sub_account.account_type.clone();
        get_sub_account_balances(account_type, ctx, address).await
    } else if !currencies.0.is_empty() {
        let balance_futures = currencies.0.iter().map(|currency| {
            let coin_type = currency.metadata.clone().coin_type.clone();
            async move {
                (
                    currency.clone(),
                    get_account_balances(ctx, address, &coin_type).await,
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
    ctx: &OnlineServerContext,
    address: SuiAddress,
    coin_type: &String,
) -> Result<i128, Error> {
    Ok(ctx
        .client
        .get_balance(address, coin_type.to_string())
        .await? as i128)
}

async fn get_sub_account_balances(
    account_type: SubAccountType,
    ctx: &OnlineServerContext,
    address: SuiAddress,
) -> Result<Vec<Amount>, Error> {
    // Get current epoch data for validator info and to determine stake status
    let epoch_data = ctx.client.get_epoch(None).await?;
    let current_epoch = epoch_data
        .epoch
        .ok_or_else(|| Error::DataError("Missing epoch number".to_string()))?;

    // Extract validator info from epoch data
    let validator_map = ctx.client.extract_validator_info_from_epoch(&epoch_data)?;

    // Collect all StakedSui objects
    let mut all_staked_objects = Vec::new();
    let mut cursor = None;

    loop {
        let response = ctx
            .client
            .list_owned_objects(
                address,
                Some("0x3::staking_pool::StakedSui".to_string()),
                cursor,
            )
            .await?;

        for object in response.objects {
            if let Some((staked_sui, object_id, validator_address)) = ctx
                .client
                .parse_staked_sui_from_proto(&object, &validator_map)?
            {
                all_staked_objects.push((staked_sui, object_id, validator_address));
            }
        }

        if response.next_page_token.is_none() {
            break;
        }
        cursor = response.next_page_token;
    }

    let amounts: Vec<SubBalance> = match account_type {
        SubAccountType::Stake => {
            all_staked_objects
                .into_iter()
                .filter_map(|(staked_sui, object_id, validator_address)| {
                    // Active stakes are those where activation epoch <= current epoch
                    if staked_sui.activation_epoch() <= current_epoch {
                        Some(SubBalance {
                            stake_id: object_id,
                            validator: validator_address,
                            value: staked_sui.principal() as i128,
                        })
                    } else {
                        None
                    }
                })
                .collect()
        }
        SubAccountType::PendingStake => {
            all_staked_objects
                .into_iter()
                .filter_map(|(staked_sui, object_id, validator_address)| {
                    // Pending stakes are those where activation epoch > current epoch
                    if staked_sui.activation_epoch() > current_epoch {
                        Some(SubBalance {
                            stake_id: object_id,
                            validator: validator_address,
                            value: staked_sui.principal() as i128,
                        })
                    } else {
                        None
                    }
                })
                .collect()
        }
        SubAccountType::EstimatedReward => {
            // For estimated rewards, calculate based on active stakes using exchange rates
            all_staked_objects
                .into_iter()
                .filter_map(|(staked_sui, object_id, validator_address)| {
                    // Only calculate rewards for active stakes
                    if staked_sui.activation_epoch() <= current_epoch {
                        // Look up the validator info to get the exchange rate
                        let pool_id = staked_sui.pool_id();
                        if let Some(validator_info) = validator_map.get(&pool_id) {
                            let principal = staked_sui.principal() as f64;
                            let exchange_rate = validator_info.exchange_rate;

                            // Calculate estimated reward: (principal * exchange_rate) - principal
                            let estimated_value = principal * exchange_rate;
                            let estimated_reward = (estimated_value - principal) as i128;

                            Some(SubBalance {
                                stake_id: object_id,
                                validator: validator_address,
                                value: estimated_reward,
                            })
                        } else {
                            // If validator not found, return 0 reward
                            Some(SubBalance {
                                stake_id: object_id,
                                validator: validator_address,
                                value: 0,
                            })
                        }
                    } else {
                        None
                    }
                })
                .collect()
        }
    };

    // Make sure there are always one amount returned
    Ok(if amounts.is_empty() {
        vec![Amount::new(0, None)]
    } else {
        vec![Amount::new_from_sub_balances(amounts)]
    })
}

/// Get an array of all unspent coins for an AccountIdentifier and the BlockIdentifier at which the lookup was performed. .
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/AccountApi.html#accountcoins)
pub async fn coins(
    State(context): State<OnlineServerContext>,
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<AccountCoinsRequest>, Error>,
) -> Result<AccountCoinsResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;

    let mut coins = Vec::new();
    let mut cursor = None;

    loop {
        let response = context
            .client
            .list_owned_objects(
                request.account_identifier.address,
                Some(SUI_COIN_TYPE.to_string()),
                cursor,
            )
            .await?;

        for object in response.objects {
            if let Some(coin) = grpc_object_to_coin(object)? {
                coins.push(coin);
            }
        }

        if response.next_page_token.is_none() {
            break;
        }
        cursor = response.next_page_token;
    }

    Ok(AccountCoinsResponse {
        block_identifier: context.blocks().current_block_identifier().await?,
        coins,
    })
}

fn grpc_object_to_coin(
    object: sui_rpc::proto::sui::rpc::v2beta2::Object,
) -> Result<Option<Coin>, Error> {
    let object_id = object
        .object_id
        .ok_or_else(|| Error::DataError("Missing object_id".to_string()))?
        .parse()
        .map_err(|e| Error::DataError(format!("Invalid object_id: {}", e)))?;

    let version = object
        .version
        .ok_or_else(|| Error::DataError("Missing version".to_string()))?;

    let balance = object.balance.unwrap_or(0);

    Ok(Some(Coin {
        coin_identifier: crate::types::CoinIdentifier {
            identifier: crate::types::CoinID {
                id: object_id,
                version: version.into(),
            },
        },
        amount: crate::types::Amount {
            value: balance as i128,
            currency: crate::SUI.clone(),
            metadata: None,
        },
    }))
}
