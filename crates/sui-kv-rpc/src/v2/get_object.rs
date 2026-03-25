// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use sui_kvstore::{BigTableClient, KeyValueStoreReader};
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsResponse;
use sui_rpc::proto::sui::rpc::v2::Object;
use sui_rpc::proto::sui::rpc::v2::{GetObjectRequest, GetObjectResponse, GetObjectResult};
use sui_rpc_api::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc_api::{
    ErrorReason, ObjectNotFoundError, RpcError,
    grpc::v2::ledger_service::validate_get_object_requests,
};
use sui_types::storage::ObjectKey;

use super::render_json;
use crate::PackageResolver;

pub const MAX_BATCH_REQUESTS: usize = 1000;

pub(crate) async fn get_object(
    mut client: BigTableClient,
    GetObjectRequest {
        object_id,
        version,
        read_mask,
        ..
    }: GetObjectRequest,
    resolver: &PackageResolver,
) -> Result<GetObjectResponse, RpcError> {
    let (requests, read_mask) =
        validate_get_object_requests(vec![(object_id, version)], read_mask)?;
    let (object_id, version) = requests[0];
    let object = match version {
        Some(version) => client
            .get_objects(&[ObjectKey(object_id.into(), version.into())])
            .await?
            .pop()
            .ok_or_else(|| ObjectNotFoundError::new_with_version(object_id, version))?,
        None => client
            .get_latest_object(&object_id.into())
            .await?
            .ok_or_else(|| ObjectNotFoundError::new(object_id))?,
    };
    let mut message = Object::default();
    if read_mask.contains(Object::JSON_FIELD)
        && let Some(move_object) = object.data.try_as_move()
    {
        message.json = render_json(
            resolver,
            &move_object.type_().clone().into(),
            move_object.contents(),
        )
        .await
        .map(Box::new);
    }
    message.merge(&object, &read_mask);
    Ok(GetObjectResponse::new(message))
}

pub(crate) async fn batch_get_objects(
    mut client: BigTableClient,
    BatchGetObjectsRequest {
        requests,
        read_mask,
        ..
    }: BatchGetObjectsRequest,
    resolver: &PackageResolver,
) -> Result<BatchGetObjectsResponse, RpcError> {
    if requests.len() > MAX_BATCH_REQUESTS {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!("number of batch requests exceed limit of {MAX_BATCH_REQUESTS}"),
        ));
    }

    // only batch requests with `object_id` and `exact_version` are supported by the KV store
    if requests.iter().any(|r| r.version.is_none()) {
        return Err(FieldViolation::new("version")
            .with_reason(ErrorReason::FieldInvalid)
            .with_description("KV store supports batch requests with exact object versioning")
            .into());
    }
    let requests = requests
        .into_iter()
        .map(|req| (req.object_id, req.version))
        .collect();
    let (requests, read_mask) = validate_get_object_requests(requests, read_mask)?;
    let object_keys: Vec<_> = requests
        .into_iter()
        .map(|(object_id, version)| {
            ObjectKey(
                object_id.into(),
                version.expect("invariant's already checked").into(),
            )
        })
        .collect();
    let response: HashMap<_, _> = client
        .get_objects(&object_keys)
        .await?
        .into_iter()
        .map(|obj| ((obj.id(), obj.version()), obj))
        .collect();

    let needs_json = read_mask.contains(Object::JSON_FIELD);
    let mut objects = Vec::with_capacity(object_keys.len());
    for object_key in object_keys {
        if let Some(object) = response.get(&(object_key.0, object_key.1)) {
            let mut message = Object::default();
            if needs_json && let Some(move_object) = object.data.try_as_move() {
                message.json = render_json(
                    resolver,
                    &move_object.type_().clone().into(),
                    move_object.contents(),
                )
                .await
                .map(Box::new);
            }
            message.merge(object, &read_mask);
            objects.push(GetObjectResult::new_object(message));
        } else {
            let err: RpcError =
                ObjectNotFoundError::new_with_version(object_key.0.into(), object_key.1.into())
                    .into();
            objects.push(GetObjectResult::new_error(err.into_status_proto()));
        }
    }

    Ok(BatchGetObjectsResponse::new(objects))
}
