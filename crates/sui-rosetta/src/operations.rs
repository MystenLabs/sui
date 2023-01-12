// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::vec;

use serde::Deserialize;
use serde::Serialize;
use serde_json::{json, Value};
use sui_sdk::rpc_types::{
    SuiEvent, SuiTransactionData, SuiTransactionKind, SuiTransactionResponse,
};

use sui_types::base_types::{SequenceNumber, SuiAddress};
use sui_types::event::BalanceChangeType;
use sui_types::gas_coin::{GasCoin, GAS};
use sui_types::messages::TransactionData;
use sui_types::object::Owner;

use crate::types::{
    AccountIdentifier, Amount, CoinAction, CoinChange, CoinID, CoinIdentifier, InternalOperation,
    OperationIdentifier, OperationStatus, OperationType,
};
use crate::Error;

#[cfg(test)]
#[path = "unit_tests/operations_tests.rs"]
mod operations_tests;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Operations(Vec<Operation>);

impl FromIterator<Operation> for Operations {
    fn from_iter<T: IntoIterator<Item = Operation>>(iter: T) -> Self {
        Operations::new(iter.into_iter().collect())
    }
}

impl FromIterator<Vec<Operation>> for Operations {
    fn from_iter<T: IntoIterator<Item = Vec<Operation>>>(iter: T) -> Self {
        iter.into_iter().flatten().collect()
    }
}

impl IntoIterator for Operations {
    type Item = Operation;
    type IntoIter = vec::IntoIter<Operation>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Operations {
    pub fn new(mut ops: Vec<Operation>) -> Self {
        for (index, mut op) in ops.iter_mut().enumerate() {
            op.operation_identifier = (index as u64).into()
        }
        Self(ops)
    }

    pub fn set_status(mut self, status: Option<OperationStatus>) -> Self {
        for op in &mut self.0 {
            op.status = status
        }
        self
    }

    pub fn type_(&self) -> Option<OperationType> {
        self.0.first().map(|op| op.type_)
    }

    /// Parse operation input from rosetta to Sui transaction
    pub fn into_internal(self) -> Result<InternalOperation, Error> {
        match self
            .type_()
            .ok_or_else(|| Error::MissingInput("Operation type".into()))?
        {
            OperationType::PaySui => self.pay_sui_ops_to_internal(),
            op => Err(Error::UnsupportedOperation(op)),
        }
    }

    fn pay_sui_ops_to_internal(self) -> Result<InternalOperation, Error> {
        let mut recipients = vec![];
        let mut amounts = vec![];
        let mut sender = None;
        for op in self {
            if let (Some(amount), Some(account)) = (op.amount.clone(), op.account.clone()) {
                if amount.value.is_negative() {
                    sender = Some(account.address)
                } else {
                    recipients.push(account.address);
                    let amount = amount.value.abs();
                    if amount > u64::MAX as i128 {
                        return Err(Error::InvalidInput(
                            "Input amount exceed u64::MAX".to_string(),
                        ));
                    }
                    amounts.push(amount as u64)
                }
            }
        }
        let sender = sender.ok_or_else(|| Error::MissingInput("Sender address".to_string()))?;
        Ok(InternalOperation::PaySui {
            sender,
            recipients,
            amounts,
        })
    }

    fn from_transaction(
        tx: &SuiTransactionKind,
        sender: SuiAddress,
        status: Option<OperationStatus>,
    ) -> Result<Vec<Operation>, Error> {
        let operations = if let SuiTransactionKind::PaySui(tx) = tx {
            let recipients = tx.recipients.iter().zip(&tx.amounts);
            let mut aggregated_recipients: HashMap<SuiAddress, u64> = HashMap::new();

            for (recipient, amount) in recipients {
                *aggregated_recipients.entry(*recipient).or_default() += *amount
            }

            let mut pay_operations = aggregated_recipients
                .into_iter()
                .map(|(recipient, amount)| Operation::pay_sui(status, recipient, amount.into()))
                .collect::<Vec<_>>();
            let total_paid = tx.amounts.iter().sum::<u64>();
            pay_operations.push(Operation::pay_sui(status, sender, -(total_paid as i128)));
            pay_operations
        } else {
            let (type_, metadata) = match tx {
                SuiTransactionKind::TransferObject(tx) => {
                    (OperationType::TransferObject, json!(tx))
                }
                SuiTransactionKind::Publish(tx) => (OperationType::Publish, json!(tx.disassembled)),
                SuiTransactionKind::Call(tx) => (OperationType::MoveCall, json!(tx)),
                SuiTransactionKind::TransferSui(tx) => (OperationType::TransferSUI, json!(tx)),
                SuiTransactionKind::Pay(tx) => (OperationType::Pay, json!(tx)),
                SuiTransactionKind::PayAllSui(tx) => (OperationType::PayAllSui, json!(tx)),
                SuiTransactionKind::ChangeEpoch(tx) => (OperationType::EpochChange, json!(tx)),
                SuiTransactionKind::Genesis(tx) => (OperationType::Genesis, json!(tx)),
                SuiTransactionKind::PaySui(_) => unreachable!(),
            };
            vec![Operation::generic_op(type_, status, sender, metadata)]
        };
        Ok(operations)
    }

    fn get_balance_operation_from_events(
        events: &[SuiEvent],
        status: Option<OperationStatus>,
        balances: HashMap<SuiAddress, i128>,
    ) -> impl Iterator<Item = Operation> {
        let (balances, gas) = events
            .iter()
            .flat_map(Self::get_balance_change_from_event)
            .fold(
                (balances, HashMap::<SuiAddress, i128>::new()),
                |(mut balances, mut gas), (type_, address, amount)| {
                    if type_ == BalanceChangeType::Gas {
                        *gas.entry(address).or_default() += amount;
                    } else {
                        *balances.entry(address).or_default() += amount;
                    }
                    (balances, gas)
                },
            );

        let balance_change = balances
            .into_iter()
            .filter(|(_, amount)| *amount != 0)
            .map(move |(addr, amount)| Operation::balance_change(status, addr, amount));
        let gas = gas
            .into_iter()
            .map(|(addr, amount)| Operation::gas(addr, amount));

        balance_change.chain(gas)
    }

    fn get_balance_change_from_event(
        event: &SuiEvent,
    ) -> Option<(BalanceChangeType, SuiAddress, i128)> {
        if let SuiEvent::CoinBalanceChange {
            owner: Owner::AddressOwner(owner),
            coin_type,
            amount,
            change_type,
            ..
        } = event
        {
            // We only interested in SUI coins and account addresses
            if coin_type == &GAS::type_().to_string() {
                return Some((*change_type, *owner, *amount));
            }
        }
        None
    }
}

impl TryFrom<SuiTransactionData> for Operations {
    type Error = Error;

