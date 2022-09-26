// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Neg;
use std::str::FromStr;

use anyhow::anyhow;
use serde::Deserialize;
use serde::Serialize;
use serde_json::{json, Value};

use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::coin::{COIN_JOIN_FUNC_NAME, COIN_MODULE_NAME, COIN_SPLIT_VEC_FUNC_NAME};
use sui_types::event::{Event, TransferType};
use sui_types::messages::{
    CallArg, InputObjectKind, MoveCall, ObjectArg, Pay, SingleTransactionKind, TransactionData,
    TransactionEffects,
};
use sui_types::move_package::disassemble_modules;
use sui_types::{parse_sui_struct_tag, SUI_FRAMEWORK_OBJECT_ID};

use crate::types::{
    AccountIdentifier, Amount, CoinAction, CoinChange, CoinID, CoinIdentifier,
    ConstructionMetadata, IndexCounter, OperationIdentifier, OperationStatus, OperationType,
    SignedValue,
};
use crate::ErrorType::UnsupportedOperation;
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
        let budget = data.gas_budget;
        let gas = data.gas();
        let sender = data.signer();
        Ok(data
            .kind
            .single_transactions()
            .flat_map(|tx| {
                parse_operations(
                    tx,
                    budget,
                    gas,
                    sender,
                    &mut IndexCounter::default(),
                    None,
                    None,
                )
            })
            .flatten()
            .collect::<Vec<_>>())
    }

    pub fn from_data_and_effect(
        data: &TransactionData,
        effects: &TransactionEffects,
    ) -> Result<Vec<Operation>, anyhow::Error> {
        let budget = data.gas_budget;
        let gas = data.gas();
        let sender = data.signer();
        let mut counter = IndexCounter::default();
        let status = Some((&effects.status).into());
        let mut operations = data
            .kind
            .single_transactions()
            .flat_map(|tx| {
                parse_operations(tx, budget, gas, sender, &mut counter, status, Some(effects))
            })
            .flatten()
            .collect::<Vec<_>>();

        operations.push(Operation {
            operation_identifier: counter.next_idx().into(),
            related_operations: vec![],
            type_: OperationType::GasSpent,
            status,
            account: Some(AccountIdentifier { address: sender }),
            amount: Some(Amount {
                value: effects.gas_used.net_gas_usage().neg().into(),
                currency: SUI.clone(),
            }),
            coin_change: None,
            metadata: None,
        });

        Ok(operations)
    }

    fn get_coin_operation_from_event(
        input_objects: Vec<InputObjectKind>,
        events: &[Event],
        status: Option<OperationStatus>,
        counter: &mut IndexCounter,
    ) -> Vec<Operation> {
        events
            .iter()
            .find_map(|event| {
                if let Event::TransferObject {
                    sender,
                    recipient,
                    object_id,
                    version,
                    type_,
                    amount,
                    ..
                } = event
                {
                    if type_ == &TransferType::Coin {
                        let input = input_objects.iter().find_map(|kind| {
                            if let InputObjectKind::ImmOrOwnedMoveObject((id, version, _)) = kind {
                                if id == object_id {
                                    return Some(CoinChange {
                                        coin_identifier: CoinIdentifier {
                                            identifier: CoinID {
                                                id: *id,
                                                version: *version,
                                            },
                                        },
                                        coin_action: CoinAction::CoinSpent,
                                    });
                                }
                            }
                            None
                        });
                        return Some(vec![
                            Operation {
                                operation_identifier: counter.next_idx().into(),
                                related_operations: vec![],
                                type_: OperationType::TransferSUI,
                                status,
                                account: Some(AccountIdentifier { address: *sender }),
                                amount: amount.map(|amount| Amount {
                                    value: SignedValue::neg(amount),
                                    currency: SUI.clone(),
                                }),
                                coin_change: input,
                                metadata: None,
                            },
                            Operation {
                                operation_identifier: counter.next_idx().into(),
                                related_operations: vec![],
                                type_: OperationType::TransferSUI,
                                status,
                                account: Some(AccountIdentifier {
                                    address: recipient.get_owner_address().ok()?,
                                }),
                                amount: amount.map(|amount| Amount {
                                    value: amount.into(),
                                    currency: SUI.clone(),
                                }),
                                coin_change: Some(CoinChange {
                                    coin_identifier: CoinIdentifier {
                                        identifier: CoinID {
                                            id: *object_id,
                                            version: *version,
                                        },
                                    },
                                    coin_action: CoinAction::CoinCreated,
                                }),
                                metadata: None,
                            },
                        ]);
                    }
                }
                None
            })
            .unwrap_or_default()
    }

    pub async fn parse_transaction_data(
        operations: Vec<Operation>,
        metadata: ConstructionMetadata,
    ) -> Result<TransactionData, Error> {
        let action: SuiAction = operations.try_into()?;
        action.try_into_data(metadata).await
    }

    pub fn gas(
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
    budget: u64,
    gas: ObjectRef,
    sender: SuiAddress,
    counter: &mut IndexCounter,
    status: Option<OperationStatus>,
    effects: Option<&TransactionEffects>,
) -> Result<Vec<Operation>, anyhow::Error> {
    let mut operations = match tx {
        SingleTransactionKind::TransferSui(tx) => transfer_sui_operations(
            budget,
            gas,
            sender,
            tx.recipient,
            tx.amount,
            counter,
            status,
        ),
        SingleTransactionKind::TransferObject(tx) => transfer_object_operations(
            budget,
            tx.object_ref,
            gas,
            sender,
            tx.recipient,
            counter,
            status,
        ),
        SingleTransactionKind::Call(c) => parse_move_call(sender, gas, budget, c, counter, status)?,
        SingleTransactionKind::Publish(p) => {
            let disassembled = disassemble_modules(p.modules.iter())?;
            vec![Operation {
                operation_identifier: counter.next_idx().into(),
                related_operations: vec![],
                type_: OperationType::Publish,
                status,
                account: Some(AccountIdentifier { address: sender }),
                amount: None,
                coin_change: None,
                metadata: Some(json!(disassembled)),
            }]
        }
        SingleTransactionKind::ChangeEpoch(change) => vec![Operation {
            operation_identifier: counter.next_idx().into(),
            related_operations: vec![],
            type_: OperationType::EpochChange,
            status,
            account: None,
            amount: None,
            coin_change: None,
            metadata: Some(json!(change)),
        }],
        SingleTransactionKind::Pay(pay) => parse_pay(sender, gas, budget, pay, counter, status),
    };
    if !matches!(tx, SingleTransactionKind::TransferSui(..)) {
        if let Some(effects) = effects {
            let coin_change_operations = Operation::get_coin_operation_from_event(
                tx.input_objects()?,
                &effects.events,
                status,
                counter,
            );
            operations.extend(coin_change_operations);
        }
    }
    Ok(operations)
}

