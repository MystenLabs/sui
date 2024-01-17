// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod benchmarks;
pub mod command;
pub(crate) mod config;
pub(crate) mod errors;
pub(crate) mod gas_pool;
pub(crate) mod gas_pool_initializer;
pub(crate) mod metrics;
pub(crate) mod rpc;
pub(crate) mod storage;
pub(crate) mod sui_client;
#[cfg(test)]
pub(crate) mod test_env;
pub(crate) mod types;

pub const AUTH_ENV_NAME: &str = "GAS_STATION_AUTH";

pub fn read_auth_env() -> String {
    std::env::var(AUTH_ENV_NAME)
        .ok()
        .unwrap_or_else(|| panic!("{} environment variable must be specified", AUTH_ENV_NAME))
        .parse::<String>()
        .unwrap()
}
