// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Neg;

use anyhow::anyhow;
use serde::Deserialize;
use serde::Serialize;
use serde_json::{json, Value};

use sui_core::authority::AuthorityState;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::coin::{COIN_JOIN_FUNC_NAME, COIN_MODULE_NAME, COIN_SPLIT_VEC_FUNC_NAME};
use sui_types::error::SuiError;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::{
    CallArg, MoveCall, ObjectArg, SingleTransactionKind, TransactionData, TransactionEffects,
};
use sui_types::move_package::disassemble_modules;
use sui_types::object::PastObjectRead::VersionFound;
use sui_types::SUI_FRAMEWORK_OBJECT_ID;

use crate::types::{
    AccountIdentifier, Amount, CoinAction, CoinChange, CoinID, CoinIdentifier, OperationIdentifier,
    OperationStatus, OperationType, SignedValue,
};
use crate::ErrorType::UnsupportedOperation;
use crate::{Error, SUI};

#[derive(Deserialize, Serialize, Clone)]
pub struct Operation {
    pub operation_identifier: OperationIdentifier,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_operations: Vec<OperationIdentifier>,
    #[serde(rename = "type")]
    pub type_: OperationType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
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
    pub async fn from_data_and_effect(
        data: &TransactionData,
        effects: Option<&TransactionEffects>,
        state: &AuthorityState,
    ) -> Result<Vec<Operation>, anyhow::Error> {
        let budget = data.gas_budget;
        let gas = data.gas();
        let sender = data.signer();
        let status = effects.map(|effect| OperationStatus::from(&effect.status).to_string());

        Ok(futures::future::try_join_all(
            data.kind.single_transactions().map(|tx| {
                parse_operations(tx, budget, gas, sender, status.clone(), effects, state)
            }),
        )
        .await?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>())
    }

    pub async fn parse_transaction_data(
        operations: Vec<Operation>,
        state: &AuthorityState,
    ) -> Result<TransactionData, Error> {
        let action: SuiAction = operations.try_into()?;
        action.try_into_data(state).await
    }

