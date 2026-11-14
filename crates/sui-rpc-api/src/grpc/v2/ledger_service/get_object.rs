// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use crate::error::ObjectNotFoundError;
use crate::grpc::v2::render::RenderContext;
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
        ..
    }: GetObjectRequest,
) -> Result<GetObjectResponse, RpcError> {
    let (requests, read_mask) =
        validate_get_object_requests(vec![(object_id, version)], read_mask)?;
    let (object_id, version) = requests[0];
    let render_context = RenderContext::empty(service.reader.inner());
    get_object_impl(service, object_id, version, &read_mask, &render_context)
        .map(GetObjectResponse::new)
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
        .map(|req| (req.object_id, req.version))
        .collect();
    let (requests, read_mask) = validate_get_object_requests(requests, read_mask)?;
    let render_context = RenderContext::empty(service.reader.inner());
    let objects = requests
        .into_iter()
        .map(|(object_id, version)| {
            get_object_impl(service, object_id, version, &read_mask, &render_context)
        })
        .map(|result| match result {
            Ok(object) => GetObjectResult::new_object(object),
            Err(error) => GetObjectResult::new_error(error.into_status_proto()),
        })
        .collect();
    Ok(BatchGetObjectsResponse::new(objects))
}

#[tracing::instrument(skip(service, render_context))]
fn get_object_impl(
    service: &RpcService,
    object_id: Address,
    version: Option<u64>,
    read_mask: &FieldMaskTree,
    render_context: &RenderContext,
) -> Result<Object, RpcError> {
    let object = if let Some(version) = version {
        get_object_version(service, object_id, version)?
    } else {
        service
            .reader
            .inner()
            .get_object(&object_id.into())
            .ok_or_else(|| ObjectNotFoundError::new(object_id))?
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
