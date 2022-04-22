// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;

use crate::{Faucet, FaucetError};

/// A naive implementation of a faucet that processes
/// request sequentially
#[derive(Clone, Debug, Default)]
pub struct SimpleFaucet {}

impl SimpleFaucet {
    pub fn new() -> Self {
        Self::default()
    }

    async fn get_coins(&self, amounts: &[u64]) -> Result<Vec<String>, FaucetError> {
        let mut result = vec![];
        for (i, amount) in amounts.iter().enumerate() {
            result.push(format!("{i}-{amount}"));
        }
        Ok(result)
    }

    async fn transfer_coins(&self, _coins: &[String]) -> Result<(), FaucetError> {
        Ok(())
    }
}

#[async_trait]
impl Faucet for SimpleFaucet {
    async fn send(&self, _recipient: &str, amounts: &[u64]) -> Result<Vec<String>, FaucetError> {
        let coins = self.get_coins(amounts).await?;
        self.transfer_coins(&coins).await?;
        Ok(coins)
    }
}

// TODO: Make sure this is thread-safe
unsafe impl Send for SimpleFaucet {}
unsafe impl Sync for SimpleFaucet {}
