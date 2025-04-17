// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::rpc::v2beta::Epoch;
use crate::proto::rpc::v2beta::GetEpochRequest;
use crate::Result;
use crate::RpcService;
use sui_sdk_types::EpochId;

#[tracing::instrument(skip(service))]
pub fn get_epoch(service: &RpcService, request: GetEpochRequest) -> Result<Epoch> {
    let mut message = Epoch::default();

    let epoch = if let Some(epoch) = request.epoch {
        epoch
    } else {
        let system_summary = service.reader.get_system_state_summary()?;

        message.reference_gas_price = Some(system_summary.reference_gas_price);

        message.protocol_config =
            Some(service.get_protocol_config(Some(system_summary.protocol_version))?);

        system_summary.epoch
    };

    message.epoch = Some(epoch);

    let committee = service
        .reader
        .get_committee(epoch)
        .ok_or_else(|| CommitteeNotFoundError::new(epoch))?;

    message.committee = Some(committee.into());

    Ok(message)
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
