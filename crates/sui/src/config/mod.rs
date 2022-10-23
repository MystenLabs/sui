// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::fmt::{Display, Formatter, Write};
use sui_keys::keystore::AccountKeystore;
use sui_keys::keystore::Keystore;
use sui_types::base_types::*;

pub use sui_config::Config;
pub use sui_config::PersistedConfig;

pub use sui_config::utils;
use sui_sdk::ClientType;

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct SuiClientConfig {
    pub keystore: Keystore,
    pub client_type: ClientType,
    pub active_address: Option<SuiAddress>,
}

impl Config for SuiClientConfig {}

impl Display for SuiClientConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();

        writeln!(
            writer,
            "Managed addresses : {}",
            self.keystore.addresses().len()
        )?;
        write!(writer, "Active address: ")?;
        match self.active_address {
            Some(r) => writeln!(writer, "{}", r)?,
            None => writeln!(writer, "None")?,
        };
        writeln!(writer, "{}", self.keystore)?;
        write!(writer, "{}", self.client_type)?;
        write!(f, "{}", writer)
    }
}
