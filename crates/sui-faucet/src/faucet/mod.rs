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
