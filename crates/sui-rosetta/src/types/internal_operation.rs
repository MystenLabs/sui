// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::StreamExt;
use serde::{Deserialize, Serialize};

use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_sdk::rpc_types::SuiExecutionStatus;
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::governance::{ADD_STAKE_FUN_NAME, WITHDRAW_STAKE_FUN_NAME};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{
    Argument, CallArg, Command, ObjectArg, ProgrammableTransaction, TransactionData,
};
use sui_types::SUI_SYSTEM_PACKAGE_ID;

use crate::errors::Error;
use crate::types::{ConstructionMetadata, Currency};

const MAX_GAS_COINS: usize = 255;
const MAX_COMMAND_ARGS: usize = 511;
const MAX_GAS_BUDGET: u64 = 50_000_000_000;
const START_BUDGET: u64 = 1_000_000;

#[derive(Serialize, Deserialize, Debug)]
pub enum InternalOperation {
    PaySui {
        sender: SuiAddress,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
    },
    PayCoin {
        sender: SuiAddress,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
        currency: Currency,
    },
    Stake {
        sender: SuiAddress,
        validator: SuiAddress,
        amount: Option<u64>,
    },
    WithdrawStake {
        sender: SuiAddress,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        stake_ids: Vec<ObjectID>,
    },
}