fn transfer_sui_operations(
    budget: u64,
    coin: ObjectRef,
    sender: SuiAddress,
    recipient: SuiAddress,
    amount: Option<u64>,
    counter: &mut IndexCounter,
    status: Option<OperationStatus>,
) -> Vec<Operation> {
    vec![
        Operation {
            operation_identifier: counter.next_idx().into(),
            related_operations: vec![],
            type_: OperationType::TransferSUI,
            status,
            account: Some(AccountIdentifier { address: sender }),
            amount: amount.map(|amount| Amount {
                value: SignedValue::neg(amount),
                currency: SUI.clone(),
            }),
            coin_change: Some(CoinChange {
                coin_identifier: CoinIdentifier {
                    identifier: coin.into(),
                },
                coin_action: CoinAction::CoinSpent,
            }),
            metadata: None,
        },
        Operation {
            operation_identifier: counter.next_idx().into(),
            related_operations: vec![],
            type_: OperationType::TransferSUI,
            status,
            account: Some(AccountIdentifier { address: recipient }),
            amount: amount.map(|amount| Amount {
                value: amount.into(),
                currency: SUI.clone(),
            }),
            coin_change: None,
            metadata: None,
        },
        Operation::gas(counter, status, coin, budget, sender),
    ]
}

fn transfer_object_operations(
    budget: u64,
    object_id: ObjectRef,
    gas: ObjectRef,
    sender: SuiAddress,
    recipient: SuiAddress,
    counter: &mut IndexCounter,
    status: Option<OperationStatus>,
) -> Vec<Operation> {
    vec![
        Operation {
            operation_identifier: counter.next_idx().into(),
            related_operations: vec![],
            type_: OperationType::TransferObject,
            status,
            account: Some(AccountIdentifier { address: sender }),
            amount: None,
            coin_change: None,
            metadata: Some(json!({ "object_id": object_id.0, "version": object_id.1 })),
        },
        Operation {
            operation_identifier: counter.next_idx().into(),
            related_operations: vec![],
            type_: OperationType::TransferObject,
            status,
            account: Some(AccountIdentifier { address: recipient }),
            amount: None,
            coin_change: None,
            metadata: None,
        },
        Operation::gas(counter, status, gas, budget, sender),
    ]
}

