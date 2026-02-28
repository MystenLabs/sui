// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use anyhow::anyhow;
use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use prost_types::FieldMask;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sui_rpc::client::Client;
use sui_rpc::proto::sui::rpc::v2::{
    BatchGetObjectsRequest, GetObjectRequest, Object, get_object_result,
};

use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{
    GasPayment, ObjectReference, ProgrammableTransaction as ProtoProgrammableTransaction,
    SimulateTransactionRequest, Transaction, TransactionKind,
    simulate_transaction_request::TransactionChecks, transaction_kind,
};
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::digests::{ChainIdentifier, CheckpointDigest};
use sui_types::gas_coin::GAS;
use sui_types::transaction::{
    Argument, CallArg, Command, FundsWithdrawalArg, ProgrammableTransaction, TransactionData,
};

use crate::errors::Error;
use crate::types::ConstructionMetadata;
pub use pay_coin::PayCoin;
pub(crate) use pay_coin::pay_coin_pt;
pub use pay_sui::PaySui;
use pay_sui::{pay_sui_pt, pay_sui_pt_coin_gas};
pub use stake::Stake;
use stake::{stake_pt, stake_pt_coin_gas};
pub use withdraw_stake::WithdrawStake;
use withdraw_stake::withdraw_stake_pt;

mod pay_coin;
mod pay_sui;
mod stake;
mod withdraw_stake;

pub const MAX_GAS_COINS: usize = 255;
const MAX_COMMAND_ARGS: usize = 511;

pub struct TransactionObjectData {
    pub gas_coins: Vec<ObjectRef>,
    /// For PaySui/Stake: extra gas coins to merge into gas
    /// For PayCoin: payment coins of the specified type
    /// For WithdrawStake: stake objects to withdraw
    pub objects: Vec<ObjectRef>,
    /// Party-owned (ConsensusAddress) version of objects
    pub party_objects: Vec<(ObjectID, SequenceNumber)>,
    /// Refers to the sum of the `Coin<SUI>` balance of the coins participating in the transaction;
    /// either as gas or as objects.
    pub total_sui_balance: i128,
    pub budget: u64,
    /// Amount to withdraw from address balance for payment
    pub address_balance_withdrawal: u64,
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
        let use_addr_balance_gas = metadata.gas_coins.is_empty();
        let withdrawal = metadata.address_balance_withdrawal;
        let pt = match self {
            Self::PaySui(PaySui {
                sender,
                recipients,
                amounts,
            }) => {
                let coins = if !metadata.objects.is_empty() {
                    &metadata.objects
                } else {
                    &metadata.extra_gas_coins
                };
                if use_addr_balance_gas {
                    pay_sui_pt(
                        sender,
                        recipients,
                        amounts,
                        coins,
                        &metadata.party_objects,
                        withdrawal,
                    )?
                } else {
                    let payment_total: u64 = amounts.iter().sum();
                    let send_remainder = metadata
                        .total_coin_value
                        .saturating_sub(payment_total as i128)
                        .saturating_sub(metadata.budget as i128)
                        .max(0) as u64;
                    pay_sui_pt_coin_gas(
                        sender,
                        recipients,
                        amounts,
                        coins,
                        &metadata.party_objects,
                        withdrawal,
                        send_remainder,
                    )?
                }
            }
            Self::PayCoin(PayCoin {
                sender,
                recipients,
                amounts,
                ..
            }) => {
                let currency = &metadata
                    .currency
                    .ok_or(anyhow!("metadata.coin_type is needed to PayCoin"))?;
                pay_coin_pt(
                    sender,
                    recipients,
                    amounts,
                    &metadata.objects,
                    &metadata.party_objects,
                    withdrawal,
                    currency,
                )?
            }
            InternalOperation::Stake(Stake {
                sender,
                validator,
                amount,
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
                let coins = if !metadata.objects.is_empty() {
                    &metadata.objects
                } else {
                    &metadata.extra_gas_coins
                };
                if use_addr_balance_gas {
                    stake_pt(
                        sender,
                        validator,
                        amount,
                        stake_all,
                        coins,
                        &metadata.party_objects,
                        withdrawal,
                    )?
                } else {
                    let send_remainder = metadata
                        .total_coin_value
                        .saturating_sub(amount as i128)
                        .saturating_sub(metadata.budget as i128)
                        .max(0) as u64;
                    stake_pt_coin_gas(
                        sender,
                        validator,
                        amount,
                        stake_all,
                        coins,
                        &metadata.party_objects,
                        withdrawal,
                        send_remainder,
                    )?
                }
            }
            InternalOperation::WithdrawStake(WithdrawStake { sender, stake_ids }) => {
                let withdraw_all = stake_ids.is_empty();
                withdraw_stake_pt(sender, metadata.objects, withdraw_all)?
            }
        };

        if metadata.gas_coins.is_empty() {
            let chain_id_str = metadata
                .chain_id
                .ok_or(anyhow!("chain_id required for address-balance gas"))?;
            let digest = CheckpointDigest::from_str(&chain_id_str)
                .map_err(|e| anyhow!("invalid chain_id: {e}"))?;
            let chain_id = ChainIdentifier::from(digest);
            let epoch = metadata
                .epoch
                .ok_or(anyhow!("epoch required for address-balance gas"))?;
            let nonce = rand::thread_rng().r#gen::<u32>();

            Ok(TransactionData::new_programmable_with_address_balance_gas(
                metadata.sender,
                pt,
                metadata.budget,
                metadata.gas_price,
                chain_id,
                epoch,
                nonce,
            ))
        } else {
            Ok(TransactionData::new_programmable(
                metadata.sender,
                metadata.gas_coins,
                pt,
                metadata.budget,
                metadata.gas_price,
            ))
        }
    }
}