impl InternalOperation {
    pub fn sender(&self) -> SuiAddress {
        match self {
            InternalOperation::PaySui { sender, .. }
            | InternalOperation::PayCoin { sender, .. }
            | InternalOperation::Stake { sender, .. }
            | InternalOperation::WithdrawStake { sender, .. } => *sender,
        }
    }
    /// Combine with ConstructionMetadata to form the TransactionData
    pub fn try_into_data(self, metadata: ConstructionMetadata) -> Result<TransactionData, Error> {
        let pt = match self {
            Self::PaySui {
                recipients,
                amounts,
                ..
            } => pay_sui_pt(recipients, amounts, metadata.objects)?,
            Self::PayCoin {
                recipients,
                amounts,
                ..
            } => {
                let mut builder = ProgrammableTransactionBuilder::new();
                builder.pay(metadata.objects.clone(), recipients, amounts)?;
                let currency_str = serde_json::to_string(&metadata.currency.unwrap()).unwrap();
                // This is a workaround in order to have the currency info available during the process
                // of constructing back the Operations object from the transaction data. A process that
                // takes place upon the request to the construction's /parse endpoint. The pure value is
                // not actually being used in any on-chain transaction execution and its sole purpose
                // is to act as a bearer of the currency info between the various steps of the flow.
                // See also the value is being later accessed within the operations.rs file's
                // parse_programmable_transaction function.
                builder.pure(currency_str)?;
                builder.finish()
            }
            InternalOperation::Stake {
                validator, amount, ..
            } => {
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
            InternalOperation::WithdrawStake { stake_ids, .. } => {
                let mut builder = ProgrammableTransactionBuilder::new();

                for stake_id in metadata.objects {
                    // [WORKAROUND] - this is a hack to work out if the withdraw stake ops is for selected stake_ids or None (all stakes) using the index of the call args.
                    // if stake_ids is not empty, id input will be created after the system object input
                    let (system_state, id) = if !stake_ids.is_empty() {
                        let system_state = builder.input(CallArg::SUI_SYSTEM_MUT)?;
                        let id = builder.obj(ObjectArg::ImmOrOwnedObject(stake_id))?;
                        (system_state, id)
                    } else {
                        let id = builder.obj(ObjectArg::ImmOrOwnedObject(stake_id))?;
                        let system_state = builder.input(CallArg::SUI_SYSTEM_MUT)?;
                        (system_state, id)
                    };

                    let arguments = vec![system_state, id];
                    builder.command(Command::move_call(
                        SUI_SYSTEM_PACKAGE_ID,
                        SUI_SYSTEM_MODULE_NAME.to_owned(),
                        WITHDRAW_STAKE_FUN_NAME.to_owned(),
                        vec![],
                        arguments,
                    ));
                }
                builder.finish()
            }
        };

        Ok(TransactionData::new_programmable(
            metadata.sender,
            metadata.coins,
            pt,
            metadata.budget,
            metadata.gas_price,
        ))
    }
}

pub async fn pay_sui_to_metadata(
    client: &SuiClient,
    gas_price: Option<u64>,
    sender: SuiAddress,
    recipients: Vec<SuiAddress>,
    amounts: Vec<u64>,
    budget: Option<u64>,
) -> Result<ConstructionMetadata, Error> {
    let gas_price = match gas_price {
        Some(p) => p,
        None => client.governance_api().get_reference_gas_price().await? + 100, // make sure it works over epoch changes
    };
    let total_amount = amounts.iter().sum::<u64>();
    if let Some(budget) = budget {
        let coins = client
            .coin_read_api()
            .select_coins(sender, None, (total_amount + budget) as u128, vec![])
            .await?;

        let total_coin_value = coins.iter().map(|c| c.balance).sum::<u64>() as i128;

        let mut coins: Vec<ObjectRef> = coins.into_iter().map(|c| c.object_ref()).collect();
        let objects = if coins.len() > MAX_GAS_COINS {
            coins.split_off(MAX_GAS_COINS)
        } else {
            vec![]
        };

        return Ok(ConstructionMetadata {
            sender,
            coins,
            budget,
            objects,
            total_coin_value,
            gas_price,
            currency: None,
        });
    };

    let mut coins_stream = Box::pin(client.coin_read_api().get_coins_stream(sender, None));

    let mut all_coins = vec![];
    let mut coins_for_gas: Vec<ObjectRef>;
    let total_amount = amounts.iter().sum::<u64>();
    let mut gathered = 0;
    let mut budget = START_BUDGET;
    // We need to dry-run in a loop, because depending on the amount of coins used the tx might
    // differ slightly: (merge / no merge / number of merge-coins)
    loop {
        while let Some(coin) = coins_stream.next().await {
            gathered += coin.balance;
            all_coins.push(coin);
            if gathered >= total_amount + budget {
                break;
            }
        }

        // The coins to merge should be used as transaction object inputs, as
        // `TransactionData::new_programmable` used in `InternalOperation::try_into_data`,
        // uses all coins passed as gas payment.
        let coins_to_merge = all_coins
            .iter()
            .skip(MAX_GAS_COINS)
            .map(|c| c.object_ref())
            .collect();
        let pt = pay_sui_pt(recipients.clone(), amounts.clone(), coins_to_merge)?;
        coins_for_gas = (if all_coins.len() > MAX_GAS_COINS {
            &all_coins[..MAX_GAS_COINS]
        } else {
            &all_coins[..]
        })
        .iter()
        .map(|c| c.object_ref())
        .collect();
        let tx_data = TransactionData::new_programmable(
            sender,
            coins_for_gas.clone(),
            pt,
            // We don't want dry run to fail due to budget, because
            // it will display the fail-budget
            MAX_GAS_BUDGET,
            gas_price,
        );

        let dry_run = client.read_api().dry_run_transaction_block(tx_data).await?;
        let effects = dry_run.effects;

        if let SuiExecutionStatus::Failure { error } = effects.status() {
            return Err(Error::TransactionDryRunError(error.to_string()));
        }
        // Update budget to be the result of the dry run
        budget =
            effects.gas_cost_summary().computation_cost + effects.gas_cost_summary().storage_cost;
        // If we have already gathered the needed amount of coins we don't need to dry run again,
        // as the transaction will be the same.
        if budget + total_amount <= gathered {
            break;
        }
    }
    let objects = all_coins
        .iter()
        .skip(MAX_GAS_COINS)
        .map(|c| c.object_ref())
        .collect();
    let total_coin_value = all_coins.into_iter().map(|c| c.balance).sum::<u64>() as i128;

    Ok(ConstructionMetadata {
        sender,
        coins: coins_for_gas,
        budget,
        objects,
        total_coin_value,
        gas_price,
        currency: None,
    })
}

/// Creates the `ProgrammableTransaction` for a pay-sui operation.
/// In case pay-sui needs more than 255 gas-coins to be smashed, it tries to merge the surplus
/// coins into the gas coin as regular transaction inputs - not gas-payment.
/// This approach has the limit at around 1650 coins in total which triggers transaction-size
/// limit (see also test_limit_many_small_coins test).
pub fn pay_sui_pt(
    recipients: Vec<SuiAddress>,
    amounts: Vec<u64>,
    coins_to_merge: Vec<ObjectRef>,
) -> anyhow::Result<ProgrammableTransaction> {
    let mut builder = ProgrammableTransactionBuilder::new();
    if !coins_to_merge.is_empty() {
        // We need to merge the rest of the coins.
        // Each merge has a limit of 511 arguments.
        coins_to_merge
            .chunks(MAX_COMMAND_ARGS)
            .try_for_each(|chunk| -> anyhow::Result<()> {
                let to_merge = chunk
                    .iter()
                    .map(|&o| builder.obj(ObjectArg::ImmOrOwnedObject(o)))
                    .collect::<Result<Vec<Argument>, anyhow::Error>>()?;
                builder.command(Command::MergeCoins(Argument::GasCoin, to_merge));
                Ok(())
            })?;
    };
    builder.pay_sui(recipients, amounts)?;
    Ok(builder.finish())
}
