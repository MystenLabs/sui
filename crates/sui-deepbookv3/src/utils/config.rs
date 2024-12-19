// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use sui_sdk::types::base_types::SuiAddress;

use crate::utils::constants::{
    MAINNET_COINS, MAINNET_PACKAGE_IDS, MAINNET_POOLS, TESTNET_COINS, TESTNET_PACKAGE_IDS,
    TESTNET_POOLS,
};

use super::types::{BalanceManager, Coin, DeepBookPackageIds, Pool};

// Constants
pub const FLOAT_SCALAR: u64 = 1_000_000_000;
pub const MAX_TIMESTAMP: u64 = 1_844_674_407_370_955_161;
pub const GAS_BUDGET: u64 = 250_000_000; // 0.5 * 500000000
pub const DEEP_SCALAR: u64 = 1_000_000;

// Type aliases
pub type CoinMap = HashMap<&'static str, Coin>;
pub type PoolMap = HashMap<&'static str, Pool>;
pub type BalanceManagerMap = HashMap<&'static str, BalanceManager>;

#[derive(Debug)]
pub enum Environment {
    Mainnet,
    Testnet,
}

#[derive(Debug, Clone)]
pub struct DeepBookConfig {
    coins: CoinMap,
    pools: PoolMap,
    balance_managers: BalanceManagerMap,
    address: SuiAddress,
    deepbook_package_id: String,
    registry_id: String,
    deep_treasury_id: String,
    admin_cap: Option<String>,
}

impl DeepBookConfig {
    pub fn new(
        env: Environment,
        address: SuiAddress,
        admin_cap: Option<String>,
        balance_managers: Option<BalanceManagerMap>,
        coins: Option<CoinMap>,
        pools: Option<PoolMap>,
    ) -> Self {
        let package_ids: DeepBookPackageIds = match env {
            Environment::Mainnet => MAINNET_PACKAGE_IDS,
            Environment::Testnet => TESTNET_PACKAGE_IDS,
        };

        Self {
            address,
            admin_cap,
            balance_managers: balance_managers.unwrap_or_default(),
            coins: coins.unwrap_or_else(|| match env {
                Environment::Mainnet => MAINNET_COINS.clone(), // Replace with mainnet coins
                Environment::Testnet => TESTNET_COINS.clone(), // Replace with testnet coins
            }),
            pools: pools.unwrap_or_else(|| match env {
                Environment::Mainnet => MAINNET_POOLS.clone(), // Replace with mainnet pools
                Environment::Testnet => TESTNET_POOLS.clone(), // Replace with testnet pools
            }),
            deepbook_package_id: package_ids.deepbook_package_id.to_string(),
            registry_id: package_ids.registry_id.to_string(),
            deep_treasury_id: package_ids.deep_treasury_id.to_string(),
        }
    }

    pub fn get_coin(&self, key: &str) -> anyhow::Result<&Coin> {
        self.coins
            .get(key)
            .ok_or(anyhow::anyhow!("Coin with key {} not found.", key))
    }

    pub fn get_pool(&self, key: &str) -> anyhow::Result<&Pool> {
        self.pools
            .get(key)
            .ok_or(anyhow::anyhow!("Pool with key {} not found.", key))
    }

    pub fn get_balance_manager(&self, manager_key: &str) -> anyhow::Result<&BalanceManager> {
        println!("Balance managers: {:?}", self.balance_managers);
        self.balance_managers
            .get(manager_key)
            .ok_or(anyhow::anyhow!(
                "Balance manager with key {} not found.",
                manager_key
            ))
    }

    pub fn address(&self) -> &SuiAddress {
        &self.address
    }

    pub fn deepbook_package_id(&self) -> &str {
        &self.deepbook_package_id
    }

    pub fn registry_id(&self) -> &str {
        &self.registry_id
    }

    pub fn deep_treasury_id(&self) -> &str {
        &self.deep_treasury_id
    }

    pub fn admin_cap(&self) -> Option<String> {
        self.admin_cap.clone()
    }
}
