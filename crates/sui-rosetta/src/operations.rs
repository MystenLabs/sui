// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;
use serde_json::{json, Value};
use serde_with::serde_as;
use serde_with::DisplayFromStr;

use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::event::Event;
use sui_types::gas_coin::GAS;
use sui_types::messages::{ExecutionStatus, SingleTransactionKind, TransactionData};
use sui_types::move_package::disassemble_modules;
use sui_types::object::Owner;

use crate::types::{
    AccountIdentifier, Amount, CoinAction, CoinChange, CoinIdentifier, ConstructionMetadata,
    IndexCounter, OperationIdentifier, OperationStatus, OperationType,
};
use crate::{Error, ErrorType, SUI};

#[cfg(test)]
#[path = "unit_tests/operations_tests.rs"]
mod operations_tests;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Operation {
    pub operation_identifier: OperationIdentifier,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_operations: Vec<OperationIdentifier>,
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
    pub fn from_data(data: &TransactionData) -> Result<Vec<Operation>, anyhow::Error> {
        let sender = data.signer();
        let mut counter = IndexCounter::default();
        let mut ops = data
            .kind
            .single_transactions()
            .flat_map(|tx| parse_operations(tx, sender, &mut counter, None, None))
            .flatten()
            .collect::<Vec<_>>();
        let gas = Operation::gas_budget(&mut counter, None, data.gas(), data.gas_budget, sender);
        ops.push(gas);
        Ok(ops)
    }

    pub fn from_data_and_events(
        data: &TransactionData,
        status: &ExecutionStatus,
        events: &Vec<Event>,
    ) -> Result<Vec<Operation>, anyhow::Error> {
        let sender = data.signer();
        let mut counter = IndexCounter::default();
        let status = Some((status).into());
        let mut ops = data
            .kind
            .single_transactions()
            .flat_map(|tx| parse_operations(tx, sender, &mut counter, status, Some(events)))
            .flatten()
            .collect::<Vec<_>>();
        let gas = Operation::gas_budget(&mut counter, status, data.gas(), data.gas_budget, sender);
        ops.push(gas);
        Ok(ops)
    }

    fn get_coin_operation_from_events(
        events: &[Event],
        status: Option<OperationStatus>,
        counter: &mut IndexCounter,
    ) -> Vec<Operation> {
        events
            .iter()
            .flat_map(|event| Self::get_coin_operation_from_event(event, status, counter))
            .collect()
    }

    fn get_coin_operation_from_event(
        event: &Event,
        status: Option<OperationStatus>,
        counter: &mut IndexCounter,
    ) -> Vec<Operation> {
        let mut operations = vec![];
        if let Event::CoinBalanceChange {
            owner: Owner::AddressOwner(owner),
            coin_type,
            amount,
            ..
        } = event
        {
            // We only interested in SUI coins and account addresses
            if coin_type == &GAS::type_().to_string() {
                operations.push(Operation {
                    operation_identifier: counter.next_idx().into(),
                    related_operations: vec![],
                    type_: OperationType::SuiBalanceChange,
                    status,
                    account: Some(AccountIdentifier { address: *owner }),
                    amount: Some(Amount {
                        value: (*amount).into(),
                        currency: SUI.clone(),
                    }),
                    coin_change: None,
                    metadata: None,
                });
            }
        }
        operations
    }

