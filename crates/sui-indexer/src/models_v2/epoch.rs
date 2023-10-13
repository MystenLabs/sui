// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{Insertable, Queryable};

use crate::errors::IndexerError;
use crate::schema_v2::epochs;
use crate::types_v2::IndexedEpochInfo;
use sui_json_rpc_types::{EndOfEpochInfo, EpochInfo};

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = epochs)]
pub struct StoredEpochInfo {
    pub epoch: i64,
    pub validators: Vec<Option<Vec<u8>>>,
    pub first_checkpoint_id: i64,
    pub epoch_start_timestamp: i64,
    pub reference_gas_price: i64,
    pub protocol_version: i64,
    pub epoch_total_transactions: Option<i64>,
    pub last_checkpoint_id: Option<i64>,
    pub epoch_end_timestamp: Option<i64>,
    pub storage_fund_reinvestment: Option<i64>,
    pub storage_charge: Option<i64>,
    pub storage_rebate: Option<i64>,
    pub storage_fund_balance: Option<i64>,
    pub stake_subsidy_amount: Option<i64>,
    pub total_gas_fees: Option<i64>,
    pub total_stake_rewards_distributed: Option<i64>,
    pub leftover_storage_fund_inflow: Option<i64>,
    pub new_total_stake: Option<i64>,
    pub epoch_commitments: Option<Vec<u8>>,
    pub next_epoch_reference_gas_price: Option<i64>,
    pub next_epoch_protocol_version: Option<i64>,
}

impl StoredEpochInfo {
    pub fn from_epoch_beginning_info(e: &IndexedEpochInfo) -> Self {
        Self {
            epoch: e.epoch as i64,
            validators: e
                .validators
                .iter()
                .map(|v| Some(bcs::to_bytes(v).unwrap()))
                .collect(),
            first_checkpoint_id: e.first_checkpoint_id as i64,
            epoch_start_timestamp: e.epoch_start_timestamp as i64,
            reference_gas_price: e.reference_gas_price as i64,
            protocol_version: e.protocol_version as i64,
            ..Default::default()
        }
    }

    pub fn from_epoch_end_info(e: &IndexedEpochInfo) -> Self {
        Self {
            epoch: e.epoch as i64,
            epoch_total_transactions: e.epoch_total_transactions.map(|v| v as i64),
            last_checkpoint_id: e.last_checkpoint_id.map(|v| v as i64),
            epoch_end_timestamp: e.epoch_end_timestamp.map(|v| v as i64),
            storage_fund_reinvestment: e.storage_fund_reinvestment.map(|v| v as i64),
            storage_charge: e.storage_charge.map(|v| v as i64),
            storage_rebate: e.storage_rebate.map(|v| v as i64),
            storage_fund_balance: e.storage_fund_balance.map(|v| v as i64),
            stake_subsidy_amount: e.stake_subsidy_amount.map(|v| v as i64),
            total_gas_fees: e.total_gas_fees.map(|v| v as i64),
            total_stake_rewards_distributed: e.total_stake_rewards_distributed.map(|v| v as i64),
            leftover_storage_fund_inflow: e.leftover_storage_fund_inflow.map(|v| v as i64),
            new_total_stake: e.new_total_stake.map(|v| v as i64),
            epoch_commitments: e
                .epoch_commitments
                .as_ref()
                .map(|v| bcs::to_bytes(&v).unwrap()),
            next_epoch_reference_gas_price: e.next_epoch_reference_gas_price.map(|v| v as i64),
            next_epoch_protocol_version: e.next_epoch_protocol_version.map(|v| v as i64),

            // For the following fields:
            // we don't update these columns when persisting EndOfEpoch data.
            // However if the data is partial, diesel would interpret them
            // as Null and hence cause errors.
            validators: vec![],
            first_checkpoint_id: 0,
            epoch_start_timestamp: 0,
            reference_gas_price: 0,
            protocol_version: 0,
        }
    }
}

impl From<&StoredEpochInfo> for Option<EndOfEpochInfo> {
    fn from(info: &StoredEpochInfo) -> Option<EndOfEpochInfo> {
        Some(EndOfEpochInfo {
            reference_gas_price: (info.reference_gas_price as u64),
            protocol_version: (info.protocol_version as u64),
            last_checkpoint_id: info.last_checkpoint_id.map(|v| v as u64)?,
            total_stake: info.new_total_stake.map(|v| v as u64)?,
            epoch_end_timestamp: info.epoch_end_timestamp.map(|v| v as u64)?,
            storage_fund_reinvestment: info.storage_fund_reinvestment.map(|v| v as u64)?,
            storage_charge: info.storage_charge.map(|v| v as u64)?,
            storage_rebate: info.storage_rebate.map(|v| v as u64)?,
            storage_fund_balance: info.storage_fund_balance.map(|v| v as u64)?,
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

        let validators = value
            .validators
            .into_iter()
            .flatten()
            .map(|v| {
                bcs::from_bytes(&v).map_err(|_| {
                    IndexerError::PersistentStorageDataCorruptionError(format!(
                        "Failed to deserialize `validators` for epoch {epoch}",
                    ))
                })
            })
            .collect::<Result<Vec<_>, IndexerError>>()?;
        Ok(EpochInfo {
            epoch: value.epoch as u64,
            validators,
            epoch_total_transactions: value.epoch_total_transactions.unwrap_or(0) as u64,
            first_checkpoint_id: value.first_checkpoint_id as u64,
            epoch_start_timestamp: value.epoch_start_timestamp as u64,
            end_of_epoch_info,
            reference_gas_price: Some(value.reference_gas_price as u64),
        })
    }
}
