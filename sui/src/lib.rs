// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

extern crate core;

use std::fs::create_dir_all;
use std::path::PathBuf;

use anyhow::bail;

pub mod benchmark;
pub mod config;
pub mod gateway_config;
pub mod keystore;
pub mod rpc_gateway;
pub mod rpc_gateway_client;
pub mod shell;
pub mod sui_commands;
pub mod sui_json;
pub mod wallet_commands;

const SUI_DIR: &str = ".sui";
const SUI_CONFIG_DIR: &str = "sui_config";
pub const SUI_NETWORK_CONFIG: &str = "network.conf";
pub const SUI_WALLET_CONFIG: &str = "wallet.conf";
pub const SUI_GATEWAY_CONFIG: &str = "gateway.conf";
pub const SUI_DEV_NET_URL: &str = "http://gateway.devnet.sui.io:9000";

pub fn sui_config_dir() -> Result<PathBuf, anyhow::Error> {
    match std::env::var_os("SUI_CONFIG_DIR") {
        Some(config_env) => Ok(config_env.into()),
        None => match dirs::home_dir() {
            Some(v) => Ok(v.join(SUI_DIR).join(SUI_CONFIG_DIR)),
            None => bail!("Cannot obtain home directory path"),
        },
    }
    .and_then(|dir| {
        if !dir.exists() {
            create_dir_all(dir.clone())?;
        }
        Ok(dir)
    })
}