fn split_coin_operations(
    budget: u64,
    coin_to_split: ObjectID,
    split_amount: Vec<u64>,
    gas: ObjectRef,
    sender: SuiAddress,
    counter: &mut IndexCounter,
    status: Option<OperationStatus>,
) -> Vec<Operation> {
    vec![
        Operation {
            operation_identifier: counter.next_idx().into(),
            related_operations: vec![],
            type_: OperationType::SplitCoin,
            status,
            account: Some(AccountIdentifier { address: sender }),
            amount: None,
            coin_change: None,
            metadata: Some(
                json!({ "budget": budget, "coin_to_split": coin_to_split, "split_amount": split_amount }),
            ),
        },
        Operation::gas(counter, status, gas, budget, sender),
    ]
}

fn merge_coin_operations(
    budget: u64,
    primary_coin: ObjectID,
    coin_to_merge: ObjectID,
    gas: ObjectRef,
    sender: SuiAddress,
    counter: &mut IndexCounter,
    status: Option<OperationStatus>,
) -> Vec<Operation> {
    vec![
        Operation {
            operation_identifier: counter.next_idx().into(),
            related_operations: vec![],
            type_: OperationType::MergeCoins,
            status,
            account: Some(AccountIdentifier { address: sender }),
            amount: None,
            coin_change: None,
            metadata: Some(json!({ "primary_coin":primary_coin, "coin_to_merge":coin_to_merge })),
        },
        Operation::gas(counter, status, gas, budget, sender),
    ]
}

fn move_call_operations(
    budget: u64,
    gas: ObjectRef,
    sender: SuiAddress,
    package: ObjectID,
    module: String,
    function: String,
    arguments: Value,
    counter: &mut IndexCounter,
    status: Option<OperationStatus>,
) -> Vec<Operation> {
    vec![
        Operation {
            operation_identifier: counter.next_idx().into(),
            related_operations: vec![],
            type_: OperationType::MoveCall,
            status,
            account: Some(AccountIdentifier { address: sender }),
            amount: None,
            coin_change: None,
            metadata: Some(json!({
                "budget": budget,
                "package": package,
                "module": module,
                "function": function,
                "arguments": arguments,
            })),
        },
        Operation::gas(counter, status, gas, budget, sender),
    ]
}

