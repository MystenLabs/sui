// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//
// Epoch store and API
//

use crate::gql_queries::EpochData;
use crate::{data_store::DataStore, errors::ReplayError};
use std::{cmp::Ordering, collections::BTreeMap};
use sui_types::{
    committee::ProtocolVersion,
    digests::TransactionDigest,
    supported_protocol_versions::{Chain, ProtocolConfig},
};
use tracing::debug;

#[derive(Debug)]
pub enum EpochStore {
    None,
    EpochInfoRpc(EpochStoreRpc),
    EpochInfoTable(EpochStoreTable),
}

impl EpochStore {
    pub fn none() -> Self {
        EpochStore::None
    }

    pub async fn gql_table(data_store: &DataStore) -> Result<Self, ReplayError> {
        let data = data_store.epochs_gql_table().await?;
        Ok(EpochStore::EpochInfoTable(EpochStoreTable { data }))
    }

    pub async fn rpc_eager(data_store: &DataStore) -> Result<Self, ReplayError> {
        let epoch_store_rpc = data_store.epoch_store_rpc().await?;
        Ok(EpochStore::EpochInfoRpc(epoch_store_rpc))
    }

    pub fn protocol_config(&self, epoch: u64, chain: Chain) -> Result<ProtocolConfig, ReplayError> {
        match self {
            EpochStore::None => todo!("None EpochStore"),
            EpochStore::EpochInfoRpc(eager) => eager.protocol_config(epoch, chain),
            EpochStore::EpochInfoTable(data) => data.protocol_config(epoch, chain),
        }
    }

    pub fn rgp(&self, epoch: u64) -> Result<u64, ReplayError> {
        match self {
            EpochStore::None => todo!("None EpochStore"),
            EpochStore::EpochInfoRpc(eager) => eager.rgp(epoch),
            EpochStore::EpochInfoTable(eager) => eager.rgp(epoch),
        }
    }

    pub fn epoch_timestamp(&self, epoch: u64) -> Result<u64, ReplayError> {
        match self {
            EpochStore::None => todo!("None EpochStore"),
            EpochStore::EpochInfoRpc(eager) => eager.epoch_timestamp(epoch),
            EpochStore::EpochInfoTable(eager) => eager.epoch_timestamp(epoch),
        }
    }
}

type EpochId = u64;

#[derive(Debug)]
pub struct EpochStoreTable {
    pub data: BTreeMap<EpochId, EpochData>,
}

impl EpochStoreTable {
    pub fn protocol_config(&self, epoch: u64, chain: Chain) -> Result<ProtocolConfig, ReplayError> {
        let epoch = self
            .data
            .get(&epoch)
            .ok_or(ReplayError::MissingProtocolConfigForEpoch { epoch })?;
        Ok(ProtocolConfig::get_for_version(
            ProtocolVersion::new(epoch.protocol_version),
            chain,
        ))
    }

    pub fn rgp(&self, epoch: u64) -> Result<u64, ReplayError> {
        let epoch = self
            .data
            .get(&epoch)
            .ok_or(ReplayError::MissingRGPForEpoch { epoch })?;
        Ok(epoch.rgp)
    }

    pub fn epoch_timestamp(&self, epoch: u64) -> Result<u64, ReplayError> {
        let epoch = self
            .data
            .get(&epoch)
            .ok_or(ReplayError::MissingTimestampForEpoch { epoch })?;
        Ok(epoch.start_timestamp)
    }
}

#[derive(Debug)]
pub struct EpochStoreRpc {
    // protocol config version and epoch range they are valid for
    pub protocol_configs: Vec<(u64, u64, u64)>,
    // rgp and epoch range they are valid for
    pub rgps: Vec<(u64, u64, u64)>,
    // epoch to start timestamp and digest
    pub epoch_info: BTreeMap<u64, (u64, TransactionDigest)>,
}

impl EpochStoreRpc {
    pub fn protocol_config(&self, epoch: u64, chain: Chain) -> Result<ProtocolConfig, ReplayError> {
        debug!("Getting protocol config for epoch {}", epoch);
        let idx = self
            .protocol_configs
            .binary_search_by(|&(_, start, end)| {
                if epoch < start {
                    Ordering::Greater
                } else if epoch > end {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            })
            .map_err(|_| ReplayError::MissingProtocolConfigForEpoch { epoch })?;
        let protocol_version = self.protocol_configs[idx].0;
        Ok(ProtocolConfig::get_for_version(
            ProtocolVersion::new(protocol_version),
            chain,
        ))
    }

    pub fn rgp(&self, epoch: u64) -> Result<u64, ReplayError> {
        debug!("Getting RGP for epoch {}", epoch);
        let idx = self
            .rgps
            .binary_search_by(|&(_, start, end)| {
                if epoch < start {
                    Ordering::Greater
                } else if epoch > end {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            })
            .map_err(|_| ReplayError::MissingRGPForEpoch { epoch })?;
        Ok(self.rgps[idx].0)
    }

    pub fn epoch_timestamp(&self, epoch: u64) -> Result<u64, ReplayError> {
        debug!("Getting epoch timestamp for epoch {}", epoch);
        self.epoch_info
            .get(&epoch)
            .map(|(timestamp, _digest)| *timestamp)
            .ok_or(ReplayError::MissingTimestampForEpoch { epoch })
    }

    pub fn epoch_digest(&self, epoch: u64) -> Result<TransactionDigest, ReplayError> {
        debug!("Getting epoch timestamp for epoch {}", epoch);
        self.epoch_info
            .get(&epoch)
            .map(|(_timestamp, digest)| *digest)
            .ok_or(ReplayError::MissingDigestForEpoch { epoch })
    }
}
