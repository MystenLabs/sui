// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost_types::FieldMask;
use sui_sdk_types::ObjectId;
use sui_types::sui_sdk_types_conversions::struct_tag_sdk_to_core;

use crate::error::ObjectNotFoundError;
use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::proto::rpc::v2beta::BatchGetObjectsRequest;
use crate::proto::rpc::v2beta::BatchGetObjectsResponse;
use crate::proto::rpc::v2beta::GetObjectRequest;
use crate::proto::rpc::v2beta::Object;
use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::merge::Merge;

type ValidationResult = Result<(Vec<(ObjectId, Option<u64>)>, FieldMaskTree), RpcError>;

pub fn validate_get_object_requests(
    requests: Vec<(Option<String>, Option<u64>)>,
    read_mask: Option<FieldMask>,
) -> ValidationResult {
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
    let requests = requests
        .into_iter()
        .enumerate()
        .map(|(idx, (object_id, version))| {
            let object_id = object_id
                .as_ref()
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
            Ok((object_id, version))
        })
        .collect::<Result<_, RpcError>>()?;
    Ok((requests, read_mask))
}

#[tracing::instrument(skip(service))]
pub fn get_object(
    service: &RpcService,
    GetObjectRequest {
        object_id,
        version,
        read_mask,
    }: GetObjectRequest,
) -> Result<Object, RpcError> {
    let (requests, read_mask) =
        validate_get_object_requests(vec![(object_id, version)], read_mask)?;
    let (object_id, version) = requests[0];
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
    let requests = requests
        .into_iter()
        .map(|req| (req.object_id, req.version))
        .collect();
    let (requests, read_mask) = validate_get_object_requests(requests, read_mask)?;
    let objects = requests
        .into_iter()
        .map(|(object_id, version)| get_object_impl(service, object_id, version, &read_mask))
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

    let mut message = Object::default();

    if read_mask.contains(Object::JSON_FIELD.name) {
        message.json = object
            .as_struct()
            .and_then(|s| {
                let struct_tag = struct_tag_sdk_to_core(s.object_type().to_owned()).ok()?;
                let layout = service
                    .reader
                    .inner()
                    .get_struct_layout(&struct_tag)
                    .ok()
                    .flatten()?;
                Some((layout, s.contents()))
            })
            .and_then(|(layout, contents)| {
                sui_types::proto_value::ProtoVisitorBuilder::new(
                    service.config.max_json_move_value_size(),
                )
                .deserialize_value(contents, &layout)
                .map_err(|e| tracing::debug!("unable to convert to JSON: {e}"))
                .ok()
                .map(Box::new)
            });
    }

    message.merge(object, read_mask);

    Ok(message)
}
