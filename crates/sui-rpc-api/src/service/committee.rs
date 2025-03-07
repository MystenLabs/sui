// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Result;
use crate::RpcService;
use sui_sdk_types::{EpochId, ValidatorCommittee};

impl RpcService {
    pub fn get_committee(&self, epoch: Option<EpochId>) -> Result<ValidatorCommittee> {
        let epoch = if let Some(epoch) = epoch {
            epoch
        } else {
            self.reader.inner().get_latest_checkpoint()?.epoch()
        };

        let committee = self
            .reader
            .get_committee(epoch)
            .ok_or_else(|| CommitteeNotFoundError::new(epoch))?;

        Ok(committee)
    }
}

#[derive(Debug)]
pub struct CommitteeNotFoundError {
    epoch: EpochId,
}

impl CommitteeNotFoundError {
    pub fn new(epoch: EpochId) -> Self {
        Self { epoch }
    }
}

impl std::fmt::Display for CommitteeNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Committee for epoch {} not found", self.epoch)
    }
}

impl std::error::Error for CommitteeNotFoundError {}

impl From<CommitteeNotFoundError> for crate::RpcError {
    fn from(value: CommitteeNotFoundError) -> Self {
        Self::new(tonic::Code::NotFound, value.to_string())
    }
}