fn parse_move_call(
    sender: SuiAddress,
    gas: ObjectRef,
    budget: u64,
    call: &MoveCall,
    counter: &mut IndexCounter,
    status: Option<OperationStatus>,
) -> Result<Vec<Operation>, anyhow::Error> {
    if call.package.0 == SUI_FRAMEWORK_OBJECT_ID {
        if call.function.as_ref() == COIN_SPLIT_VEC_FUNC_NAME {
            let coin_to_split = call
                .arguments
                .first()
                .map(try_into_object_id)
                .ok_or_else(|| anyhow!("Error parsing object from split coin move call."))??;
            let split_amounts = call
                .arguments
                .last()
                .and_then(|v| {
                    if let CallArg::Pure(p) = v {
                        bcs::from_bytes(p).ok()
                    } else {
                        None
                    }
                })
                .ok_or_else(|| anyhow!("Error parsing amounts from split coin move call."))?;
            return Ok(split_coin_operations(
                budget,
                coin_to_split,
                split_amounts,
                gas,
                sender,
                counter,
                status,
            ));
        } else if call.function.as_ref() == COIN_JOIN_FUNC_NAME {
            let coins = call
                .arguments
                .iter()
                .map(try_into_object_id)
                .collect::<Result<Vec<_>, _>>()?;
            let primary_coin = *coins.first().ok_or_else(|| {
                anyhow!("Error parsing [primary_coin] from merge coin move call.")
            })?;
            let coin_to_merge = *coins.last().ok_or_else(|| {
                anyhow!("Error parsing [coin_to_merge] from merge coin move call.")
            })?;
            return Ok(merge_coin_operations(
                budget,
                primary_coin,
                coin_to_merge,
                gas,
                sender,
                counter,
                status,
            ));
        }
    }
    Ok(move_call_operations(
        budget,
        gas,
        sender,
        call.package.0,
        call.module.to_string(),
        call.function.to_string(),
        json!(call.arguments),
        counter,
        status,
    ))
}

fn parse_pay(
    sender: SuiAddress,
    gas: ObjectRef,
    budget: u64,
    pay: &Pay,
    counter: &mut IndexCounter,
    status: Option<OperationStatus>,
) -> Vec<Operation> {
    vec![
        Operation {
            operation_identifier: counter.next_idx().into(),
            related_operations: vec![],
            type_: OperationType::Pay,
            status,
            account: Some(AccountIdentifier { address: sender }),
            amount: None,
            coin_change: None,
            metadata: Some(json!(pay)),
        },
        Operation::gas(counter, status, gas, budget, sender),
    ]
}

fn try_into_object_id(arg: &CallArg) -> Result<ObjectID, anyhow::Error> {
    if let CallArg::Object(arg) = arg {
        Ok(*match arg {
            ObjectArg::ImmOrOwnedObject((o, ..)) | ObjectArg::SharedObject(o) => o,
        })
    } else {
        Err(anyhow!("Arg [{arg:?}] is not an object."))
    }
}

#[derive(Debug)]
pub enum SuiAction {
    TransferSui {
        budget: u64,
        coin: ObjectID,
        sender: SuiAddress,
        recipient: SuiAddress,
        amount: Option<u64>,
    },

    Transfer {
        budget: u64,
        coin: ObjectID,
        gas: ObjectID,
        sender: SuiAddress,
        recipient: SuiAddress,
    },

    MergeCoin {
        budget: u64,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas: ObjectID,
        sender: SuiAddress,
    },
    SplitCoin {
        budget: u64,
        coin_to_split: ObjectID,
        split_amounts: Vec<u64>,
        gas: ObjectID,
        sender: SuiAddress,
    },
}

