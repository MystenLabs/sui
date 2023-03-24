// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use sui_types::base_types::{AuthorityName, EpochId, ObjectID, SuiAddress};
use sui_types::committee::{Committee, StakeUnit};

use crate::SuiEpochId;

/// RPC representation of the [Committee] type.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename = "CommitteeInfo")]
pub struct SuiCommittee {
    pub epoch: EpochId,
    pub validators: Vec<(AuthorityName, StakeUnit)>,
}

impl From<Committee> for SuiCommittee {
    fn from(committee: Committee) -> Self {
        Self {
            epoch: committee.epoch,
            validators: committee.voting_rights,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DelegatedStake {
    /// Validator's Address.
    pub validator_address: SuiAddress,
    /// Staking pool object id.
    pub staking_pool: ObjectID,
    pub stakes: Vec<Stake>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "status")]
pub enum StakeStatus {
    Pending,
    #[serde(rename_all = "camelCase")]
    Active {
        estimated_reward: u64,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Stake {
    /// ID of the StakedSui receipt object.
    pub staked_sui_id: ObjectID,
    pub stake_request_epoch: SuiEpochId,
    pub stake_active_epoch: SuiEpochId,
    pub principal: u64,
    #[serde(flatten)]
    pub status: StakeStatus,
}
