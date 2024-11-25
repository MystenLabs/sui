use std::future;

use anyhow::anyhow;
use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use sui_json_rpc_types::{Coin, SuiExecutionStatus, SuiTransactionBlockEffectsAPI};
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{
    Argument, Command, ObjectArg, ProgrammableTransaction, TransactionData,
};

use crate::types::internal_operation::MAX_GAS_COINS;
use crate::{errors::Error, Currency};

use super::{TransactionAndObjectData, TryConstructTransaction, MAX_COMMAND_ARGS, MAX_GAS_BUDGET};

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
    ) -> Result<TransactionAndObjectData, Error> {
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
            .await
            .ok()
            .unwrap_or_default()
            .iter()
            .map(|coin| coin.object_ref())
            .collect();

        let pt = pay_coin_pt(recipients, amounts, &coin_objs, &currency)?;
        let gas_price = match gas_price {
            Some(p) => p,
            None => client.governance_api().get_reference_gas_price().await? + 100, // make sure it works over epoch changes
        };
        let budget = match budget {
            Some(budget) => budget,
            None => {
                let tx_data = TransactionData::new_programmable(
                    sender,
                    vec![],
                    pt.clone(),
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
                effects.gas_cost_summary().computation_cost
                    + effects.gas_cost_summary().storage_cost
            }
        };

        // The below logic should work exactly as select_coins if the first 255 coins
        // are enough for budget. If they are not it continues iterating moving the smallest
        // ones on the end of the vector and keeping the largest ones in the start, so we can
        // satisfy the limit of max 255 gas-coins
        let mut sum_largest_255 = 0;
        let mut gathered_coins_reverse_sorted = vec![];
        client
            .coin_read_api()
            .get_coins_stream(sender, None)
            .take_while(|coin: &Coin| {
                if gathered_coins_reverse_sorted.is_empty() {
                    sum_largest_255 += coin.balance;
                    gathered_coins_reverse_sorted.push(coin.clone());
                } else {
                    let pos =
                        insert_in_reverse_order(&mut gathered_coins_reverse_sorted, coin.clone());
                    if pos < MAX_GAS_COINS {
                        sum_largest_255 += coin.balance;
                        if gathered_coins_reverse_sorted.len() > MAX_GAS_COINS {
                            sum_largest_255 -= gathered_coins_reverse_sorted[MAX_GAS_COINS].balance;
                        }
                    }
                }
                future::ready(budget < sum_largest_255)
            })
            .collect::<Vec<_>>()
            .await;

        let gas_coins_iter = gathered_coins_reverse_sorted
            .into_iter()
            .take(MAX_GAS_COINS);
        let total_sui_balance = gas_coins_iter.clone().map(|c| c.balance).sum::<u64>() as i128;
        let gas_coins = gas_coins_iter.map(|c| c.object_ref()).collect();

        Ok(TransactionAndObjectData {
            gas_coins,
            extra_gas_coins: vec![],
            objects: coin_objs,
            pt,
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

    let mut commands = 0;
    let mut builder = ProgrammableTransactionBuilder::new();

    let mut merged = coins
        .chunks(MAX_COMMAND_ARGS)
        .map(|chunk| -> anyhow::Result<Argument> {
            let mut to_merge: Vec<Argument> = chunk
                .iter()
                .map(|&o| builder.obj(ObjectArg::ImmOrOwnedObject(o)))
                .collect::<Result<Vec<Argument>, anyhow::Error>>()?;
            if to_merge.is_empty() {
                return Err(anyhow!("Tried to pay with no coins"));
            }
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
