// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use sui_protocol_config::ProtocolVersion;
use sui_types::base_types::EpochId;
use sui_types::epoch_data::EpochData;
use sui_types::messages_checkpoint::CheckpointDigest;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemState;

/// Parameters of the epoch fixed at epoch start.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct EpochStartConfiguration {
    system_state: EpochStartSystemState,
    /// epoch_digest is defined as following
    /// (1) For the genesis epoch it is set to 0
    /// (2) For all other epochs it is a digest of the last checkpoint of a previous epoch
    /// Note that this is in line with how epoch start timestamp is defined
    epoch_digest: CheckpointDigest,
}

impl EpochStartConfiguration {
    pub fn new(system_state: EpochStartSystemState, epoch_digest: CheckpointDigest) -> Self {
        Self {
            system_state,
            epoch_digest,
        }
    }

    pub fn new_for_testing() -> Self {
        Self::new(
            EpochStartSystemState::new_for_testing(),
            CheckpointDigest::default(),
        )
    }

    pub fn epoch_data(&self) -> EpochData {
        EpochData::new(
            self.epoch(),
            self.epoch_start_timestamp_ms(),
            self.epoch_digest(),
        )
    }

    pub fn epoch_digest(&self) -> CheckpointDigest {
        self.epoch_digest
    }

    pub fn epoch(&self) -> EpochId {
        self.system_state.epoch
    }

    pub fn protocol_version(&self) -> ProtocolVersion {
        ProtocolVersion::new(self.system_state.protocol_version)
    }

    pub fn reference_gas_price(&self) -> u64 {
        self.system_state.reference_gas_price
    }

    pub fn safe_mode(&self) -> bool {
        self.system_state.safe_mode
    }

    pub fn epoch_start_timestamp_ms(&self) -> u64 {
        self.system_state.epoch_start_timestamp_ms
    }

    pub fn epoch_start_state(&self) -> &EpochStartSystemState {
        &self.system_state
    }
}