impl SuiAction {
    pub async fn try_into_data(
        self,
        metadata: ConstructionMetadata,
    ) -> Result<TransactionData, Error> {
        Ok(match self {
            SuiAction::TransferSui {
                budget,
                coin,
                sender,
                recipient,
                amount,
            } => {
                let gas = metadata.try_get_info(&coin)?;
                TransactionData::new_transfer_sui(recipient, sender, amount, gas.into(), budget)
            }
            SuiAction::Transfer {
                budget,
                coin,
                gas,
                sender,
                recipient,
            } => {
                let gas = metadata.try_get_info(&gas)?;
                let coin = metadata.try_get_info(&coin)?;
                TransactionData::new_transfer(recipient, coin.into(), sender, gas.into(), budget)
            }
            SuiAction::MergeCoin {
                budget,
                primary_coin,
                coin_to_merge,
                gas,
                sender,
            } => {
                let gas = metadata.try_get_info(&gas)?;
                let primary_coin = metadata.try_get_info(&primary_coin)?;
                let coin_to_merge = metadata.try_get_info(&coin_to_merge)?;
                let type_args = parse_sui_struct_tag(&primary_coin.type_)?.type_params;

                TransactionData::new_move_call(
                    sender,
                    metadata.try_get_info(&SUI_FRAMEWORK_OBJECT_ID)?.into(),
                    COIN_MODULE_NAME.to_owned(),
                    COIN_JOIN_FUNC_NAME.to_owned(),
                    type_args,
                    gas.into(),
                    vec![
                        CallArg::Object(ObjectArg::ImmOrOwnedObject(primary_coin.into())),
                        CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_to_merge.into())),
                    ],
                    budget,
                )
            }
            SuiAction::SplitCoin {
                budget,
                coin_to_split,
                split_amounts,
                gas,
                sender,
            } => {
                let gas = metadata.try_get_info(&gas)?;
                let coin_to_split = metadata.try_get_info(&coin_to_split)?;
                let type_args = parse_sui_struct_tag(&coin_to_split.type_)?.type_params;
                TransactionData::new_move_call(
                    sender,
                    metadata.try_get_info(&SUI_FRAMEWORK_OBJECT_ID)?.into(),
                    COIN_MODULE_NAME.to_owned(),
                    COIN_SPLIT_VEC_FUNC_NAME.to_owned(),
                    type_args,
                    gas.into(),
                    vec![
                        CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_to_split.into())),
                        CallArg::Pure(bcs::to_bytes(&split_amounts)?),
                    ],
                    budget,
                )
            }
        })
    }

    pub fn input_objects(&self) -> Vec<ObjectID> {
        match self {
            SuiAction::TransferSui { coin, .. } => {
                vec![*coin]
            }
            SuiAction::Transfer { coin, gas, .. } => vec![*coin, *gas],
            SuiAction::MergeCoin {
                primary_coin,
                coin_to_merge,
                gas,
                ..
            } => vec![SUI_FRAMEWORK_OBJECT_ID, *primary_coin, *coin_to_merge, *gas],
            SuiAction::SplitCoin {
                coin_to_split, gas, ..
            } => vec![SUI_FRAMEWORK_OBJECT_ID, *coin_to_split, *gas],
        }
    }

    pub fn signer(&self) -> SuiAddress {
        *match self {
            SuiAction::TransferSui { sender, .. }
            | SuiAction::Transfer { sender, .. }
            | SuiAction::MergeCoin { sender, .. }
            | SuiAction::SplitCoin { sender, .. } => sender,
        }
    }
}

impl TryInto<SuiAction> for Vec<Operation> {
    type Error = Error;

    fn try_into(self) -> Result<SuiAction, Self::Error> {
        let mut builder = SuiActionBuilder::default();

        for op in self {
            match op.type_ {
                OperationType::TransferSUI => {
                    let account = op
                        .account
                        .as_ref()
                        .ok_or_else(|| Error::missing_input("operation.account"))?;
                    let address = account.address;
                    builder.operation_type = Some(op.type_);
                    if let Some(amount) = op.amount.as_ref() {
                        if amount.value.is_negative() {
                            builder.sender = Some(address);
                            builder.send_amount = Some(amount.value.abs());
                            if let Some(coin) = op.coin_change.as_ref() {
                                builder.gas = Some(coin.coin_identifier.identifier.id);
                            }
                        } else {
                            builder.recipient = Some(address);
                        }
                    } else if let Some(coin) = op.coin_change.as_ref() {
                        // no amount specified, sending the whole coin
                        builder.sender = Some(address);
                        builder.coin = Some(coin.coin_identifier.identifier.id);
                    } else {
                        builder.recipient = Some(address);
                    }
                }
                OperationType::MergeCoins => {
                    let coin = op
                        .coin_change
                        .ok_or_else(|| Error::missing_input("coin_change"))?;
                    if let Some(account) = &op.account {
                        let address = account.address;
                        builder.operation_type = Some(op.type_);
                        builder.sender = Some(address);
                        builder.coin = Some(coin.coin_identifier.identifier.id);
                    } else {
                        builder.coin_to_merge = Some(coin.coin_identifier.identifier.id);
                    }
                }
                OperationType::SplitCoin => {
                    if let Some(coin_change) = op.coin_change {
                        let account = &op.account.ok_or_else(|| Error::missing_input("account"))?;
                        let address = account.address;
                        builder.operation_type = Some(op.type_);
                        builder.sender = Some(address);
                        builder.coin = Some(coin_change.coin_identifier.identifier.id);
                    } else {
                        let amount = &op.amount.ok_or_else(|| Error::missing_input("amount"))?;
                        builder.add_split_amount(amount.value.abs());
                    }
                }
                OperationType::GasBudget => {
                    if let Some(coin) = op.coin_change.as_ref() {
                        builder.gas = Some(coin.coin_identifier.identifier.id);
                    }
                    let budget_value = op
                        .metadata
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
                    builder.gas_budget = Some(budget);
                }
                OperationType::TransferObject
                | OperationType::Pay
                | OperationType::GasSpent
                | OperationType::Genesis
                | OperationType::MoveCall
                | OperationType::Publish
                | OperationType::EpochChange => return Err(Error::unsupported_operation(op.type_)),
            }
        }
        builder.build()
    }
}

