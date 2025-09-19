// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sui_rpc::client::v2::Client;

use sui_types::base_types::ObjectRef;
use sui_types::governance::ADD_STAKE_FUN_NAME;
use sui_types::rpc_proto_conversions::ObjectReferenceExt;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{Argument, CallArg, Command, ObjectArg, ProgrammableTransaction};
use sui_types::SUI_SYSTEM_PACKAGE_ID;
use sui_types::{
    base_types::SuiAddress, programmable_transaction_builder::ProgrammableTransactionBuilder,
};

use crate::errors::Error;
use crate::types::internal_operation::MAX_GAS_COINS;

use super::{
    simulate_transaction, TransactionObjectData, TryConstructTransaction, MAX_COMMAND_ARGS,
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
        client: &mut Client,
        gas_price: Option<u64>,
        budget: Option<u64>,
    ) -> Result<TransactionObjectData, Error> {
        let Self {
            sender,
            validator,
            amount,
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

        // Always simulate to validate the transaction
        // For stake_all (amount is None), simulate with minimal amount
        // For specific amount, simulate with actual amount
        let simulation_amount = amount.unwrap_or(1_000_000_000); // 1 SUI minimum staking threshold

        let pt = stake_pt(validator, simulation_amount, false, &extra_gas_coins)?;
        let (budget, _) =
            simulate_transaction(client, pt, sender, gas_coins.clone(), gas_price, budget).await?;

        Ok(TransactionObjectData {
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
    // TODO: Investigate whether using asimple input argument with relevant metadata, similar
    // to PayCoinOperation, would work as well or even better. Would help with consistency.
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
