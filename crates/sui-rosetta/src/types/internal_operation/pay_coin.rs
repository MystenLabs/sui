use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use sui_sdk::SuiClient;
use sui_types::base_types::SuiAddress;

use crate::{errors::Error, Currency};

use super::{GasCoinsAndObjects, TryFetchNeededObjects};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PayCoin {
    pub sender: SuiAddress,
    pub recipients: Vec<SuiAddress>,
    pub amounts: Vec<u64>,
    pub currency: Currency,
}

#[async_trait]
impl TryFetchNeededObjects for PayCoin {
    async fn try_fetch_needed_objects(
        self,
        client: &SuiClient,
        gas_price: Option<u64>,
        budget: Option<u64>,
    ) -> Result<GasCoinsAndObjects, Error> {
        todo!();
    }
}
