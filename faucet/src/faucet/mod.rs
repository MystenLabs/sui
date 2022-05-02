// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::FaucetError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    gas_coin::GasCoin,
    object::Object,
};

mod simple_faucet;
pub use self::simple_faucet::SimpleFaucet;

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
        recipient: SuiAddress,
        amounts: &[u64],
    ) -> Result<FaucetReceipt, FaucetError>;
}

impl<'a> FromIterator<&'a Object> for FaucetReceipt {
    fn from_iter<T: IntoIterator<Item = &'a Object>>(iter: T) -> Self {
        FaucetReceipt {
            sent: iter.into_iter().map(|o| o.into()).collect(),
        }
    }
}

impl From<&Object> for CoinInfo {
    fn from(v: &Object) -> Self {
        let gas_coin = GasCoin::try_from(v).unwrap();
        Self {
            amount: gas_coin.value(),
            id: *gas_coin.id(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::setup_network_and_wallet;

    use super::*;

    #[tokio::test]
    async fn simple_faucet_basic_interface_should_work() {
        let (network, context, _address) = setup_network_and_wallet().await.unwrap();
        let faucet = SimpleFaucet::new(context).await.unwrap();
        test_basic_interface(faucet).await;
        network.kill().await.unwrap();
    }

    async fn test_basic_interface(faucet: impl Faucet) {
        let recipient = SuiAddress::random_for_testing_only();
        let amounts = vec![1, 2, 3];

        let FaucetReceipt { sent } = faucet.send(recipient, &amounts).await.unwrap();
        let mut actual_amounts: Vec<u64> = sent.iter().map(|c| c.amount).collect();
        actual_amounts.sort_unstable();
        assert_eq!(actual_amounts, amounts);
    }
}
