// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use pay_coin::pay_coin_pt;
use pay_sui::pay_sui_pt;
use serde::{Deserialize, Serialize};

use sui_json_rpc_types::Coin;
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::governance::ADD_STAKE_FUN_NAME;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{
    Argument, CallArg, Command, ProgrammableTransaction, TransactionData,
};
use sui_types::SUI_SYSTEM_PACKAGE_ID;
use withdraw_stake::withdraw_stake_pt;

use crate::errors::Error;
use crate::types::ConstructionMetadata;
pub use pay_coin::PayCoin;
pub use pay_sui::PaySui;
pub use stake::Stake;
pub use withdraw_stake::WithdrawStake;

mod pay_coin;
mod pay_sui;
mod stake;
mod withdraw_stake;

pub const MAX_GAS_COINS: usize = 255;
const MAX_COMMAND_ARGS: usize = 511;
const MAX_GAS_BUDGET: u64 = 50_000_000_000;
const START_BUDGET: u64 = 1_000_000;

pub struct TransactionAndObjectData {
    pub gas_coins: Vec<ObjectRef>,
    pub extra_gas_coins: Vec<ObjectRef>,
    pub objects: Vec<ObjectRef>,
    pub pt: ProgrammableTransaction,
    /// Refers to the sum of the Coin<SUI> balance of the coins participating in the transaction;
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
    ) -> Result<TransactionAndObjectData, Error>;
}

#[enum_dispatch(TryFetchNeededObjects)]
#[derive(Serialize, Deserialize, Debug)]
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
                let mut builder = ProgrammableTransactionBuilder::new();

                // [WORKAROUND] - this is a hack to work out if the staking ops is for a selected amount or None amount (whole wallet).
                // if amount is none, validator input will be created after the system object input
                let (validator, system_state, amount) = if let Some(amount) = amount {
                    let amount = builder.pure(amount)?;
                    let validator = builder.input(CallArg::Pure(bcs::to_bytes(&validator)?))?;
                    let state = builder.input(CallArg::SUI_SYSTEM_MUT)?;
                    (validator, state, amount)
                } else {
                    let amount =
                        builder.pure(metadata.total_coin_value as u64 - metadata.budget)?;
                    let state = builder.input(CallArg::SUI_SYSTEM_MUT)?;
                    let validator = builder.input(CallArg::Pure(bcs::to_bytes(&validator)?))?;
                    (validator, state, amount)
                };
                let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount]));

                let arguments = vec![system_state, coin, validator];

                builder.command(Command::move_call(
                    SUI_SYSTEM_PACKAGE_ID,
                    SUI_SYSTEM_MODULE_NAME.to_owned(),
                    ADD_STAKE_FUN_NAME.to_owned(),
                    vec![],
                    arguments,
                ));
                builder.finish()
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

fn insert_in_reverse_order(vec: &mut Vec<Coin>, coin: Coin) -> usize {
    match vec
        .iter()
        .enumerate()
        .find(|(_pos, existing)| existing.balance < coin.balance)
    {
        Some((pos, _)) => {
            vec.insert(pos, coin);
            pos
        }
        None => {
            vec.push(coin);
            vec.len() - 1
        }
    }
}
