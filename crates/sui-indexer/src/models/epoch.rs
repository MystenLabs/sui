// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{Insertable, Queryable, Selectable};

use crate::schema::epochs;
use crate::types::IndexedEpochInfo;
use crate::{errors::IndexerError, schema::feature_flags, schema::protocol_configs};
use sui_json_rpc_types::{EndOfEpochInfo, EpochInfo};
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = epochs)]
pub struct StoredEpochInfo {
    pub epoch: i64,
    pub first_checkpoint_id: i64,
    pub epoch_start_timestamp: i64,
    pub reference_gas_price: i64,
    pub protocol_version: i64,
    pub total_stake: i64,
    pub storage_fund_balance: i64,
    pub system_state: Vec<u8>,
    pub epoch_total_transactions: Option<i64>,
    pub last_checkpoint_id: Option<i64>,
    pub epoch_end_timestamp: Option<i64>,
    pub storage_fund_reinvestment: Option<i64>,
    pub storage_charge: Option<i64>,
    pub storage_rebate: Option<i64>,
    pub stake_subsidy_amount: Option<i64>,
    pub total_gas_fees: Option<i64>,
    pub total_stake_rewards_distributed: Option<i64>,
    pub leftover_storage_fund_inflow: Option<i64>,
    pub epoch_commitments: Option<Vec<u8>>,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = protocol_configs)]
pub struct StoredProtocolConfig {
    pub protocol_version: i64,
    pub config_name: String,
    pub config_value: Option<String>,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = feature_flags)]
pub struct StoredFeatureFlag {
    pub protocol_version: i64,
    pub flag_name: String,
    pub flag_value: bool,
}

#[derive(Queryable, Selectable, Clone)]
#[diesel(table_name = epochs)]
pub struct QueryableEpochInfo {
    pub epoch: i64,
    pub first_checkpoint_id: i64,
    pub epoch_start_timestamp: i64,
    pub reference_gas_price: i64,
    pub protocol_version: i64,
    pub total_stake: i64,
    pub storage_fund_balance: i64,
    pub epoch_total_transactions: Option<i64>,
    pub last_checkpoint_id: Option<i64>,
    pub epoch_end_timestamp: Option<i64>,
    pub storage_fund_reinvestment: Option<i64>,
    pub storage_charge: Option<i64>,
    pub storage_rebate: Option<i64>,
    pub stake_subsidy_amount: Option<i64>,
    pub total_gas_fees: Option<i64>,
    pub total_stake_rewards_distributed: Option<i64>,
    pub leftover_storage_fund_inflow: Option<i64>,
    pub epoch_commitments: Option<Vec<u8>>,
}

#[derive(Queryable)]
pub struct QueryableEpochSystemState {
    pub epoch: i64,
    pub system_state: Vec<u8>,
}

impl StoredEpochInfo {
    pub fn from_epoch_beginning_info(e: &IndexedEpochInfo) -> Self {
        Self {
            epoch: e.epoch as i64,
            first_checkpoint_id: e.first_checkpoint_id as i64,
            epoch_start_timestamp: e.epoch_start_timestamp as i64,
            reference_gas_price: e.reference_gas_price as i64,
            protocol_version: e.protocol_version as i64,
            total_stake: e.total_stake as i64,
            storage_fund_balance: e.storage_fund_balance as i64,
            ..Default::default()
        }
    }

    pub fn from_epoch_end_info(e: &IndexedEpochInfo) -> Self {
        Self {
            epoch: e.epoch as i64,
            system_state: e.system_state.clone(),
            epoch_total_transactions: e.epoch_total_transactions.map(|v| v as i64),
            last_checkpoint_id: e.last_checkpoint_id.map(|v| v as i64),
            epoch_end_timestamp: e.epoch_end_timestamp.map(|v| v as i64),
            storage_fund_reinvestment: e.storage_fund_reinvestment.map(|v| v as i64),
            storage_charge: e.storage_charge.map(|v| v as i64),
            storage_rebate: e.storage_rebate.map(|v| v as i64),
            stake_subsidy_amount: e.stake_subsidy_amount.map(|v| v as i64),
            total_gas_fees: e.total_gas_fees.map(|v| v as i64),
            total_stake_rewards_distributed: e.total_stake_rewards_distributed.map(|v| v as i64),
            leftover_storage_fund_inflow: e.leftover_storage_fund_inflow.map(|v| v as i64),
            epoch_commitments: e
                .epoch_commitments
                .as_ref()
                .map(|v| bcs::to_bytes(&v).unwrap()),

            // For the following fields:
            // we don't update these columns when persisting EndOfEpoch data.
            // However if the data is partial, diesel would interpret them
            // as Null and hence cause errors.
            first_checkpoint_id: 0,
            epoch_start_timestamp: 0,
            reference_gas_price: 0,
            protocol_version: 0,
            total_stake: 0,
            storage_fund_balance: 0,
        }
    }
}

impl From<&StoredEpochInfo> for Option<EndOfEpochInfo> {
    fn from(info: &StoredEpochInfo) -> Option<EndOfEpochInfo> {
        Some(EndOfEpochInfo {
            reference_gas_price: (info.reference_gas_price as u64),
            protocol_version: (info.protocol_version as u64),
            last_checkpoint_id: info.last_checkpoint_id.map(|v| v as u64)?,
            total_stake: info.total_stake as u64,
            storage_fund_balance: info.storage_fund_balance as u64,
            epoch_end_timestamp: info.epoch_end_timestamp.map(|v| v as u64)?,
            storage_fund_reinvestment: info.storage_fund_reinvestment.map(|v| v as u64)?,
            storage_charge: info.storage_charge.map(|v| v as u64)?,
            storage_rebate: info.storage_rebate.map(|v| v as u64)?,
            stake_subsidy_amount: info.stake_subsidy_amount.map(|v| v as u64)?,
            total_gas_fees: info.total_gas_fees.map(|v| v as u64)?,
            total_stake_rewards_distributed: info
                .total_stake_rewards_distributed
                .map(|v| v as u64)?,
            leftover_storage_fund_inflow: info.leftover_storage_fund_inflow.map(|v| v as u64)?,
        })
    }
}

impl TryFrom<StoredEpochInfo> for EpochInfo {
    type Error = IndexerError;

    fn try_from(value: StoredEpochInfo) -> Result<Self, Self::Error> {
        let epoch = value.epoch as u64;
        let end_of_epoch_info = (&value).into();
        let system_state: Option<SuiSystemStateSummary> = bcs::from_bytes(&value.system_state)
            .map_err(|_| {
                IndexerError::PersistentStorageDataCorruptionError(format!(
                    "Failed to deserialize `system_state` for epoch {epoch}",
                ))
            })
            .ok();
        Ok(EpochInfo {
            epoch: value.epoch as u64,
            validators: system_state
                .map(|s| s.active_validators)
                .unwrap_or_default(),
            epoch_total_transactions: value.epoch_total_transactions.unwrap_or(0) as u64,
            first_checkpoint_id: value.first_checkpoint_id as u64,
            epoch_start_timestamp: value.epoch_start_timestamp as u64,
            end_of_epoch_info,
            reference_gas_price: Some(value.reference_gas_price as u64),
        })
    }
}
