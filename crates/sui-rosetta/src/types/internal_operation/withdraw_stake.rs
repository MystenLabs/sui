use serde::{Deserialize, Serialize};
use sui_types::base_types::{ObjectID, SuiAddress};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WithdrawStake {
    pub sender: SuiAddress,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stake_ids: Vec<ObjectID>,
}