    fn try_from(data: SuiTransactionData) -> Result<Self, Self::Error> {
        let sender = data.sender;
        data.transactions
            .iter()
            .map(|tx| Self::from_transaction(tx, sender, None))
            .collect()
    }
}

impl TryFrom<SuiTransactionResponse> for Operations {
    type Error = Error;
    fn try_from(response: SuiTransactionResponse) -> Result<Self, Self::Error> {
        let status = Some(response.effects.status.into());
        let ops: Operations = response.certificate.data.try_into()?;
        let ops = ops.set_status(status).into_iter();

        // We will need to subtract the PaySui operation amounts from the actual balance
        // change amount extracted from event to prevent double counting.
        let mut pay_sui_balances = HashMap::new();

        let pay_sui_ops =
            ops.as_ref()
                .iter()
                .filter_map(|op| match (op.type_, &op.account, &op.amount) {
                    (OperationType::PaySui, Some(acc), Some(amount)) => {
                        Some((acc.address, -amount.value))
                    }
                    _ => None,
                });

        for (addr, amount) in pay_sui_ops {
            *pay_sui_balances.entry(addr).or_default() += amount
        }

        // Extract coin change operations from events
        let coin_change_operations = Self::get_balance_operation_from_events(
            &response.effects.events,
            status,
            pay_sui_balances,
        );

        Ok(ops.into_iter().chain(coin_change_operations).collect())
    }
}

impl TryFrom<TransactionData> for Operations {
    type Error = Error;
    fn try_from(data: TransactionData) -> Result<Self, Self::Error> {
        SuiTransactionData::try_from(data)?.try_into()
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Operation {
    operation_identifier: OperationIdentifier,
    #[serde(rename = "type")]
    pub type_: OperationType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<OperationStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<AccountIdentifier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<Amount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coin_change: Option<CoinChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl Operation {
    fn generic_op(
        type_: OperationType,
        status: Option<OperationStatus>,
        sender: SuiAddress,
        metadata: Value,
    ) -> Self {
        Operation {
            operation_identifier: Default::default(),
            type_,
            status,
            account: Some(sender.into()),
            amount: None,
            coin_change: None,
            metadata: Some(metadata),
        }
    }

    pub fn genesis(index: u64, sender: SuiAddress, coin: GasCoin) -> Self {
        Operation {
            operation_identifier: index.into(),
            type_: OperationType::Genesis,
            status: Some(OperationStatus::Success),
            account: Some(sender.into()),
            amount: Some(Amount::new(coin.value().into())),
            coin_change: Some(CoinChange {
                coin_identifier: CoinIdentifier {
                    identifier: CoinID {
                        id: *coin.id(),
                        version: SequenceNumber::new(),
                    },
                },
                coin_action: CoinAction::CoinCreated,
            }),
            metadata: None,
        }
    }

    fn pay_sui(status: Option<OperationStatus>, address: SuiAddress, amount: i128) -> Self {
        Operation {
            operation_identifier: Default::default(),
            type_: OperationType::PaySui,
            status,
            account: Some(address.into()),
            amount: Some(Amount::new(amount)),
            coin_change: None,
            metadata: None,
        }
    }

    fn balance_change(status: Option<OperationStatus>, addr: SuiAddress, amount: i128) -> Self {
        Self {
            operation_identifier: Default::default(),
            type_: OperationType::SuiBalanceChange,
            status,
            account: Some(addr.into()),
            amount: Some(Amount::new(amount)),
            coin_change: None,
            metadata: None,
        }
    }
    fn gas(addr: SuiAddress, amount: i128) -> Self {
        Self {
            operation_identifier: Default::default(),
            type_: OperationType::Gas,
            status: Some(OperationStatus::Success),
            account: Some(addr.into()),
            amount: Some(Amount::new(amount)),
            coin_change: None,
            metadata: None,
        }
    }
}
