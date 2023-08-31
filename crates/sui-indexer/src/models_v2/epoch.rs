// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{Insertable, Queryable};

use crate::errors::IndexerError;
use crate::schema_v2::epochs;
use crate::types_v2::{IndexedEndOfEpochInfo, IndexedEpochInfo};
use sui_json_rpc_types::EpochInfo;

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = epochs)]
pub struct StoredEpochInfo {
    pub epoch: i64,
    pub validators: Vec<Vec<u8>>,
    pub epoch_total_transactions: i64,
    pub first_checkpoint_id: i64,
    pub epoch_start_timestamp: i64,
    // Serialized `EndOfEpochInfo`
    pub end_of_epoch_info: Option<Vec<u8>>,
    // Serialized `EndOfEpochData`
    pub end_of_epoch_data: Option<Vec<u8>>,
    pub reference_gas_price: i64,
    pub protocol_version: i64,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = epochs)]
pub struct StoredEndOfEpochInfo {
    pub epoch: i64,
    pub epoch_total_transactions: i64,
    pub end_of_epoch_info: Vec<u8>,
    pub end_of_epoch_data: Vec<u8>,
}

impl From<&IndexedEndOfEpochInfo> for StoredEndOfEpochInfo {
    fn from(e: &IndexedEndOfEpochInfo) -> Self {
        Self {
            epoch: e.epoch as i64,
            epoch_total_transactions: e.epoch_total_transactions as i64,
            end_of_epoch_info: bcs::to_bytes(&e.end_of_epoch_info).unwrap(),
            end_of_epoch_data: bcs::to_bytes(&e.end_of_epoch_data).unwrap(),
        }
    }
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
            end_of_epoch_info: e
                .end_of_epoch_info
                .as_ref()
                .map(|v| bcs::to_bytes(&v).unwrap()),
            end_of_epoch_data: e
                .end_of_epoch_data
                .as_ref()
                .map(|v| bcs::to_bytes(&v).unwrap()),
            reference_gas_price: e.reference_gas_price as i64,
            protocol_version: e.protocol_version as i64,
        }
    }
}

impl TryInto<EpochInfo> for StoredEpochInfo {
    type Error = IndexerError;
    fn try_into(self) -> Result<EpochInfo, Self::Error> {
        let validators = self
            .validators
            .into_iter()
            .map(|v| {
                bcs::from_bytes(&v).map_err(|_| {
                    IndexerError::SerdeError(format!(
                        "Failed to deserialize `validators` for epoch {}",
                        self.epoch
                    ))
                })
            })
            .collect::<Result<Vec<_>, IndexerError>>()?;
        let end_of_epoch_info = match self.end_of_epoch_info {
            None => Ok(None),
            Some(end_of_epoch_info) => bcs::from_bytes(&end_of_epoch_info).map_err(|_| {
                IndexerError::SerdeError(format!(
                    "Failed to deserialize `end_of_epoch_info` for epoch {}",
                    self.epoch
                ))
            }),
        }?;

        Ok(EpochInfo {
            epoch: self.epoch as u64,
            validators,
            epoch_total_transactions: self.epoch_total_transactions as u64,
            first_checkpoint_id: self.first_checkpoint_id as u64,
            epoch_start_timestamp: self.epoch_start_timestamp as u64,
            end_of_epoch_info,
            reference_gas_price: Some(self.reference_gas_price as u64),
        })
    }
}
