// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use sui_json_rpc_types::{
    StakeStatus, SuiExecutionStatus, SuiObjectDataOptions, SuiTransactionBlockEffectsAPI,
};
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::error::SuiError;
use sui_types::governance::WITHDRAW_STAKE_FUN_NAME;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{
    CallArg, Command, ObjectArg, ProgrammableTransaction, TransactionData,
};
use sui_types::SUI_SYSTEM_PACKAGE_ID;

use crate::errors::Error;

use super::{
    gather_coins_in_balance_reverse_order, TransactionAndObjectData, TryConstructTransaction,
    MAX_GAS_BUDGET, MAX_GAS_COINS,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WithdrawStake {
    pub sender: SuiAddress,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stake_ids: Vec<ObjectID>,
}

#[async_trait]
impl TryConstructTransaction for WithdrawStake {
    async fn try_fetch_needed_objects(
        self,
        client: &SuiClient,
        gas_price: Option<u64>,
        budget: Option<u64>,
    ) -> Result<TransactionAndObjectData, Error> {
        let Self { sender, stake_ids } = self;

        let withdraw_all = stake_ids.is_empty();
        let stake_ids = if withdraw_all {
            // unstake all
            client
                .governance_api()
                .get_stakes(sender)
                .await?
                .into_iter()
                .flat_map(|s| {
                    s.stakes.into_iter().filter_map(|s| {
                        if let StakeStatus::Active { .. } = s.status {
                            Some(s.staked_sui_id)
                        } else {
                            None
                        }
                    })
                })
                .collect()
        } else {
            stake_ids
        };

        if stake_ids.is_empty() {
            return Err(Error::InvalidInput("No active stake to withdraw".into()));
        }

        let responses = client
            .read_api()
            .multi_get_object_with_options(stake_ids, SuiObjectDataOptions::default())
            .await?;
        let stake_refs = responses
            .into_iter()
            .map(|stake| stake.into_object().map(|o| o.object_ref()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(SuiError::from)?;

        let pt = withdraw_stake_pt(stake_refs.clone(), withdraw_all)?;
        // dry run
        let budget = match budget {
            Some(budget) => budget,
            None => {
                let gas_price = match gas_price {
                    Some(p) => p,
                    None => client.governance_api().get_reference_gas_price().await? + 100, // make sure it works over epoch changes
                };
                // Dry run the transaction to get the gas used, amount doesn't really matter here when using mock coins.
                // get gas estimation from dry-run, this will also return any tx error.
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

        let gathered_coins_reverse_sorted =
            gather_coins_in_balance_reverse_order(client, sender, budget).await?;

        let gas_coins_iter = gathered_coins_reverse_sorted
            .into_iter()
            .take(MAX_GAS_COINS);
        let total_sui_balance = gas_coins_iter.clone().map(|c| c.balance).sum::<u64>() as i128;
        let gas_coins = gas_coins_iter.map(|c| c.object_ref()).collect();

        Ok(TransactionAndObjectData {
            gas_coins,
            extra_gas_coins: vec![],
            objects: stake_refs,
            pt,
            total_sui_balance,
            budget,
        })
    }
}

pub fn withdraw_stake_pt(
    stake_objs: Vec<ObjectRef>,
    withdraw_all: bool,
) -> anyhow::Result<ProgrammableTransaction> {
    let mut builder = ProgrammableTransactionBuilder::new();

    for stake_id in stake_objs {
        // [WORKAROUND] - this is a hack to work out if the withdraw stake ops is for selected stake_ids or None (all stakes) using the index of the call args.
        // if stake_ids is not empty, id input will be created after the system object input
        let (system_state, id) = if !withdraw_all {
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
    Ok(builder.finish())
}
