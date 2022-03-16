// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::ser::Error;
use serde::Serialize;
use std::fmt;
use std::fmt::Write;
use std::fmt::{Display, Formatter};
use sui_types::base_types::ObjectRef;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::CertifiedTransaction;
use sui_types::object::Object;

#[derive(Serialize)]
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
            new_coin_text.push(format!("{}", coin))
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

#[derive(Serialize)]
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

#[derive(Serialize)]
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