/// Withdraw from address balance as a Coin<T>.
/// FundsWithdrawal → redeem_funds → from_balance → Coin<T>
pub(crate) fn withdraw_coin_from_address_balance(
    builder: &mut sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder,
    amount: u64,
    type_tag: TypeTag,
) -> anyhow::Result<Argument> {
    let withdrawal_arg = builder.input(CallArg::FundsWithdrawal(
        FundsWithdrawalArg::balance_from_sender(amount, type_tag.clone()),
    ))?;

    let balance = builder.command(Command::move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance")?,
        Identifier::new("redeem_funds")?,
        vec![type_tag.clone()],
        vec![withdrawal_arg],
    ));

    let coin = builder.command(Command::move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin")?,
        Identifier::new("from_balance")?,
        vec![type_tag],
        vec![balance],
    ));

    Ok(coin)
}

/// Send the GasCoin remainder to the sender's address balance (Path B).
/// SplitCoins(GasCoin, [amount]) → into_balance → send_funds
pub(crate) fn send_gas_remainder_to_address_balance(
    builder: &mut sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder,
    sender: SuiAddress,
    amount: u64,
) -> anyhow::Result<()> {
    let sui_type_tag = GAS::type_tag();

    let amount_arg = builder.pure(amount)?;
    let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount_arg]));

    let balance = builder.command(Command::move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin")?,
        Identifier::new("into_balance")?,
        vec![sui_type_tag.clone()],
        vec![coin],
    ));

    let sender_arg = builder.pure(sender)?;
    builder.command(Command::move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance")?,
        Identifier::new("send_funds")?,
        vec![sui_type_tag],
        vec![balance, sender_arg],
    ));

    Ok(())
}

/// RPC auto-selects gas coins if empty, uses reference gas price if None, and estimates budget if None.
/// Returns the resolved budget and gas coins used by the transaction.
async fn simulate_transaction(
    client: &mut Client,
    pt: ProgrammableTransaction,
    sender: SuiAddress,
    gas_coins: Vec<ObjectRef>,
    gas_price: Option<u64>,
    budget: Option<u64>,
) -> Result<(u64, Vec<Object>), Error> {
    let ptb_proto: ProtoProgrammableTransaction = pt.into();
    let mut transaction = Transaction::default()
        .with_kind(
            TransactionKind::default()
                .with_programmable_transaction(ptb_proto)
                .with_kind(transaction_kind::Kind::ProgrammableTransaction),
        )
        .with_sender(sender.to_string());

    let mut gas_payment = GasPayment::default();
    gas_payment.objects = gas_coins
        .into_iter()
        .map(|gas_ref| {
            let mut obj_ref = ObjectReference::default();
            obj_ref.object_id = Some(gas_ref.0.to_string());
            obj_ref.version = Some(gas_ref.1.value());
            obj_ref.digest = Some(gas_ref.2.to_string());
            obj_ref
        })
        .collect();
    gas_payment.budget = budget;
    gas_payment.price = gas_price;
    gas_payment.owner = Some(sender.to_string());
    transaction.gas_payment = Some(gas_payment);

    let request = SimulateTransactionRequest::default()
        .with_transaction(transaction)
        .with_read_mask(FieldMask::from_paths([
            "transaction.effects.status",
            "transaction.transaction.gas_payment",
        ]))
        .with_checks(TransactionChecks::Enabled)
        .with_do_gas_selection(true);

    let response = client
        .execution_client()
        .simulate_transaction(request)
        .await?
        .into_inner();

    let executed_tx = response.transaction();
    let effects = executed_tx.effects();
    if !effects.status().success() {
        return Err(Error::TransactionDryRunError(Box::new(
            effects.status().error().clone(),
        )));
    }

    let resolved_tx = executed_tx.transaction();
    let gas_payment = resolved_tx.gas_payment();

    // When gas_payment has no objects, the transaction uses address-balance gas.
    // Skip the batch fetch and return empty gas coins to signal this.
    let gas_objects = gas_payment.objects();
    if gas_objects.is_empty() {
        return Ok((gas_payment.budget(), vec![]));
    }

    let mut batch_request =
        BatchGetObjectsRequest::default().with_read_mask(FieldMask::from_paths([
            "object_id",
            "version",
            "digest",
            "balance",
        ]));

    for obj_ref in gas_objects {
        let get_request = GetObjectRequest::default()
            .with_object_id(obj_ref.object_id().to_string())
            .with_version(obj_ref.version());
        batch_request.requests.push(get_request);
    }

    let batch_response = client
        .ledger_client()
        .batch_get_objects(batch_request)
        .await?
        .into_inner();

    let mut gas_coins = Vec::new();
    for result in batch_response.objects {
        match result.result {
            Some(get_object_result::Result::Object(obj)) => {
                gas_coins.push(obj);
            }
            Some(get_object_result::Result::Error(err)) => {
                return Err(Error::DataError(format!(
                    "Failed to fetch gas coin object: {:?}",
                    err
                )));
            }
            None => {
                return Err(Error::DataError(
                    "Failed to fetch gas coin object: no result returned".to_string(),
                ));
            }
            Some(_) => {
                return Err(Error::DataError(
                    "Failed to fetch gas coin object: unexpected result type".to_string(),
                ));
            }
        }
    }

    Ok((gas_payment.budget(), gas_coins))
}
