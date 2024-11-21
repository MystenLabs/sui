use serde::{Deserialize, Serialize};

use sui_types::base_types::SuiAddress;

use crate::Currency;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PayCoin {
    pub sender: SuiAddress,
    pub recipients: Vec<SuiAddress>,
    pub amounts: Vec<u64>,
    pub currency: Currency,
}

