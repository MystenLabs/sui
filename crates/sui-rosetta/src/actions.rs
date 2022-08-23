// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::str::FromStr;

use anyhow::anyhow;
use serde_json::{json, Value};

use sui_sdk::json::SuiJsonValue;
use sui_sdk::rpc_types::{SuiChangeEpoch, SuiMoveCall, SuiTransactionData, SuiTransactionKind};
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::messages::TransactionData;
use sui_types::{coin, SUI_FRAMEWORK_OBJECT_ID};

use crate::types::{
    AccountIdentifier, Amount, CoinAction, CoinChange, CoinIdentifier, Operation,
    OperationIdentifier, OperationType, SignedValue,
};
use crate::ErrorType::UnsupportedOperation;
use crate::{Error, SUI};

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
        gas: Option<ObjectID>,
        sender: SuiAddress,
        recipient: SuiAddress,
    },

    MergeCoin {
        budget: u64,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas: Option<ObjectID>,
        sender: SuiAddress,
    },
    SplitCoin {
        budget: u64,
        coin_to_split: ObjectID,
        split_amount: Vec<u64>,
        gas: Option<ObjectID>,
        sender: SuiAddress,
    },
    MoveCall {
        budget: u64,
        gas: Option<ObjectID>,
        sender: SuiAddress,
        package: ObjectID,
        module: String,
        function: String,
        arguments: Vec<Value>,
    },
    Publish {
        budget: u64,
        gas: Option<ObjectID>,
        sender: SuiAddress,
        disassembled: BTreeMap<String, Value>,
    },
    EpochChange(SuiChangeEpoch),
}

impl SuiAction {
    pub async fn try_into_data(self, client: &SuiClient) -> Result<TransactionData, Error> {
        Ok(match self {
            SuiAction::TransferSui {
                budget,
                coin,
                sender,
                recipient,
                amount,
            } => {
                client
                    .transaction_builder()
                    .transfer_sui(sender, coin, budget, recipient, amount)
                    .await?
            }
            SuiAction::Transfer {
                budget,
                coin: object,
                gas,
                sender,
                recipient,
            } => {
                client
                    .transaction_builder()
                    .transfer_object(sender, object, gas, budget, recipient)
                    .await?
            }
            SuiAction::MergeCoin {
                budget,
                primary_coin,
                coin_to_merge,
                gas,
                sender,
            } => {
                client
                    .transaction_builder()
                    .merge_coins(sender, primary_coin, coin_to_merge, gas, budget)
                    .await?
            }
            SuiAction::SplitCoin {
                budget,
                coin_to_split,
                split_amount,
                gas,
                sender,
            } => {
                client
                    .transaction_builder()
                    .split_coin(sender, coin_to_split, split_amount, gas, budget)
                    .await?
            }
            SuiAction::MoveCall { .. } | SuiAction::Publish { .. } | SuiAction::EpochChange(_) => {
                return Err(Error::new_with_msg(
                    UnsupportedOperation,
                    format!("Unsupported Operation [{self:?}]").as_str(),
                ))
            }
        })
    }

    pub fn try_from_data(data: &SuiTransactionData) -> Result<Vec<Self>, anyhow::Error> {
        data.transactions
            .iter()
            .map(|tx| {
                Ok(match tx {
                    SuiTransactionKind::TransferSui(tx) => SuiAction::TransferSui {
                        budget: data.gas_budget,
                        coin: data.gas_payment.object_id,
                        sender: data.sender,
                        recipient: tx.recipient,
                        amount: tx.amount,
                    },
                    SuiTransactionKind::TransferObject(tx) => SuiAction::Transfer {
                        budget: data.gas_budget,
                        coin: tx.object_ref.object_id,
                        gas: Some(data.gas_payment.object_id),
                        sender: data.sender,
                        recipient: tx.recipient,
                    },
                    SuiTransactionKind::Call(call) => parse_move_call(data, call)?,
                    SuiTransactionKind::Publish(p) => SuiAction::Publish {
                        budget: data.gas_budget,
                        gas: Some(data.gas_payment.object_id),
                        sender: data.sender,
                        disassembled: p.disassembled.clone(),
                    },
                    SuiTransactionKind::ChangeEpoch(c) => SuiAction::EpochChange(c.clone()),
                })
            })
            .collect::<Result<_, _>>()
    }
}

