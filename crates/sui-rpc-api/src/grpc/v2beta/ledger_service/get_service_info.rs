// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::rpc::v2beta::GetServiceInfoResponse;
use crate::proto::types::timestamp_ms_to_proto;
use crate::RpcError;
use crate::RpcService;
use sui_sdk_types::CheckpointDigest;
use tap::Pipe;

#[tracing::instrument(skip(service))]
pub fn get_service_info(service: &RpcService) -> Result<GetServiceInfoResponse, RpcError> {
    let latest_checkpoint = service.reader.inner().get_latest_checkpoint()?;
    let lowest_available_checkpoint = service
        .reader
        .inner()
        .get_lowest_available_checkpoint()?
        .pipe(Some);
    let lowest_available_checkpoint_objects = service
        .reader
        .inner()
        .get_lowest_available_checkpoint_objects()?
        .pipe(Some);

    GetServiceInfoResponse {
        chain_id: Some(CheckpointDigest::new(service.chain_id().as_bytes().to_owned()).to_string()),
        chain: Some(service.chain_id().chain().as_str().into()),
        epoch: Some(latest_checkpoint.epoch()),
        checkpoint_height: Some(latest_checkpoint.sequence_number),
        timestamp: Some(timestamp_ms_to_proto(latest_checkpoint.timestamp_ms)),
        lowest_available_checkpoint,
        lowest_available_checkpoint_objects,
        server_version: Some(service.software_version().into()),
    }
    .pipe(Ok)
}
