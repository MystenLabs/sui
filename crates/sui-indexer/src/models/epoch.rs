// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{Insertable, Queryable};
use jsonrpsee::core::__reexports::serde::Deserialize;

use sui_json_rpc_types::{EndOfEpochInfo, EpochInfo};
use sui_types::sui_system_state::sui_system_state_summary::SuiValidatorSummary;

use crate::errors::IndexerError;
use crate::models::system_state::DBValidatorSummary;
use crate::schema::epochs::{self, end_of_epoch_info};

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = epochs)]
pub struct StoredEpochInfo {
    pub epoch: i64,
    pub validators: Vec<Vec<u8>>,
    pub epoch_total_transactions: i64,
    pub first_checkpoint_id: i64,
    pub epoch_start_timestamp: i64,
    pub end_of_epoch_info: Option<Vec<u8>>,
    pub reference_gas_price: i64,
    pub protocol_version: i64,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = epochs)]
pub struct StoredEndOfEpochInfo {
    pub epoch: i64,
    pub epoch_total_transactions: i64,
    pub end_of_epoch_info: Vec<u8>,
}

#[derive(Debug)]
pub struct IndexedEpochInfo {
    pub epoch: u64,
    pub validators: Vec<SuiValidatorSummary>,
    pub epoch_total_transactions: u64,
    pub first_checkpoint_id: u64,
    pub epoch_start_timestamp: u64,
    pub end_of_epoch_info: Option<EndOfEpochInfo>,
    pub reference_gas_price: u64,
    pub protocol_version: u64,
}

impl From<&IndexedEpochInfo> for StoredEpochInfo {
    fn from(e: &IndexedEpochInfo) -> Self {
        Self {
            epoch: e.epoch as i64,
            validators: e
                .validators
                .iter()
                .map(|v| bcs::to_bytes(v).unwrap())
                .collect(),
            epoch_total_transactions: e.epoch_total_transactions as i64,
            first_checkpoint_id: e.first_checkpoint_id as i64,
            epoch_start_timestamp: e.epoch_start_timestamp as i64,
            end_of_epoch_info: match &e.end_of_epoch_info {
                None => None,
                Some(v) => Some(bcs::to_bytes(&v).unwrap()),
            },
            reference_gas_price: e.reference_gas_price as i64,
            protocol_version: e.protocol_version as i64,
        }
    }
}

impl TryInto<EpochInfo> for StoredEpochInfo {
    type Error = IndexerError;
    fn try_into(self) -> Result<EpochInfo, Self::Error> {
        let validators = self.validators.into_iter().map(|v|
            bcs::from_bytes(&v).map_err(IndexerError::SerdeError(format!(
                "Failed to deserialize `validators` for epoch {}",
                self.epoch
            )))).collect::<Result<Vec<DBValidatorSummary>, IndexerError>>()?;
        let end_of_epoch_info = match self.end_of_epoch_info {
            None => Ok(None),
            Some(end_of_epoch_info) => {
                bcs::from_bytes(&self.end_of_epoch_info).map_err(IndexerError::SerdeError(format!(
                    "Failed to deserialize `end_of_epoch_info` for epoch {}",
                    self.epoch
                )))
            }
        }?;

        Ok(EpochInfo {
            epoch: self.epoch as u64,
            validators,
            epoch_total_transactions: self.epoch_total_transactions as u64,
            first_checkpoint_id: self.first_checkpoint_id as u64,
            epoch_start_timestamp: self.epoch_start_timestamp as u64,
            end_of_epoch_info,
            reference_gas_price: self.reference_gas_price as u64,
        })
    }
}


#[derive(Deserialize)]
pub struct SystemEpochInfoEvent {
    pub epoch: u64,
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
