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
use sui_types::base_types::ObjectID;
use sui_types::full_checkpoint_content::ObjectSet;

pub const MAX_BATCH_REQUESTS: usize = 1000;
pub const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::OBJECT;

/// Which state of an object a `GetObjectRequest` reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectSelector {
    /// The latest live version (none of `version`, `max_version` and `at_checkpoint` set).
    Latest,
    /// Exactly `version`. With `at_checkpoint`, only if that version was written at or before
    /// the checkpoint.
    Version {
        version: u64,
        at_checkpoint: Option<u64>,
    },
    /// The newest version at or below `max_version`, unless the object was deleted or wrapped at
    /// that version. With `at_checkpoint`, the newest such version among those written at or
    /// before the checkpoint, so versions written after it never change the answer. Without it,
    /// kv-rpc reads as though `at_checkpoint` were its served height, since versions above that
    /// height may be in its tables before their deletions and wraps are.
    MaxVersion {
        max_version: u64,
        at_checkpoint: Option<u64>,
    },
    /// The object's state at the end of this checkpoint (`at_checkpoint` alone).
    AtCheckpoint(u64),
}

impl ObjectSelector {
    /// The request's `at_checkpoint`: the checkpoint an [`Self::AtCheckpoint`] read is pinned
    /// to, or the checkpoint that bounds a [`Self::Version`] or [`Self::MaxVersion`] read to
    /// versions written at or before it. Either way, the read fails if the checkpoint is outside
    /// the range the server can answer for.
    pub fn at_checkpoint(&self) -> Option<u64> {
        match *self {
            Self::Latest => None,
            Self::Version { at_checkpoint, .. } | Self::MaxVersion { at_checkpoint, .. } => {
                at_checkpoint
            }
            Self::AtCheckpoint(checkpoint) => Some(checkpoint),
        }
    }

    /// Whether the read needs versions that later transactions replaced, which a node without
    /// object version history no longer has: every read with `at_checkpoint`, and every
    /// `max_version` read.
    pub fn needs_object_version_history(&self) -> bool {
        matches!(self, Self::MaxVersion { .. }) || self.at_checkpoint().is_some()
    }

    /// The `NotFound` error for `object_id` when this selector finds no object.
    pub fn not_found(&self, object_id: Address) -> ObjectNotFoundError {
        let error = match *self {
            Self::Latest | Self::AtCheckpoint(_) => ObjectNotFoundError::new(object_id),
            Self::Version { version, .. } => {
                ObjectNotFoundError::new_with_version(object_id, version)
            }
            Self::MaxVersion { max_version, .. } => {
                ObjectNotFoundError::new_with_max_version(object_id, max_version)
            }
        };
        match self.at_checkpoint() {
            Some(checkpoint) => error.at_checkpoint(checkpoint),
            None => error,
        }
    }
}

type ValidationResult = Result<(Vec<(Address, ObjectSelector)>, FieldMaskTree), RpcError>;

/// Validate each request's `object_id` and selector fields, and the read mask shared by all of
/// them; each request's own `read_mask` is ignored. A request that sets both `version` and
/// `max_version` is rejected. `at_checkpoint` combines with either.
pub fn validate_get_object_requests(
    requests: &[GetObjectRequest],
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
        .iter()
        .enumerate()
        .map(|(idx, request)| {
            let object_id = request
                .object_id
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
            let selector = match (request.version, request.max_version, request.at_checkpoint) {
                (Some(_), Some(_), _) => {
                    return Err(FieldViolation::new("max_version")
                        .with_description("version and max_version are mutually exclusive")
                        .with_reason(ErrorReason::FieldInvalid)
                        .nested_at("requests", idx)
                        .into());
                }
                (None, None, None) => ObjectSelector::Latest,
                (None, None, Some(checkpoint)) => ObjectSelector::AtCheckpoint(checkpoint),
                (Some(version), None, at_checkpoint) => ObjectSelector::Version {
                    version,
                    at_checkpoint,
                },
                (None, Some(max_version), at_checkpoint) => ObjectSelector::MaxVersion {
                    max_version,
                    at_checkpoint,
                },
            };
            Ok((object_id, selector))
        })
        .collect::<Result<_, RpcError>>()?;
    Ok((requests, read_mask))
}