fn parse_move_call(
    data: &SuiTransactionData,
    call: &SuiMoveCall,
) -> Result<SuiAction, anyhow::Error> {
    if call.package.object_id == SUI_FRAMEWORK_OBJECT_ID {
        if call.function == coin::COIN_SPLIT_VEC_FUNC_NAME.to_string() {
            let coin_to_split = call
                .arguments
                .first()
                .map(try_into_object_id)
                .ok_or_else(|| anyhow!("Error parsing object from split coin move call."))??;
            let split_amount = call
                .arguments
                .last()
                .and_then(|v| v.to_json_value().as_array().cloned())
                .map(|v| v.iter().flat_map(|v| v.as_u64()).collect())
                .ok_or_else(|| anyhow!("Error parsing amounts from split coin move call."))?;

            return Ok(SuiAction::SplitCoin {
                budget: data.gas_budget,
                coin_to_split,
                split_amount,
                gas: Some(data.gas_payment.object_id),
                sender: data.sender,
            });
        } else if call.function == coin::COIN_JOIN_FUNC_NAME.to_string() {
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

            return Ok(SuiAction::MergeCoin {
                budget: data.gas_budget,
                primary_coin,
                coin_to_merge,
                gas: Some(data.gas_payment.object_id),
                sender: data.sender,
            });
        }
    }
    Ok(SuiAction::MoveCall {
        budget: data.gas_budget,
        gas: Some(data.gas_payment.object_id),
        sender: data.sender,
        package: call.package.object_id,
        module: call.module.clone(),
        function: call.function.clone(),
        arguments: call
            .arguments
            .iter()
            .map(|arg| arg.to_json_value())
            .collect(),
    })
}

fn try_into_object_id(value: &SuiJsonValue) -> Result<ObjectID, anyhow::Error> {
    let value = value.to_json_value();
    let s = value
        .as_str()
        .ok_or_else(|| anyhow!("Cannot parse value [{value:?}] as string."))?;
    Ok(ObjectID::from_str(s)?)
}

impl From<SuiAction> for Vec<Operation> {
    fn from(action: SuiAction) -> Self {
        match action {
            SuiAction::TransferSui {
                budget,
                coin,
                sender,
                recipient,
                amount,
            } => transfer_sui_operations(budget, coin, sender, recipient, amount),
            SuiAction::Transfer {
                budget,
                coin,
                gas,
                sender,
                recipient,
            } => transfer_coin_operations(budget, coin, gas, sender, recipient),
            SuiAction::MoveCall {
                budget,
                gas,
                sender,
                package,
                module,
                function,
                arguments,
            } => move_call_operations(budget, gas, sender, package, module, function, arguments),
            SuiAction::SplitCoin {
                budget,
                coin_to_split,
                split_amount,
                gas,
                sender,
            } => split_coin_operations(budget, coin_to_split, split_amount, gas, sender),
            SuiAction::MergeCoin {
                budget,
                primary_coin,
                coin_to_merge,
                gas,
                sender,
            } => merge_coin_operations(budget, primary_coin, coin_to_merge, gas, sender),

            SuiAction::Publish {
                budget,
                gas,
                sender,
                disassembled,
            } => vec![
                Operation {
                    operation_identifier: OperationIdentifier {
                        index: 0,
                        network_index: None,
                    },
                    related_operations: vec![],
                    type_: OperationType::Publish,
                    status: None,
                    account: Some(AccountIdentifier { address: sender }),
                    amount: None,
                    coin_change: None,
                    metadata: Some(json!(disassembled)),
                },
                Operation::budget(1, budget, gas),
            ],
            SuiAction::EpochChange(change) => vec![Operation {
                operation_identifier: OperationIdentifier {
                    index: 0,
                    network_index: None,
                },
                related_operations: vec![],
                type_: OperationType::EpochChange,
                status: None,
                account: None,
                amount: None,
                coin_change: None,
                metadata: Some(json!(change)),
            }],
        }
    }
}

