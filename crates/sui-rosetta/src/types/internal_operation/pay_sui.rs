// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use sui_rpc::client::Client;
use sui_rpc::proto::sui::rpc::v2::{GetBalanceRequest, Object, owner::OwnerKind};
use sui_sdk_types::{Address, StructTag};
use sui_types::Identifier;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::gas_coin::GAS;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::rpc_proto_conversions::ObjectReferenceExt;
use sui_types::transaction::{Argument, Command, ObjectArg, ProgrammableTransaction};

use crate::errors::Error;

use super::{
    MAX_COMMAND_ARGS, MAX_GAS_COINS, TransactionObjectData, TryConstructTransaction,
    send_gas_remainder_to_address_balance, simulate_transaction,
    withdraw_coin_from_address_balance,
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

        // PaySui needs enough SUI to cover both payment amount and gas. We select up to 1500
        // coins (~1650 hits transaction size limits) and merge them all, then split off payment.
        //
        // This handles fragmented balances where no single coin covers payment + gas.
        // E.g. with [40, 35, 25] SUI and needing to pay 50 + gas, we must merge first.
        //
        // Storage refunds from merging dust outweigh smashing costs by an order of magnitude.
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

        // Compute deficit: how much of the payment comes from pre-TX address balance
        let total_payment: u64 = amounts.iter().sum();
        let deficit = total_payment.saturating_sub(coin_objects_total);

        // Path A: merge coins, withdraw deficit from AB, pay. No GasCoin → AB gas.
        let pt_a = pay_sui_pt(
            sender,
            recipients.clone(),
            amounts.clone(),
            &non_party_refs,
            &party_refs,
            deficit,
        )?;
        let sim_result =
            simulate_transaction(client, pt_a, sender, vec![], gas_price, budget).await;

        match sim_result {
            Ok((budget, gas_coin_objs)) if gas_coin_objs.is_empty() => {
                // Path A succeeded with address-balance gas
                let total_sui_balance = (coin_objects_total as i128) + (address_balance as i128);
                Ok(TransactionObjectData {
                    gas_coins: vec![],
                    objects: non_party_refs,
                    party_objects: party_refs,
                    total_sui_balance,
                    budget,
                    address_balance_withdrawal: deficit,
                })
            }
            _ => {
                // Path B: merge coins into GasCoin, pay from GasCoin, send remainder to address balance.
                let address_balance_withdrawal = address_balance;
                let total_sui_balance =
                    (coin_objects_total as i128) + (address_balance_withdrawal as i128);

                let mut gas_coin_iter = non_party_refs.iter().copied();
                let gas_coins: Vec<ObjectRef> =
                    gas_coin_iter.by_ref().take(MAX_GAS_COINS).collect();
                let extra_coins: Vec<ObjectRef> = gas_coin_iter.collect();

                // send_remainder is just a placeholder for simulation — the actual value
                // is recomputed in try_into_data. We only need the send_funds commands
                // present so the gas estimator accounts for them.
                let pt_b = pay_sui_pt_coin_gas(
                    sender,
                    recipients,
                    amounts,
                    &extra_coins,
                    &party_refs,
                    address_balance_withdrawal,
                    1,
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

/// Path A: merge coins, optionally withdraw deficit from AB, pay, send remainder to address balance.
/// No GasCoin reference → simulator auto-selects address-balance gas.
pub(crate) fn pay_sui_pt(
    sender: SuiAddress,
    recipients: Vec<SuiAddress>,
    amounts: Vec<u64>,
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
        source.ok_or_else(|| anyhow::anyhow!("No coins or address balance to pay from"))?;

    // Step 3: Split payment amounts and transfer to recipients
    let amount_args: Vec<Argument> = amounts
        .iter()
        .map(|&v| builder.pure(v))
        .collect::<Result<Vec<_>, _>>()?;
    let split_result = builder.command(Command::SplitCoins(source, amount_args));
    let Argument::Result(split_idx) = split_result else {
        anyhow::bail!("Expected Result argument from SplitCoins");
    };

    for (i, recipient) in recipients.into_iter().enumerate() {
        builder.transfer_arg(recipient, Argument::NestedResult(split_idx, i as u16));
    }

    // Step 4: Send remainder to sender's address balance
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

/// Path B: merge coins into GasCoin, pay from GasCoin, send remainder to address balance.
pub fn pay_sui_pt_coin_gas(
    sender: SuiAddress,
    recipients: Vec<SuiAddress>,
    amounts: Vec<u64>,
    coins_to_merge: &[ObjectRef],
    party_coins: &[(ObjectID, SequenceNumber)],
    address_balance_withdrawal: u64,
    send_remainder: u64,
) -> anyhow::Result<ProgrammableTransaction> {
    let mut builder = ProgrammableTransactionBuilder::new();

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

    // Withdraw address balance and merge into GasCoin
    if address_balance_withdrawal > 0 {
        let withdrawn_coin = withdraw_coin_from_address_balance(
            &mut builder,
            address_balance_withdrawal,
            GAS::type_tag(),
        )?;
        builder.command(Command::MergeCoins(Argument::GasCoin, vec![withdrawn_coin]));
    }

    // Pay from GasCoin
    builder.pay_sui(recipients, amounts)?;

    // Send GasCoin remainder to address balance
    if send_remainder > 0 {
        send_gas_remainder_to_address_balance(&mut builder, sender, send_remainder)?;
    }

    Ok(builder.finish())
}