#[derive(Default)]
struct SuiActionBuilder {
    sender: Option<SuiAddress>,
    recipient: Option<SuiAddress>,
    gas: Option<ObjectID>,
    coin: Option<ObjectID>,
    coin_to_merge: Option<ObjectID>,
    send_amount: Option<u64>,
    split_amount: Option<Vec<u64>>,
    gas_budget: Option<u64>,
    operation_type: Option<OperationType>,
}

impl SuiActionBuilder {
    fn build(self) -> Result<SuiAction, Error> {
        let type_ = self
            .operation_type
            .ok_or_else(|| Error::missing_input("operation_type"))?;
        match type_ {
            OperationType::TransferSUI => {
                let sender = self.sender.ok_or_else(|| Error::missing_input("sender"))?;
                let recipient = self
                    .recipient
                    .ok_or_else(|| Error::missing_input("recipient"))?;
                let gas = self.gas.ok_or_else(|| Error::missing_input("gas"))?;
                let budget = self
                    .gas_budget
                    .ok_or_else(|| Error::missing_input("gas_budget"))?;
                Ok(SuiAction::TransferSui {
                    budget,
                    coin: gas,
                    sender,
                    recipient,
                    amount: self.send_amount,
                })
            }
            OperationType::MergeCoins => {
                let sender = self.sender.ok_or_else(|| Error::missing_input("sender"))?;
                let primary_coin = self
                    .coin
                    .ok_or_else(|| Error::missing_input("primary_coin"))?;
                let coin_to_merge = self
                    .coin_to_merge
                    .ok_or_else(|| Error::missing_input("coin_to_merge"))?;
                let gas = self.gas.ok_or_else(|| Error::missing_input("gas"))?;
                let budget = self
                    .gas_budget
                    .ok_or_else(|| Error::missing_input("gas_budget"))?;

                Ok(SuiAction::MergeCoin {
                    budget,
                    primary_coin,
                    coin_to_merge,
                    gas,
                    sender,
                })
            }
            OperationType::SplitCoin => {
                let sender = self.sender.ok_or_else(|| Error::missing_input("sender"))?;
                let coin_to_split = self
                    .coin
                    .ok_or_else(|| Error::missing_input("coin_to_split"))?;
                let gas = self.gas.ok_or_else(|| Error::missing_input("gas"))?;
                let budget = self
                    .gas_budget
                    .ok_or_else(|| Error::missing_input("gas_budget"))?;
                let split_amount = self
                    .split_amount
                    .ok_or_else(|| Error::missing_input("split_amount"))?;

                Ok(SuiAction::SplitCoin {
                    budget,
                    coin_to_split,
                    gas,
                    sender,
                    split_amounts: split_amount,
                })
            }
            _ => Err(Error::new_with_msg(
                UnsupportedOperation,
                format!("Unsupported operation [{type_:?}]").as_str(),
            )),
        }
    }

    pub fn add_split_amount(&mut self, amount: u64) -> &mut Self {
        self.split_amount.get_or_insert(vec![]).push(amount);
        self
    }
}
