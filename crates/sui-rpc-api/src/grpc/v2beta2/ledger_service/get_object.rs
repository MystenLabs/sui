// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::ObjectNotFoundError;
use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::merge::Merge;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2beta2::BatchGetObjectsRequest;
use sui_rpc::proto::sui::rpc::v2beta2::BatchGetObjectsResponse;
use sui_rpc::proto::sui::rpc::v2beta2::GetObjectRequest;
use sui_rpc::proto::sui::rpc::v2beta2::GetObjectResponse;
use sui_rpc::proto::sui::rpc::v2beta2::GetObjectResult;
use sui_rpc::proto::sui::rpc::v2beta2::Object;
use sui_sdk_types::Address;

pub const READ_MASK_DEFAULT: &str = "object_id,version,digest";

type ValidationResult = Result<(Vec<(Address, Option<u64>)>, FieldMaskTree), RpcError>;

pub fn validate_get_object_requests(
    requests: Vec<(Option<String>, Option<u64>)>,
    read_mask: Option<FieldMask>,
) -> ValidationResult {
    let read_mask = {
        let read_mask = read_mask.unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
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
) -> Result<GetObjectResponse, RpcError> {
    let (requests, read_mask) =
        validate_get_object_requests(vec![(object_id, version)], read_mask)?;
    let (object_id, version) = requests[0];
    get_object_impl(service, object_id, version, &read_mask).map(|object| GetObjectResponse {
        object: Some(object),
    })
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
        .map(|result| match result {
            Ok(object) => GetObjectResult::new_object(object),
            Err(error) => GetObjectResult::new_error(error.into_status_proto()),
        })
        .collect();
    Ok(BatchGetObjectsResponse { objects })
}

#[tracing::instrument(skip(service))]
fn get_object_impl(
    service: &RpcService,
    object_id: Address,
    version: Option<u64>,
    read_mask: &FieldMaskTree,
) -> Result<Object, RpcError> {
    let object = if let Some(version) = version {
        service
            .reader
            .inner()
            .get_object_by_key(&object_id.into(), version.into())
            .ok_or_else(|| ObjectNotFoundError::new_with_version(object_id, version))?
    } else {
        service
            .reader
            .inner()
            .get_object(&object_id.into())
            .ok_or_else(|| ObjectNotFoundError::new(object_id))?
    };

    let mut message = Object::default();

    if read_mask.contains(Object::JSON_FIELD.name) {
        message.json = crate::grpc::v2beta2::render_object_to_json(service, &object).map(Box::new);
    }

    message.merge(object, read_mask);

    Ok(message)
}
