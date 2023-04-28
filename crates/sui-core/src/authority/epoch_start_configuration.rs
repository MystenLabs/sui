// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};

use std::fmt;
use sui_types::epoch_data::EpochData;
use sui_types::messages_checkpoint::{CheckpointDigest, CheckpointTimestamp};
use sui_types::sui_system_state::epoch_start_sui_system_state::{
    EpochStartSystemState, EpochStartSystemStateTrait,
};

#[enum_dispatch]
pub trait EpochStartConfigTrait {
    fn epoch_digest(&self) -> CheckpointDigest;
    fn epoch_start_state(&self) -> &EpochStartSystemState;
    fn flags(&self) -> &[EpochFlag];
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum EpochFlag {
    InMemoryCheckpointRoots,
}

/// Parameters of the epoch fixed at epoch start.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
#[enum_dispatch(EpochStartConfigTrait)]
pub enum EpochStartConfiguration {
    V1(EpochStartConfigurationV1),
    V2(EpochStartConfigurationV2),
}

impl EpochStartConfiguration {
    pub fn new(system_state: EpochStartSystemState, epoch_digest: CheckpointDigest) -> Self {
        Self::new_v2(
            system_state,
            epoch_digest,
            EpochFlag::default_flags_for_new_epoch(),
        )
    }

    pub fn new_v1(system_state: EpochStartSystemState, epoch_digest: CheckpointDigest) -> Self {
        Self::V1(EpochStartConfigurationV1::new(system_state, epoch_digest))
    }

    pub fn new_v2(
        system_state: EpochStartSystemState,
        epoch_digest: CheckpointDigest,
        flags: Vec<EpochFlag>,
    ) -> Self {
        Self::V2(EpochStartConfigurationV2::new(
            system_state,
            epoch_digest,
            flags,
        ))
    }

    pub fn epoch_data(&self) -> EpochData {
        EpochData::new(
            self.epoch_start_state().epoch(),
            self.epoch_start_state().epoch_start_timestamp_ms(),
            self.epoch_digest(),
        )
    }

    pub fn epoch_start_timestamp_ms(&self) -> CheckpointTimestamp {
        self.epoch_start_state().epoch_start_timestamp_ms()
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct EpochStartConfigurationV1 {
    system_state: EpochStartSystemState,
    /// epoch_digest is defined as following
    /// (1) For the genesis epoch it is set to 0
    /// (2) For all other epochs it is a digest of the last checkpoint of a previous epoch
    /// Note that this is in line with how epoch start timestamp is defined
    epoch_digest: CheckpointDigest,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct EpochStartConfigurationV2 {
    system_state: EpochStartSystemState,
    epoch_digest: CheckpointDigest,
    flags: Vec<EpochFlag>,
}

impl EpochStartConfigurationV1 {
    pub fn new(system_state: EpochStartSystemState, epoch_digest: CheckpointDigest) -> Self {
        Self {
            system_state,
            epoch_digest,
        }
    }
}

impl EpochStartConfigurationV2 {
    pub fn new(
        system_state: EpochStartSystemState,
        epoch_digest: CheckpointDigest,
        flags: Vec<EpochFlag>,
    ) -> Self {
        Self {
            system_state,
            epoch_digest,
            flags,
        }
    }
}

impl EpochStartConfigTrait for EpochStartConfigurationV1 {
    fn epoch_digest(&self) -> CheckpointDigest {
        self.epoch_digest
    }

    fn epoch_start_state(&self) -> &EpochStartSystemState {
        &self.system_state
    }

    fn flags(&self) -> &[EpochFlag] {
        &[]
    }
}

impl EpochStartConfigTrait for EpochStartConfigurationV2 {
    fn epoch_digest(&self) -> CheckpointDigest {
        self.epoch_digest
    }

    fn epoch_start_state(&self) -> &EpochStartSystemState {
        &self.system_state
    }

    fn flags(&self) -> &[EpochFlag] {
        &self.flags
    }
}

impl EpochFlag {
    pub fn default_flags_for_new_epoch() -> Vec<Self> {
        vec![EpochFlag::InMemoryCheckpointRoots]
    }
}

impl fmt::Display for EpochFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Important - implementation should return low cardinality values because this is used as metric key
        match self {
            EpochFlag::InMemoryCheckpointRoots => write!(f, "InMemoryCheckpointRoots"),
        }
    }
}
