// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use sui_rpc::client::v2::Client;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::rpc_proto_conversions::ObjectReferenceExt;
use sui_types::transaction::{Argument, Command, ObjectArg, ProgrammableTransaction};

use crate::errors::Error;

use super::{
    simulate_transaction, TransactionObjectData, TryConstructTransaction, MAX_COMMAND_ARGS,
    MAX_GAS_COINS,
};

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
        client: &mut Client,
        gas_price: Option<u64>,
        budget: Option<u64>,
    ) -> Result<TransactionObjectData, Error> {
        let Self {
            sender,
            recipients,
            amounts,
        } = self;

        let all_coins = client
            .select_up_to_n_largest_coins(
                &Address::from(sender),
                &StructTag::sui().into(),
                1500,
                &[],
            )
            .await?;

        let total_sui_balance = all_coins.iter().map(|c| c.balance()).sum::<u64>() as i128;

        let mut iter = all_coins
            .iter()
            .map(|obj| obj.object_reference().try_to_object_ref());
        let gas_coins = iter
            .by_ref()
            .take(MAX_GAS_COINS)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Error::from)?;
        let extra_gas_coins = iter.collect::<Result<Vec<_>, _>>().map_err(Error::from)?;

        // Simulate to get budget if necessary and validate we can cover payment + gas amount.
        let pt = pay_sui_pt(recipients, amounts, &extra_gas_coins)?;
        let (budget, gas_coin_objs) =
            simulate_transaction(client, pt, sender, gas_coins, gas_price, budget).await?;

        let gas_coins = gas_coin_objs
            .iter()
            .map(|obj| {
                obj.object_reference()
                    .try_to_object_ref()
                    .map_err(Error::from)
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(TransactionObjectData {
            gas_coins,
            extra_gas_coins,
            objects: vec![],
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