#[tracing::instrument(skip(service))]
pub fn get_object(
    service: &RpcService,
    mut request: GetObjectRequest,
) -> Result<GetObjectResponse, RpcError> {
    let read_mask = request.read_mask.take();
    let (requests, read_mask) =
        validate_get_object_requests(std::slice::from_ref(&request), read_mask)?;
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

    let (requests, read_mask) = validate_get_object_requests(&requests, read_mask)?;

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
        .any(|(_, selector)| selector.at_checkpoint().is_some())
}

/// Before the reads, once per request: refuse the whole request if some item needs object
/// version history and this node keeps none, and load the `at_checkpoint` upper bound if some
/// item sets `at_checkpoint`. Other reads still work on nodes without rpc indexes or object
/// version history.
fn load_at_checkpoint_upper_bound(
    service: &RpcService,
    requests: &[(Address, ObjectSelector)],
) -> Result<Option<AtCheckpointUpperBound>, RpcError> {
    if requests
        .iter()
        .any(|(_, selector)| selector.needs_object_version_history())
    {
        service.reader.require_object_version_history()?;
    }
    has_at_checkpoint(requests)
        .then(|| service.reader.at_checkpoint_upper_bound())
        .transpose()
}

/// After the reads: answer as pruned every item whose `at_checkpoint` is below the lower bound
/// (see `StateReader::at_checkpoint_lower_bound`), replacing whatever its read returned, since
/// that read may have seen pruned data.
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
        if let Some(checkpoint) = selector.at_checkpoint()
            && let Err(error) = lower_bound.check(checkpoint)
        {
            *result = Err(error);
        }
    }
    Ok(())
}

/// Read one object. `upper_bound` is set whenever `selector` has an `at_checkpoint` (see
/// [`load_at_checkpoint_upper_bound`]); the reader checks it before any read at the checkpoint,
/// so a checkpoint above it fails the read whatever the object's state.
#[tracing::instrument(skip(service))]
fn get_object_impl(
    service: &RpcService,
    object_id: Address,
    selector: ObjectSelector,
    upper_bound: Option<&AtCheckpointUpperBound>,
    read_mask: &FieldMaskTree,
) -> Result<Object, RpcError> {
    let reader = &service.reader;
    let id = ObjectID::from(object_id);
    let not_found = || RpcError::from(selector.not_found(object_id));
    let upper_bound = || {
        upper_bound.ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                "at_checkpoint upper bound was not loaded",
            )
        })
    };
    let object = match selector {
        ObjectSelector::Latest => reader.inner().get_object(&id),
        ObjectSelector::Version {
            version,
            at_checkpoint,
        } => {
            let version = version.into();
            // A version above the object's version at the end of the checkpoint was written
            // after it.
            let visible = match at_checkpoint {
                None => true,
                Some(checkpoint) => reader
                    .get_object_version_at_checkpoint(id, checkpoint, upper_bound()?)?
                    .is_some_and(|version_at_checkpoint| version <= version_at_checkpoint),
            };
            if visible {
                reader.inner().get_object_by_key(&id, version)
            } else {
                None
            }
        }
        ObjectSelector::MaxVersion {
            max_version,
            at_checkpoint,
        } => {
            let max_version = max_version.into();
            // The newest version at or below both `max_version` and the object's version at the
            // end of the checkpoint is the newest version at or below `max_version` written by
            // then.
            let scan_from = match at_checkpoint {
                None => Some(max_version),
                Some(checkpoint) => reader
                    .get_object_version_at_checkpoint(id, checkpoint, upper_bound()?)?
                    .map(|version_at_checkpoint| version_at_checkpoint.min(max_version)),
            };
            match scan_from {
                Some(version) => reader.inner().get_object_lt_or_eq_version(&id, version)?,
                None => None,
            }
        }
        ObjectSelector::AtCheckpoint(checkpoint) => {
            reader.get_object_at_checkpoint(id, checkpoint, upper_bound()?)?
        }
    }
    .ok_or_else(not_found)?;

    Ok(service.render_object_to_proto(&object, read_mask, &ObjectSet::default()))
}
