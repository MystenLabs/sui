// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//! This module implements the [Mesh Account API](https://docs.cdp.coinbase.com/mesh/mesh-api-spec/api-reference#account)
use axum::extract::State;
use axum::{Extension, Json};
use axum_extra::extract::WithRejection;
use futures::{TryStreamExt, future::join_all};

use prost_types::FieldMask;
use sui_rpc::client::Client;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{
    GetBalanceRequest, GetCheckpointRequest, ListOwnedObjectsRequest,
};
use std::str::FromStr;
use sui_sdk_types::{Address, StructTag};
use sui_types::base_types::SuiAddress;

use crate::errors::Error;
use crate::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountCoinsRequest, AccountCoinsResponse,
    Amount, Coin, CoinID, CoinIdentifier, Currencies, Currency, SubAccountType, SubBalance,
};
use crate::{OnlineServerContext, SuiEnv};
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

/// Get an array of all AccountBalances for an AccountIdentifier and the BlockIdentifier
/// at which the balance lookup was performed.
/// [Mesh API Spec](https://docs.cdp.coinbase.com/api-reference/mesh/account/get-an-account-balance)
pub async fn balance(
    State(mut ctx): State<OnlineServerContext>,
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<AccountBalanceRequest>, Error>,
) -> Result<AccountBalanceResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let address = request.account_identifier.address;
    let currencies = &request.currencies;

    let checkpoint = get_checkpoint(&mut ctx).await?;
    let balances = get_balances(&mut ctx, &request, address, currencies.clone()).await?;

    Ok(AccountBalanceResponse {
        block_identifier: ctx.blocks().create_block_identifier(checkpoint).await?,
        balances,
    })
}

async fn get_checkpoint(ctx: &mut OnlineServerContext) -> Result<CheckpointSequenceNumber, Error> {
    let request =
        GetCheckpointRequest::latest().with_read_mask(FieldMask::from_paths(["sequence_number"]));

    Ok(ctx
        .client
        .ledger_client()
        .get_checkpoint(request)
        .await?
        .into_inner()
        .checkpoint()
        .sequence_number())
}

