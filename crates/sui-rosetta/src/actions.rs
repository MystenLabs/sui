// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use anyhow::anyhow;
use serde_json::{json, Value};

use sui_core::authority::AuthorityState;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::coin::{COIN_JOIN_FUNC_NAME, COIN_MODULE_NAME, COIN_SPLIT_VEC_FUNC_NAME};
use sui_types::error::SuiError;
use sui_types::messages::{
    CallArg, ChangeEpoch, MoveCall, ObjectArg, SingleTransactionKind, TransactionData,
};
use sui_types::move_package::disassemble_modules;
use sui_types::SUI_FRAMEWORK_OBJECT_ID;

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
    MoveCall {
        budget: u64,
        gas: ObjectID,
        sender: SuiAddress,
        package: ObjectID,
        module: String,
        function: String,
        arguments: Vec<Value>,
    },
    Publish {
        budget: u64,
        gas: ObjectID,
        sender: SuiAddress,
        disassembled: BTreeMap<String, Value>,
    },
    EpochChange(ChangeEpoch),
}

impl SuiAction {
    pub async fn try_into_data(self, state: &AuthorityState) -> Result<TransactionData, Error> {
        Ok(match self {
            SuiAction::TransferSui {
                budget,
                coin,
                sender,
                recipient,
                amount,
            } => {
                let gas = get_object_ref(state, &coin).await?;
                TransactionData::new_transfer_sui(recipient, sender, amount, gas, budget)
            }
            SuiAction::Transfer {
                budget,
                coin,
                gas,
                sender,
                recipient,
            } => {
                let gas = get_object_ref(state, &gas).await?;
                let coin = get_object_ref(state, &coin).await?;
                TransactionData::new_transfer(recipient, coin, sender, gas, budget)
            }
            SuiAction::MergeCoin {
                budget,
                primary_coin,
                coin_to_merge,
                gas,
                sender,
            } => {
                let gas = get_object_ref(state, &gas).await?;
                let primary_coin = state.get_object_read(&primary_coin).await?.into_object()?;
                let coin_to_merge = get_object_ref(state, &coin_to_merge).await?;
                let type_args = vec![primary_coin.get_move_template_type()?];
                let primary_coin = primary_coin.compute_object_reference();

                TransactionData::new_move_call(
                    sender,
                    state.get_framework_object_ref().await?,
                    COIN_MODULE_NAME.to_owned(),
                    COIN_JOIN_FUNC_NAME.to_owned(),
                    type_args,
                    gas,
                    vec![
                        CallArg::Object(ObjectArg::ImmOrOwnedObject(primary_coin)),
                        CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_to_merge)),
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
                let gas = get_object_ref(state, &gas).await?;
                let coin_to_split = state.get_object_read(&coin_to_split).await?.into_object()?;
                let type_args = vec![coin_to_split.get_move_template_type()?];
                let coin_to_split = coin_to_split.compute_object_reference();
                TransactionData::new_move_call(
                    sender,
                    state.get_framework_object_ref().await?,
                    COIN_MODULE_NAME.to_owned(),
                    COIN_SPLIT_VEC_FUNC_NAME.to_owned(),
                    type_args,
                    gas,
                    vec![
                        CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_to_split)),
                        CallArg::Pure(bcs::to_bytes(&split_amounts)?),
                    ],
                    budget,
                )
            }
            SuiAction::MoveCall { .. } | SuiAction::Publish { .. } | SuiAction::EpochChange(_) => {
                return Err(Error::new_with_msg(
                    UnsupportedOperation,
                    format!("Unsupported Operation [{self:?}]").as_str(),
                ))
            }
        })
    }

    pub fn try_from_data(data: &TransactionData) -> Result<Vec<Self>, anyhow::Error> {
        let budget = data.gas_budget;
        let (gas, _, _) = data.gas();
        let sender = data.signer();
        data.kind
            .single_transactions()
            .map(|tx| {
                Ok(match tx {
                    SingleTransactionKind::TransferSui(tx) => SuiAction::TransferSui {
                        budget,
                        coin: gas,
                        sender,
                        recipient: tx.recipient,
                        amount: tx.amount,
                    },
                    SingleTransactionKind::TransferObject(tx) => SuiAction::Transfer {
                        budget,
                        coin: tx.object_ref.0,
                        gas,
                        sender,
                        recipient: tx.recipient,
                    },
                    SingleTransactionKind::Call(call) => {
                        parse_move_call(sender, gas, budget, call)?
                    }
                    SingleTransactionKind::Publish(p) => SuiAction::Publish {
                        budget,
                        gas,
                        sender,
                        disassembled: disassemble_modules(p.modules.iter())?,
                    },
                    SingleTransactionKind::ChangeEpoch(c) => SuiAction::EpochChange(c.clone()),
                })
            })
            .collect::<Result<_, _>>()
    }
}
async fn get_object_ref(state: &AuthorityState, id: &ObjectID) -> Result<ObjectRef, SuiError> {
    Ok(state
        .get_object_read(id)
        .await?
        .into_object()?
        .compute_object_reference())
}
fn parse_move_call(
    sender: SuiAddress,
    gas: ObjectID,
    budget: u64,
    call: &MoveCall,
) -> Result<SuiAction, anyhow::Error> {
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

            return Ok(SuiAction::SplitCoin {
                budget,
                coin_to_split,
                split_amounts,
                gas,
                sender,
            });
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

            return Ok(SuiAction::MergeCoin {
                budget,
                primary_coin,
                coin_to_merge,
                gas,
                sender,
            });
        }
    }
    Ok(SuiAction::MoveCall {
        budget,
        gas,
        sender,
        package: call.package.0,
        module: call.module.to_string(),
        function: call.function.to_string(),
        arguments: vec![],
    })
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
                split_amounts: split_amount,
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
                Operation::budget(1, budget, gas, sender),
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
    gas: ObjectID,
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
        Operation::budget(2, budget, gas, sender),
    ]
}

fn split_coin_operations(
    budget: u64,
    coin_to_split: ObjectID,
    split_amount: Vec<u64>,
    gas: ObjectID,
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
    ops.push(Operation::budget(ops.len() as u64, budget, gas, sender));
    ops
}

fn merge_coin_operations(
    budget: u64,
    primary_coin: ObjectID,
    coin_to_merge: ObjectID,
    gas: ObjectID,
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
    ops.push(Operation::budget(ops.len() as u64, budget, gas, sender));
    ops
}

fn move_call_operations(
    budget: u64,
    gas: ObjectID,
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
        Operation::budget(1, budget, gas, sender),
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
                let gas = self.gas.ok_or_else(|| Error::missing_input("gas"))?;
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
