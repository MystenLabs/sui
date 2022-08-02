// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::FaucetError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sui_json_rpc_types::SuiParsedObject;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    gas_coin::GasCoin,
};
use uuid::Uuid;

mod simple_faucet;
pub use self::simple_faucet::{CoinPair, SimpleFaucet};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FaucetReceipt {
    pub sent: Vec<CoinInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CoinInfo {
    pub amount: u64,
    pub id: ObjectID,
}

#[async_trait]
pub trait Faucet {
    /// Send `Coin<SUI>` of the specified amount to the recipient
    async fn send(
        &self,
        id: Uuid,
        recipient: SuiAddress,
        amounts: &[u64],
    ) -> Result<FaucetReceipt, FaucetError>;
}

impl<'a> FromIterator<&'a SuiParsedObject> for FaucetReceipt {
    fn from_iter<T: IntoIterator<Item = &'a SuiParsedObject>>(iter: T) -> Self {
        FaucetReceipt {
            sent: iter.into_iter().map(|o| o.into()).collect(),
        }
    }
}

impl From<&SuiParsedObject> for CoinInfo {
    fn from(v: &SuiParsedObject) -> Self {
        let gas_coin = GasCoin::try_from(v).unwrap();
        Self {
            amount: gas_coin.value(),
            id: *gas_coin.id(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use sui::client_commands::{SuiClientCommands, SuiClientCommandResult};
    use test_utils::network::setup_network_and_wallet;

    use super::*;

    #[tokio::test]
    async fn simple_faucet_basic_interface_should_work() {
        let (_network, context, _address) = setup_network_and_wallet().await.unwrap();
        let faucet = SimpleFaucet::new(context, 1).await.unwrap();
        test_basic_interface(faucet).await;
    }

    #[tokio::test]
    async fn test_concurrency() {
        telemetry_subscribers::init_for_testing();
        let (_network, mut context, address) = setup_network_and_wallet().await.unwrap();
        let results = SuiClientCommands::Gas {
            address: Some(address),
        }
        .execute(&mut context)
        .await
        .unwrap();
        let gas_count = match results {
            SuiClientCommandResult::Gas(gases) => gases.len(),
            other => panic!("Expect SuiClientCommandResult::Gas, but got {:?}", other),
        };
        let faucet = SimpleFaucet::new(context, gas_count / 2).await.unwrap();
        let coins = faucet.coins.lock().await;
        assert_eq!(coins.len(), gas_count / 2);
        let mut head = *coins.peek().unwrap();
        drop(coins);

        let recipient = SuiAddress::random_for_testing_only();
        let amounts = vec![1, 2, 3];
        let future = faucet.send(Uuid::new_v4(), recipient, &amounts);
        tokio::time::timeout(Duration::from_secs(10), future)
            .await
            .unwrap()
            .unwrap();

        let mut coins = faucet.coins.lock().await;
        let mut last: Option<CoinPair> = None;
        for coin in coins.drain() {
            last = Some(coin);
        }
        head.usage = 1;
        // Expect the last pair in heap is the old head but with usage incremented
        assert_eq!(last.unwrap(), head);
    }

    #[tokio::test]
    #[should_panic]
    async fn test_concurrency_not_enough_coins() {
        let (_network, mut context, address) = setup_network_and_wallet().await.unwrap();
        let results = SuiClientCommands::Gas {
            address: Some(address),
        }
        .execute(&mut context)
        .await
        .unwrap();
        let gas_count = match results {
            SuiClientCommandResult::Gas(gases) => gases.len(),
            other => panic!("Expect SuiClientCommandResult::Gas, but got {:?}", other),
        };
        SimpleFaucet::new(context, (gas_count / 2) + 1)
            .await
            .unwrap();
    }

    async fn test_basic_interface(faucet: impl Faucet) {
        let recipient = SuiAddress::random_for_testing_only();
        let amounts = vec![1, 2, 3];

        let FaucetReceipt { sent } = faucet
            .send(Uuid::new_v4(), recipient, &amounts)
            .await
            .unwrap();
        let mut actual_amounts: Vec<u64> = sent.iter().map(|c| c.amount).collect();
        actual_amounts.sort_unstable();
        assert_eq!(actual_amounts, amounts);
    }
}
