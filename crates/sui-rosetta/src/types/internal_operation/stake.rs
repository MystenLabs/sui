// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use sui_rpc::client::Client;
use sui_rpc::proto::sui::rpc::v2::{Object, owner::OwnerKind};
use sui_sdk_types::{Address, StructTag};

use sui_types::SUI_SYSTEM_PACKAGE_ID;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber};
use sui_types::governance::ADD_STAKE_FUN_NAME;
use sui_types::rpc_proto_conversions::ObjectReferenceExt;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{Argument, CallArg, Command, ObjectArg, ProgrammableTransaction};
use sui_types::{
    base_types::SuiAddress, programmable_transaction_builder::ProgrammableTransactionBuilder,
};

use crate::errors::Error;
use crate::types::internal_operation::MAX_GAS_COINS;

use super::{
    MAX_COMMAND_ARGS, TransactionObjectData, TryConstructTransaction, simulate_transaction,
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

        // Staking needs enough SUI to cover both the stake amount and gas. We select up to 1500
        // coins (we observed ~1650 coins in a single transaction hits transaction size limits)
        // and merge them all together, then split off the stake amount.
        //
        // This handles cases where the user has sufficient total balance but no single coin or
        // simple combination covers stake + gas without merging/splitting. For example, with
        // [8, 6, 4] SUI coins and wanting to stake 10 + gas, no discrete set works - we must
        // merge first to create a coin large enough to split appropriately.
        //
        // This approach is optimal because storage refunds from merging dust outweigh smashing
        // costs by an order of magnitude, and ensures we can handle fragmented balances.
        // Use the full Coin<SUI> struct tag
        let all_coins = client
            .select_up_to_n_largest_coins(
                &Address::from(sender),
                &StructTag::sui().into(),
                1500,
                &[],
            )
            .await?;

        let total_sui_balance = all_coins.iter().map(|c| c.balance()).sum::<u64>() as i128;

        // Separate party objects (ConsensusAddressOwner) from regular objects.
        // Party objects cannot be used as gas but can be merged into the gas coin using SharedObject.
        let (party_objects, non_party_objects): (Vec<_>, Vec<_>) = all_coins
            .iter()
            .partition(|obj| obj.owner().kind() == OwnerKind::ConsensusAddress);

        let mut iter = non_party_objects
            .iter()
            .map(|obj: &&Object| obj.object_reference().try_to_object_ref());
        let gas_coins = iter
            .by_ref()
            .take(MAX_GAS_COINS)
            .collect::<Result<Vec<_>, _>>()?;

        let extra_gas_coins = iter.collect::<Result<Vec<_>, _>>()?;

        let extra_party_coins: Vec<(ObjectID, SequenceNumber)> = party_objects
            .iter()
            .map(|obj: &&Object| -> Result<_, Error> {
                let id = ObjectID::from_str(obj.object_id())
                    .map_err(|e| Error::DataError(format!("Invalid party object ID: {}", e)))?;
                let start_version = SequenceNumber::from_u64(obj.owner().version());
                Ok((id, start_version))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Always simulate to validate the transaction
        // For stake_all (amount is None), simulate with minimal amount
        // For specific amount, simulate with actual amount
        let simulation_amount = amount.unwrap_or(1_000_000_000); // 1 SUI minimum staking threshold

        let pt = stake_pt(
            validator,
            simulation_amount,
            false,
            &extra_gas_coins,
            &extra_party_coins,
        )?;
        let (budget, _) =
            simulate_transaction(client, pt, sender, gas_coins.clone(), gas_price, budget).await?;

        Ok(TransactionObjectData {
            gas_coins,
            objects: extra_gas_coins,
            party_objects: extra_party_coins,
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
    party_coins: &[(ObjectID, SequenceNumber)],
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
