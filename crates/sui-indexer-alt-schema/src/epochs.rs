// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use sui_field_count::FieldCount;

use crate::schema::{kv_epoch_ends, kv_epoch_starts, kv_feature_flags, kv_protocol_configs};

#[derive(Insertable, Debug, Clone, FieldCount, Queryable)]
#[diesel(table_name = kv_epoch_ends)]
#[diesel(treat_none_as_default_value = false)]
pub struct StoredEpochEnd {
    pub epoch: i64,
    pub cp_hi: i64,
    pub tx_hi: i64,
    pub end_timestamp_ms: i64,
    pub safe_mode: bool,
    pub total_stake: Option<i64>,
    pub storage_fund_balance: Option<i64>,
    pub storage_fund_reinvestment: Option<i64>,
    pub storage_charge: Option<i64>,
    pub storage_rebate: Option<i64>,
    pub stake_subsidy_amount: Option<i64>,
    pub total_gas_fees: Option<i64>,
    pub total_stake_rewards_distributed: Option<i64>,
    pub leftover_storage_fund_inflow: Option<i64>,
    pub epoch_commitments: Vec<u8>,
}

#[derive(Insertable, Debug, Clone, FieldCount)]
#[diesel(table_name = kv_epoch_starts)]
pub struct StoredEpochStart {
    pub epoch: i64,
    pub protocol_version: i64,
    pub cp_lo: i64,
    pub start_timestamp_ms: i64,
    pub reference_gas_price: i64,
    pub system_state: Vec<u8>,
}

#[derive(Insertable, Debug, Clone, FieldCount)]
#[diesel(table_name = kv_feature_flags)]
pub struct StoredFeatureFlag {
    pub protocol_version: i64,
    pub flag_name: String,
    pub flag_value: bool,
}

#[derive(Insertable, Debug, Clone, FieldCount)]
#[diesel(table_name = kv_protocol_configs)]
pub struct StoredProtocolConfig {
    pub protocol_version: i64,
    pub config_name: String,
    pub config_value: Option<String>,
}
