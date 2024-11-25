use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use sui_sdk::SuiClient;
use sui_types::base_types::SuiAddress;

use crate::errors::Error;

use super::{TransactionAndObjectData, TryConstructTransaction};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Stake {
    pub sender: SuiAddress,
    pub validator: SuiAddress,
    pub amount: Option<u64>,
}

#[async_trait]
impl TryConstructTransaction for Stake {
    async fn try_fetch_needed_objects(
        self,
        _client: &SuiClient,
        _gas_price: Option<u64>,
        _budget: Option<u64>,
    ) -> Result<TransactionAndObjectData, Error> {
        todo!();
    }
}
