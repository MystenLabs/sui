// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

use sui_types::base_types::EpochId;
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
