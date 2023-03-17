// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//! This module implements the [Rosetta Account API](https://www.rosetta-api.org/docs/AccountApi.html)

use axum::extract::State;
use axum::{Extension, Json};
use axum_extra::extract::WithRejection;
use futures::StreamExt;

use sui_sdk::rpc_types::StakeStatus;
use sui_sdk::{SuiClient, SUI_COIN_TYPE};
use sui_types::base_types::SuiAddress;

use crate::errors::Error;
use crate::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountCoinsRequest, AccountCoinsResponse,
    Amount, Coin, SubAccount, SubAccountType,
};
use crate::{OnlineServerContext, SuiEnv};

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
    if let Some(SubAccount { account_type }) = request.account_identifier.sub_account {
        let balances = get_sub_account_balances(account_type, &ctx.client, address).await?;
        Ok(AccountBalanceResponse {
            block_identifier: ctx.blocks().current_block_identifier().await?,
            balances,
        })
    } else {
        let block_identifier = if let Some(index) = request.block_identifier.index {
            let response = ctx.blocks().get_block_by_index(index).await?;
            response.block.block_identifier
        } else if let Some(hash) = request.block_identifier.hash {
            let response = ctx.blocks().get_block_by_hash(hash).await?;
            response.block.block_identifier
        } else {
            ctx.blocks().current_block_identifier().await?
        };

        ctx.blocks()
            .get_balance_at_block(address, block_identifier.index)
            .await
            .map(|balance| AccountBalanceResponse {
                block_identifier,
                balances: vec![Amount::new(balance)],
            })
    }
}

async fn get_sub_account_balances(
    account_type: SubAccountType,
    client: &SuiClient,
    address: SuiAddress,
) -> Result<Vec<Amount>, Error> {
    let mut amounts = match account_type {
        SubAccountType::Stake => {
            let delegations = client.governance_api().get_stakes(address).await?;
            delegations.into_iter().fold(vec![], |mut amounts, stakes| {
                for stake in &stakes.stakes {
                    if let StakeStatus::Active { .. } = stake.status {
                        amounts.push(Amount::new_stake(
                            stake.principal as i128,
                            stake.staked_sui_id,
                            stakes.validator_address,
                        ));
                    }
                }
                amounts
            })
        }
        SubAccountType::PendingStake => {
            let delegations = client.governance_api().get_stakes(address).await?;
            delegations.into_iter().fold(vec![], |mut amounts, stakes| {
                for stake in &stakes.stakes {
                    if let StakeStatus::Pending = stake.status {
                        amounts.push(Amount::new_stake(
                            stake.principal as i128,
                            stake.staked_sui_id,
                            stakes.validator_address,
                        ));
                    }
                }
                amounts
            })
        }

        SubAccountType::EstimatedReward => {
            let delegations = client.governance_api().get_stakes(address).await?;
            delegations.into_iter().fold(vec![], |mut amounts, stakes| {
                for stake in &stakes.stakes {
                    if let StakeStatus::Active { estimated_reward } = stake.status {
                        amounts.push(Amount::new_stake(
                            estimated_reward as i128,
                            stake.staked_sui_id,
                            stakes.validator_address,
                        ));
                    }
                }
                amounts
            })
        }
    };

    // Make sure there are always one amount returned
    if amounts.is_empty() {
        amounts.push(Amount::new(0))
    }

    Ok(amounts)
}

/// Get an array of all unspent coins for an AccountIdentifier and the BlockIdentifier at which the lookup was performed. .
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/AccountApi.html#accountcoins)
pub async fn coins(
    State(context): State<OnlineServerContext>,
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<AccountCoinsRequest>, Error>,
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
