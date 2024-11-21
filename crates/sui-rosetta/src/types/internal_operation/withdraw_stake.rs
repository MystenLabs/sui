use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectID, SuiAddress};

use crate::errors::Error;

use super::{TransactionAndObjectData, TryConstructTransaction};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WithdrawStake {
    pub sender: SuiAddress,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stake_ids: Vec<ObjectID>,
}

#[async_trait]
impl TryConstructTransaction for WithdrawStake {
    async fn try_fetch_needed_objects(
        self,
        client: &SuiClient,
        gas_price: Option<u64>,
        budget: Option<u64>,
    ) -> Result<TransactionAndObjectData, Error> {
        todo!();
    }
}
