// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::collections::HashMap;
use std::str::FromStr;
use std::vec;

use serde::Deserialize;
use serde::Serialize;
use sui_sdk::rpc_types::{
    SuiEvent, SuiMoveCall, SuiPaySui, SuiTransactionData, SuiTransactionKind,
    SuiTransactionResponse,
};

use sui_types::base_types::{SequenceNumber, SuiAddress};
use sui_types::committee::EpochId;
use sui_types::event::BalanceChangeType;
use sui_types::gas_coin::{GasCoin, GAS};
use sui_types::governance::{
    ADD_DELEGATION_LOCKED_COIN_FUN_NAME, ADD_DELEGATION_MUL_COIN_FUN_NAME,
};
use sui_types::messages::TransactionData;
use sui_types::object::Owner;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::SUI_FRAMEWORK_OBJECT_ID;

use crate::types::{
    AccountIdentifier, Amount, CoinAction, CoinChange, CoinID, CoinIdentifier, InternalOperation,
    OperationIdentifier, OperationStatus, OperationType, PreprocessMetadata,
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
    pub fn into_internal(
        self,
        metadata: Option<PreprocessMetadata>,
    ) -> Result<InternalOperation, Error> {
        match (
            self.type_()
                .ok_or_else(|| Error::MissingInput("Operation type".into()))?,
            metadata,
        ) {
            (OperationType::PaySui, _) => self.pay_sui_ops_to_internal(),
            (
                OperationType::Delegation,
                Some(PreprocessMetadata::Delegation { locked_until_epoch }),
            ) => self.delegation_ops_to_internal(locked_until_epoch),
            (OperationType::Delegation, _) => self.delegation_ops_to_internal(None),
            (op, _) => Err(Error::UnsupportedOperation(op)),
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

    fn delegation_ops_to_internal(
        self,
        locked_until_epoch: Option<EpochId>,
    ) -> Result<InternalOperation, Error> {
        if self.0.len() != 1 {
            return Err(Error::MalformedOperationError(
                "Delegation should only have one operation.".into(),
            ));
        }
        // Checked above, safe to unwrap.
        let op = self.into_iter().next().unwrap();
        let sender = op
            .account
            .ok_or_else(|| Error::MissingInput("Sender address".to_string()))?
            .address;
        let metadata = op
            .metadata
            .ok_or_else(|| Error::MissingInput("Delegation metadata".to_string()))?;

        let amount = op
            .amount
            .ok_or_else(|| Error::MissingInput("Amount".to_string()))?
            .value
            .unsigned_abs();

        let OperationMetadata::Delegation {  validator } = metadata else {
            return Err(Error::InvalidInput("Cannot find delegation info from metadata.".into()))
        };

        Ok(InternalOperation::Delegation {
            sender,
            validator,
            amount,
            locked_until_epoch,
        })
    }

    fn from_transaction(
        tx: SuiTransactionKind,
        sender: SuiAddress,
        status: Option<OperationStatus>,
    ) -> Result<Vec<Operation>, Error> {
        Ok(match tx {
            SuiTransactionKind::PaySui(tx) => Self::parse_pay_sui_operations(sender, tx, status),
            SuiTransactionKind::Call(tx) => Self::parse_call_operations(sender, status, tx)?,
            _ => vec![Operation::generic_op(status, sender, tx)],
        })
    }

    fn parse_call_operations(
        sender: SuiAddress,
        status: Option<OperationStatus>,
        tx: SuiMoveCall,
    ) -> Result<Vec<Operation>, Error> {
        if Self::is_delegation_call(&tx) {
            let (amount, validator) = match &tx.arguments[..] {
                [_, _, amount, validator] => {
                    let amount = amount.to_json_value().as_array().map(|v| {
                        // value is a byte array
                        let bytes = v.iter().flat_map(|v| v.as_u64().map(|n| n as u8)).collect::<Vec<_>>();
                        let option: Vec<u64> = bcs::from_bytes(&bytes)?;
                        if let Some(amount) = option.first() {
                            Ok(*amount as u128)
                        } else {
                            Err(Error::InternalError(anyhow!("Cannot extract delegation amount from move call.")))
                        }
                    }).transpose()?;
                    let validator = validator
                        .to_json_value()
                        .as_str()
                        .map(SuiAddress::from_str)
                        .transpose()?
                        .ok_or_else(|| Error::InternalError(anyhow!("Error parsing Validator address from call arg.")))?;
                    (amount, validator)
                },
                _ => return Err(Error::InternalError(anyhow!("Error encountered when extracting arguments from move call, expecting 4 elements, got {}", tx.arguments.len()))),
            };

            let amount = amount.map(|amount| Amount::new(-(amount as i128)));

            return Ok(vec![Operation {
                operation_identifier: Default::default(),
                type_: OperationType::Delegation,
                status,
                account: Some(sender.into()),
                amount,
                coin_change: None,
                metadata: Some(OperationMetadata::Delegation { validator }),
            }]);
        }
        Ok(vec![Operation::generic_op(
            status,
            sender,
            SuiTransactionKind::Call(tx),
        )])
    }

    fn is_delegation_call(tx: &SuiMoveCall) -> bool {
        tx.package.object_id == SUI_FRAMEWORK_OBJECT_ID
            && tx.module == SUI_SYSTEM_MODULE_NAME.as_str()
            && (tx.function == ADD_DELEGATION_LOCKED_COIN_FUN_NAME.as_str()
                || tx.function == ADD_DELEGATION_MUL_COIN_FUN_NAME.as_str())
    }

    fn parse_pay_sui_operations(
        sender: SuiAddress,
        tx: SuiPaySui,
        status: Option<OperationStatus>,
    ) -> Vec<Operation> {
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
            .into_iter()
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

        // We will need to subtract the operation amounts from the actual balance
        // change amount extracted from event to prevent double counting.
        let accounted_balances = ops
            .as_ref()
            .iter()
            .filter_map(|op| match (&op.account, &op.amount) {
                (Some(acc), Some(amount)) => Some((acc.address, -amount.value)),
                _ => None,
            })
            .fold(HashMap::new(), |mut balances, (addr, amount)| {
                *balances.entry(addr).or_default() += amount;
                balances
            });

        // Extract coin change operations from events
        let coin_change_operations = Self::get_balance_operation_from_events(
            &response.effects.events,
            status,
            accounted_balances,
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
    pub metadata: Option<OperationMetadata>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum OperationMetadata {
    GenericTransaction(SuiTransactionKind),
    Delegation { validator: SuiAddress },
}

impl Operation {
    fn generic_op(
        status: Option<OperationStatus>,
        sender: SuiAddress,
        tx: SuiTransactionKind,
    ) -> Self {
        Operation {
            operation_identifier: Default::default(),
            type_: (&tx).into(),
            status,
            account: Some(sender.into()),
            amount: None,
            coin_change: None,
            metadata: Some(OperationMetadata::GenericTransaction(tx)),
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
