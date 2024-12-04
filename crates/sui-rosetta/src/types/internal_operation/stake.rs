// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use sui_sdk::SuiClient;
use sui_types::base_types::ObjectRef;
use sui_types::governance::ADD_STAKE_FUN_NAME;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{Argument, CallArg, Command, ObjectArg, ProgrammableTransaction};
use sui_types::SUI_SYSTEM_PACKAGE_ID;
use sui_types::{
    base_types::SuiAddress, programmable_transaction_builder::ProgrammableTransactionBuilder,
};

use crate::errors::Error;
use crate::types::internal_operation::MAX_GAS_COINS;

use super::{
    budget_from_dry_run, TransactionAndObjectData, TryConstructTransaction, MAX_COMMAND_ARGS,
    START_GAS_UNITS,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Stake {
    pub sender: SuiAddress,
    pub validator: SuiAddress,
    pub amount: Option<u64>,
}

#[async_trait]
impl TryConstructTransaction for Stake {
    async fn try_fetch_needed_objects(
        self,
        client: &SuiClient,
        gas_price: Option<u64>,
        budget: Option<u64>,
    ) -> Result<TransactionAndObjectData, Error> {
        let Self {
            sender,
            validator,
            amount,
        } = self;
        let gas_price = match gas_price {
            Some(p) => p,
            None => client.governance_api().get_reference_gas_price().await? + 100, // make sure it works over epoch changes
        };

        if amount.is_none() {
            let all_coins = client
                .coin_read_api()
                .get_coins_stream(sender, None)
                .collect::<Vec<_>>()
                .await;

            let total_sui_balance = all_coins.iter().map(|c| c.balance).sum::<u64>() as i128;
            let mut iter = all_coins.into_iter().map(|c| c.object_ref());
            let gas_coins: Vec<_> = iter.by_ref().take(MAX_GAS_COINS).collect();
            let extra_gas_coins: Vec<_> = iter.collect();

            // For some reason dry run fails if we use a total_sui_balance - big-budget and also
            // provide the gas-coins. Not using gas_coins should not matter in the dry-run.
            // This seems like a bug of the dry-run implementation?
            let pt = stake_pt(validator, total_sui_balance as u64, true, &extra_gas_coins)?;
            let actual_budget =
                budget_from_dry_run(client, pt.clone(), sender, Some(gas_price)).await?;

            return Ok(TransactionAndObjectData {
                gas_coins,
                extra_gas_coins,
                objects: vec![],
                total_sui_balance,
                budget: budget.unwrap_or(actual_budget),
            });
        }

        let amount = amount.expect("We already handled amount: None");

        // amount and budget is given
        if let Some(budget) = budget {
            let all_coins = client
                .coin_read_api()
                .select_coins(sender, None, (amount + budget) as u128, vec![])
                .await?;
            let total_sui_balance = all_coins.iter().map(|c| c.balance).sum::<u64>() as i128;

            let mut iter = all_coins.into_iter().map(|c| c.object_ref());
            let gas_coins: Vec<_> = iter.by_ref().take(MAX_GAS_COINS).collect();
            let extra_gas_coins: Vec<_> = iter.collect();

            return Ok(TransactionAndObjectData {
                gas_coins,
                extra_gas_coins,
                objects: vec![],
                total_sui_balance,
                budget,
            });
        }

        // amount is given, budget is not
        let mut coins_stream = Box::pin(client.coin_read_api().get_coins_stream(sender, None));

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
                    "Address {sender} does not have enough Sui balance to stake {amount} with needed budget: {budget}. Sui balance: {gathered}."
                )));
            }

            // The coins to merge should be used as transaction object inputs, as
            // `TransactionData::new_programmable` used in `InternalOperation::try_into_data`,
            // uses all coins passed as gas payment.
            let mut iter = all_coins.iter().map(|c| c.object_ref());
            gas_coins = iter.by_ref().take(MAX_GAS_COINS).collect();
            extra_gas_coins = iter.collect();
            let pt = stake_pt(validator, amount, false, &extra_gas_coins)?;
            budget = budget_from_dry_run(client, pt.clone(), sender, Some(gas_price)).await?;
            // If we have already gathered the needed amount of coins we don't need to dry run again,
            // as the transaction will be the same.
            if budget + amount <= gathered {
                break;
            }
        }
        let total_sui_balance = all_coins.iter().map(|c| c.balance).sum::<u64>() as i128;

        Ok(TransactionAndObjectData {
            gas_coins,
            extra_gas_coins,
            objects: vec![],
            total_sui_balance,
            budget,
        })
    }
}

pub fn stake_pt(
    validator: SuiAddress,
    amount: u64,
    stake_all: bool,
    coins_to_merge: &[ObjectRef],
) -> anyhow::Result<ProgrammableTransaction> {
    let mut builder = ProgrammableTransactionBuilder::new();

    // [WORKAROUND] - this is a hack to work out if the staking ops is for a selected amount or None amount (whole wallet).
    // if amount is none, validator input will be created after the system object input
    let amount = builder.pure(amount)?;
    let (validator, system_state) = if !stake_all {
        let validator = builder.input(CallArg::Pure(bcs::to_bytes(&validator)?))?;
        let state = builder.input(CallArg::SUI_SYSTEM_MUT)?;
        (validator, state)
    } else {
        let state = builder.input(CallArg::SUI_SYSTEM_MUT)?;
        let validator = builder.input(CallArg::Pure(bcs::to_bytes(&validator)?))?;
        (validator, state)
    };

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

    // Theoretically, if stake_all is true, we could not use amount, and instead,
    // directly use Argument::GasCoin here, but this is how this Operation has always worked.
    // Changing this now would require editing other endpoints too.
    let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount]));

    let arguments = vec![system_state, coin, validator];

    builder.command(Command::move_call(
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        ADD_STAKE_FUN_NAME.to_owned(),
        vec![],
        arguments,
    ));
    Ok(builder.finish())
}
