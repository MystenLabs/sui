// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::fmt::Write;
use std::fmt::{Display, Formatter};

use serde::ser::Error;
use serde::Serialize;

use schemars::JsonSchema;
use serde::Deserialize;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::error::SuiError;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::{CertifiedTransaction, TransactionEffects};
use sui_types::object::Object;

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub enum TransactionResponse {
    EffectResponse(CertifiedTransaction, TransactionEffects),
    PublishResponse(PublishResponse),
    MergeCoinResponse(MergeCoinResponse),
    SplitCoinResponse(SplitCoinResponse),
}

impl TransactionResponse {
    pub fn to_publish_response(self) -> Result<PublishResponse, SuiError> {
        match self {
            TransactionResponse::PublishResponse(resp) => Ok(resp),
            _ => Err(SuiError::UnexpectedMessage),
        }
    }

    pub fn to_merge_coin_response(self) -> Result<MergeCoinResponse, SuiError> {
        match self {
            TransactionResponse::MergeCoinResponse(resp) => Ok(resp),
            _ => Err(SuiError::UnexpectedMessage),
        }
    }

    pub fn to_split_coin_response(self) -> Result<SplitCoinResponse, SuiError> {
        match self {
            TransactionResponse::SplitCoinResponse(resp) => Ok(resp),
            _ => Err(SuiError::UnexpectedMessage),
        }
    }

    pub fn to_effect_response(
        self,
    ) -> Result<(CertifiedTransaction, TransactionEffects), SuiError> {
        match self {
            TransactionResponse::EffectResponse(cert, effects) => Ok((cert, effects)),
            _ => Err(SuiError::UnexpectedMessage),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct SplitCoinResponse {
    /// Certificate of the transaction
    pub certificate: CertifiedTransaction,
    /// The updated original coin object after split
    pub updated_coin: Object,
    /// All the newly created coin objects generated from the split
    pub new_coins: Vec<Object>,
    /// The updated gas payment object after deducting payment
    pub updated_gas: Object,
}

impl Display for SplitCoinResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "----- Certificate ----")?;
        write!(writer, "{}", self.certificate)?;
        writeln!(writer, "----- Split Coin Results ----")?;

        let coin = GasCoin::try_from(&self.updated_coin).map_err(fmt::Error::custom)?;
        writeln!(writer, "Updated Coin : {}", coin)?;
        let mut new_coin_text = Vec::new();
        for coin in &self.new_coins {
            let coin = GasCoin::try_from(coin).map_err(fmt::Error::custom)?;
            new_coin_text.push(format!("{coin}"))
        }
        writeln!(
            writer,
            "New Coins : {}",
            new_coin_text.join(",\n            ")
        )?;
        let gas_coin = GasCoin::try_from(&self.updated_gas).map_err(fmt::Error::custom)?;
        writeln!(writer, "Updated Gas : {}", gas_coin)?;
        write!(f, "{}", writer)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct MergeCoinResponse {
    /// Certificate of the transaction
    pub certificate: CertifiedTransaction,
    /// The updated original coin object after merge
    pub updated_coin: Object,
    /// The updated gas payment object after deducting payment
    pub updated_gas: Object,
}

impl Display for MergeCoinResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "----- Certificate ----")?;
        write!(writer, "{}", self.certificate)?;
        writeln!(writer, "----- Merge Coin Results ----")?;

        let coin = GasCoin::try_from(&self.updated_coin).map_err(fmt::Error::custom)?;
        writeln!(writer, "Updated Coin : {}", coin)?;
        let gas_coin = GasCoin::try_from(&self.updated_gas).map_err(fmt::Error::custom)?;
        writeln!(writer, "Updated Gas : {}", gas_coin)?;
        write!(f, "{}", writer)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct PublishResponse {
    /// Certificate of the transaction
    pub certificate: CertifiedTransaction,
    /// The newly published package object reference.
    pub package: ObjectRef,
    /// List of Move objects created as part of running the module initializers in the package
    pub created_objects: Vec<Object>,
    /// The updated gas payment object after deducting payment
    pub updated_gas: Object,
}

impl Display for PublishResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "----- Certificate ----")?;
        write!(writer, "{}", self.certificate)?;
        writeln!(writer, "----- Publish Results ----")?;
        writeln!(
            writer,
            "The newly published package object: {:?}",
            self.package
        )?;
        writeln!(
            writer,
            "List of objects created by running module initializers:"
        )?;
        for obj in &self.created_objects {
            writeln!(writer, "{}", obj)?;
        }
        let gas_coin = GasCoin::try_from(&self.updated_gas).map_err(fmt::Error::custom)?;
        writeln!(writer, "Updated Gas : {}", gas_coin)?;
        write!(f, "{}", writer)
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct SwitchResponse {
    /// Active address
    pub address: SuiAddress,
}

impl Display for SwitchResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "Active address switched to {}", self.address)?;
        write!(f, "{}", writer)
    }
}
