// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost_types::FieldMask;
use sui_sdk_types::ObjectId;

use crate::error::ObjectNotFoundError;
use crate::field_mask::FieldMaskTree;
use crate::field_mask::FieldMaskUtil;
use crate::message::MessageMergeFrom;
use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::proto::rpc::v2beta::BatchGetObjectsRequest;
use crate::proto::rpc::v2beta::BatchGetObjectsResponse;
use crate::proto::rpc::v2beta::GetObjectRequest;
use crate::proto::rpc::v2beta::Object;
use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;

#[tracing::instrument(skip(service))]
pub fn get_object(
    service: &RpcService,
    GetObjectRequest {
        object_id,
        version,
        read_mask,
    }: GetObjectRequest,
) -> Result<Object, RpcError> {
    let object_id = object_id
        .ok_or_else(|| FieldViolation::new("object_id").with_reason(ErrorReason::FieldMissing))?
        .parse()
        .map_err(|e| {
            FieldViolation::new("object_id")
                .with_description(format!("invalid object_id: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    let read_mask = {
        let read_mask =
            read_mask.unwrap_or_else(|| FieldMask::from_str(GetObjectRequest::READ_MASK_DEFAULT));
        read_mask.validate::<Object>().map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        FieldMaskTree::from(read_mask)
    };

    get_object_impl(service, object_id, version, &read_mask)
}

#[tracing::instrument(skip(service))]
pub fn batch_get_objects(
    service: &RpcService,
    BatchGetObjectsRequest {
        requests,
        read_mask,
    }: BatchGetObjectsRequest,
) -> Result<BatchGetObjectsResponse, RpcError> {
    let read_mask = {
        let read_mask = read_mask
            .unwrap_or_else(|| FieldMask::from_str(BatchGetObjectsRequest::READ_MASK_DEFAULT));
        read_mask.validate::<Object>().map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        FieldMaskTree::from(read_mask)
    };

    let objects = requests
        .into_iter()
        .enumerate()
        .map(|(idx, request)| {
            let object_id = request
                .object_id
                .ok_or_else(|| {
                    FieldViolation::new("object_id")
                        .with_reason(ErrorReason::FieldMissing)
                        .nested_at("requests", idx)
                })?
                .parse()
                .map_err(|e| {
                    FieldViolation::new("object_id")
                        .with_description(format!("invalid object_id: {e}"))
                        .with_reason(ErrorReason::FieldInvalid)
                        .nested_at("requests", idx)
                })?;

            get_object_impl(service, object_id, request.version, &read_mask)
        })
        .collect::<Result<_, _>>()?;

    Ok(BatchGetObjectsResponse { objects })
}

#[tracing::instrument(skip(service))]
fn get_object_impl(
    service: &RpcService,
    object_id: ObjectId,
    version: Option<u64>,
    read_mask: &FieldMaskTree,
) -> Result<Object, RpcError> {
    let object = if let Some(version) = version {
        service
            .reader
            .get_object_with_version(object_id, version)?
            .ok_or_else(|| ObjectNotFoundError::new_with_version(object_id, version))?
    } else {
        service
            .reader
            .get_object(object_id)?
            .ok_or_else(|| ObjectNotFoundError::new(object_id))?
    };

    Ok(Object::merge_from(object, read_mask))
}
