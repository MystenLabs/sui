// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::RpcError;
use crate::RpcService;
use sui_rpc::proto::sui::rpc::v2beta2::GetServiceInfoResponse;
use sui_rpc::proto::timestamp_ms_to_proto;
use sui_sdk_types::Digest;
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

    let mut message = GetServiceInfoResponse::default();
    message.chain_id = Some(Digest::new(service.chain_id().as_bytes().to_owned()).to_string());
    message.chain = Some(service.chain_id().chain().as_str().into());
    message.epoch = Some(latest_checkpoint.epoch());
    message.checkpoint_height = Some(latest_checkpoint.sequence_number);
    message.timestamp = Some(timestamp_ms_to_proto(latest_checkpoint.timestamp_ms));
    message.lowest_available_checkpoint = lowest_available_checkpoint;
    message.lowest_available_checkpoint_objects = lowest_available_checkpoint_objects;
    message.server = service.server_version().map(ToString::to_string);
    Ok(message)
}
