// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use sui_rpc::client::Client;
use sui_rpc::proto::sui::rpc::v2::{Object, owner::OwnerKind};
use sui_sdk_types::{Address, TypeTag};
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::rpc_proto_conversions::ObjectReferenceExt;
use sui_types::transaction::{Argument, Command, ObjectArg, ProgrammableTransaction};

use crate::{Currency, errors::Error};

use super::{
    MAX_COMMAND_ARGS, TransactionObjectData, TryConstructTransaction, simulate_transaction,
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

        let amount = amounts.iter().sum::<u64>();
        let coin_type = TypeTag::from_str(&currency.metadata.coin_type)
            .map_err(|e| Error::DataError(format!("Invalid coin type: {}", e)))?;
        let all_coins = client
            .select_coins(&Address::from(sender), &coin_type, amount, &[])
            .await?;

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

        // If budget is provided, we still need to select gas coins
        let pt = pay_coin_pt(recipients, amounts, &coins, &party_coins, &currency)?;
        let (budget, gas_coin_objs) =
            simulate_transaction(client, pt, sender, vec![], gas_price, budget).await?;

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
    party_coins: &[(ObjectID, SequenceNumber)],
    currency: &Currency,
) -> anyhow::Result<ProgrammableTransaction> {
    if recipients.len() != amounts.len() {
        return Err(anyhow!("Amounts length does not match recipients"));
    }
    if coins.is_empty() && party_coins.is_empty() {
        return Err(anyhow!("Cannot PayCoin without any coins"));
    }

    let mut builder = ProgrammableTransactionBuilder::new();

    let all_chunks: Vec<Vec<ObjectArg>> = coins
        .chunks(MAX_COMMAND_ARGS)
        .map(|chunk| {
            chunk
                .iter()
                .map(|&o| ObjectArg::ImmOrOwnedObject(o))
                .collect::<Vec<_>>()
        })
        .chain(party_coins.chunks(MAX_COMMAND_ARGS).map(|chunk| {
            chunk
                .iter()
                .map(|&(id, initial_shared_version)| ObjectArg::SharedObject {
                    id,
                    initial_shared_version,
                    mutability: sui_types::transaction::SharedObjectMutability::Mutable,
                })
                .collect::<Vec<_>>()
        }))
        .collect();

    let mut commands = 0;
    let mut merged: Vec<Argument> = all_chunks
        .into_iter()
        .map(|chunk| -> anyhow::Result<Argument> {
            let mut to_merge: Vec<Argument> = chunk
                .into_iter()
                .map(|o| builder.obj(o))
                .collect::<Result<Vec<Argument>, anyhow::Error>>()?;
            let merge_into = to_merge
                .pop()
                .expect("chunks() guarantees non-empty chunks");
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
        .expect("At least one of coins or party_coins is non-empty");
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
