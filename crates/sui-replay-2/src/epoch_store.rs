// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//
// Epoch store and API
//

use crate::errors::ReplayError;
use crate::{data_store::DataStore, gql_queries::EpochData};
use std::collections::BTreeMap;
use sui_types::{
    committee::ProtocolVersion,
    supported_protocol_versions::{Chain, ProtocolConfig},
};
use tracing::debug;

type EpochId = u64;

// Eager loading of the epoch table from GQL.
// Maps an epoch to data vital to trascation execution:
// framework versions, protocol version, RGP, epoch start timestamp
#[derive(Debug)]
pub struct EpochStore {
    pub data: BTreeMap<EpochId, EpochData>,
}

impl EpochStore {
    pub async fn new(data_store: &DataStore) -> Result<Self, ReplayError> {
        debug!("Start EpochStore::new");
        let data = data_store.epochs_gql_table().await?;
        debug!("End EpochStore::new");
        Ok(Self { data })
    }

    // Get the protocol config for an epoch
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

    // Get the RGP for an epoch
    pub fn rgp(&self, epoch: u64) -> Result<u64, ReplayError> {
        let epoch = self
            .data
            .get(&epoch)
            .ok_or(ReplayError::MissingRGPForEpoch { epoch })?;
        Ok(epoch.rgp)
    }

    // Get the start timestamp for an epoch
    pub fn epoch_timestamp(&self, epoch: u64) -> Result<u64, ReplayError> {
        let epoch = self
            .data
            .get(&epoch)
            .ok_or(ReplayError::MissingTimestampForEpoch { epoch })?;
        Ok(epoch.start_timestamp)
    }
}
