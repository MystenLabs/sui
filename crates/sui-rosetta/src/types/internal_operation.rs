// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use futures::StreamExt;
use prost_types::FieldMask;
use serde::{Deserialize, Serialize};
use sui_rpc::client::v2::Client;
use sui_rpc::proto::sui::rpc::v2::Bcs;

use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{
    simulate_transaction_request::TransactionChecks, ListOwnedObjectsRequest,
    SimulateTransactionRequest, Transaction,
};
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::digests::ObjectDigest;
use sui_types::rpc_proto_conversions::ObjectReferenceExt;
use sui_types::transaction::{ProgrammableTransaction, TransactionData};

use crate::errors::Error;
use crate::types::ConstructionMetadata;
use pay_coin::pay_coin_pt;
pub use pay_coin::PayCoin;
use pay_sui::pay_sui_pt;
pub use pay_sui::PaySui;
use stake::stake_pt;
pub use stake::Stake;
use withdraw_stake::withdraw_stake_pt;
pub use withdraw_stake::WithdrawStake;

mod pay_coin;
mod pay_sui;
mod stake;
mod withdraw_stake;

pub const MAX_GAS_COINS: usize = 255;
const MAX_COMMAND_ARGS: usize = 511;
const MAX_GAS_BUDGET: u64 = 50_000_000_000;
/// Minimum gas-units a tx might need
const START_GAS_UNITS: u64 = 1_000;

pub struct TransactionObjectData {
    pub gas_coins: Vec<ObjectRef>,
    pub extra_gas_coins: Vec<ObjectRef>,
    pub objects: Vec<ObjectRef>,
    /// Refers to the sum of the `Coin<SUI>` balance of the coins participating in the transaction;
    /// either as gas or as objects.
    pub total_sui_balance: i128,
    pub budget: u64,
}

#[async_trait]
#[enum_dispatch]
pub trait TryConstructTransaction {
    async fn try_fetch_needed_objects(
        self,
        client: &mut Client,
        gas_price: Option<u64>,
        budget: Option<u64>,
    ) -> Result<TransactionObjectData, Error>;
}

#[enum_dispatch(TryConstructTransaction)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum InternalOperation {
    PaySui(PaySui),
    PayCoin(PayCoin),
    Stake(Stake),
    WithdrawStake(WithdrawStake),
}

impl InternalOperation {
    pub fn sender(&self) -> SuiAddress {
        match self {
            InternalOperation::PaySui(PaySui { sender, .. })
            | InternalOperation::PayCoin(PayCoin { sender, .. })
            | InternalOperation::Stake(Stake { sender, .. })
            | InternalOperation::WithdrawStake(WithdrawStake { sender, .. }) => *sender,
        }
    }

    /// Combine with ConstructionMetadata to form the TransactionData
    pub fn try_into_data(self, metadata: ConstructionMetadata) -> Result<TransactionData, Error> {
        let pt = match self {
            Self::PaySui(PaySui {
                recipients,
                amounts,
                ..
            }) => pay_sui_pt(recipients, amounts, &metadata.extra_gas_coins)?,
            Self::PayCoin(PayCoin {
                recipients,
                amounts,
                ..
            }) => {
                let currency = &metadata
                    .currency
                    .ok_or(anyhow!("metadata.coin_type is needed to PayCoin"))?;
                pay_coin_pt(recipients, amounts, &metadata.objects, currency)?
            }
            InternalOperation::Stake(Stake {
                validator, amount, ..
            }) => {
                let (stake_all, amount) = match amount {
                    Some(amount) => (false, amount),
                    None => {
                        if (metadata.total_coin_value - metadata.budget as i128) < 0 {
                            return Err(anyhow!(
                                "ConstructionMetadata malformed. total_coin_value - budget < 0"
                            )
                            .into());
                        }
                        (true, metadata.total_coin_value as u64 - metadata.budget)
                    }
                };
                stake_pt(validator, amount, stake_all, &metadata.extra_gas_coins)?
            }
            InternalOperation::WithdrawStake(WithdrawStake { stake_ids, .. }) => {
                let withdraw_all = stake_ids.is_empty();
                withdraw_stake_pt(metadata.objects, withdraw_all)?
            }
        };

        Ok(TransactionData::new_programmable(
            metadata.sender,
            metadata.gas_coins,
            pt,
            metadata.budget,
            metadata.gas_price,
        ))
    }
}

