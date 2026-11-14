// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use crate::error::ObjectNotFoundError;
use crate::grpc::v2::render::RenderContext;
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

pub const MAX_BATCH_REQUESTS: usize = 1000;
pub const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::OBJECT;

/// Which state of an object a `GetObjectRequest` reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    let render_context = RenderContext::empty(service.reader.inner());
    let mut results = [get_object_impl(
        service,
        object_id,
        selector,
        upper_bound.as_ref(),
        &read_mask,
        &render_context,
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
    let render_context = RenderContext::empty(service.reader.inner());
    let mut results: Vec<_> = requests
        .iter()
        .map(|&(object_id, selector)| {
            get_object_impl(
                service,
                object_id,
                selector,
                upper_bound.as_ref(),
                &read_mask,
                &render_context,
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

/// Before the reads, once per request and only when some item reads at a checkpoint, so that
/// latest and exact-version reads still work on nodes without rpc indexes or object version
/// history: refuse the whole request if this node keeps no object version history, else load
/// the `at_checkpoint` upper bound.
fn load_at_checkpoint_upper_bound(
    service: &RpcService,
    requests: &[(Address, ObjectSelector)],
) -> Result<Option<AtCheckpointUpperBound>, RpcError> {
    if !has_at_checkpoint(requests) {
        return Ok(None);
    }
    service.reader.require_object_version_history()?;
    service.reader.at_checkpoint_upper_bound().map(Some)
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
#[tracing::instrument(skip(service, render_context))]
fn get_object_impl(
    service: &RpcService,
    object_id: Address,
    selector: ObjectSelector,
    upper_bound: Option<&AtCheckpointUpperBound>,
    read_mask: &FieldMaskTree,
    render_context: &RenderContext,
) -> Result<Object, RpcError> {
    let object = match selector {
        ObjectSelector::Latest => service
            .reader
            .inner()
            .get_object(&object_id.into())
            .ok_or_else(|| ObjectNotFoundError::new(object_id))?,
        ObjectSelector::Version(version) => get_object_version(service, object_id, version)?,
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

    Ok(service.render_object_to_proto(&object, read_mask, render_context))
}

/// Reads `object_id` at exactly `version`. A node without object version history serves only
/// each object's latest version. A version above it has not been written yet and is `NotFound`,
/// as on any node. Any other version, and any version of an object with no live version, is
/// refused (`Unimplemented`, `NOT_SERVED_HERE`, cause `no_object_history`) even while it is still
/// stored, so that answer does not change as compaction reaches it.
fn get_object_version(
    service: &RpcService,
    object_id: Address,
    version: u64,
) -> Result<sui_types::object::Object, RpcError> {
    let store = service.reader.inner();
    let not_found =
        || -> RpcError { ObjectNotFoundError::new_with_version(object_id, version).into() };
    if let Err(refusal) = service.reader.require_object_version_history() {
        return match store.get_object(&object_id.into()) {
            Some(object) if object.version().value() == version => Ok(object),
            Some(object) if object.version().value() < version => Err(not_found()),
            _ => Err(refusal),
        };
    }
    store
        .get_object_by_key(&object_id.into(), version.into())
        .ok_or_else(not_found)
}

#[cfg(test)]
mod tests {
    use sui_rpc::proto::sui::rpc::v2::get_object_result;
    use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
    use sui_types::object::Owner;

    use super::*;
    use crate::reader::{NotServedHereCause, assert_not_served_here};
    use crate::stub_store::StubStore;

    /// A store holding versions 1 and 2 of one object, and that object's id.
    fn store_with_two_versions(retains_object_version_history: bool) -> (RpcService, Address) {
        let id = ObjectID::new([1; ObjectID::LENGTH]);
        let owner = Owner::AddressOwner(SuiAddress::ZERO);
        let objects = [1, 2]
            .map(|version| {
                sui_types::object::Object::with_id_owner_version_for_testing(
                    id,
                    SequenceNumber::from_u64(version),
                    owner.clone(),
                )
            })
            .to_vec();
        let service = StubStore {
            objects,
            retains_object_version_history,
            ..StubStore::new()
        }
        .into_service();
        (service, id.into())
    }

    fn request(object_id: Address, version: Option<u64>) -> GetObjectRequest {
        let mut request = GetObjectRequest::default();
        request.object_id = Some(object_id.to_string());
        request.version = version;
        request.read_mask = Some(FieldMask::from_str("object_id,version"));
        request
    }

    fn version_of(response: Result<GetObjectResponse, RpcError>) -> u64 {
        response.unwrap().object.unwrap().version.unwrap()
    }

    /// A node without object version history serves an object's latest version, by `version` or
    /// without one, answers `NotFound` for a version not written yet, and refuses every older
    /// version, including one it still stores.
    #[test]
    fn without_history_only_the_latest_version_is_served() {
        let (service, id) = store_with_two_versions(false);

        assert_eq!(version_of(get_object(&service, request(id, None))), 2);
        assert_eq!(version_of(get_object(&service, request(id, Some(2)))), 2);
        assert_not_served_here(
            get_object(&service, request(id, Some(1))).unwrap_err(),
            NotServedHereCause::NoObjectHistory,
        );
        let error = get_object(&service, request(id, Some(3))).unwrap_err();
        assert_eq!(error.into_status_proto().code, tonic::Code::NotFound as i32);
        let missing = Address::new([2; 32]);
        assert_not_served_here(
            get_object(&service, request(missing, Some(1))).unwrap_err(),
            NotServedHereCause::NoObjectHistory,
        );

        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![request(id, Some(1)), request(id, Some(2))];
        batch.read_mask = Some(FieldMask::from_str("object_id,version"));
        let response = batch_get_objects(&service, batch).unwrap();
        let [first, second] = response.objects.as_slice() else {
            panic!("expected two results in {response:?}");
        };
        assert!(
            matches!(
                &first.result,
                Some(get_object_result::Result::Error(status))
                    if status.code == tonic::Code::Unimplemented as i32
            ),
            "{response:?}"
        );
        assert!(
            matches!(
                &second.result,
                Some(get_object_result::Result::Object(object)) if object.version == Some(2)
            ),
            "{response:?}"
        );
    }

    /// A node with object version history serves any stored version and answers `NotFound` for
    /// one it does not store.
    #[test]
    fn with_history_any_stored_version_is_served() {
        let (service, id) = store_with_two_versions(true);

        assert_eq!(version_of(get_object(&service, request(id, Some(1)))), 1);
        let error = get_object(&service, request(id, Some(3))).unwrap_err();
        assert_eq!(error.into_status_proto().code, tonic::Code::NotFound as i32);
    }
}
