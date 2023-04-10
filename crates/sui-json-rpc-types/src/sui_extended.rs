// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::identifier::Identifier;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_with::serde_as;
use serde_with::DisplayFromStr;

use sui_types::base_types::{EpochId, ObjectID};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::sui_system_state::sui_system_state_summary::SuiValidatorSummary;

use crate::Page;

pub type EpochPage = Page<EpochInfo, EpochId>;

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EpochInfo {
    /// epoch number
    pub epoch: EpochId,
    /// list of validators included in epoch
    pub validators: Vec<SuiValidatorSummary>,
    /// count of tx in epoch
    pub epoch_total_transactions: u64,
    /// first, last checkpoint sequence numbers
    pub first_checkpoint_id: CheckpointSequenceNumber,
    pub epoch_start_timestamp: u64,
    pub end_of_epoch_info: Option<EndOfEpochInfo>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EndOfEpochInfo {
    pub last_checkpoint_id: CheckpointSequenceNumber,
    pub epoch_end_timestamp: u64,
    /// existing fields from `SystemEpochInfo`
    pub protocol_version: u64,
    pub reference_gas_price: u64,
    pub total_stake: u64,
    pub storage_fund_reinvestment: u64,
    pub storage_charge: u64,
    pub storage_rebate: u64,
    pub storage_fund_balance: u64,
    pub stake_subsidy_amount: u64,
    pub total_gas_fees: u64,
    pub total_stake_rewards_distributed: u64,
    pub leftover_storage_fund_inflow: u64,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NetworkMetrics {
    /// Current TPS - Transaction Blocks per Second.
    pub current_tps: f64,
    /// Peak TPS in the past 30 days
    pub tps_30_days: f64,
    /// Total number of packages published in the network
    pub total_packages: u64,
    /// Total number of addresses seen in the network
    pub total_addresses: u64,
    /// Total number of live objects in the network
    pub total_objects: u64,
    /// Current epoch number
    pub current_epoch: u64,
    /// Current checkpoint number
    pub current_checkpoint: u64,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MoveCallMetrics {
    pub rank_3_days: Vec<(MoveFunctionName, usize)>,
    pub rank_7_days: Vec<(MoveFunctionName, usize)>,
    pub rank_30_days: Vec<(MoveFunctionName, usize)>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MoveFunctionName {
    pub package: ObjectID,
    #[schemars(with = "String")]
    #[serde_as(as = "DisplayFromStr")]
    pub module: Identifier,
    #[schemars(with = "String")]
    #[serde_as(as = "DisplayFromStr")]
    pub function: Identifier,
}
