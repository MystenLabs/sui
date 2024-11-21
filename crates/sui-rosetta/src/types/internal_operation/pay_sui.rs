use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use sui_sdk::SuiClient;
use sui_types::base_types::SuiAddress;

use crate::errors::Error;

use super::{GasCoinsAndObjects, TryFetchNeededObjects};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PaySui {
    pub sender: SuiAddress,
    pub recipients: Vec<SuiAddress>,
    pub amounts: Vec<u64>,
}

#[async_trait]
impl TryFetchNeededObjects for PaySui {
    async fn try_fetch_needed_objects(
        self,
        client: &SuiClient,
        gas_price: Option<u64>,
        budget: Option<u64>,
    ) -> Result<GasCoinsAndObjects, Error> {
        todo!();
        // let gas_price = match gas_price {
        //     Some(p) => p,
        //     None => client.governance_api().get_reference_gas_price().await? + 100, // make sure it works over epoch changes
        // };
        // let total_amount = amounts.iter().sum::<u64>();
        // if let Some(budget) = budget {
        //     let coins = client
        //         .coin_read_api()
        //         .select_coins(sender, None, (total_amount + budget) as u128, vec![])
        //         .await?;
        //
        //     let total_coin_value = coins.iter().map(|c| c.balance).sum::<u64>() as i128;
        //
        //     let mut coins: Vec<ObjectRef> = coins.into_iter().map(|c| c.object_ref()).collect();
        //     let objects = if coins.len() > MAX_GAS_COINS {
        //         coins.split_off(MAX_GAS_COINS)
        //     } else {
        //         vec![]
        //     };
        //
        //     return Ok(ConstructionMetadata {
        //         sender,
        //         coins,
        //         budget,
        //         objects,
        //         total_coin_value,
        //         gas_price,
        //         currency: None,
        //     });
        // };
    }
}
