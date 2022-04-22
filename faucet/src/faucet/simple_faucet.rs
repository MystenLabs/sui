// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use futures::lock::Mutex;
use sui::wallet_commands::WalletContext;
use tracing::debug;

use crate::{Faucet, FaucetError};

/// A naive implementation of a faucet that processes
/// request sequentially
pub struct SimpleFaucet {
    // TODO: handle concurrency correctly
    wallet: Arc<Mutex<WalletContext>>,
}

impl SimpleFaucet {
    pub fn new(mut wallet: WalletContext) -> Self {
        debug!(
            "SimpleFaucet::new with active address: {}",
            wallet.active_address().unwrap()
        );
        Self {
            wallet: Arc::new(Mutex::new(wallet)),
        }
    }

    async fn get_coins(&self, amounts: &[u64]) -> Result<Vec<String>, FaucetError> {
        let mut result = vec![];
        for (i, amount) in amounts.iter().enumerate() {
            result.push(format!("{i}-{amount}"));
        }
        Ok(result)
    }

    async fn transfer_coins(&self, _coins: &[String], recipient: &str) -> Result<(), FaucetError> {
        let address = self.wallet.lock().await.active_address().unwrap();
        debug!("transfer_coins from {} to {}", address, recipient);
        Ok(())
    }
}

#[async_trait]
impl Faucet for SimpleFaucet {
    async fn send(&self, recipient: &str, amounts: &[u64]) -> Result<Vec<String>, FaucetError> {
        let coins = self.get_coins(amounts).await?;
        self.transfer_coins(&coins, recipient).await?;
        Ok(coins)
    }
}

// TODO: Make sure this is thread-safe
unsafe impl Send for SimpleFaucet {}
unsafe impl Sync for SimpleFaucet {}