async fn get_balances(
    ctx: &mut OnlineServerContext,
    request: &AccountBalanceRequest,
    address: SuiAddress,
    currencies: Currencies,
) -> Result<Vec<Amount>, Error> {
    if let Some(sub_account) = &request.account_identifier.sub_account {
        let account_type = sub_account.account_type.clone();
        get_sub_account_balances(account_type, &mut ctx.client, address).await
    } else if !currencies.0.is_empty() {
        let balance_futures = currencies.0.iter().map(|currency| {
            let coin_type = currency.metadata.clone().coin_type.clone();
            let mut client = ctx.client.clone();
            async move {
                (
                    currency.clone(),
                    get_account_balances(&mut client, address, &coin_type).await,
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
                    )));
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
    client: &mut Client,
    address: SuiAddress,
    coin_type: &str,
) -> Result<i128, Error> {
    let request = GetBalanceRequest::default()
        .with_owner(address.to_string())
        .with_coin_type(coin_type.to_string());
    let response = client.state_client().get_balance(request).await?;
    Ok(response.into_inner().balance().balance() as i128)
}

async fn get_sub_account_balances(
    account_type: SubAccountType,
    client: &mut Client,
    address: SuiAddress,
) -> Result<Vec<Amount>, Error> {
    let current_epoch = crate::get_current_epoch(client).await?;
    let address = Address::from(address);
    let delegated_stakes = client.list_delegated_stake(&address).await?;

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

        SubAccountType::FungibleStake => {
            return get_fungible_stake_balances(client, address).await;
        }

        SubAccountType::AllStakes => {
            let staked_sui: Vec<SubBalance> = delegated_stakes
                .into_iter()
                .filter(|stake| current_epoch >= stake.activation_epoch)
                .map(|stake| SubBalance {
                    stake_id: stake.staked_sui_id,
                    validator: stake.validator_address,
                    value: stake.principal as i128,
                })
                .collect();

            let fungible_result = get_fungible_stake_balances(client, address).await?;
            let fungible_subs: Vec<SubBalance> = fungible_result
                .into_iter()
                .flat_map(|amount| {
                    amount
                        .metadata
                        .map(|m| m.sub_balances)
                        .unwrap_or_default()
                })
                .collect();

            let mut all: Vec<SubBalance> = staked_sui;
            all.extend(fungible_subs);

            return Ok(if all.is_empty() {
                vec![Amount::new(0, None)]
            } else {
                vec![Amount::new_from_sub_balances(all)]
            });
        }
    };

    Ok(if amounts.is_empty() {
        vec![Amount::new(0, None)]
    } else {
        vec![Amount::new_from_sub_balances(amounts)]
    })
}

async fn get_fungible_stake_balances(
    client: &mut Client,
    address: Address,
) -> Result<Vec<Amount>, Error> {
    let request = ListOwnedObjectsRequest::default()
        .with_owner(address.to_string())
        .with_object_type("0x3::staking_pool::FungibleStakedSui".to_string())
        .with_page_size(1000u32)
        .with_read_mask(FieldMask::from_paths(["object_id", "bcs"]));

    let objects: Vec<_> = client
        .clone()
        .list_owned_objects(request)
        .map_err(Error::from)
        .and_then(|object| async move {
            let object_id = ObjectID::from_str(object.object_id())
                .map_err(|e| Error::DataError(format!("Invalid object_id: {}", e)))?;

            let bcs_data = object
                .bcs
                .as_ref()
                .ok_or_else(|| Error::DataError("BCS data missing for object".to_string()))?;

            let sui_object: sui_types::object::Object =
                bcs_data.deserialize().map_err(|e| {
                    Error::DataError(format!("Failed to deserialize object BCS: {}", e))
                })?;

            let move_object = sui_object.data.try_as_move().ok_or_else(|| {
                Error::DataError("FungibleStakedSui is not a Move object".to_string())
            })?;

            let content: FungibleStakedSuiContent =
                bcs::from_bytes(move_object.contents()).map_err(|e| {
                    Error::DataError(format!(
                        "Failed to parse FungibleStakedSui content: {}",
                        e
                    ))
                })?;

            let pool_object_id: ObjectID = content.pool_id.bytes;
            Ok(SubBalance {
                stake_id: object_id.into(),
                validator: pool_object_id.into(),
                value: content.value as i128,
            })
        })
        .try_collect()
        .await?;

    Ok(if objects.is_empty() {
        vec![Amount::new(0, None)]
    } else {
        vec![Amount::new_from_sub_balances(objects)]
    })
}

#[derive(serde::Deserialize)]
struct FungibleStakedSuiContent {
    _id: sui_types::id::UID,
    pool_id: sui_types::id::ID,
    value: u64,
}

/// Get an array of all unspent coins for an AccountIdentifier and the BlockIdentifier at which the lookup was performed. .
/// [Mesh API Spec](https://docs.cdp.coinbase.com/api-reference/mesh/account/get-an-account-unspent-coins)
/// TODO This API is supposed to return coins of all types, not just SUI. It also has a 'currencies' parameter that we
/// are igorning which can be used to filter the type of coins that are returned.
pub async fn coins(
    State(context): State<OnlineServerContext>,
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<AccountCoinsRequest>, Error>,
) -> Result<AccountCoinsResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;

    let coin_request = ListOwnedObjectsRequest::default()
        .with_owner(request.account_identifier.address.to_string())
        .with_object_type(StructTag::gas_coin().to_string())
        .with_page_size(5000u32)
        .with_read_mask(FieldMask::from_paths(["object_id", "version", "balance"]));

    let coins = context
        .client
        .list_owned_objects(coin_request)
        .map_err(Error::from)
        .and_then(|object| async move {
            Ok(Coin {
                coin_identifier: CoinIdentifier {
                    identifier: CoinID {
                        id: ObjectID::from_hex_literal(object.object_id())
                            .map_err(|e| Error::DataError(format!("Invalid object_id: {}", e)))?,
                        version: SequenceNumber::from(object.version()),
                    },
                },
                amount: Amount::new(object.balance() as i128, None),
            })
        })
        .try_collect()
        .await?;

    Ok(AccountCoinsResponse {
        block_identifier: context.blocks().current_block_identifier().await?,
        coins,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_types::id::{ID, UID};

    #[test]
    fn test_fungible_staked_sui_content_bcs_parsing() {
        let object_id = ObjectID::random();
        let pool_id = ObjectID::random();
        let value: u64 = 1_500_000_000;

        let uid = UID::new(object_id);
        let id = ID::new(pool_id);

        let bcs_bytes = bcs::to_bytes(&(uid, id, value)).unwrap();
        let parsed: FungibleStakedSuiContent = bcs::from_bytes(&bcs_bytes).unwrap();

        assert_eq!(parsed.pool_id.bytes, pool_id);
        assert_eq!(parsed.value, value);
    }

    #[test]
    fn test_fungible_staked_sui_content_zero_value() {
        let uid = UID::new(ObjectID::random());
        let id = ID::new(ObjectID::random());
        let value: u64 = 0;

        let bcs_bytes = bcs::to_bytes(&(uid, id, value)).unwrap();
        let parsed: FungibleStakedSuiContent = bcs::from_bytes(&bcs_bytes).unwrap();

        assert_eq!(parsed.value, 0);
    }

    #[test]
    fn test_fungible_staked_sui_content_max_value() {
        let uid = UID::new(ObjectID::random());
        let id = ID::new(ObjectID::random());
        let value: u64 = u64::MAX;

        let bcs_bytes = bcs::to_bytes(&(uid, id, value)).unwrap();
        let parsed: FungibleStakedSuiContent = bcs::from_bytes(&bcs_bytes).unwrap();

        assert_eq!(parsed.value, u64::MAX);
    }

    #[test]
    fn test_fungible_staked_sui_content_invalid_bcs() {
        let result: Result<FungibleStakedSuiContent, _> = bcs::from_bytes(&[0u8; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn test_fungible_staked_sui_pool_id_extraction() {
        let pool_id = ObjectID::random();
        let uid = UID::new(ObjectID::random());
        let id = ID::new(pool_id);
        let value: u64 = 500_000_000;

        let bcs_bytes = bcs::to_bytes(&(uid, id, value)).unwrap();
        let parsed: FungibleStakedSuiContent = bcs::from_bytes(&bcs_bytes).unwrap();

        let extracted_pool_id: ObjectID = parsed.pool_id.bytes;
        assert_eq!(extracted_pool_id, pool_id);

        let address: sui_sdk_types::Address = extracted_pool_id.into();
        assert_eq!(address.to_string(), pool_id.to_hex_literal());
    }
}
