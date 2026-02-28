// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use sui_rpc::client::Client;
use sui_rpc::proto::sui::rpc::v2::{GetBalanceRequest, Object, owner::OwnerKind};
use sui_sdk_types::{Address, StructTag};
use sui_types::gas_coin::GAS;

use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::SUI_SYSTEM_PACKAGE_ID;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber};
use sui_types::governance::ADD_STAKE_FUN_NAME;
use sui_types::rpc_proto_conversions::ObjectReferenceExt;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{Argument, CallArg, Command, ObjectArg, ProgrammableTransaction};
use sui_types::{
    Identifier, base_types::SuiAddress,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
};

use crate::errors::Error;
use crate::types::internal_operation::MAX_GAS_COINS;

use super::{
    MAX_COMMAND_ARGS, TransactionObjectData, TryConstructTransaction,
    send_gas_remainder_to_address_balance, simulate_transaction,
    withdraw_coin_from_address_balance,
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

        // Get balance breakdown from the API directly
        let balance_info = {
            let request = GetBalanceRequest::default()
                .with_owner(sender.to_string())
                .with_coin_type("0x2::sui::SUI".to_string());
            client
                .state_client()
                .get_balance(request)
                .await?
                .into_inner()
        };
        let address_balance = balance_info.balance().address_balance_opt().unwrap_or(0);

        // Select coin objects (up to 1500). Storage refunds from merging dust outweigh
        // smashing costs, so we merge as many as possible.
        let all_coins = client
            .select_up_to_n_largest_coins(
                &Address::from(sender),
                &StructTag::sui().into(),
                1500,
                &[],
            )
            .await?;

        let coin_objects_total = all_coins.iter().map(|c| c.balance()).sum::<u64>();

        // Separate party objects (ConsensusAddressOwner) from regular objects.
        let (party_objects, non_party_objects): (Vec<_>, Vec<_>) = all_coins
            .iter()
            .partition(|obj| obj.owner().kind() == OwnerKind::ConsensusAddress);

        let non_party_refs: Vec<ObjectRef> = non_party_objects
            .iter()
            .map(|obj: &&Object| obj.object_reference().try_to_object_ref())
            .collect::<Result<Vec<_>, _>>()?;

        let party_refs: Vec<(ObjectID, SequenceNumber)> = party_objects
            .iter()
            .map(|obj: &&Object| -> Result<_, Error> {
                let id = ObjectID::from_str(obj.object_id())
                    .map_err(|e| Error::DataError(format!("Invalid party object ID: {}", e)))?;
                let start_version = SequenceNumber::from_u64(obj.owner().version());
                Ok((id, start_version))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // For stake_all, simulate with minimal amount; the actual amount is computed in try_into_data.
        let simulation_amount = amount.unwrap_or(1_000_000_000);

        // Compute deficit for simulation: how much we need from pre-TX address balance
        let sim_deficit = simulation_amount.saturating_sub(coin_objects_total);

        // Path A: merge coins, withdraw deficit from AB, stake. No GasCoin → AB gas.
        let pt_a = stake_pt(
            sender,
            validator,
            simulation_amount,
            false,
            &non_party_refs,
            &party_refs,
            sim_deficit,
        )?;
        let sim_result =
            simulate_transaction(client, pt_a, sender, vec![], gas_price, budget).await;

        match sim_result {
            Ok((budget, gas_coin_objs)) if gas_coin_objs.is_empty() => {
                // Path A succeeded with address-balance gas.
                let total_sui_balance = (coin_objects_total as i128) + (address_balance as i128);

                // Compute deficit for actual transaction:
                // For specific amount: need max(0, amount - coins_total) from AB
                // For stake_all: need (AB - budget) from AB (rest comes from coins)
                let actual_deficit = match amount {
                    Some(amt) => amt.saturating_sub(coin_objects_total),
                    None => address_balance.saturating_sub(budget),
                };

                Ok(TransactionObjectData {
                    gas_coins: vec![],
                    objects: non_party_refs,
                    party_objects: party_refs,
                    total_sui_balance,
                    budget,
                    address_balance_withdrawal: actual_deficit,
                })
            }
            _ => {
                // Path B: merge coins into GasCoin, split stake, send remainder to address balance.
                let address_balance_withdrawal = address_balance;
                let total_sui_balance =
                    (coin_objects_total as i128) + (address_balance_withdrawal as i128);

                let mut gas_coin_iter = non_party_refs.iter().copied();
                let gas_coins: Vec<ObjectRef> =
                    gas_coin_iter.by_ref().take(MAX_GAS_COINS).collect();
                let extra_coins: Vec<ObjectRef> = gas_coin_iter.collect();

                let pt_b = stake_pt_coin_gas(
                    sender,
                    validator,
                    simulation_amount,
                    false,
                    &extra_coins,
                    &party_refs,
                    address_balance_withdrawal,
                    1, // placeholder for simulation; recomputed in try_into_data
                )?;
                let (budget, gas_coin_objs) =
                    simulate_transaction(client, pt_b, sender, gas_coins, gas_price, budget)
                        .await?;

                let gas_coins = gas_coin_objs
                    .iter()
                    .map(|obj| obj.object_reference().try_to_object_ref())
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(TransactionObjectData {
                    gas_coins,
                    objects: extra_coins,
                    party_objects: party_refs,
                    total_sui_balance,
                    budget,
                    address_balance_withdrawal,
                })
            }
        }
    }
}

/// Path A: merge coins, optionally withdraw deficit from AB, stake, send remainder to address balance.
/// No GasCoin reference → simulator auto-selects address-balance gas.
pub fn stake_pt(
    sender: SuiAddress,
    validator: SuiAddress,
    amount: u64,
    stake_all: bool,
    coins: &[ObjectRef],
    party_coins: &[(ObjectID, SequenceNumber)],
    address_balance_withdrawal: u64,
) -> anyhow::Result<ProgrammableTransaction> {
    let mut builder = ProgrammableTransactionBuilder::new();
    let mut source: Option<Argument> = None;

    // Step 1: Merge all coin objects into one
    if !coins.is_empty() || !party_coins.is_empty() {
        let target = if let Some(&first) = coins.first() {
            builder.obj(ObjectArg::ImmOrOwnedObject(first))?
        } else {
            let &(id, initial_shared_version) = &party_coins[0];
            builder.obj(ObjectArg::SharedObject {
                id,
                initial_shared_version,
                mutability: sui_types::transaction::SharedObjectMutability::Mutable,
            })?
        };

        if coins.len() > 1 {
            coins[1..]
                .chunks(MAX_COMMAND_ARGS)
                .try_for_each(|chunk| -> anyhow::Result<()> {
                    let to_merge = chunk
                        .iter()
                        .map(|&o| builder.obj(ObjectArg::ImmOrOwnedObject(o)))
                        .collect::<Result<Vec<Argument>, _>>()?;
                    builder.command(Command::MergeCoins(target, to_merge));
                    Ok(())
                })?;
        }

        let party_skip = if coins.is_empty() { 1 } else { 0 };
        let party_slice = &party_coins[party_skip..];
        if !party_slice.is_empty() {
            party_slice
                .chunks(MAX_COMMAND_ARGS)
                .try_for_each(|chunk| -> anyhow::Result<()> {
                    let to_merge = chunk
                        .iter()
                        .map(|&(id, initial_shared_version)| {
                            builder.obj(ObjectArg::SharedObject {
                                id,
                                initial_shared_version,
                                mutability: sui_types::transaction::SharedObjectMutability::Mutable,
                            })
                        })
                        .collect::<Result<Vec<Argument>, _>>()?;
                    builder.command(Command::MergeCoins(target, to_merge));
                    Ok(())
                })?;
        }

        source = Some(target);
    }

    // Step 2: Withdraw deficit from address balance and merge into coin
    if address_balance_withdrawal > 0 {
        let withdrawal_coin = withdraw_coin_from_address_balance(
            &mut builder,
            address_balance_withdrawal,
            GAS::type_tag(),
        )?;
        match source {
            Some(target) => {
                builder.command(Command::MergeCoins(target, vec![withdrawal_coin]));
            }
            None => {
                source = Some(withdrawal_coin);
            }
        }
    }

    let source =
        source.ok_or_else(|| anyhow::anyhow!("No coins or address balance to stake from"))?;

    // [WORKAROUND] Input ordering hack for stake_all detection during parsing.
    let amount_arg = builder.pure(amount)?;
    let (validator_arg, system_state) = if !stake_all {
        let v = builder.input(CallArg::Pure(bcs::to_bytes(&validator)?))?;
        let s = builder.input(CallArg::SUI_SYSTEM_MUT)?;
        (v, s)
    } else {
        let s = builder.input(CallArg::SUI_SYSTEM_MUT)?;
        let v = builder.input(CallArg::Pure(bcs::to_bytes(&validator)?))?;
        (v, s)
    };

    // SplitCoins for stake amount
    let coin = builder.command(Command::SplitCoins(source, vec![amount_arg]));

    builder.command(Command::move_call(
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        ADD_STAKE_FUN_NAME.to_owned(),
        vec![],
        vec![system_state, coin, validator_arg],
    ));

    // Send remainder to sender's address balance
    let remainder_balance = builder.command(Command::move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin")?,
        Identifier::new("into_balance")?,
        vec![GAS::type_tag()],
        vec![source],
    ));
    let sender_arg = builder.pure(sender)?;
    builder.command(Command::move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance")?,
        Identifier::new("send_funds")?,
        vec![GAS::type_tag()],
        vec![remainder_balance, sender_arg],
    ));

    Ok(builder.finish())
}

