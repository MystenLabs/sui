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

use crate::errors::Error;

use super::{
    TransactionAndObjectData, TryConstructTransaction, MAX_COMMAND_ARGS, MAX_GAS_BUDGET,
    MAX_GAS_COINS,
};

const START_BUDGET: u64 = 1_000_000;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PaySui {
    pub sender: SuiAddress,
    pub recipients: Vec<SuiAddress>,
    pub amounts: Vec<u64>,
}

#[async_trait]
impl TryConstructTransaction for PaySui {
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
        } = self;

        let total_amount = amounts.iter().sum::<u64>();
        if let Some(budget) = budget {
            let mut coins = client
                .coin_read_api()
                .select_coins(sender, None, (total_amount + budget) as u128, vec![])
                .await?;

            let total_sui_balance = coins.iter().map(|c| c.balance).sum::<u64>() as i128;

            let objects = if coins.len() > MAX_GAS_COINS {
                coins
                    .split_off(MAX_GAS_COINS)
                    .iter()
                    .map(|c| c.object_ref())
                    .collect()
            } else {
                vec![]
            };
            let pt = pay_sui_pt(recipients, amounts, &objects)?;

            return Ok(TransactionAndObjectData {
                coins,
                objects,
                pt,
                total_sui_balance,
                budget,
            });
        };

        let gas_price = match gas_price {
            Some(p) => p,
            None => client.governance_api().get_reference_gas_price().await? + 100, // make sure it works over epoch changes
        };

        let mut coins_stream = Box::pin(client.coin_read_api().get_coins_stream(sender, None));

        let mut all_coins = vec![];
        let mut coins_for_gas: Vec<Coin>;
        let total_amount = amounts.iter().sum::<u64>();
        let mut gathered = 0;
        let mut budget = START_BUDGET;
        let mut pt;
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
            let coins_to_merge: Vec<_> = all_coins
                .iter()
                .skip(MAX_GAS_COINS)
                .map(|c| c.object_ref())
                .collect();
            pt = pay_sui_pt(recipients.clone(), amounts.clone(), &coins_to_merge)?;
            coins_for_gas = all_coins.iter().take(MAX_GAS_COINS).cloned().collect();
            let tx_data = TransactionData::new_programmable(
                sender,
                coins_for_gas.iter().map(|c| c.object_ref()).collect(),
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
            // Update budget to be the result of the dry run
            budget = effects.gas_cost_summary().computation_cost
                + effects.gas_cost_summary().storage_cost;
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
        let total_sui_balance = all_coins.into_iter().map(|c| c.balance).sum::<u64>() as i128;
        Ok(TransactionAndObjectData {
            coins: coins_for_gas,
            objects,
            pt,
            total_sui_balance,
            budget,
        })
    }
}

/// Creates the `ProgrammableTransaction` for a pay-sui operation.
/// In case pay-sui needs more than 255 gas-coins to be smashed, it tries to merge the surplus
/// coins into the gas coin as regular transaction inputs - not gas-payment.
/// This approach has the limit at around 1650 coins in total which triggers transaction-size
/// limit (see also test_limit_many_small_coins test).
pub fn pay_sui_pt(
    recipients: Vec<SuiAddress>,
    amounts: Vec<u64>,
    coins_to_merge: &[ObjectRef],
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
