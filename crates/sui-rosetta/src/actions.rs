// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde_json::json;
use std::str::FromStr;
use sui_sdk::SuiClient;

use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::messages::TransactionData;

use crate::types::{Operation, OperationType};
use crate::{Error, ErrorType};

pub enum SuiAction {
    TransferSui {
        budget: u64,
        coin: ObjectID,
        sender: SuiAddress,
        recipient: SuiAddress,
        amount: Option<u64>,
    },

    TransferCoin {
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
        recipient: SuiAddress,
    },
}

impl SuiAction {
    pub async fn into_transaction_data(self, client: &SuiClient) -> Result<TransactionData, Error> {
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
            _ => todo!(),
        })
    }
}

impl TryInto<SuiAction> for Vec<Operation> {
    type Error = Error;

    fn try_into(self) -> Result<SuiAction, Self::Error> {
        let mut builder = SuiActionBuilder::default();

        for op in self {
            match op.type_ {
                OperationType::TransferSUI => {
                    let account = op.account.as_ref().ok_or_else(|| {
                        Error::new_with_detail(
                            ErrorType::MissingInput,
                            json!({"input": "operation.account"}),
                        )
                    })?;
                    let address = SuiAddress::from_str(&account.address).map_err(|e| {
                        Error::new_with_detail(
                            ErrorType::InvalidInput,
                            json! ({"cause": e.to_string()}),
                        )
                    })?;
                    builder.set_operation_type(op.type_);
                    if let Some(amount) = op.amount.as_ref() {
                        if amount.value.is_negative() {
                            let coin = op.coin_change.as_ref().ok_or_else(|| {
                                Error::new_with_detail(
                                    ErrorType::MissingInput,
                                    json!({"input": "operation.coin_change"}),
                                )
                            })?;
                            builder.set_sender(address);
                            builder.set_send_amount(amount.value.abs().try_into()?);
                            builder.set_coin(ObjectID::from_str(&coin.coin_identifier.identifier)?);
                        } else {
                            builder.set_recipient(address);
                        }
                    } else if let Some(coin) = op.coin_change.as_ref() {
                        // no amount specified, sending the whole coin
                        builder.set_sender(address);
                        builder.set_coin(ObjectID::from_str(&coin.coin_identifier.identifier)?);
                    } else {
                        builder.set_recipient(address);
                    }
                }
                OperationType::Gas => {
                    // Coin object id is optional
                    if let Some(coin) = op.coin_change.as_ref() {
                        builder.set_gas(ObjectID::from_str(&coin.coin_identifier.identifier)?);
                    }
                    let amount = op.amount.as_ref().ok_or_else(|| {
                        Error::new_with_detail(
                            ErrorType::MissingInput,
                            json!({"input": "gas budget"}),
                        )
                    })?;
                    builder.set_gas_budget(amount.value.try_into()?);
                }
                _ => {
                    return Err(Error::new_with_detail(
                        ErrorType::UnsupportedOperation,
                        json!({"operation type": op.type_}),
                    ))
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
    send_amount: Option<u64>,
    gas_budget: Option<u64>,
    operation_type: Option<OperationType>,
}

impl SuiActionBuilder {
    fn build(self) -> Result<SuiAction, Error> {
        let operation_type = self.operation_type.ok_or_else(|| {
            Error::new_with_detail(ErrorType::MissingInput, json!({"input" : "operation type"}))
        })?;
        match operation_type {
            OperationType::TransferSUI => {
                let sender = self.sender.ok_or_else(|| {
                    Error::new_with_detail(ErrorType::MissingInput, json!({"input" : "sender"}))
                })?;
                let recipient = self.recipient.ok_or_else(|| {
                    Error::new_with_detail(ErrorType::MissingInput, json!({"input" : "recipient"}))
                })?;
                let coin = self.coin.ok_or_else(|| {
                    Error::new_with_detail(ErrorType::MissingInput, json!({"input" : "coin"}))
                })?;
                let budget = self.gas_budget.ok_or_else(|| {
                    Error::new_with_detail(ErrorType::MissingInput, json!({"input" : "recipient"}))
                })?;
                Ok(SuiAction::TransferSui {
                    budget,
                    coin,
                    sender,
                    recipient,
                    amount: self.send_amount,
                })
            }
            _ => Err(Error::new_with_detail(
                ErrorType::UnsupportedOperation,
                json!({"operation_type": self.operation_type}),
            )),
        }
    }

    pub fn set_operation_type(&mut self, operation_type: OperationType) -> &mut Self {
        self.operation_type = Some(operation_type);
        self
    }
    pub fn set_sender(&mut self, sender: SuiAddress) -> &mut Self {
        self.sender = Some(sender);
        self
    }
    pub fn set_recipient(&mut self, recipient: SuiAddress) -> &mut Self {
        self.recipient = Some(recipient);
        self
    }
    pub fn set_send_amount(&mut self, send_amount: u64) -> &mut Self {
        self.send_amount = Some(send_amount);
        self
    }
    pub fn set_gas(&mut self, gas: ObjectID) -> &mut Self {
        self.gas = Some(gas);
        self
    }
    pub fn set_coin(&mut self, coin: ObjectID) -> &mut Self {
        self.coin = Some(coin);
        self
    }
    pub fn set_gas_budget(&mut self, gas_budget: u64) -> &mut Self {
        self.gas_budget = Some(gas_budget);
        self
    }
}
