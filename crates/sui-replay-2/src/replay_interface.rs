// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::ReplayError;
use sui_types::{
    base_types::ObjectID, effects::TransactionEffects, object::Object,
    supported_protocol_versions::ProtocolConfig, transaction::TransactionData,
};

pub trait TransactionStore {
    fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<(TransactionData, TransactionEffects), ReplayError>;
}

#[derive(Clone, Debug)]
pub struct EpochData {
    pub epoch_id: u64,
    pub protocol_version: u64,
    pub rgp: u64,
    pub start_timestamp: u64,
}

pub trait EpochStore {
    fn epoch_info(&self, epoch: u64) -> Result<EpochData, ReplayError>;
    fn protocol_config(&self, epoch: u64) -> Result<ProtocolConfig, ReplayError>;
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObjectKey {
    pub object_id: ObjectID,
    pub version_query: VersionQuery,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum VersionQuery {
    Version(u64),
    RootVersion(u64),
    AtCheckpoint(u64),
    ImmutableOrLatest,
}

pub trait ObjectStore {
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Object>, ReplayError>;
}
