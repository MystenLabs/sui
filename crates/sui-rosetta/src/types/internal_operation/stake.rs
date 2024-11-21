use serde::{Deserialize, Serialize};

use sui_types::base_types::SuiAddress;


#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Stake {
    pub sender: SuiAddress,
    pub validator: SuiAddress,
    pub amount: Option<u64>,
}

