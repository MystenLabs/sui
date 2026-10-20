// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use crate::error::ObjectNotFoundError;
use crate::reader::AtCheckpointUpperBound;
use mysten_common::ZipDebugEqIteratorExt;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsResponse;
use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
use sui_rpc::proto::sui::rpc::v2::GetObjectResponse;
use sui_rpc::proto::sui::rpc::v2::GetObjectResult;
use sui_rpc::proto::sui::rpc::v2::Object;
use sui_sdk_types::Address;
use sui_types::full_checkpoint_content::ObjectSet;

pub const MAX_BATCH_REQUESTS: usize = 1000;
pub const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::OBJECT;

/// Which state of an object a `GetObjectRequest` reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectSelector {
    /// The latest live version (neither `version` nor `at_checkpoint` set).
    Latest,
    /// Exactly this version.
    Version(u64),
    /// The object's state at the end of this checkpoint.
    AtCheckpoint(u64),
}

type ValidationResult = Result<(Vec<(Address, ObjectSelector)>, FieldMaskTree), RpcError>;

/// Validate `(object_id, version, at_checkpoint)` for each request, and the read mask shared by
/// all of them. A request that sets both `version` and `at_checkpoint` is rejected.
pub fn validate_get_object_requests(
    requests: Vec<(Option<String>, Option<u64>, Option<u64>)>,
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
        .map(|(idx, (object_id, version, at_checkpoint))| {
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
            let selector = match (version, at_checkpoint) {
                (None, None) => ObjectSelector::Latest,
                (Some(version), None) => ObjectSelector::Version(version),
                (None, Some(checkpoint)) => ObjectSelector::AtCheckpoint(checkpoint),
                (Some(_), Some(_)) => {
                    return Err(FieldViolation::new("at_checkpoint")
                        .with_description("version and at_checkpoint are mutually exclusive")
                        .with_reason(ErrorReason::FieldInvalid)
                        .nested_at("requests", idx)
                        .into());
                }
            };
            Ok((object_id, selector))
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
        at_checkpoint,
        ..
    }: GetObjectRequest,
) -> Result<GetObjectResponse, RpcError> {
    let (requests, read_mask) =
        validate_get_object_requests(vec![(object_id, version, at_checkpoint)], read_mask)?;
    let upper_bound = load_at_checkpoint_upper_bound(service, &requests)?;
    let (object_id, selector) = requests[0];
    let mut results = [get_object_impl(
        service,
        object_id,
        selector,
        upper_bound.as_ref(),
        &read_mask,
    )];
    apply_at_checkpoint_lower_bound(service, &requests, &mut results)?;
    let [result] = results;
    result.map(GetObjectResponse::new)
}

#[tracing::instrument(skip(service))]
pub fn batch_get_objects(
    service: &RpcService,
    BatchGetObjectsRequest {
        requests,
        read_mask,
        ..
    }: BatchGetObjectsRequest,
) -> Result<BatchGetObjectsResponse, RpcError> {
    if requests.len() > MAX_BATCH_REQUESTS {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!("number of batch requests exceed limit of {MAX_BATCH_REQUESTS}"),
        ));
    }

    let requests = requests
        .into_iter()
        .map(|req| (req.object_id, req.version, req.at_checkpoint))
        .collect();
    let (requests, read_mask) = validate_get_object_requests(requests, read_mask)?;

    let upper_bound = load_at_checkpoint_upper_bound(service, &requests)?;
    let mut results: Vec<_> = requests
        .iter()
        .map(|&(object_id, selector)| {
            get_object_impl(
                service,
                object_id,
                selector,
                upper_bound.as_ref(),
                &read_mask,
            )
        })
        .collect();
    apply_at_checkpoint_lower_bound(service, &requests, &mut results)?;
    let objects = results
        .into_iter()
        .map(|result| match result {
            Ok(object) => GetObjectResult::new_object(object),
            Err(error) => GetObjectResult::new_error(error.into_status_proto()),
        })
        .collect();
    Ok(BatchGetObjectsResponse::new(objects))
}

fn has_at_checkpoint(requests: &[(Address, ObjectSelector)]) -> bool {
    requests
        .iter()
        .any(|(_, selector)| matches!(selector, ObjectSelector::AtCheckpoint(_)))
}

/// Before the reads: the `at_checkpoint` upper bound, read once per request and only when some
/// item reads at a checkpoint, so that latest and exact-version reads still work on nodes
/// without rpc indexes.
fn load_at_checkpoint_upper_bound(
    service: &RpcService,
    requests: &[(Address, ObjectSelector)],
) -> Result<Option<AtCheckpointUpperBound>, RpcError> {
    has_at_checkpoint(requests)
        .then(|| service.reader.at_checkpoint_upper_bound())
        .transpose()
}

/// After the reads: answer as pruned every `at_checkpoint` item below the lowest available
/// checkpoint, replacing whatever its read returned, since that read may have seen pruned data.
/// See `StateReader::at_checkpoint_lower_bound`.
fn apply_at_checkpoint_lower_bound(
    service: &RpcService,
    requests: &[(Address, ObjectSelector)],
    results: &mut [Result<Object, RpcError>],
) -> Result<(), RpcError> {
    if !has_at_checkpoint(requests) {
        return Ok(());
    }
    let lower_bound = service.reader.at_checkpoint_lower_bound()?;
    for (&(_, selector), result) in requests.iter().zip_debug_eq(results.iter_mut()) {
        if let ObjectSelector::AtCheckpoint(checkpoint) = selector
            && let Err(error) = lower_bound.check(checkpoint)
        {
            *result = Err(error);
        }
    }
    Ok(())
}

/// Read one object. `upper_bound` is set whenever `selector` is
/// [`ObjectSelector::AtCheckpoint`] (see [`load_at_checkpoint_upper_bound`]).
#[tracing::instrument(skip(service))]
fn get_object_impl(
    service: &RpcService,
    object_id: Address,
    selector: ObjectSelector,
    upper_bound: Option<&AtCheckpointUpperBound>,
    read_mask: &FieldMaskTree,
) -> Result<Object, RpcError> {
    let object = match selector {
        ObjectSelector::Latest => service
            .reader
            .inner()
            .get_object(&object_id.into())
            .ok_or_else(|| ObjectNotFoundError::new(object_id))?,
        ObjectSelector::Version(version) => service
            .reader
            .inner()
            .get_object_by_key(&object_id.into(), version.into())
            .ok_or_else(|| ObjectNotFoundError::new_with_version(object_id, version))?,
        ObjectSelector::AtCheckpoint(checkpoint) => {
            let upper_bound = upper_bound.ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    "at_checkpoint upper bound was not loaded",
                )
            })?;
            service
                .reader
                .get_object_at_checkpoint(object_id.into(), checkpoint, upper_bound)?
                .ok_or_else(|| ObjectNotFoundError::new_at_checkpoint(object_id, checkpoint))?
        }
    };

    Ok(service.render_object_to_proto(&object, read_mask, &ObjectSet::default()))
}
