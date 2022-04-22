// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::FaucetError;
use async_trait::async_trait;

mod simple_faucet;
pub use self::simple_faucet::SimpleFaucet;

#[async_trait]
pub trait Faucet {
    /// Send `Coin<SUI>` of the specified amount to the recipient
    // TODO: change the return type to `Vec<ObjectId>` or something else
    async fn send(&self, recipient: &str, amounts: &[u64]) -> Result<Vec<String>, FaucetError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn simple_faucet_basic_interface_should_work() {
        let store = SimpleFaucet::new();
        test_basic_interface(store).await;
    }

    async fn test_basic_interface(faucet: impl Faucet) {
        let recipient = "recipient";
        let amounts = [1, 2, 3];

        let result = faucet.send(recipient, &amounts).await.unwrap();

        for (i, amount) in amounts.iter().enumerate() {
            assert!(result.contains(&format!("{i}-{amount}")));
        }
    }
}
