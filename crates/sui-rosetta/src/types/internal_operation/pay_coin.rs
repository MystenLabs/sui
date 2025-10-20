// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::error::{SuiErrorKind, UserInputError};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{Argument, Command, ObjectArg, ProgrammableTransaction};

use crate::types::internal_operation::MAX_GAS_COINS;
use crate::{errors::Error, Currency};

use super::{
    budget_from_dry_run, TransactionObjectData, TryConstructTransaction, MAX_COMMAND_ARGS,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PayCoin {
    pub sender: SuiAddress,
    pub recipients: Vec<SuiAddress>,
    pub amounts: Vec<u64>,
    pub currency: Currency,
}

#[async_trait]
impl TryConstructTransaction for PayCoin {
    async fn try_fetch_needed_objects(
        self,
        client: &SuiClient,
        gas_price: Option<u64>,
        budget: Option<u64>,
    ) -> Result<TransactionObjectData, Error> {
        let Self {
            sender,
            recipients,
            amounts,
            currency,
        } = self;

        let amount = amounts.iter().sum::<u64>();
        let coin_objs: Vec<ObjectRef> = client
            .coin_read_api()
            .select_coins(
                sender,
                Some(currency.metadata.coin_type.clone()),
                amount.into(),
                vec![],
            )
            .await?
            .iter()
            .map(|coin| coin.object_ref())
            .collect();

        let budget = match budget {
            Some(budget) => budget,
            None => {
                let pt = pay_coin_pt(recipients, amounts, &coin_objs, &currency)?;
                budget_from_dry_run(client, pt, sender, gas_price).await?
            }
        };

        let gas_coins = client
            .coin_read_api()
            .select_coins(sender, None, budget as u128, vec![])
            .await?;
        if gas_coins.len() > MAX_GAS_COINS {
            return Err(SuiErrorKind::UserInputError {
                error: UserInputError::SizeLimitExceeded {
                    limit: "maximum number of gas payment objects".to_string(),
                    value: MAX_GAS_COINS.to_string(),
                },
            }
            .into());
        }

        let gas_coins_iter = gas_coins.into_iter();
        let total_sui_balance = gas_coins_iter.clone().map(|c| c.balance).sum::<u64>() as i128;
        let gas_coins = gas_coins_iter.map(|c| c.object_ref()).collect();

        Ok(TransactionObjectData {
            gas_coins,
            extra_gas_coins: vec![],
            objects: coin_objs,
            total_sui_balance,
            budget,
        })
    }
}

// If the transaction budget is not enough here, there is nothing we can do.
// Merging gas inside the transaction works only when gas is used also for other purposes other than
// transaction fees.
pub fn pay_coin_pt(
    recipients: Vec<SuiAddress>,
    amounts: Vec<u64>,
    coins: &[ObjectRef],
    currency: &Currency,
) -> anyhow::Result<ProgrammableTransaction> {
    if recipients.len() != amounts.len() {
        return Err(anyhow!("Amounts length does not match recipients"));
    }
    if coins.is_empty() {
        return Err(anyhow!("Cannot PayCoin without any coins"));
    }

    let mut commands = 0;
    let mut builder = ProgrammableTransactionBuilder::new();

    let mut merged = coins
        .chunks(MAX_COMMAND_ARGS)
        .map(|chunk| -> anyhow::Result<Argument> {
            let mut to_merge: Vec<Argument> = chunk
                .iter()
                .map(|&o| builder.obj(ObjectArg::ImmOrOwnedObject(o)))
                .collect::<Result<Vec<Argument>, anyhow::Error>>()?;
            let merge_into = to_merge.pop().expect("Already checked for non-zero length");
            if !to_merge.is_empty() {
                builder.command(Command::MergeCoins(merge_into, to_merge));
                commands += 1;
            }
            Ok(merge_into)
        })
        .collect::<Result<Vec<_>, anyhow::Error>>()?;
    // Accumulate all dust coins into a single one
    let single_coin = merged
        .pop()
        .expect("Already checked for non-zero coins above");
    if !merged.is_empty() {
        builder.command(Command::MergeCoins(single_coin, merged));
        commands += 1;
    }

    // We could optimally not split the last coin if the sum of the coins.balance given matches
    // the amounts.sum. This would require changes in the ConstructionMetadata type, as information
    // about the total-coin-value would be needed.
    let amount_args = amounts
        .into_iter()
        .map(|v| builder.pure(v))
        .collect::<Result<Vec<_>, anyhow::Error>>()?;
    let split_command = commands;
    builder.command(Command::SplitCoins(single_coin, amount_args));

    recipients
        .into_iter()
        .enumerate()
        .for_each(|(i, recipient)| {
            builder.transfer_arg(recipient, Argument::NestedResult(split_command, i as u16));
        });

    // This is a workaround in order to have the currency info available during the process
    // of constructing back the Operations object from the transaction data. A process that
    // takes place upon the request to the construction's /parse endpoint. The pure value is
    // not actually being used in any on-chain transaction execution and its sole purpose
    // is to act as a bearer of the currency info between the various steps of the flow.
    // See also the value is being later accessed within the operations.rs file's
    // parse_programmable_transaction function.
    let currency_string = serde_json::to_string(currency)?;
    builder.pure(currency_string)?;
    Ok(builder.finish())
}