async fn budget_from_dry_run(
    client: &mut Client,
    pt: ProgrammableTransaction,
    sender: SuiAddress,
    gas_price: Option<u64>,
) -> Result<u64, Error> {
    //TODO this should probably be in the SDK
    let gas_price = match gas_price {
        Some(p) => p,
        None => client.get_reference_gas_price().await? + 100,
    };

    let tx_data = TransactionData::new_programmable(sender, vec![], pt, MAX_GAS_BUDGET, gas_price);

    let tx_bytes = bcs::to_bytes(&tx_data)
        .map_err(|e| Error::InvalidInput(format!("Failed to serialize transaction: {}", e)))?;

    let bcs = Bcs::default().with_value(tx_bytes);
    let transaction = Transaction::default().with_bcs(bcs);

    let request = SimulateTransactionRequest::new(transaction)
        .with_read_mask(FieldMask::from_paths([
            "transaction.effects.status",
            "transaction.effects.gas_used",
        ]))
        .with_checks(TransactionChecks::Enabled)
        .with_do_gas_selection(false);

    let response = client
        .execution_client()
        .simulate_transaction(request)
        .await?
        .into_inner();

    let effects = response.transaction().effects();

    if !effects.status().success() {
        return Err(Error::TransactionDryRunError(
            effects.status().error().clone(),
        ));
    }

    let gas_used = effects.gas_used();
    Ok(gas_used.computation_cost() + gas_used.storage_cost())
}

async fn collect_coins_until_budget_met(
    client: &mut Client,
    sender: SuiAddress,
    pt: impl Fn(&[(ObjectID, SequenceNumber, ObjectDigest)]) -> anyhow::Result<ProgrammableTransaction>,
    amount: u64,
    gas_price: Option<u64>,
) -> Result<TransactionObjectData, Error> {
    // Fetch it once instead of fetching it again and again in the below loop.
    let gas_price = match gas_price {
        Some(p) => p,
        None => client.get_reference_gas_price().await? + 100,
    };

    let mut all_coins = vec![];
    let mut gas_coins: Vec<_>;
    let mut extra_gas_coins: Vec<_>;
    let mut gathered = 0;
    let mut budget = START_GAS_UNITS * gas_price;
    // We need to dry-run in a loop, because depending on the amount of coins used the tx might
    // differ slightly: (merge / no merge / number of merge-coins)

    let coin_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type(format!("0x2::coin::Coin<{}>", sui_sdk::SUI_COIN_TYPE))
        .with_page_size(1000u32)
        .with_read_mask(FieldMask::from_paths([
            "object_id",
            "version",
            "digest",
            "balance",
        ]));

    let cloned_client = client.clone();
    let mut coins_stream = Box::pin(cloned_client.list_owned_objects(coin_request.clone()));

    loop {
        // Collect coins until we have enough for amount + budget using the persistent stream
        while gathered < amount + budget {
            match coins_stream.next().await {
                Some(Ok(object)) => {
                    gathered += object.balance();
                    all_coins.push(object);

                    if gathered >= amount + budget {
                        break;
                    }
                }
                Some(Err(e)) => return Err(e.into()),
                None => break, // Stream exhausted
            }
        }

        if gathered < amount + budget {
            return Err(Error::InvalidInput(format!(
                "Address {sender} does not have amount: {amount} + budget: {budget} balance. SUI balance: {gathered}."
            )));
        }

        // The coins to merge should be used as transaction object inputs, as
        // `TransactionData::new_programmable` used in `InternalOperation::try_into_data`,
        // uses all coins passed as gas payment.
        let mut iter = all_coins
            .iter()
            .map(|obj| obj.object_reference().try_to_object_ref());
        gas_coins = iter
            .by_ref()
            .take(MAX_GAS_COINS)
            .collect::<Result<Vec<_>, _>>()?;
        extra_gas_coins = iter.collect::<Result<Vec<_>, _>>()?;
        let pt = pt(&extra_gas_coins)?;
        budget = budget_from_dry_run(client, pt.clone(), sender, Some(gas_price)).await?;
        // If we have already gathered the needed amount of coins we don't need to dry run again,
        // as the transaction will be the same.
        if budget + amount <= gathered {
            break;
        }
    }

    let total_sui_balance = all_coins.iter().map(|o| o.balance()).sum::<u64>() as i128;
    Ok(TransactionObjectData {
        gas_coins,
        extra_gas_coins,
        objects: vec![],
        total_sui_balance,
        budget,
    })
}