    /// Parse operation input from rosetta to Sui transaction
    pub async fn create_data(
        operations: Vec<Operation>,
        metadata: ConstructionMetadata,
    ) -> Result<TransactionData, Error> {
        // Currently only PaySui is support,
        // first operation is PaySui operation and second operation is the budget operation.
        if operations.len() != 2 || operations[0].type_ != OperationType::PaySui {
            return Err(Error::new_with_msg(
                ErrorType::InvalidInput,
                "Malformed operation.",
            ));
        }
        let pay_sui_op = &operations[0];
        let budget_op = &operations[1];

        let account = pay_sui_op
            .account
            .as_ref()
            .ok_or_else(|| Error::missing_input("operation.account"))?;
        let address = account.address;
        let pay_sui = pay_sui_op
            .metadata
            .clone()
            .ok_or_else(|| Error::missing_input("operation.metadata"))?;
        let pay_sui: PaySuiMetadata = serde_json::from_value(pay_sui)
            .map_err(|e| Error::new_with_cause(ErrorType::MalformedOperationError, e))?;
        let gas = metadata.sender_coins[0];
        let budget_value = budget_op
            .metadata
            .clone()
            .and_then(|v| v.pointer("/budget").cloned())
            .ok_or_else(|| Error::missing_input("gas budget"))?;
        let budget = budget_value
            .as_u64()
            .or_else(|| budget_value.as_str().and_then(|s| u64::from_str(s).ok()))
            .ok_or_else(|| {
                Error::new_with_msg(
                    ErrorType::InvalidInput,
                    format!("Cannot parse gas budget : [{budget_value}]").as_str(),
                )
            })?;

        Ok(TransactionData::new_pay_sui(
            address,
            metadata.sender_coins,
            pay_sui.recipients,
            pay_sui.amounts,
            gas,
            budget,
        ))
    }

    pub fn gas_budget(
        counter: &mut IndexCounter,
        status: Option<OperationStatus>,
        gas: ObjectRef,
        budget: u64,
        sender: SuiAddress,
    ) -> Self {
        Self {
            operation_identifier: counter.next_idx().into(),
            related_operations: vec![],
            type_: OperationType::GasBudget,
            status,
            account: Some(AccountIdentifier { address: sender }),
            amount: None,
            coin_change: Some(CoinChange {
                coin_identifier: CoinIdentifier {
                    identifier: gas.into(),
                },
                coin_action: CoinAction::CoinSpent,
            }),
            metadata: Some(json!({ "budget": budget })),
        }
    }
}

fn parse_operations(
    tx: &SingleTransactionKind,
    sender: SuiAddress,
    counter: &mut IndexCounter,
    status: Option<OperationStatus>,
    events: Option<&Vec<Event>>,
) -> Result<Vec<Operation>, anyhow::Error> {
    let (type_, metadata) = match tx {
        SingleTransactionKind::TransferObject(tx) => (OperationType::TransferObject, json!(tx)),
        SingleTransactionKind::Publish(tx) => {
            let disassembled = disassemble_modules(tx.modules.iter())?;
            (OperationType::Publish, json!(disassembled))
        }
        SingleTransactionKind::Call(tx) => (OperationType::MoveCall, json!(tx)),
        SingleTransactionKind::TransferSui(tx) => (OperationType::TransferSUI, json!(tx)),
        SingleTransactionKind::Pay(tx) => (OperationType::Pay, json!(tx)),
        SingleTransactionKind::PaySui(tx) => {
            let pay_sui = PaySuiMetadata {
                recipients: tx.recipients.clone(),
                amounts: tx.amounts.clone(),
            };
            (OperationType::PaySui, json!(pay_sui))
        }
        SingleTransactionKind::PayAllSui(tx) => (OperationType::PayAllSui, json!(tx)),
        SingleTransactionKind::ChangeEpoch(tx) => (OperationType::EpochChange, json!(tx)),
    };

    let mut operations = vec![Operation {
        operation_identifier: counter.next_idx().into(),
        related_operations: vec![],
        type_,
        status,
        account: Some(AccountIdentifier { address: sender }),
        amount: None,
        coin_change: None,
        metadata: Some(metadata),
    }];

    // Extract coin change operations from events
    if let Some(events) = events {
        let coin_change_operations =
            Operation::get_coin_operation_from_events(events, status, counter);
        operations.extend(coin_change_operations);
    }
    Ok(operations)
}

#[serde_as]
#[derive(Serialize, Deserialize)]
struct PaySuiMetadata {
    pub recipients: Vec<SuiAddress>,
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub amounts: Vec<u64>,
}
