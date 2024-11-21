use serde::{Deserialize, Serialize};
use sui_types::base_types::SuiAddress;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PaySui {
    pub sender: SuiAddress,
    pub recipients: Vec<SuiAddress>,
    pub amounts: Vec<u64>,
}

