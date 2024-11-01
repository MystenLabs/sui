// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::epochs;
use crate::{errors::IndexerError, schema::feature_flags, schema::protocol_configs};
use diesel::prelude::{AsChangeset, Identifiable};
use diesel::{Insertable, Queryable, Selectable};
use sui_json_rpc_types::{EndOfEpochInfo, EpochInfo};
use sui_types::event::SystemEpochInfoEvent;
use sui_types::messages_checkpoint::CertifiedCheckpointSummary;
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
    pub system_state: Option<Vec<u8>>,
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
    /// This is the system state summary at the beginning of the epoch, serialized as JSON.
    pub system_state_summary_json: Option<serde_json::Value>,
    /// First transaction sequence number of this epoch.
    pub first_tx_sequence_number: Option<i64>,
}

#[derive(Insertable, Identifiable, AsChangeset, Clone, Debug)]
#[diesel(primary_key(epoch))]
#[diesel(table_name = epochs)]
pub struct StartOfEpochUpdate {
    pub epoch: i64,
    pub first_checkpoint_id: i64,
    pub first_tx_sequence_number: i64,
    pub epoch_start_timestamp: i64,
    pub reference_gas_price: i64,
    pub protocol_version: i64,
    pub total_stake: i64,
    pub storage_fund_balance: i64,
    pub system_state_summary_json: serde_json::Value,
}

#[derive(Identifiable, AsChangeset, Clone, Debug)]
#[diesel(primary_key(epoch))]
#[diesel(table_name = epochs)]
pub struct EndOfEpochUpdate {
    pub epoch: i64,
    pub epoch_total_transactions: i64,
    pub last_checkpoint_id: i64,
    pub epoch_end_timestamp: i64,
    pub storage_fund_reinvestment: i64,
    pub storage_charge: i64,
    pub storage_rebate: i64,
    pub stake_subsidy_amount: i64,
    pub total_gas_fees: i64,
    pub total_stake_rewards_distributed: i64,
    pub leftover_storage_fund_inflow: i64,
    pub epoch_commitments: Vec<u8>,
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
    pub first_tx_sequence_number: Option<i64>,
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

#[derive(Default)]
pub struct EpochStartInfo {
    pub first_checkpoint_id: u64,
    pub first_tx_sequence_number: u64,
    pub total_stake: u64,
    pub storage_fund_balance: u64,
}

impl EpochStartInfo {
    pub fn new(
        first_checkpoint_id: u64,
        first_tx_sequence_number: u64,
        epoch_event_opt: Option<&SystemEpochInfoEvent>,
    ) -> Self {
        Self {
            first_checkpoint_id,
            first_tx_sequence_number,
            total_stake: epoch_event_opt.map(|e| e.total_stake).unwrap_or_default(),
            storage_fund_balance: epoch_event_opt
                .map(|e| e.storage_fund_balance)
                .unwrap_or_default(),
        }
    }
}

impl StartOfEpochUpdate {
    pub fn new(
        new_system_state_summary: SuiSystemStateSummary,
        epoch_start_info: EpochStartInfo,
    ) -> Self {
        Self {
            epoch: new_system_state_summary.epoch as i64,
            system_state_summary_json: serde_json::to_value(new_system_state_summary.clone())
                .unwrap(),
            first_checkpoint_id: epoch_start_info.first_checkpoint_id as i64,
            first_tx_sequence_number: epoch_start_info.first_tx_sequence_number as i64,
            epoch_start_timestamp: new_system_state_summary.epoch_start_timestamp_ms as i64,
            reference_gas_price: new_system_state_summary.reference_gas_price as i64,
            protocol_version: new_system_state_summary.protocol_version as i64,
            total_stake: epoch_start_info.total_stake as i64,
            storage_fund_balance: epoch_start_info.storage_fund_balance as i64,
        }
    }
}

#[derive(Default)]
pub struct EpochEndInfo {
    pub storage_fund_reinvestment: u64,
    pub storage_charge: u64,
    pub storage_rebate: u64,
    pub leftover_storage_fund_inflow: u64,
    pub stake_subsidy_amount: u64,
    pub total_gas_fees: u64,
    pub total_stake_rewards_distributed: u64,
}

impl EpochEndInfo {
    pub fn new(epoch_event_opt: Option<&SystemEpochInfoEvent>) -> Self {
        epoch_event_opt.map_or_else(Self::default, |epoch_event| Self {
            storage_fund_reinvestment: epoch_event.storage_fund_reinvestment,
            storage_charge: epoch_event.storage_charge,
            storage_rebate: epoch_event.storage_rebate,
            leftover_storage_fund_inflow: epoch_event.leftover_storage_fund_inflow,
            stake_subsidy_amount: epoch_event.stake_subsidy_amount,
            total_gas_fees: epoch_event.total_gas_fees,
            total_stake_rewards_distributed: epoch_event.total_stake_rewards_distributed,
        })
    }
}

impl EndOfEpochUpdate {
    pub fn new(
        last_checkpoint_summary: &CertifiedCheckpointSummary,
        first_tx_sequence_number: u64,
        epoch_end_info: EpochEndInfo,
    ) -> Self {
        Self {
            epoch: last_checkpoint_summary.epoch as i64,
            epoch_total_transactions: (last_checkpoint_summary.network_total_transactions
                - first_tx_sequence_number) as i64,
            last_checkpoint_id: *last_checkpoint_summary.sequence_number() as i64,
            epoch_end_timestamp: last_checkpoint_summary.timestamp_ms as i64,
            storage_fund_reinvestment: epoch_end_info.storage_fund_reinvestment as i64,
            storage_charge: epoch_end_info.storage_charge as i64,
            storage_rebate: epoch_end_info.storage_rebate as i64,
            leftover_storage_fund_inflow: epoch_end_info.leftover_storage_fund_inflow as i64,
            stake_subsidy_amount: epoch_end_info.stake_subsidy_amount as i64,
            total_gas_fees: epoch_end_info.total_gas_fees as i64,
            total_stake_rewards_distributed: epoch_end_info.total_stake_rewards_distributed as i64,
            epoch_commitments: bcs::to_bytes(
                &last_checkpoint_summary
                    .end_of_epoch_data
                    .clone()
                    .unwrap()
                    .epoch_commitments,
            )
            .unwrap(),
        }
    }
}

impl StoredEpochInfo {
    pub fn get_json_system_state_summary(&self) -> Result<SuiSystemStateSummary, IndexerError> {
        let Some(system_state_summary_json) = self.system_state_summary_json.clone() else {
            return Err(IndexerError::PersistentStorageDataCorruptionError(
                "System state summary is null for the given epoch".into(),
            ));
        };
        let system_state_summary: SuiSystemStateSummary =
            serde_json::from_value(system_state_summary_json).map_err(|_| {
                IndexerError::PersistentStorageDataCorruptionError(format!(
                    "Failed to deserialize `system_state` for epoch {:?}",
                    self.epoch,
                ))
            })?;
        debug_assert_eq!(system_state_summary.epoch, self.epoch as u64);
        Ok(system_state_summary)
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
        let end_of_epoch_info = (&value).into();
        let system_state_summary = value.get_json_system_state_summary()?;
        Ok(EpochInfo {
            epoch: value.epoch as u64,
            validators: system_state_summary.active_validators,
            epoch_total_transactions: value.epoch_total_transactions.unwrap_or(0) as u64,
            first_checkpoint_id: value.first_checkpoint_id as u64,
            epoch_start_timestamp: value.epoch_start_timestamp as u64,
            end_of_epoch_info,
            reference_gas_price: Some(value.reference_gas_price as u64),
        })
    }
}
