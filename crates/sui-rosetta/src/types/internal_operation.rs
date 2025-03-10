// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use sui_json_rpc_types::{SuiExecutionStatus, SuiTransactionBlockEffectsAPI};
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::digests::ObjectDigest;
use sui_types::transaction::{ProgrammableTransaction, TransactionData};

use crate::errors::Error;
use crate::types::ConstructionMetadata;
use pay_coin::pay_coin_pt;
pub use pay_coin::PayCoin;
use pay_sui::pay_sui_pt;
pub use pay_sui::PaySui;
use stake::stake_pt;
pub use stake::Stake;
use withdraw_stake::withdraw_stake_pt;
pub use withdraw_stake::WithdrawStake;

mod pay_coin;
mod pay_sui;
mod stake;
mod withdraw_stake;

pub const MAX_GAS_COINS: usize = 255;
const MAX_COMMAND_ARGS: usize = 511;
const MAX_GAS_BUDGET: u64 = 50_000_000_000;
/// Minimum gas-units a tx might need
const START_GAS_UNITS: u64 = 1_000;

pub struct TransactionObjectData {
    pub gas_coins: Vec<ObjectRef>,
    pub extra_gas_coins: Vec<ObjectRef>,
    pub objects: Vec<ObjectRef>,
    /// Refers to the sum of the `Coin<SUI>` balance of the coins participating in the transaction;
    /// either as gas or as objects.
    pub total_sui_balance: i128,
    pub budget: u64,
}

#[async_trait]
#[enum_dispatch]
pub trait TryConstructTransaction {
    async fn try_fetch_needed_objects(
        self,
        client: &SuiClient,
        gas_price: Option<u64>,
        budget: Option<u64>,
    ) -> Result<TransactionObjectData, Error>;
}

#[enum_dispatch(TryConstructTransaction)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum InternalOperation {
    PaySui(PaySui),
    PayCoin(PayCoin),
    Stake(Stake),
    WithdrawStake(WithdrawStake),
}

impl InternalOperation {
    pub fn sender(&self) -> SuiAddress {
        match self {
            InternalOperation::PaySui(PaySui { sender, .. })
            | InternalOperation::PayCoin(PayCoin { sender, .. })
            | InternalOperation::Stake(Stake { sender, .. })
            | InternalOperation::WithdrawStake(WithdrawStake { sender, .. }) => *sender,
        }
    }

    /// Combine with ConstructionMetadata to form the TransactionData
    pub fn try_into_data(self, metadata: ConstructionMetadata) -> Result<TransactionData, Error> {
        let pt = match self {
            Self::PaySui(PaySui {
                recipients,
                amounts,
                ..
            }) => pay_sui_pt(recipients, amounts, &metadata.extra_gas_coins)?,
            Self::PayCoin(PayCoin {
                recipients,
                amounts,
                ..
            }) => {
                let currency = &metadata
                    .currency
                    .ok_or(anyhow!("metadata.coin_type is needed to PayCoin"))?;
                pay_coin_pt(recipients, amounts, &metadata.objects, currency)?
            }
            InternalOperation::Stake(Stake {
                validator, amount, ..
            }) => {
                let (stake_all, amount) = match amount {
                    Some(amount) => (false, amount),
                    None => {
                        if (metadata.total_coin_value - metadata.budget as i128) < 0 {
                            return Err(anyhow!(
                                "ConstructionMetadata malformed. total_coin_value - budget < 0"
                            )
                            .into());
                        }
                        (true, metadata.total_coin_value as u64 - metadata.budget)
                    }
                };
                stake_pt(validator, amount, stake_all, &metadata.extra_gas_coins)?
            }
            InternalOperation::WithdrawStake(WithdrawStake { stake_ids, .. }) => {
                let withdraw_all = stake_ids.is_empty();
                withdraw_stake_pt(metadata.objects, withdraw_all)?
            }
        };

        Ok(TransactionData::new_programmable(
            metadata.sender,
            metadata.gas_coins,
            pt,
            metadata.budget,
            metadata.gas_price,
        ))
    }
}

async fn budget_from_dry_run(
    client: &SuiClient,
    pt: ProgrammableTransaction,
    sender: SuiAddress,
    gas_price: Option<u64>,
) -> Result<u64, Error> {
    let gas_price = match gas_price {
        Some(p) => p,
        None => client.governance_api().get_reference_gas_price().await? + 100, // make sure it works over epoch changes
    };
    // We don't want dry run to fail due to budget, so we leave coins empty and set MAX_GAS_BUDGET
    let dry_run = client
        .read_api()
        .dry_run_transaction_block(TransactionData::new_programmable(
            sender,
            vec![],
            pt.clone(),
            MAX_GAS_BUDGET,
            gas_price,
        ))
        .await?;
    let effects = dry_run.effects;

    if let SuiExecutionStatus::Failure { error } = effects.status() {
        return Err(Error::TransactionDryRunError(error.to_string()));
    }
    // Update budget to be the result of the dry run
    Ok(effects.gas_cost_summary().computation_cost + effects.gas_cost_summary().storage_cost)
}

async fn collect_coins_until_budget_met(
    client: &SuiClient,
    sender: SuiAddress,
    pt: impl Fn(&[(ObjectID, SequenceNumber, ObjectDigest)]) -> anyhow::Result<ProgrammableTransaction>,
    amount: u64,
    gas_price: Option<u64>,
) -> Result<TransactionObjectData, Error> {
    let mut coins_stream = Box::pin(client.coin_read_api().get_coins_stream(sender, None));
    // Fetch it once instead of fetching it again and again in the below loop.
    let gas_price = match gas_price {
        Some(p) => p,
        None => client.governance_api().get_reference_gas_price().await? + 100, // make sure it works over epoch changes
    };

    let mut all_coins = vec![];
    let mut gas_coins: Vec<_>;
    let mut extra_gas_coins: Vec<_>;
    let mut gathered = 0;
    let mut budget = START_GAS_UNITS * gas_price;
    // We need to dry-run in a loop, because depending on the amount of coins used the tx might
    // differ slightly: (merge / no merge / number of merge-coins)
    loop {
        while let Some(coin) = coins_stream.next().await {
            gathered += coin.balance;
            all_coins.push(coin);
            if gathered >= amount + budget {
                break;
            }
        }
        if gathered < amount + budget {
            return Err(Error::InvalidInput(format!(
                "Address {sender} does not have amount: {amount} + budget: {budget} balance. SUI balance: {gathered}."
            )));
        }

        // The coins to merge should be used as transaction object inputs, as
        // `TransactionData::new_programmable` used in `InternalOperation::try_into_data`,
        // uses all coins passed as gas payment.
        let mut iter = all_coins.iter().map(|c| c.object_ref());
        gas_coins = iter.by_ref().take(MAX_GAS_COINS).collect();
        extra_gas_coins = iter.collect();
        let pt = pt(&extra_gas_coins)?;
        budget = budget_from_dry_run(client, pt.clone(), sender, Some(gas_price)).await?;
        // If we have already gathered the needed amount of coins we don't need to dry run again,
        // as the transaction will be the same.
        if budget + amount <= gathered {
            break;
        }
    }

    let total_sui_balance = all_coins.iter().map(|c| c.balance).sum::<u64>() as i128;
    Ok(TransactionObjectData {
        gas_coins,
        extra_gas_coins,
        objects: vec![],
        total_sui_balance,
        budget,
    })
}