    pub fn gas(
        index: u64,
        status: Option<String>,
        gas: ObjectRef,
        sender: SuiAddress,
        effects: Option<&TransactionEffects>,
    ) -> Self {
        let amount = effects.map(|e| Amount {
            value: e.gas_used.net_gas_usage().neg().into(),
            currency: SUI.clone(),
        });
        Self {
            operation_identifier: OperationIdentifier {
                index,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::Gas,
            status,
            account: Some(AccountIdentifier { address: sender }),
            amount,
            coin_change: Some(CoinChange {
                coin_identifier: CoinIdentifier {
                    identifier: gas.into(),
                },
                coin_action: CoinAction::CoinSpent,
            }),
            metadata: None,
        }
    }
}

async fn parse_operations(
    tx: &SingleTransactionKind,
    budget: u64,
    gas: ObjectRef,
    sender: SuiAddress,
    status: Option<String>,
    effects: Option<&TransactionEffects>,
    state: &AuthorityState,
) -> Result<Vec<Operation>, anyhow::Error> {
    Ok(match tx {
        SingleTransactionKind::TransferSui(tx) => transfer_sui_operations(
            budget,
            gas,
            sender,
            tx.recipient,
            tx.amount,
            status,
            effects,
        ),
        SingleTransactionKind::TransferObject(tx) => {
            transfer_object_operations(
                budget,
                tx.object_ref,
                gas,
                sender,
                tx.recipient,
                status,
                effects,
                state,
            )
            .await?
        }
        SingleTransactionKind::Call(c) => parse_move_call(sender, gas, budget, c, status, effects)?,
        SingleTransactionKind::Publish(p) => {
            let disassembled = disassemble_modules(p.modules.iter())?;
            vec![
                Operation {
                    operation_identifier: OperationIdentifier {
                        index: 0,
                        network_index: None,
                    },
                    related_operations: vec![],
                    type_: OperationType::Publish,
                    status: status.clone(),
                    account: Some(AccountIdentifier { address: sender }),
                    amount: None,
                    coin_change: None,
                    metadata: Some(json!(disassembled)),
                },
                Operation::gas(1, status, gas, sender, effects),
            ]
        }
        SingleTransactionKind::ChangeEpoch(change) => vec![Operation {
            operation_identifier: OperationIdentifier {
                index: 0,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::EpochChange,
            status,
            account: None,
            amount: None,
            coin_change: None,
            metadata: Some(json!(change)),
        }],
    })
}

fn transfer_sui_operations(
    budget: u64,
    coin: ObjectRef,
    sender: SuiAddress,
    recipient: SuiAddress,
    amount: Option<u64>,
    status: Option<String>,
    effects: Option<&TransactionEffects>,
) -> Vec<Operation> {
    let coin_change = effects
        .and_then(|e| e.created.first())
        .map(|(oref, _)| CoinChange {
            coin_identifier: CoinIdentifier {
                identifier: (*oref).into(),
            },
            coin_action: CoinAction::CoinCreated,
        });
    vec![
        Operation {
            operation_identifier: OperationIdentifier {
                index: 0,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::TransferSUI,
            status: status.clone(),
            account: Some(AccountIdentifier { address: sender }),
            amount: amount.map(|amount| Amount {
                value: SignedValue::neg(amount),
                currency: SUI.clone(),
            }),
            coin_change: None,
            metadata: Some(json!({ "budget": budget })),
        },
        Operation {
            operation_identifier: OperationIdentifier {
                index: 1,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::TransferSUI,
            status: status.clone(),
            account: Some(AccountIdentifier { address: recipient }),
            amount: amount.map(|amount| Amount {
                value: amount.into(),
                currency: SUI.clone(),
            }),
            coin_change,
            metadata: None,
        },
        Operation::gas(2, status, coin, sender, effects),
    ]
}

async fn transfer_object_operations(
    budget: u64,
    object_id: ObjectRef,
    gas: ObjectRef,
    sender: SuiAddress,
    recipient: SuiAddress,
    status: Option<String>,
    effects: Option<&TransactionEffects>,
    state: &AuthorityState,
) -> Result<Vec<Operation>, anyhow::Error> {
    let object = if let Ok(VersionFound(_, o, _)) =
        state.get_past_object_read(&object_id.0, object_id.1).await
    {
        o
    } else {
        return Err(anyhow!(
            "Cannot find input object {}:{}",
            object_id.0,
            object_id.1
        ));
    };
    let coin = GasCoin::try_from(&object).ok();
    let type_ = if coin.is_some() {
        OperationType::TransferCoin
    } else {
        OperationType::TransferObject
    };
    let sender_coin_change = if coin.is_some() {
        Some(CoinChange {
            coin_identifier: CoinIdentifier {
                identifier: object_id.into(),
            },
            coin_action: CoinAction::CoinSpent,
        })
    } else {
        None
    };

    let recipient_coin_change = if coin.is_some() {
        effects
            .and_then(|effect| {
                effect
                    .mutated
                    .iter()
                    .find(|((id, _, _), _)| id == &object_id.0)
            })
            .map(|(oref, _)| CoinChange {
                coin_identifier: CoinIdentifier {
                    identifier: (*oref).into(),
                },
                coin_action: CoinAction::CoinCreated,
            })
    } else {
        None
    };

    let sender_amount = coin.as_ref().map(|coin| Amount {
        value: SignedValue::neg(coin.value()),
        currency: SUI.clone(),
    });

    let recipient_amount = coin.map(|coin| Amount {
        value: SignedValue::from(coin.value()),
        currency: SUI.clone(),
    });

    Ok(vec![
        Operation {
            operation_identifier: OperationIdentifier {
                index: 0,
                network_index: None,
            },
            related_operations: vec![],
            type_,
            status: status.clone(),
            account: Some(AccountIdentifier { address: sender }),
            amount: sender_amount,
            coin_change: sender_coin_change,
            metadata: Some(json!({"object_type": object.type_(), "budget": budget})),
        },
        Operation {
            operation_identifier: OperationIdentifier {
                index: 1,
                network_index: None,
            },
            related_operations: vec![],
            type_,
            status: status.clone(),
            account: Some(AccountIdentifier { address: recipient }),
            amount: recipient_amount,
            coin_change: recipient_coin_change,
            metadata: Some(json!({"object_type": object.type_()})),
        },
        Operation::gas(2, status, gas, sender, effects),
    ])
}

fn split_coin_operations(
    budget: u64,
    coin_to_split: ObjectID,
    split_amount: Vec<u64>,
    gas: ObjectRef,
    sender: SuiAddress,
    status: Option<String>,
    effects: Option<&TransactionEffects>,
) -> Vec<Operation> {
    let coin_to_split = find_mutated_coin_ref_from_effect(effects, coin_to_split);

    let mut ops = vec![Operation {
        operation_identifier: OperationIdentifier {
            index: 0,
            network_index: None,
        },
        related_operations: vec![],
        type_: OperationType::SplitCoin,
        status: status.clone(),
        account: Some(AccountIdentifier { address: sender }),
        amount: None,
        coin_change: Some(CoinChange {
            coin_identifier: CoinIdentifier {
                identifier: coin_to_split,
            },
            coin_action: CoinAction::CoinSpent,
        }),
        metadata: Some(json!({ "budget": budget })),
    }];

    for amount in split_amount {
        ops.push(Operation {
            operation_identifier: OperationIdentifier {
                index: ops.len() as u64,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::SplitCoin,
            status: status.clone(),
            account: None,
            amount: Some(Amount::new(amount.into())),
            coin_change: None,
            metadata: None,
        });
    }
    ops.push(Operation::gas(
        ops.len() as u64,
        status,
        gas,
        sender,
        effects,
    ));
    ops
}

fn find_mutated_coin_ref_from_effect(
    effects: Option<&TransactionEffects>,
    object_id: ObjectID,
) -> CoinID {
    effects
        .and_then(|e| e.mutated.iter().find(|((id, _, _), _)| id == &object_id))
        .map(|((id, version, _), _)| CoinID {
            id: *id,
            version: Some(*version),
        })
        .unwrap_or_else(|| CoinID {
            id: object_id,
            version: None,
        })
}

fn find_deleted_coin_ref_from_effect(
    effects: Option<&TransactionEffects>,
    object_id: ObjectID,
) -> CoinID {
    effects
        .and_then(|e| e.deleted.iter().find(|(id, _, _)| id == &object_id))
        .map(|(id, version, _)| CoinID {
            id: *id,
            version: Some(*version),
        })
        .unwrap_or_else(|| CoinID {
            id: object_id,
            version: None,
        })
}

fn merge_coin_operations(
    budget: u64,
    primary_coin: ObjectID,
    coin_to_merge: ObjectID,
    gas: ObjectRef,
    sender: SuiAddress,
    status: Option<String>,
    effects: Option<&TransactionEffects>,
) -> Vec<Operation> {
    let primary_coin = find_mutated_coin_ref_from_effect(effects, primary_coin);
    let coin_to_merge = find_deleted_coin_ref_from_effect(effects, coin_to_merge);
    let mut ops = vec![
        Operation {
            operation_identifier: OperationIdentifier {
                index: 0,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::MergeCoins,
            status: status.clone(),
            account: Some(AccountIdentifier { address: sender }),
            amount: None,
            coin_change: Some(CoinChange {
                coin_identifier: CoinIdentifier {
                    identifier: primary_coin,
                },
                coin_action: CoinAction::CoinSpent,
            }),
            metadata: Some(json!({ "budget": budget })),
        },
        Operation {
            operation_identifier: OperationIdentifier {
                index: 1,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::MergeCoins,
            status: status.clone(),
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
    ops.push(Operation::gas(
        ops.len() as u64,
        status,
        gas,
        sender,
        effects,
    ));
    ops
}

fn move_call_operations(
    budget: u64,
    gas: ObjectRef,
    sender: SuiAddress,
    package: ObjectID,
    module: String,
    function: String,
    arguments: Vec<Value>,
    status: Option<String>,
    effects: Option<&TransactionEffects>,
) -> Vec<Operation> {
    vec![
        Operation {
            operation_identifier: OperationIdentifier {
                index: 0,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::MoveCall,
            status: status.clone(),
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
        Operation::gas(1, status, gas, sender, effects),
    ]
}

fn parse_move_call(
    sender: SuiAddress,
    gas: ObjectRef,
    budget: u64,
    call: &MoveCall,
    status: Option<String>,
    effects: Option<&TransactionEffects>,
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
                status,
                effects,
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
                status,
                effects,
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
        vec![],
        status,
        effects,
    ))
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
enum SuiAction {
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
        })
    }
}
async fn get_object_ref(state: &AuthorityState, id: &ObjectID) -> Result<ObjectRef, SuiError> {
    Ok(state
        .get_object_read(id)
        .await?
        .into_object()?
        .compute_object_reference())
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
                            let amount = op
                                .metadata
                                .and_then(|v| v.pointer("/budget").cloned())
                                .and_then(|v| v.as_u64())
                                .ok_or_else(|| Error::missing_input("gas budget"))?;
                            builder.gas_budget = Some(amount);
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
                OperationType::TransferCoin => {
                    let account = &op
                        .account
                        .ok_or_else(|| Error::missing_input("operation.account"))?;
                    let address = account.address;
                    builder.operation_type = Some(op.type_);
                    if let Some(coin) = op.coin_change.as_ref() {
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
                OperationType::TransferObject
                | OperationType::Gas
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
