// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//! This module implements the [Mesh Account API](https://docs.cdp.coinbase.com/mesh/mesh-api-spec/api-reference#account)
use axum::extract::State;
use axum::{Extension, Json};
use axum_extra::extract::WithRejection;
use futures::{TryStreamExt, future::join_all};

use prost_types::FieldMask;
use std::str::FromStr;
use sui_rpc::client::Client;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{
    GetBalanceRequest, GetCheckpointRequest, GetEpochRequest, ListOwnedObjectsRequest,
};
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

/// BCS layout for `0x3::staking_pool::FungibleStakedSui`.
/// Field order must match the Move struct definition exactly (BCS is positional).
/// See: crates/sui-framework/packages/sui-system/sources/staking_pool.move
#[derive(serde::Deserialize)]
pub(crate) struct FungibleStakedSuiBcs {
    pub _id: Address,
    pub pool_id: Address,
    pub value: u64,
}

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

struct EpochTimingInfo {
    epoch: u64,
    epoch_start_timestamp_ms: u64,
    epoch_duration_ms: u64,
}

async fn get_epoch_timing(client: &mut Client) -> Result<EpochTimingInfo, Error> {
    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths([
        "epoch",
        "system_state.epoch_start_timestamp_ms",
        "system_state.parameters.epoch_duration_ms",
    ]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await?
        .into_inner();
    let epoch_info = response.epoch();
    let system_state = epoch_info.system_state();
    Ok(EpochTimingInfo {
        epoch: epoch_info.epoch(),
        epoch_start_timestamp_ms: system_state.epoch_start_timestamp_ms(),
        epoch_duration_ms: system_state.parameters().epoch_duration_ms(),
    })
}

/// Exchange rate info for a staking pool.
pub(crate) struct PoolRateInfo {
    pub sui_balance: u64,
    pub pool_token_balance: u64,
    pub validator_address: Address,
    /// `pool.extra_fields.id` — the Bag's UID, needed by callers that want to
    /// derive dynamic field ids inside the pool (e.g.,
    /// `FungibleStakedSuiData`). `None` if the proto omitted the field.
    pub pool_extra_fields_id: Option<String>,
}

/// Reads exchange rates for all active validator staking pools.
pub(crate) async fn get_pool_exchange_rates(
    client: &mut Client,
) -> Result<std::collections::HashMap<String, PoolRateInfo>, Error> {
    Ok(get_pool_exchange_rates_with_epoch(client).await?.0)
}

/// Atomic snapshot of validator-set state from a single `GetEpochRequest::latest()`.
///
/// Splitting these reads across multiple RPCs allows an epoch transition to
/// land between them: the caller could observe rate from epoch N, then read
/// epoch N+1, and bind the transaction to N+1 with a stale N rate, silently
/// violating AtMost caps and aborting AtLeast guards. Bundling rate, epoch,
/// and the inactive-table id into one response eliminates that race.
pub(crate) struct ValidatorSetSnapshot {
    /// Active pool rates keyed by `pool.id` (canonical 0x-prefixed hex string).
    pub active_rates: std::collections::HashMap<String, PoolRateInfo>,
    /// Current epoch the snapshot was taken in.
    pub epoch: u64,
    /// `validators.inactive_validators.id` — UID of the
    /// `Table<ID, ValidatorWrapper>` storing deactivated pools. `None` only if
    /// the proto omitted the field.
    pub inactive_validators_table_id: Option<String>,
}

/// Reads exchange rates and the epoch they're snapshotted in from a single
/// `GetEpochRequest::latest()` response. Used by amount-sensitive operations
/// (e.g. `MergeAndRedeemFungibleStakedSui::AtLeast`/`AtMost`) that must pin
/// the rate quote to the same epoch the resulting transaction will be bound to.
pub(crate) async fn get_pool_exchange_rates_with_epoch(
    client: &mut Client,
) -> Result<(std::collections::HashMap<String, PoolRateInfo>, u64), Error> {
    let snap = get_validator_set_snapshot(client).await?;
    Ok((snap.active_rates, snap.epoch))
}

/// Read the full validator-set snapshot atomically.
pub(crate) async fn get_validator_set_snapshot(
    client: &mut Client,
) -> Result<ValidatorSetSnapshot, Error> {
    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths([
        "epoch",
        "system_state.validators.active_validators",
        "system_state.validators.inactive_validators",
    ]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await?
        .into_inner();
    let epoch_obj = response.epoch();
    let epoch = epoch_obj.epoch();
    let system_state = epoch_obj.system_state();
    let validators_proto = system_state.validators();
    let validators = validators_proto.active_validators();

    let inactive_validators_table_id = validators_proto
        .inactive_validators
        .as_ref()
        .and_then(|t| t.id.as_ref())
        .cloned();

    let mut rates = std::collections::HashMap::new();
    for validator in validators {
        let pool = validator.staking_pool();
        let pool_id = pool.id().to_string();
        let validator_address = Address::from_str(validator.address())
            .map_err(|e| Error::DataError(format!("Invalid validator address: {}", e)))?;
        let pool_extra_fields_id = pool
            .extra_fields_opt()
            .and_then(|t| t.id_opt())
            .map(|s| s.to_string());
        rates.insert(
            pool_id,
            PoolRateInfo {
                sui_balance: pool.sui_balance(),
                pool_token_balance: pool.pool_token_balance(),
                validator_address,
                pool_extra_fields_id,
            },
        );
    }
    Ok(ValidatorSetSnapshot {
        active_rates: rates,
        epoch,
        inactive_validators_table_id,
    })
}

async fn get_sub_account_balances(
    account_type: SubAccountType,
    client: &mut Client,
    address: SuiAddress,
) -> Result<Vec<Amount>, Error> {
    let epoch_timing = get_epoch_timing(client).await?;
    let current_epoch = epoch_timing.epoch;
    let address = Address::from(address);

    if matches!(account_type, SubAccountType::FungibleStakedSuiValue) {
        return get_fungible_staked_sui_value(client, address, &epoch_timing).await;
    }

    let delegated_stakes = client.list_delegated_stake(&address).await?;

    let amounts: Vec<SubBalance> = match account_type {
        SubAccountType::Stake => delegated_stakes
            .into_iter()
            .filter(|stake| current_epoch >= stake.activation_epoch)
            .map(|stake| SubBalance {
                stake_id: stake.staked_sui_id,
                validator: stake.validator_address,
                value: stake.principal as i128,
                activation_epoch: Some(stake.activation_epoch),
            })
            .collect(),
        SubAccountType::PendingStake => delegated_stakes
            .into_iter()
            .filter(|stake| current_epoch < stake.activation_epoch)
            .map(|stake| SubBalance {
                stake_id: stake.staked_sui_id,
                validator: stake.validator_address,
                value: stake.principal as i128,
                activation_epoch: Some(stake.activation_epoch),
            })
            .collect(),
        SubAccountType::EstimatedReward => delegated_stakes
            .into_iter()
            .filter(|stake| current_epoch >= stake.activation_epoch)
            .map(|stake| SubBalance {
                stake_id: stake.staked_sui_id,
                validator: stake.validator_address,
                value: stake.rewards as i128,
                activation_epoch: None,
            })
            .collect(),
        SubAccountType::FungibleStakedSuiValue => unreachable!(),
    };

    let amount = if amounts.is_empty() {
        Amount::new(0, None)
    } else {
        Amount::new_from_sub_balances(amounts)
    };

    Ok(vec![amount.with_epoch_timing(
        epoch_timing.epoch,
        epoch_timing.epoch_start_timestamp_ms,
        epoch_timing.epoch_duration_ms,
    )])
}

async fn get_fungible_staked_sui_value(
    client: &mut Client,
    address: Address,
    epoch_timing: &EpochTimingInfo,
) -> Result<Vec<Amount>, Error> {
    use futures::TryStreamExt;

    let list_request = ListOwnedObjectsRequest::default()
        .with_owner(address.to_string())
        .with_object_type("0x3::staking_pool::FungibleStakedSui".to_string())
        .with_page_size(1000u32)
        .with_read_mask(FieldMask::from_paths(["object_id", "contents"]));

    let fss_objects: Vec<_> = client
        .list_owned_objects(list_request)
        .map_err(Error::from)
        .try_collect()
        .await?;

    if fss_objects.is_empty() {
        return Ok(vec![Amount::new(0, None).with_epoch_timing(
            epoch_timing.epoch,
            epoch_timing.epoch_start_timestamp_ms,
            epoch_timing.epoch_duration_ms,
        )]);
    }

    let pool_rates = get_pool_exchange_rates(client).await?;

    let mut sub_balances = Vec::new();
    for obj in &fss_objects {
        let contents = obj
            .contents
            .as_ref()
            .ok_or_else(|| Error::DataError("FungibleStakedSui missing contents".to_string()))?;
        let fss: FungibleStakedSuiBcs = contents.deserialize().map_err(|e| {
            Error::DataError(format!("Failed to deserialize FungibleStakedSui: {}", e))
        })?;

        let pool_id_str = fss.pool_id.to_string();
        let rate = pool_rates.get(&pool_id_str).ok_or_else(|| {
            Error::DataError(format!("No exchange rate found for pool {}", pool_id_str))
        })?;

        let sui_equivalent = if rate.pool_token_balance > 0 {
            (fss.value as u128 * rate.sui_balance as u128 / rate.pool_token_balance as u128) as u64
        } else {
            fss.value
        };

        sub_balances.push(SubBalance {
            stake_id: Address::from_str(obj.object_id())
                .map_err(|e| Error::DataError(format!("Invalid FSS object_id: {}", e)))?,
            validator: rate.validator_address,
            value: sui_equivalent as i128,
            activation_epoch: None,
        });
    }

    let amount = if sub_balances.is_empty() {
        Amount::new(0, None)
    } else {
        Amount::new_from_sub_balances(sub_balances)
    };

    Ok(vec![amount.with_epoch_timing(
        epoch_timing.epoch,
        epoch_timing.epoch_start_timestamp_ms,
        epoch_timing.epoch_duration_ms,
    )])
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
