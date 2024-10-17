// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Observable;
use async_trait::async_trait;
use ethers::providers::Provider;
use ethers::types::{Address as EthAddress, U256};
use prometheus::IntGauge;
use std::sync::Arc;
use sui_bridge::abi::EthERC20;
use sui_bridge::metered_eth_provider::MeteredEthHttpProvier;
use tokio::time::Duration;
use tracing::{error, info};

const TEN_ZEROS: u64 = 10_u64.pow(10);

pub struct EthVaultBalance {
    coin_contract: EthERC20<Provider<MeteredEthHttpProvier>>,
    vault_address: EthAddress,
    ten_zeros: U256,
    metric: IntGauge,
}

impl EthVaultBalance {
    pub fn new(
        provider: Arc<Provider<MeteredEthHttpProvier>>,
        vault_address: EthAddress,
        coin_address: EthAddress, // for now this only support one coin which is WETH
        metric: IntGauge,
    ) -> Self {
        let ten_zeros = U256::from(TEN_ZEROS);
        let coin_contract = EthERC20::new(coin_address, provider);
        Self {
            coin_contract,
            vault_address,
            ten_zeros,
            metric,
        }
    }
}

#[async_trait]
impl Observable for EthVaultBalance {
    fn name(&self) -> &str {
        "EthVaultBalance"
    }

    async fn observe_and_report(&self) {
        match self
            .coin_contract
            .balance_of(self.vault_address)
            .call()
            .await
        {
            Ok(balance) => {
                // Why downcasting is safe:
                // 1. On Ethereum we only take the first 8 decimals into account,
                // meaning the trailing 10 digits can be ignored
                // 2. i64::MAX is 9_223_372_036_854_775_807, with 8 decimal places is
                // 92_233_720_368. We likely won't see any balance higher than this
                // in the next 12 months.
                let balance = (balance / self.ten_zeros).as_u64() as i64;
                self.metric.set(balance);
                info!("Eth Vault Balance: {:?}", balance);
            }
            Err(e) => {
                error!("Error getting balance from vault: {:?}", e);
            }
        }
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(10)
    }
}
