// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use sui_rpc::client::Client;
use sui_rpc::proto::sui::rpc::v2::{GetBalanceRequest, Object, owner::OwnerKind};
use sui_sdk_types::{Address, TypeTag as SdkTypeTag};
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::rpc_proto_conversions::ObjectReferenceExt;
use sui_types::sui_sdk_types_conversions::type_tag_sdk_to_core;
use sui_types::transaction::{Argument, Command, ObjectArg, ProgrammableTransaction};

use crate::{Currency, errors::Error};

use super::{
    MAX_COMMAND_ARGS, TransactionObjectData, TryConstructTransaction, simulate_transaction,
    withdraw_coin_from_address_balance,
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
        client: &mut Client,
        gas_price: Option<u64>,
        budget: Option<u64>,
    ) -> Result<TransactionObjectData, Error> {
        let Self {
            sender,
            recipients,
            amounts,
            currency,
        } = self;

        let sdk_coin_type = SdkTypeTag::from_str(&currency.metadata.coin_type)
            .map_err(|e| Error::DataError(format!("Invalid coin type: {}", e)))?;

        let total_payment: u64 = amounts.iter().sum();

        // Query address balance for the payment coin type
        let address_balance = {
            let request = GetBalanceRequest::default()
                .with_owner(sender.to_string())
                .with_coin_type(currency.metadata.coin_type.clone());
            client
                .state_client()
                .get_balance(request)
                .await?
                .into_inner()
                .balance()
                .address_balance()
        };

        // Select all coin objects (up to 1500). Storage refunds from merging dust outweigh
        // smashing costs, so we merge as many as possible.
        let all_coins = client
            .select_up_to_n_largest_coins(&Address::from(sender), &sdk_coin_type, 1500, &[])
            .await?;

        let coins_total: u64 = all_coins.iter().map(|c| c.balance()).sum();

        // Separate party objects (ConsensusAddressOwner) from regular objects.
        let (party_objects, non_party_objects): (Vec<_>, Vec<_>) = all_coins
            .iter()
            .partition(|obj| obj.owner().kind() == OwnerKind::ConsensusAddress);

        let coins: Vec<ObjectRef> = non_party_objects
            .iter()
            .map(|obj: &&Object| obj.object_reference().try_to_object_ref())
            .collect::<Result<Vec<_>, _>>()?;

        let party_coins: Vec<(ObjectID, SequenceNumber)> = party_objects
            .iter()
            .map(|obj: &&Object| -> Result<_, Error> {
                let id = ObjectID::from_str(obj.object_id())
                    .map_err(|e| Error::DataError(format!("Invalid party object ID: {}", e)))?;
                let start_version = SequenceNumber::from_u64(obj.owner().version());
                Ok((id, start_version))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Compute deficit: how much we need from address balance beyond what coins provide
        let deficit = total_payment.saturating_sub(coins_total);
        if deficit > address_balance {
            return Err(Error::InvalidInput(format!(
                "Insufficient funds: need {} but only have {} in coins + {} in address balance",
                total_payment, coins_total, address_balance
            )));
        }

        // Merge coins directly, optionally withdraw deficit from address balance,
        // split payments and transfer coins to recipients.
        // No GasCoin reference → simulator auto-selects SUI gas.
        let pt = pay_coin_pt(
            sender,
            recipients,
            amounts,
            &coins,
            &party_coins,
            deficit,
            &currency,
        )?;
        let (budget, gas_coin_objs) =
            simulate_transaction(client, pt, sender, vec![], gas_price, budget).await?;

        if gas_coin_objs.is_empty() {
            Ok(TransactionObjectData {
                gas_coins: vec![],
                objects: coins,
                party_objects: party_coins,
                total_sui_balance: budget as i128,
                budget,
                address_balance_withdrawal: deficit,
                fss_object_count: None,
                redeem_token_amount: None,
                redeem_plan: None,
                bind_epoch: None,
            })
        } else {
            let total_sui_balance = gas_coin_objs.iter().map(|c| c.balance()).sum::<u64>() as i128;
            let gas_coins = gas_coin_objs
                .iter()
                .map(|obj: &Object| obj.object_reference().try_to_object_ref())
                .collect::<Result<Vec<_>, _>>()?;

            Ok(TransactionObjectData {
                gas_coins,
                objects: coins,
                party_objects: party_coins,
                total_sui_balance,
                budget,
                address_balance_withdrawal: deficit,
                fss_object_count: None,
                redeem_token_amount: None,
                redeem_plan: None,
                bind_epoch: None,
            })
        }
    }
}

/// Merge coin objects, optionally withdraw deficit from address balance,
/// split payments and transfer coins to each recipient.
/// Remainder stays as a coin owned by the sender.
/// No GasCoin reference → simulator auto-selects SUI gas.
pub fn pay_coin_pt(
    sender: SuiAddress,
    recipients: Vec<SuiAddress>,
    amounts: Vec<u64>,
    coins: &[ObjectRef],
    party_coins: &[(ObjectID, SequenceNumber)],
    address_balance_withdrawal: u64,
    currency: &Currency,
) -> anyhow::Result<ProgrammableTransaction> {
    let sdk_type = SdkTypeTag::from_str(&currency.metadata.coin_type)?;
    let core_type = type_tag_sdk_to_core(sdk_type)?;

    let mut builder = ProgrammableTransactionBuilder::new();

    // Step 1: Merge all coin objects into a single target (input object).
    let mut source: Option<Argument> = None;
    // Track whether the source is a command result (vs input object).
    // Command results need explicit transfer after SplitCoins; input objects
    // are implicitly returned.
    let mut source_is_command_result = false;

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

    // Step 2: Withdraw deficit from address balance and merge into source
    if address_balance_withdrawal > 0 {
        let withdrawal_coin = withdraw_coin_from_address_balance(
            &mut builder,
            address_balance_withdrawal,
            core_type.clone(),
        )?;
        match source {
            Some(target) => {
                // Merge command result into input object — input is implicitly returned
                builder.command(Command::MergeCoins(target, vec![withdrawal_coin]));
            }
            None => {
                // No coin objects; source is entirely from AB (a command result)
                source = Some(withdrawal_coin);
                source_is_command_result = true;
            }
        }
    }

    let source =
        source.ok_or_else(|| anyhow::anyhow!("No coins or address balance to pay from"))?;

    // Step 3: Split payment amounts and transfer coins to recipients
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

    // Command results (from AB withdrawal) aren't implicitly returned like input objects.
    // Transfer the remainder back to the sender (who is always the negative-amount account).
    if source_is_command_result {
        builder.transfer_arg(sender, source);
    }

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
