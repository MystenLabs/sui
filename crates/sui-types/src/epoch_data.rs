// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::EpochId;
use crate::message_envelope::Message;
use crate::messages_checkpoint::{CheckpointDigest, CheckpointSummary, CheckpointTimestamp};

/// The static epoch information that is accessible to move smart contracts
#[derive(Default)]
pub struct EpochData {
    epoch_id: EpochId,
    epoch_start_timestamp: CheckpointTimestamp,
    epoch_digest: CheckpointDigest,
}

impl EpochData {
    pub fn new(
        epoch_id: EpochId,
        epoch_start_timestamp: CheckpointTimestamp,
        epoch_digest: CheckpointDigest,
    ) -> Self {
        Self {
            epoch_id,
            epoch_start_timestamp,
            epoch_digest,
        }
    }

    pub fn new_genesis(epoch_start_timestamp: CheckpointTimestamp) -> Self {
        Self {
            epoch_id: 0,
            epoch_start_timestamp,
            epoch_digest: Default::default(),
        }
    }

    pub fn new_from_epoch_checkpoint(
        epoch_id: EpochId,
        epoch_checkpoint: &CheckpointSummary,
    ) -> Self {
        Self {
            epoch_id,
            epoch_start_timestamp: epoch_checkpoint.timestamp_ms,
            epoch_digest: epoch_checkpoint.digest(),
        }
    }

    pub fn new_test() -> Self {
        Default::default()
    }

    pub fn epoch_id(&self) -> EpochId {
        self.epoch_id
    }

    pub fn epoch_start_timestamp(&self) -> CheckpointTimestamp {
        self.epoch_start_timestamp
    }

    pub fn epoch_digest(&self) -> CheckpointDigest {
        self.epoch_digest
    }
}