fn transfer_sui_operations(
    budget: u64,
    coin: ObjectID,
    sender: SuiAddress,
    recipient: SuiAddress,
    amount: Option<u64>,
) -> Vec<Operation> {
    vec![
        Operation {
            operation_identifier: OperationIdentifier {
                index: 0,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::TransferSUI,
            status: None,
            account: Some(AccountIdentifier { address: sender }),
            amount: amount.map(|amount| Amount {
                value: SignedValue::neg(amount),
                currency: SUI.clone(),
            }),
            coin_change: Some(CoinChange {
                coin_identifier: CoinIdentifier { identifier: coin },
                coin_action: CoinAction::CoinSpent,
            }),
            metadata: None,
        },
        Operation {
            operation_identifier: OperationIdentifier {
                index: 1,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::TransferSUI,
            status: None,
            account: Some(AccountIdentifier { address: recipient }),
            amount: amount.map(|amount| Amount {
                value: amount.into(),
                currency: SUI.clone(),
            }),
            coin_change: None,
            metadata: None,
        },
        Operation {
            operation_identifier: OperationIdentifier {
                index: 2,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::GasBudget,
            status: None,
            account: None,
            amount: Some(Amount {
                value: budget.into(),
                currency: SUI.clone(),
            }),
            coin_change: None,
            metadata: None,
        },
    ]
}

fn transfer_coin_operations(
    budget: u64,
    coin: ObjectID,
    gas: Option<ObjectID>,
    sender: SuiAddress,
    recipient: SuiAddress,
) -> Vec<Operation> {
    vec![
        Operation {
            operation_identifier: OperationIdentifier {
                index: 0,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::TransferCoin,
            status: None,
            account: Some(AccountIdentifier { address: sender }),
            amount: None,
            coin_change: Some(CoinChange {
                coin_identifier: CoinIdentifier { identifier: coin },
                coin_action: CoinAction::CoinSpent,
            }),
            metadata: None,
        },
        Operation {
            operation_identifier: OperationIdentifier {
                index: 1,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::TransferCoin,
            status: None,
            account: Some(AccountIdentifier { address: recipient }),
            amount: None,
            coin_change: None,
            metadata: None,
        },
        Operation::budget(2, budget, gas),
    ]
}

fn split_coin_operations(
    budget: u64,
    coin_to_split: ObjectID,
    split_amount: Vec<u64>,
    gas: Option<ObjectID>,
    sender: SuiAddress,
) -> Vec<Operation> {
    let mut ops = vec![Operation {
        operation_identifier: OperationIdentifier {
            index: 0,
            network_index: None,
        },
        related_operations: vec![],
        type_: OperationType::SplitCoin,
        status: None,
        account: Some(AccountIdentifier { address: sender }),
        amount: None,
        coin_change: Some(CoinChange {
            coin_identifier: CoinIdentifier {
                identifier: coin_to_split,
            },
            coin_action: CoinAction::CoinSpent,
        }),
        metadata: None,
    }];

    for amount in split_amount {
        ops.push(Operation {
            operation_identifier: OperationIdentifier {
                index: ops.len() as u64,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::SplitCoin,
            status: None,
            account: None,
            amount: Some(Amount::new(amount.into())),
            coin_change: None,
            metadata: None,
        });
    }
    ops.push(Operation::budget(ops.len() as u64, budget, gas));
    ops
}

fn merge_coin_operations(
    budget: u64,
    primary_coin: ObjectID,
    coin_to_merge: ObjectID,
    gas: Option<ObjectID>,
    sender: SuiAddress,
) -> Vec<Operation> {
    let mut ops = vec![
        Operation {
            operation_identifier: OperationIdentifier {
                index: 0,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::MergeCoins,
            status: None,
            account: Some(AccountIdentifier { address: sender }),
            amount: None,
            coin_change: Some(CoinChange {
                coin_identifier: CoinIdentifier {
                    identifier: primary_coin,
                },
                coin_action: CoinAction::CoinSpent,
            }),
            metadata: None,
        },
        Operation {
            operation_identifier: OperationIdentifier {
                index: 1,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::MergeCoins,
            status: None,
            account: None,
            amount: None,
            coin_change: Some(CoinChange {
                coin_identifier: CoinIdentifier {
                    identifier: coin_to_merge,
                },
                coin_action: CoinAction::CoinSpent,
            }),
            metadata: None,
        },
    ];
    ops.push(Operation::budget(ops.len() as u64, budget, gas));
    ops
}

fn move_call_operations(
    budget: u64,
    gas: Option<ObjectID>,
    sender: SuiAddress,
    package: ObjectID,
    module: String,
    function: String,
    arguments: Vec<Value>,
) -> Vec<Operation> {
    vec![
        Operation {
            operation_identifier: OperationIdentifier {
                index: 0,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::MoveCall,
            status: None,
            account: Some(AccountIdentifier { address: sender }),
            amount: None,
            coin_change: None,
            metadata: Some(json!({
                "package": package,
                "module": module,
                "function": function,
                "arguments": arguments,
            })),
        },
        Operation::budget(1, budget, gas),
    ]
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
                            let coin = op
                                .coin_change
                                .as_ref()
                                .ok_or_else(|| Error::missing_input("operation.coin_change"))?;
                            builder.sender = Some(address);
                            builder.send_amount = Some(amount.value.abs());
                            builder.coin = Some(coin.coin_identifier.identifier);
                        } else {
                            builder.recipient = Some(address);
                        }
                    } else if let Some(coin) = op.coin_change.as_ref() {
                        // no amount specified, sending the whole coin
                        builder.sender = Some(address);
                        builder.coin = Some(coin.coin_identifier.identifier);
                    } else {
                        builder.recipient = Some(address);
                    }
                }
                OperationType::GasBudget => {
                    // Coin object id is optional
                    if let Some(coin) = op.coin_change.as_ref() {
                        builder.gas = Some(coin.coin_identifier.identifier);
                    }
                    let amount = op
                        .amount
                        .as_ref()
                        .ok_or_else(|| Error::missing_input("gas budget"))?;
                    builder.gas_budget = Some(amount.value.abs());
                }
                OperationType::TransferCoin => {
                    let account = &op
                        .account
                        .ok_or_else(|| Error::missing_input("operation.account"))?;
                    let address = account.address;
                    builder.operation_type = Some(op.type_);
                    if let Some(coin) = op.coin_change.as_ref() {
                        builder.sender = Some(address);
                        builder.coin = Some(coin.coin_identifier.identifier);
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
                        builder.coin = Some(coin.coin_identifier.identifier);
                    } else {
                        builder.coin_to_merge = Some(coin.coin_identifier.identifier);
                    }
                }
                OperationType::SplitCoin => {
                    if let Some(coin_change) = op.coin_change {
                        let account = &op.account.ok_or_else(|| Error::missing_input("account"))?;
                        let address = account.address;
                        builder.operation_type = Some(op.type_);
                        builder.sender = Some(address);
                        builder.coin = Some(coin_change.coin_identifier.identifier);
                    } else {
                        let amount = &op.amount.ok_or_else(|| Error::missing_input("amount"))?;
                        builder.add_split_amount(amount.value.abs());
                    }
                }
                OperationType::MoveCall | OperationType::Publish | OperationType::EpochChange => {
                    return Err(Error::unsupported_operation(op.type_))
                }
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
                let coin = self.coin.ok_or_else(|| Error::missing_input("coin"))?;
                let budget = self
                    .gas_budget
                    .ok_or_else(|| Error::missing_input("gas_budget"))?;
                Ok(SuiAction::TransferSui {
                    budget,
                    coin,
                    sender,
                    recipient,
                    amount: self.send_amount,
                })
            }
            OperationType::TransferCoin => {
                let sender = self.sender.ok_or_else(|| Error::missing_input("sender"))?;
                let recipient = self
                    .recipient
                    .ok_or_else(|| Error::missing_input("recipient"))?;
                let coin = self.coin.ok_or_else(|| Error::missing_input("coin"))?;
                let gas = self.gas;
                let budget = self
                    .gas_budget
                    .ok_or_else(|| Error::missing_input("gas_budget"))?;
                Ok(SuiAction::Transfer {
                    budget,
                    coin,
                    gas,
                    sender,
                    recipient,
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
                let gas = self.gas;
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
                let gas = self.gas;
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
                    split_amount,
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
