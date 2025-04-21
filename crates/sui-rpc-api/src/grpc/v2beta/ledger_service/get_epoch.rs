// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::field_mask::FieldMaskTree;
use crate::field_mask::FieldMaskUtil;
use crate::message::MessageMergeFrom;
use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::proto::rpc::v2beta::Epoch;
use crate::proto::rpc::v2beta::GetEpochRequest;
use crate::proto::rpc::v2beta::ProtocolConfig;
use crate::proto::timestamp_ms_to_proto;
use crate::ErrorReason;
use crate::Result;
use crate::RpcService;
use prost_types::FieldMask;
use sui_sdk_types::EpochId;

#[tracing::instrument(skip(service))]
pub fn get_epoch(service: &RpcService, request: GetEpochRequest) -> Result<Epoch> {
    let read_mask = {
        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(GetEpochRequest::READ_MASK_DEFAULT));
        read_mask.validate::<Epoch>().map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        FieldMaskTree::from(read_mask)
    };

    let mut message = Epoch::default();

    let epoch = if let Some(epoch) = request.epoch {
        epoch
    } else {
        service.reader.inner().get_latest_checkpoint()?.epoch()
    };

    if read_mask.contains(Epoch::EPOCH_FIELD.name) {
        message.epoch = Some(epoch);
    }

    if let Some(epoch_info) = service
        .reader
        .inner()
        .indexes()
        .and_then(|indexes| indexes.get_epoch_info(epoch).ok().flatten())
    {
        if read_mask.contains(Epoch::FIRST_CHECKPOINT_FIELD.name) {
            message.first_checkpoint = epoch_info.start_checkpoint;
        }

        if read_mask.contains(Epoch::LAST_CHECKPOINT_FIELD.name) {
            message.last_checkpoint = epoch_info.end_checkpoint;
        }

        if read_mask.contains(Epoch::START_FIELD.name) {
            message.start = epoch_info.start_timestamp_ms.map(timestamp_ms_to_proto);
        }

        if read_mask.contains(Epoch::END_FIELD.name) {
            message.end = epoch_info.end_timestamp_ms.map(timestamp_ms_to_proto);
        }

        if read_mask.contains(Epoch::REFERENCE_GAS_PRICE_FIELD.name) {
            message.reference_gas_price = epoch_info.reference_gas_price;
        }

        if let Some(submask) = read_mask.subtree(Epoch::PROTOCOL_CONFIG_FIELD.name) {
            let protocol_config = epoch_info
                .protocol_version
                .map(|version| service.get_protocol_config(Some(version)))
                .transpose()?;

            message.protocol_config =
                protocol_config.map(|config| ProtocolConfig::merge_from(config, &submask));
        }
    }

    if read_mask.contains(Epoch::COMMITTEE_FIELD.name) {
        message.committee = Some(
            service
                .reader
                .get_committee(epoch)
                .ok_or_else(|| CommitteeNotFoundError::new(epoch))?
                .into(),
        );
    }

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