/// Path B: merge coins into GasCoin, split stake, send remainder to address balance.
pub fn stake_pt_coin_gas(
    sender: SuiAddress,
    validator: SuiAddress,
    amount: u64,
    stake_all: bool,
    coins_to_merge: &[ObjectRef],
    party_coins: &[(ObjectID, SequenceNumber)],
    address_balance_withdrawal: u64,
    send_remainder: u64,
) -> anyhow::Result<ProgrammableTransaction> {
    let mut builder = ProgrammableTransactionBuilder::new();

    // Withdraw address balance and merge into GasCoin
    if address_balance_withdrawal > 0 {
        let withdrawn_coin = withdraw_coin_from_address_balance(
            &mut builder,
            address_balance_withdrawal,
            GAS::type_tag(),
        )?;
        builder.command(Command::MergeCoins(Argument::GasCoin, vec![withdrawn_coin]));
    }

    // [WORKAROUND] Input ordering hack for stake_all detection during parsing.
    let amount_arg = builder.pure(amount)?;
    let (validator_arg, system_state) = if !stake_all {
        let v = builder.input(CallArg::Pure(bcs::to_bytes(&validator)?))?;
        let s = builder.input(CallArg::SUI_SYSTEM_MUT)?;
        (v, s)
    } else {
        let s = builder.input(CallArg::SUI_SYSTEM_MUT)?;
        let v = builder.input(CallArg::Pure(bcs::to_bytes(&validator)?))?;
        (v, s)
    };

    // Merge extra coins into GasCoin
    if !coins_to_merge.is_empty() {
        coins_to_merge
            .chunks(MAX_COMMAND_ARGS)
            .try_for_each(|chunk| -> anyhow::Result<()> {
                let to_merge = chunk
                    .iter()
                    .map(|&o| builder.obj(ObjectArg::ImmOrOwnedObject(o)))
                    .collect::<Result<Vec<Argument>, _>>()?;
                builder.command(Command::MergeCoins(Argument::GasCoin, to_merge));
                Ok(())
            })?;
    }

    // Merge party coins into GasCoin
    if !party_coins.is_empty() {
        party_coins
            .chunks(MAX_COMMAND_ARGS)
            .try_for_each(|chunk| -> anyhow::Result<()> {
                let to_merge = chunk
                    .iter()
                    .map(|&(id, initial_shared_version)| {
                        builder.obj(ObjectArg::SharedObject {
                            id,
                            initial_shared_version,
                            mutability: sui_types::transaction::SharedObjectMutability::Mutable,
                        })
                    })
                    .collect::<Result<Vec<Argument>, _>>()?;
                builder.command(Command::MergeCoins(Argument::GasCoin, to_merge));
                Ok(())
            })?;
    }

    let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount_arg]));

    builder.command(Command::move_call(
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        ADD_STAKE_FUN_NAME.to_owned(),
        vec![],
        vec![system_state, coin, validator_arg],
    ));

    // Send GasCoin remainder to address balance
    if send_remainder > 0 {
        send_gas_remainder_to_address_balance(&mut builder, sender, send_remainder)?;
    }

    Ok(builder.finish())
}
