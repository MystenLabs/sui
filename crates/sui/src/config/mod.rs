// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::fmt::{Display, Formatter, Write};

use serde::{Deserialize, Serialize};
use serde_with::serde_as;

pub use sui_config::utils;
pub use sui_config::Config;
pub use sui_config::PersistedConfig;
use sui_config::SUI_DEV_NET_URL;
use sui_keys::keystore::AccountKeystore;
use sui_keys::keystore::Keystore;
use sui_sdk::SuiClient;
use sui_types::base_types::*;

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct SuiClientConfig {
    pub keystore: Keystore,
    pub envs: Vec<SuiEnv>,
    pub active_env: String,
    pub active_address: Option<SuiAddress>,
}

impl SuiClientConfig {
    pub fn get_env(&self, alias: &str) -> Option<&SuiEnv> {
        self.envs.iter().find(|env| env.alias == alias)
    }

    pub fn get_active_env(&self) -> Result<&SuiEnv, anyhow::Error> {
        self.get_env(&self.active_env).ok_or_else(|| {
            anyhow!(
                "Environment configuration not found for env [{}]",
                self.active_env
            )
        })
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SuiEnv {
    pub alias: String,
    pub rpc: String,
    pub ws: Option<String>,
}

impl SuiEnv {
    pub async fn init(&self) -> Result<SuiClient, anyhow::Error> {
        SuiClient::new_rpc_client(&self.rpc, self.ws.as_deref()).await
    }

    pub fn devnet() -> Self {
        Self {
            alias: "devnet".to_string(),
            rpc: SUI_DEV_NET_URL.into(),
            ws: None,
        }
    }
}

impl Display for SuiEnv {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "Active environment : {}", self.alias)?;
        write!(writer, "RPC URL: {}", self.rpc)?;
        if let Some(ws) = &self.ws {
            writeln!(writer)?;
            write!(writer, "Websocket URL: {ws}")?;
        }
        write!(f, "{}", writer)
    }
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
        if let Ok(env) = self.get_active_env() {
            write!(writer, "{}", env)?;
        }
        write!(f, "{}", writer)
    }
}
