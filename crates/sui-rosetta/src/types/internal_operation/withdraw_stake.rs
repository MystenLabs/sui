// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use sui_json_rpc_types::{StakeStatus, SuiObjectDataOptions};
use sui_sdk::SuiClient;
use sui_types::SUI_SYSTEM_PACKAGE_ID;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::error::{SuiError, SuiErrorKind, UserInputError};
use sui_types::governance::WITHDRAW_STAKE_FUN_NAME;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{CallArg, Command, ObjectArg, ProgrammableTransaction};

use crate::errors::Error;

use super::{MAX_GAS_COINS, TransactionObjectData, TryConstructTransaction, budget_from_dry_run};

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
    ) -> Result<TransactionObjectData, Error> {
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

        // dry run
        let budget = match budget {
            Some(budget) => budget,
            None => {
                let pt = withdraw_stake_pt(stake_refs.clone(), withdraw_all)?;
                budget_from_dry_run(client, pt.clone(), sender, gas_price).await?
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
            objects: stake_refs,
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
        // TODO: Investigate whether using asimple input argument with relevant metadata, similar
        // to PayCoinOperation, would work as well or even better. Would help with consistency.
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
