// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_kvstore::BigTableClient;
use sui_kvstore::KeyValueStoreReader;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::GetDatatypeRequest;
use sui_rpc::proto::sui::rpc::v2::GetDatatypeResponse;
use sui_rpc::proto::sui::rpc::v2::GetFunctionRequest;
use sui_rpc::proto::sui::rpc::v2::GetFunctionResponse;
use sui_rpc::proto::sui::rpc::v2::GetPackageRequest;
use sui_rpc::proto::sui::rpc::v2::GetPackageResponse;
use sui_rpc::proto::sui::rpc::v2::ListPackageVersionsRequest;
use sui_rpc::proto::sui::rpc::v2::ListPackageVersionsResponse;
use sui_rpc::proto::sui::rpc::v2::get_package_request::Selector;
use sui_rpc::proto::sui::rpc::v2::move_package_service_server::MovePackageService;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_rpc_api::checkpoint_not_yet_indexed_error;
use sui_rpc_api::grpc::v2::move_package_service::AllPackageVersionsQuery;
use sui_rpc_api::grpc::v2::move_package_service::PackageVersionsQuery;
use sui_rpc_api::grpc::v2::move_package_service::get_datatype_response;
use sui_rpc_api::grpc::v2::move_package_service::get_function_response;
use sui_rpc_api::grpc::v2::move_package_service::get_package_response;
use sui_types::base_types::ObjectID;
use sui_types::move_package::MovePackage;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use sui_types::storage::PackagePublishKey;

use crate::KvRpcServer;
use crate::served_height::ServedHeight;

#[tonic::async_trait]
impl MovePackageService for KvRpcServer {
    async fn get_package(
        &self,
        request: tonic::Request<GetPackageRequest>,
    ) -> Result<tonic::Response<GetPackageResponse>, tonic::Status> {
        get_package(
            self.client.clone(),
            self.served_height()?,
            request.into_inner(),
        )
        .await
        .map(tonic::Response::new)
        .map_err(Into::into)
    }

    async fn get_datatype(
        &self,
        request: tonic::Request<GetDatatypeRequest>,
    ) -> Result<tonic::Response<GetDatatypeResponse>, tonic::Status> {
        get_datatype(self.client.clone(), request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_function(
        &self,
        request: tonic::Request<GetFunctionRequest>,
    ) -> Result<tonic::Response<GetFunctionResponse>, tonic::Status> {
        get_function(self.client.clone(), request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn list_package_versions(
        &self,
        request: tonic::Request<ListPackageVersionsRequest>,
    ) -> Result<tonic::Response<ListPackageVersionsResponse>, tonic::Status> {
        list_package_versions(
            self.client.clone(),
            self.served_height()?,
            request.into_inner(),
        )
        .await
        .map(tonic::Response::new)
        .map_err(Into::into)
    }
}

/// Serve a package. `Version` and `AtCheckpoint` lookups are answered as of `served_height`;
/// the bare-id path reads the package object by its storage ID, whose row records no
/// checkpoint to compare against.
async fn get_package(
    mut client: BigTableClient,
    served_height: ServedHeight,
    request: GetPackageRequest,
) -> Result<GetPackageResponse, RpcError> {
    let package_id_str = request.package_id.as_ref().ok_or_else(|| {
        FieldViolation::new("package_id")
            .with_description("missing package_id")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let package_id = parse_package_id(package_id_str)?;

    let selector = match request.selector {
        None => {
            let package = load_package(client, package_id).await?;
            return get_package_response(&package);
        }
        Some(selector) => selector,
    };

    let data = match selector {
        Selector::Version(version) => {
            let original_id = resolve_original_package_id(client.clone(), package_id).await?;
            client
                .get_packages_by_version(&[(original_id, version)])
                .await
                .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
                .pop()
                .filter(|data| data.cp_sequence_number <= served_height.checkpoint)
        }
        Selector::AtCheckpoint(at_checkpoint) => {
            // The response does not echo the checkpoint it resolved at, so a bound above the
            // served height would let a caller persist a wrong answer; fail closed instead.
            if at_checkpoint > served_height.checkpoint {
                return Err(checkpoint_not_yet_indexed_error(
                    at_checkpoint,
                    served_height.checkpoint,
                ));
            }
            let original_id = resolve_original_package_id(client.clone(), package_id).await?;
            client
                .get_package_latest(original_id, at_checkpoint)
                .await
                .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        }
        _ => {
            return Err(FieldViolation::new("selector")
                .with_description("unknown selector variant")
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }
    }
    .ok_or_else(RpcError::not_found)?;

    let storage_id = ObjectID::from_bytes(&data.package_id).map_err(|e| {
        RpcError::new(
            tonic::Code::Internal,
            format!("invalid stored package id: {e}"),
        )
    })?;

    let object = client
        .get_objects(&[ObjectKey(storage_id, data.package_version.into())])
        .await
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        .pop()
        .ok_or_else(RpcError::not_found)?;
    let package = into_package(object)?;

    get_package_response(&package)
}

async fn get_datatype(
    client: BigTableClient,
    request: GetDatatypeRequest,
) -> Result<GetDatatypeResponse, RpcError> {
    let package_id_str = request.package_id.as_ref().ok_or_else(|| {
        FieldViolation::new("package_id")
            .with_description("missing package_id")
            .with_reason(ErrorReason::FieldMissing)
    })?;

    let module_name = request.module_name.as_ref().ok_or_else(|| {
        FieldViolation::new("module_name")
            .with_description("missing module_name")
            .with_reason(ErrorReason::FieldMissing)
    })?;

    let datatype_name = request.name.as_ref().ok_or_else(|| {
        FieldViolation::new("name")
            .with_description("missing name")
            .with_reason(ErrorReason::FieldMissing)
    })?;

    let package = load_package(client, parse_package_id(package_id_str)?).await?;
    get_datatype_response(&package, module_name, datatype_name)
}

async fn get_function(
    client: BigTableClient,
    request: GetFunctionRequest,
) -> Result<GetFunctionResponse, RpcError> {
    let package_id_str = request.package_id.as_ref().ok_or_else(|| {
        FieldViolation::new("package_id")
            .with_description("missing package_id")
            .with_reason(ErrorReason::FieldMissing)
    })?;

    let module_name = request.module_name.as_ref().ok_or_else(|| {
        FieldViolation::new("module_name")
            .with_description("missing module_name")
            .with_reason(ErrorReason::FieldMissing)
    })?;

    let function_name = request.name.as_ref().ok_or_else(|| {
        FieldViolation::new("name")
            .with_description("missing name")
            .with_reason(ErrorReason::FieldMissing)
    })?;

    let package = load_package(client, parse_package_id(package_id_str)?).await?;
    get_function_response(&package, module_name, function_name)
}

/// List the versions of a package published at or below `served_height`, or at or below the
/// request's `at_checkpoint` when set, or with `package_id` unset, the versions of every package.
async fn list_package_versions(
    mut client: BigTableClient,
    served_height: ServedHeight,
    request: ListPackageVersionsRequest,
) -> Result<ListPackageVersionsResponse, RpcError> {
    let Some(package_id_str) = request.package_id.as_deref() else {
        return list_all_package_versions(client, served_height, &request).await;
    };
    let original_package_id =
        resolve_original_package_id(client.clone(), parse_package_id(package_id_str)?).await?;

    let query = PackageVersionsQuery::new(&request, original_package_id)?;

    // BigTable holds full history, so there is no lower bound to check.
    if let Some(at_checkpoint) = request.at_checkpoint
        && at_checkpoint > served_height.checkpoint
    {
        return Err(checkpoint_not_yet_indexed_error(
            at_checkpoint,
            served_height.checkpoint,
        ));
    }
    let cp_bound = request.at_checkpoint.unwrap_or(served_height.checkpoint);

    // The reader's version bounds are exclusive, and no version reaches `u64::MAX`.
    let after_version = query.versions.start.checked_sub(1);
    let before_version = (query.versions.end != u64::MAX).then_some(query.versions.end);

    // An ascending scan ends at the first version published after `cp_bound`; a descending scan
    // skips the versions published after it.
    let versions: Vec<_> = client
        .get_package_versions(
            original_package_id,
            cp_bound,
            after_version,
            before_version,
            query.read_limit(),
            query.descending,
        )
        .await
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        .into_iter()
        .map(|pkg| {
            let storage_id = ObjectID::from_bytes(&pkg.package_id)
                .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
            Ok(query.listed_version(
                storage_id,
                pkg.package_version,
                Some(pkg.cp_sequence_number),
            ))
        })
        .collect::<Result<_, RpcError>>()?;

    // Every package has a first version, so a first page over every version at or below the
    // served height that finds nothing means the package is not published yet as of that
    // height. A package published at or below the served height but outside the request's
    // `at_checkpoint` or version bounds lists no versions instead.
    if query.is_first_page() && versions.is_empty() {
        let bounded = cp_bound < served_height.checkpoint || query.is_version_bounded();
        let published = bounded
            && !client
                .get_package_versions(
                    original_package_id,
                    served_height.checkpoint,
                    None,
                    None,
                    1,
                    false,
                )
                .await
                .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
                .is_empty();
        if !published {
            return Err(RpcError::not_found());
        }
    }

    Ok(query.response(versions))
}

/// List the versions of every package published from the request's `start_checkpoint` through
/// its `at_checkpoint`, or through `served_height` when that is unset, in publish order.
async fn list_all_package_versions(
    mut client: BigTableClient,
    served_height: ServedHeight,
    request: &ListPackageVersionsRequest,
) -> Result<ListPackageVersionsResponse, RpcError> {
    // BigTable holds full history, so a listing can start anywhere from genesis.
    let query = AllPackageVersionsQuery::new(
        request,
        0,
        request.at_checkpoint.unwrap_or(served_height.checkpoint),
    )?;
    if let Some(at_checkpoint) = request.at_checkpoint
        && at_checkpoint > served_height.checkpoint
    {
        return Err(checkpoint_not_yet_indexed_error(
            at_checkpoint,
            served_height.checkpoint,
        ));
    }

    let versions = client
        .get_packages_by_checkpoint_range(query.lower, query.upper, query.limit(), query.descending)
        .await
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        .into_iter()
        .map(|package| {
            let storage_id = ObjectID::from_bytes(&package.package_id)
                .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
            let original_id = ObjectID::from_bytes(&package.original_id)
                .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
            let key = PackagePublishKey {
                checkpoint: package.cp_sequence_number,
                original_id,
                version: package.package_version,
            };
            Ok(query.listed_version(key, storage_id))
        })
        .collect::<Result<_, RpcError>>()?;

    Ok(ListPackageVersionsResponse::new(versions, None))
}

/// Load a package by its storage ID: a storage ID identifies exactly one immutable package
/// version, so the latest object at that ID is the package itself.
async fn load_package(
    mut client: BigTableClient,
    package_id: ObjectID,
) -> Result<MovePackage, RpcError> {
    let object = client
        .get_latest_object(&package_id)
        .await
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        .ok_or_else(RpcError::not_found)?;

    into_package(object)
}

fn into_package(object: Object) -> Result<MovePackage, RpcError> {
    object
        .into_inner()
        .data
        .try_into_package()
        .ok_or_else(|| RpcError::new(tonic::Code::InvalidArgument, "object is not a package"))
}

/// Resolve the original (first-version) package ID for a storage ID. `packages_by_id` answers
/// with a 32-byte point lookup; on a miss the object itself is read so the error matches what a
/// full node returns (`NotFound` vs "object is not a package"), and so a package whose
/// `packages_by_id` row is not yet written is still served.
async fn resolve_original_package_id(
    mut client: BigTableClient,
    package_id: ObjectID,
) -> Result<ObjectID, RpcError> {
    let pairs = client
        .get_package_original_ids(&[package_id])
        .await
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
    if let Some((_, original_id)) = pairs
        .iter()
        .find(|(storage_id, _)| *storage_id == package_id)
    {
        return Ok(*original_id);
    }

    load_package(client, package_id)
        .await
        .map(|package| package.original_package_id())
}

fn parse_package_id(package_id_str: &str) -> Result<ObjectID, RpcError> {
    package_id_str.parse::<ObjectID>().map_err(|e| {
        FieldViolation::new("package_id")
            .with_description(format!("invalid package_id: {}", e))
            .with_reason(ErrorReason::FieldInvalid)
            .into()
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bytes::Bytes;
    use sui_kvstore::tables;
    use sui_kvstore::testing::MockBigtableServer;
    use sui_rpc::proto::sui::rpc::v2::Ordering;
    use sui_rpc::proto::sui::rpc::v2::QueryOptions;
    use sui_rpc_api::grpc::v2::move_package_service::PackageVersionCursor;
    use sui_types::base_types::{ObjectID, SequenceNumber};
    use sui_types::digests::TransactionDigest;
    use sui_types::move_package::MovePackage;
    use sui_types::object::{Data, Object};
    use sui_types::storage::ObjectKey;

    use super::*;
    use crate::v2::test_utils::UNBOUNDED_SERVED_HEIGHT;

    #[tokio::test]
    async fn list_package_versions_falls_back_to_object_when_packages_by_id_missing() {
        let mock = MockBigtableServer::new();
        let (addr, _handle) = mock.start().await.unwrap();
        let client = BigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .unwrap();

        let original_id = ObjectID::random();
        let pkg = MovePackage::new(
            original_id,
            SequenceNumber::from(1),
            BTreeMap::new(),
            100_000,
            vec![],
            BTreeMap::new(),
        )
        .unwrap();
        let obj = Object::new_package_from_data(Data::Package(pkg), TransactionDigest::ZERO);

        // Insert package object into `objects` table
        let obj_key = tables::objects::encode_key(&ObjectKey(original_id, SequenceNumber::from(1)));
        let obj_cells = tables::objects::encode(&obj).unwrap();
        mock.insert_row(tables::objects::NAME, obj_key, obj_cells)
            .await;

        // Insert 3 versions into `packages` table: v1 at cp 10, v2 at cp 20, v3 at cp 30
        for (v, cp) in [(1, 10), (2, 20), (3, 30)] {
            let row_key = tables::packages::encode_key(original_id.as_ref(), v);
            let cells = tables::packages::encode(cp, original_id.as_ref(), false);
            mock.insert_row(tables::packages::NAME, row_key, cells)
                .await;
        }

        let mut req = ListPackageVersionsRequest::default();
        req.package_id = Some(original_id.to_string());
        req.page_size = Some(10);
        let resp = list_package_versions(client.clone(), UNBOUNDED_SERVED_HEIGHT, req)
            .await
            .unwrap();
        assert_eq!(resp.versions.len(), 3);
        assert_eq!(resp.versions[0].version, Some(1));
        assert_eq!(resp.versions[1].version, Some(2));
        assert_eq!(resp.versions[2].version, Some(3));
        assert!(resp.next_page_token.is_none());
    }

    #[tokio::test]
    async fn list_package_versions_resolves_original_id_without_reading_objects() {
        let mock = MockBigtableServer::new();
        let (addr, _handle) = mock.start().await.unwrap();
        let client = BigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .unwrap();

        let storage_id = ObjectID::random();
        let original_id = ObjectID::random();

        // Insert ONLY the `packages_by_id` mapping and the `packages` version rows; no
        // `objects` row exists, so resolution must come from the mapping alone.
        mock.insert_row(
            tables::packages_by_id::NAME,
            tables::packages_by_id::encode_key(storage_id.as_ref()),
            tables::packages_by_id::encode(original_id.as_ref()),
        )
        .await;
        for (v, cp) in [(1, 10), (2, 20), (3, 30)] {
            let row_key = tables::packages::encode_key(original_id.as_ref(), v);
            let cells = tables::packages::encode(cp, original_id.as_ref(), false);
            mock.insert_row(tables::packages::NAME, row_key, cells)
                .await;
        }

        let mut req = ListPackageVersionsRequest::default();
        req.package_id = Some(storage_id.to_string());
        req.page_size = Some(10);
        let resp = list_package_versions(client.clone(), UNBOUNDED_SERVED_HEIGHT, req)
            .await
            .unwrap();
        assert_eq!(resp.versions.len(), 3);
        assert_eq!(resp.versions[0].version, Some(1));
        assert_eq!(resp.versions[1].version, Some(2));
        assert_eq!(resp.versions[2].version, Some(3));
        assert!(resp.next_page_token.is_none());

        assert!(
            mock.read_rows_calls()
                .await
                .into_iter()
                .all(|call| call.table != tables::objects::NAME),
            "no objects ReadRows should be issued when packages_by_id has the row"
        );
    }

    #[tokio::test]
    async fn list_package_versions_unknown_package_is_not_found() {
        let mock = MockBigtableServer::new();
        let (addr, _handle) = mock.start().await.unwrap();
        let client = BigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .unwrap();

        let mut req = ListPackageVersionsRequest::default();
        req.package_id = Some(ObjectID::random().to_string());
        req.page_size = Some(10);
        let err = list_package_versions(client.clone(), UNBOUNDED_SERVED_HEIGHT, req)
            .await
            .unwrap_err();
        let status: tonic::Status = err.into();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    struct LineageFixture {
        client: BigTableClient,
        original_id: ObjectID,
        upgraded_id: ObjectID,
        plain_object_id: ObjectID,
        _server: tokio::task::JoinHandle<()>,
    }

    /// The served height the lineage fixture's tests read at: above both publishes.
    const FIXTURE_SERVED_HEIGHT: ServedHeight = ServedHeight {
        checkpoint: 25,
        epoch: 0,
    };

    /// Seed the mock with a two-version package lineage (v1 at `original_id`
    /// published at checkpoint 5, v2 at `upgraded_id` published at checkpoint
    /// 20) plus one non-package object.
    async fn setup_lineage_fixture() -> LineageFixture {
        use move_binary_format::file_format::empty_module;
        use move_core_types::account_address::AccountAddress;
        use sui_protocol_config::ProtocolConfig;
        use sui_types::base_types::SuiAddress;

        let mock = MockBigtableServer::new();
        let (addr, server) = mock.start().await.expect("start mock BigTable");
        let client = BigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");

        let original_id = ObjectID::from_single_byte(0xAA);
        let upgraded_id = ObjectID::from_single_byte(0xBB);
        let plain_object_id = ObjectID::from_single_byte(0xCC);

        // A structurally valid module whose self-address becomes the
        // package's id, standing in for real compiled code.
        let mut module = empty_module();
        module.address_identifiers[0] = AccountAddress::from(original_id);
        let config = ProtocolConfig::get_for_max_version_UNSAFE();

        let v1 = Object::new_package(
            &[module.clone()],
            TransactionDigest::genesis_marker(),
            &config,
            [],
        )
        .expect("v1 package");
        let Data::Package(v1_package) = &v1.data else {
            unreachable!("new_package builds a package");
        };
        let v2 = Object::new_package_from_data(
            Data::Package(
                v1_package
                    .new_upgraded(upgraded_id, &[module], &config, [])
                    .expect("v2 package"),
            ),
            TransactionDigest::genesis_marker(),
        );
        let plain = Object::with_id_owner_for_testing(plain_object_id, SuiAddress::ZERO);

        for object in [&v1, &v2, &plain] {
            let key = tables::objects::encode_key(&ObjectKey(object.id(), object.version()));
            mock.insert_row(
                tables::objects::NAME,
                Bytes::from(key),
                tables::objects::encode(object).expect("encode object"),
            )
            .await;
        }
        for (package_id, version, checkpoint) in [(original_id, 1u64, 5u64), (upgraded_id, 2, 20)] {
            mock.insert_row(
                tables::packages_by_id::NAME,
                Bytes::from(tables::packages_by_id::encode_key(package_id.as_ref())),
                tables::packages_by_id::encode(original_id.as_ref()),
            )
            .await;
            mock.insert_row(
                tables::packages::NAME,
                Bytes::from(tables::packages::encode_key(original_id.as_ref(), version)),
                tables::packages::encode(checkpoint, package_id.as_ref(), false),
            )
            .await;
        }

        LineageFixture {
            client,
            original_id,
            upgraded_id,
            plain_object_id,
            _server: server,
        }
    }

    fn get_package_req(package_id: ObjectID) -> GetPackageRequest {
        let mut request = GetPackageRequest::default();
        request.package_id = Some(package_id.to_canonical_string(true));
        request
    }

    async fn fetch_pkg(
        fixture: &LineageFixture,
        request: GetPackageRequest,
    ) -> sui_rpc::proto::sui::rpc::v2::Package {
        get_package(fixture.client.clone(), FIXTURE_SERVED_HEIGHT, request)
            .await
            .expect("get_package succeeds")
            .package
            .expect("response carries a package")
    }

    async fn fetch_pkg_err(fixture: &LineageFixture, request: GetPackageRequest) -> tonic::Status {
        get_package(fixture.client.clone(), FIXTURE_SERVED_HEIGHT, request)
            .await
            .expect_err("get_package fails")
            .into()
    }

    #[tokio::test]
    async fn bare_id_is_an_exact_storage_lookup() {
        let fixture = setup_lineage_fixture().await;

        let package = fetch_pkg(&fixture, get_package_req(fixture.original_id)).await;
        assert_eq!(
            package.storage_id,
            Some(fixture.original_id.to_canonical_string(true)),
        );
        assert_eq!(
            package.original_id,
            Some(fixture.original_id.to_canonical_string(true)),
        );
        assert_eq!(package.version, Some(1));

        let package = fetch_pkg(&fixture, get_package_req(fixture.upgraded_id)).await;
        assert_eq!(
            package.storage_id,
            Some(fixture.upgraded_id.to_canonical_string(true)),
        );
        assert_eq!(
            package.original_id,
            Some(fixture.original_id.to_canonical_string(true)),
        );
        assert_eq!(package.version, Some(2));
    }

    #[tokio::test]
    async fn bounded_lookups_resolve_from_any_lineage_member() {
        let fixture = setup_lineage_fixture().await;

        // Exact version through the upgraded id resolves back to v1.
        let mut req = get_package_req(fixture.upgraded_id);
        req.selector = Some(Selector::Version(1));
        let package = fetch_pkg(&fixture, req).await;
        assert_eq!(
            package.storage_id,
            Some(fixture.original_id.to_canonical_string(true)),
        );
        assert_eq!(package.version, Some(1));

        // Exact version through the original id resolves forward to v2.
        let mut req = get_package_req(fixture.original_id);
        req.selector = Some(Selector::Version(2));
        let package = fetch_pkg(&fixture, req).await;
        assert_eq!(
            package.storage_id,
            Some(fixture.upgraded_id.to_canonical_string(true)),
        );
        assert_eq!(package.version, Some(2));

        // A checkpoint bound between the two publishes resolves v1.
        let mut req = get_package_req(fixture.upgraded_id);
        req.selector = Some(Selector::AtCheckpoint(19));
        let package = fetch_pkg(&fixture, req).await;
        assert_eq!(
            package.storage_id,
            Some(fixture.original_id.to_canonical_string(true)),
        );
        assert_eq!(package.version, Some(1));

        // A checkpoint bound equal to the served height resolves the latest version.
        let mut req = get_package_req(fixture.original_id);
        req.selector = Some(Selector::AtCheckpoint(25));
        let package = fetch_pkg(&fixture, req).await;
        assert_eq!(
            package.storage_id,
            Some(fixture.upgraded_id.to_canonical_string(true)),
        );
        assert_eq!(package.version, Some(2));
    }

    #[tokio::test]
    async fn at_checkpoint_above_served_height_is_rejected() {
        let fixture = setup_lineage_fixture().await;

        for at_checkpoint in [26u64, u64::MAX] {
            let mut req = get_package_req(fixture.original_id);
            req.selector = Some(Selector::AtCheckpoint(at_checkpoint));
            let status = fetch_pkg_err(&fixture, req).await;
            assert_eq!(status.code(), tonic::Code::NotFound, "{at_checkpoint}");
            assert!(
                status
                    .message()
                    .contains("is not yet indexed; highest indexed checkpoint is 25"),
                "unexpected message for {at_checkpoint}: {}",
                status.message()
            );
        }
    }

    #[tokio::test]
    async fn version_published_above_served_height_is_not_found() {
        let fixture = setup_lineage_fixture().await;

        // v2 is published at checkpoint 20.
        for (served_checkpoint, found) in [(19, false), (20, true)] {
            let mut req = get_package_req(fixture.original_id);
            req.selector = Some(Selector::Version(2));
            let served_height = ServedHeight {
                checkpoint: served_checkpoint,
                epoch: 0,
            };
            let result = get_package(fixture.client.clone(), served_height, req).await;
            match result {
                Ok(response) => {
                    assert!(found, "served height {served_checkpoint} served v2");
                    assert_eq!(response.package.and_then(|p| p.version), Some(2));
                }
                Err(err) => {
                    assert!(!found, "served height {served_checkpoint} hid v2");
                    let status: tonic::Status = err.into();
                    assert_eq!(status.code(), tonic::Code::NotFound);
                }
            }
        }
    }

    #[tokio::test]
    async fn missing_packages_are_not_found() {
        let fixture = setup_lineage_fixture().await;
        let unknown = ObjectID::from_single_byte(0xDD);

        let unknown_versioned = {
            let mut req = get_package_req(unknown);
            req.selector = Some(Selector::Version(1));
            req
        };
        let missing_version = {
            let mut req = get_package_req(fixture.original_id);
            req.selector = Some(Selector::Version(3));
            req
        };
        let before_first_publish = {
            let mut req = get_package_req(fixture.original_id);
            req.selector = Some(Selector::AtCheckpoint(4));
            req
        };
        for (label, req) in [
            ("unknown bare id", get_package_req(unknown)),
            ("unknown versioned package", unknown_versioned),
            ("missing version", missing_version),
            ("checkpoint before first publish", before_first_publish),
        ] {
            let status = fetch_pkg_err(&fixture, req).await;
            assert_eq!(status.code(), tonic::Code::NotFound, "{label}");
        }
    }

    #[tokio::test]
    async fn non_package_object_is_rejected() {
        let fixture = setup_lineage_fixture().await;

        let status = fetch_pkg_err(&fixture, get_package_req(fixture.plain_object_id)).await;
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert!(status.message().contains("not a package"));
    }

    #[tokio::test]
    async fn list_package_versions_stops_at_served_height() {
        let fixture = setup_lineage_fixture().await;

        // v2 is published at checkpoint 20: listed only once the served height reaches it.
        for (served_checkpoint, expected) in [(19, vec![1]), (20, vec![1, 2])] {
            let mut req = ListPackageVersionsRequest::default();
            req.package_id = Some(fixture.upgraded_id.to_string());
            let served_height = ServedHeight {
                checkpoint: served_checkpoint,
                epoch: 0,
            };
            let versions: Vec<u64> =
                list_package_versions(fixture.client.clone(), served_height, req)
                    .await
                    .expect("list_package_versions succeeds")
                    .versions
                    .into_iter()
                    .map(|version| version.version.expect("version is set"))
                    .collect();
            assert_eq!(versions, expected, "served height {served_checkpoint}");
        }

        // v1 is published at checkpoint 5: below it the package does not exist yet.
        let mut req = ListPackageVersionsRequest::default();
        req.package_id = Some(fixture.upgraded_id.to_string());
        let served_height = ServedHeight {
            checkpoint: 4,
            epoch: 0,
        };
        let status: tonic::Status =
            list_package_versions(fixture.client.clone(), served_height, req)
                .await
                .expect_err("package published above the served height")
                .into();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    fn list_req(package_id: ObjectID, at_checkpoint: Option<u64>) -> ListPackageVersionsRequest {
        let mut req = ListPackageVersionsRequest::default();
        req.package_id = Some(package_id.to_string());
        req.at_checkpoint = at_checkpoint;
        req
    }

    /// `(version, checkpoint)` of each listed version.
    fn listed(response: &ListPackageVersionsResponse) -> Vec<(u64, Option<u64>)> {
        response
            .versions
            .iter()
            .map(|v| (v.version.expect("version is set"), v.checkpoint))
            .collect()
    }

    #[tokio::test]
    async fn list_package_versions_at_checkpoint_bounds_the_lineage() {
        let fixture = setup_lineage_fixture().await;

        // v1 is published at checkpoint 5 and v2 at 20; the served height is 25.
        for (at_checkpoint, expected) in [
            (None, vec![(1, Some(5)), (2, Some(20))]),
            (Some(4), vec![]),
            (Some(5), vec![(1, Some(5))]),
            (Some(19), vec![(1, Some(5))]),
            (Some(20), vec![(1, Some(5)), (2, Some(20))]),
            (Some(25), vec![(1, Some(5)), (2, Some(20))]),
        ] {
            let response = list_package_versions(
                fixture.client.clone(),
                FIXTURE_SERVED_HEIGHT,
                list_req(fixture.upgraded_id, at_checkpoint),
            )
            .await
            .expect("list_package_versions succeeds");
            assert_eq!(
                listed(&response),
                expected,
                "at_checkpoint {at_checkpoint:?}"
            );
            assert!(response.next_page_token.is_none());
        }
    }

    #[tokio::test]
    async fn list_package_versions_at_checkpoint_above_served_height_is_rejected() {
        let fixture = setup_lineage_fixture().await;

        for at_checkpoint in [26u64, u64::MAX] {
            let status: tonic::Status = list_package_versions(
                fixture.client.clone(),
                FIXTURE_SERVED_HEIGHT,
                list_req(fixture.original_id, Some(at_checkpoint)),
            )
            .await
            .expect_err("bound above the served height")
            .into();
            assert_eq!(status.code(), tonic::Code::NotFound, "{at_checkpoint}");
            assert_eq!(
                status.message(),
                format!(
                    "requested checkpoint {at_checkpoint} is not yet indexed; highest indexed \
                     checkpoint is 25"
                ),
            );
        }
    }

    #[tokio::test]
    async fn list_package_versions_at_checkpoint_before_first_publish() {
        let fixture = setup_lineage_fixture().await;

        // Published at or below the served height: an empty list.
        let response = list_package_versions(
            fixture.client.clone(),
            FIXTURE_SERVED_HEIGHT,
            list_req(fixture.original_id, Some(4)),
        )
        .await
        .expect("package is published as of the served height");
        assert!(response.versions.is_empty());
        assert!(response.next_page_token.is_none());

        // Not yet published as of the served height: not found, as without a bound.
        let served_height = ServedHeight {
            checkpoint: 4,
            epoch: 0,
        };
        let status: tonic::Status = list_package_versions(
            fixture.client.clone(),
            served_height,
            list_req(fixture.original_id, Some(3)),
        )
        .await
        .expect_err("package published above the served height")
        .into();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn list_package_versions_paginates_within_the_bound() {
        let fixture = setup_lineage_fixture().await;

        // Under a bound both versions satisfy, a one-version page points at v2.
        let mut req = list_req(fixture.original_id, Some(20));
        req.page_size = Some(1);
        let first = list_package_versions(fixture.client.clone(), FIXTURE_SERVED_HEIGHT, req)
            .await
            .expect("first page");
        assert_eq!(listed(&first), vec![(1, Some(5))]);
        let token = first.next_page_token.expect("v2 is within the bound");

        let mut req = list_req(fixture.original_id, Some(20));
        req.page_size = Some(1);
        req.page_token = Some(token);
        let second = list_package_versions(fixture.client.clone(), FIXTURE_SERVED_HEIGHT, req)
            .await
            .expect("second page");
        assert_eq!(listed(&second), vec![(2, Some(20))]);
        assert!(second.next_page_token.is_none());

        // A bound that cuts v2 ends the listing after v1, with no page token.
        let mut req = list_req(fixture.original_id, Some(19));
        req.page_size = Some(1);
        let only = list_package_versions(fixture.client.clone(), FIXTURE_SERVED_HEIGHT, req)
            .await
            .expect("bounded page");
        assert_eq!(listed(&only), vec![(1, Some(5))]);
        assert!(only.next_page_token.is_none());
    }

    struct VersionsFixture {
        client: BigTableClient,
        original_id: ObjectID,
        _server: tokio::task::JoinHandle<()>,
    }

    /// The served height the versions fixture's tests read at: above every publish.
    const VERSIONS_FIXTURE_SERVED_HEIGHT: ServedHeight = ServedHeight {
        checkpoint: 60,
        epoch: 0,
    };

    /// Seed the mock with a five-version lineage, resolvable through `packages_by_id`, whose
    /// version `n` is published at checkpoint `10 * n`.
    async fn setup_versions_fixture() -> VersionsFixture {
        let mock = MockBigtableServer::new();
        let (addr, server) = mock.start().await.expect("start mock BigTable");
        let client = BigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");

        let original_id = ObjectID::from_single_byte(0xAA);
        mock.insert_row(
            tables::packages_by_id::NAME,
            tables::packages_by_id::encode_key(original_id.as_ref()),
            tables::packages_by_id::encode(original_id.as_ref()),
        )
        .await;
        for version in 1..=5u64 {
            let storage_id = ObjectID::from_single_byte(version as u8);
            mock.insert_row(
                tables::packages::NAME,
                tables::packages::encode_key(original_id.as_ref(), version),
                tables::packages::encode(10 * version, storage_id.as_ref(), false),
            )
            .await;
        }

        VersionsFixture {
            client,
            original_id,
            _server: server,
        }
    }

    fn query_options(
        limit: Option<u32>,
        ordering: Ordering,
        after: Option<&Bytes>,
        before: Option<&Bytes>,
    ) -> QueryOptions {
        let mut options = QueryOptions::default();
        options.limit = limit;
        options.ordering = Some(ordering as i32);
        options.after = after.cloned();
        options.before = before.cloned();
        options
    }

    fn cursor_req(package_id: ObjectID, options: QueryOptions) -> ListPackageVersionsRequest {
        let mut req = ListPackageVersionsRequest::default();
        req.package_id = Some(package_id.to_string());
        req.options = Some(options);
        req
    }

    async fn list_versions_fixture(
        fixture: &VersionsFixture,
        req: ListPackageVersionsRequest,
    ) -> Result<ListPackageVersionsResponse, tonic::Status> {
        list_package_versions(fixture.client.clone(), VERSIONS_FIXTURE_SERVED_HEIGHT, req)
            .await
            .map_err(Into::into)
    }

    /// The versions a response lists, in order, checking that each carries a cursor.
    fn listed_versions(response: &ListPackageVersionsResponse) -> Vec<u64> {
        response
            .versions
            .iter()
            .map(|v| {
                assert!(v.cursor.is_some(), "version {:?} has no cursor", v.version);
                v.version.expect("version is set")
            })
            .collect()
    }

    /// The cursor of each version of the fixture's lineage, by version.
    async fn fixture_cursors(fixture: &VersionsFixture) -> BTreeMap<u64, Bytes> {
        let response = list_versions_fixture(
            fixture,
            cursor_req(
                fixture.original_id,
                query_options(None, Ordering::Ascending, None, None),
            ),
        )
        .await
        .expect("full listing");
        assert_eq!(listed_versions(&response), vec![1, 2, 3, 4, 5]);
        response
            .versions
            .into_iter()
            .map(|v| (v.version.unwrap(), v.cursor.unwrap()))
            .collect()
    }

    #[tokio::test]
    async fn list_package_versions_pages_backward_then_forward_from_cursors() {
        let fixture = setup_versions_fixture().await;
        let list =
            |options| list_versions_fixture(&fixture, cursor_req(fixture.original_id, options));
        let last_cursor = |response: &ListPackageVersionsResponse| {
            response.versions.last().and_then(|v| v.cursor.clone())
        };

        // Backward from the end, each page before the lowest version received so far, until a
        // short page reaches the start of the lineage.
        let first = list(query_options(Some(2), Ordering::Descending, None, None))
            .await
            .unwrap();
        assert_eq!(listed_versions(&first), vec![5, 4]);
        assert!(first.next_page_token.is_none());
        let second = list(query_options(
            Some(2),
            Ordering::Descending,
            None,
            last_cursor(&first).as_ref(),
        ))
        .await
        .unwrap();
        assert_eq!(listed_versions(&second), vec![3, 2]);
        let third = list(query_options(
            Some(2),
            Ordering::Descending,
            None,
            last_cursor(&second).as_ref(),
        ))
        .await
        .unwrap();
        assert_eq!(listed_versions(&third), vec![1]);

        // A cursor from a descending page bounds an ascending one: forward from version 2 until
        // a short page reaches the end of the lineage.
        let forward = list(query_options(
            Some(2),
            Ordering::Ascending,
            last_cursor(&second).as_ref(),
            None,
        ))
        .await
        .unwrap();
        assert_eq!(listed_versions(&forward), vec![3, 4]);
        let rest = list(query_options(
            Some(2),
            Ordering::Ascending,
            last_cursor(&forward).as_ref(),
            None,
        ))
        .await
        .unwrap();
        assert_eq!(listed_versions(&rest), vec![5]);
        assert!(rest.next_page_token.is_none());
    }

    #[tokio::test]
    async fn list_package_versions_lists_between_two_cursors() {
        let fixture = setup_versions_fixture().await;
        let cursors = fixture_cursors(&fixture).await;

        // A limited page takes the versions nearest its start: the lowest ascending, the
        // highest descending.
        for (after, before, limit, ascending, descending) in [
            (1, 5, None, vec![2, 3, 4], vec![4, 3, 2]),
            (1, 5, Some(2), vec![2, 3], vec![4, 3]),
            (2, 4, None, vec![3], vec![3]),
            (3, 3, None, vec![], vec![]),
            (4, 2, None, vec![], vec![]),
        ] {
            for (ordering, expected) in [
                (Ordering::Ascending, ascending),
                (Ordering::Descending, descending),
            ] {
                let response = list_versions_fixture(
                    &fixture,
                    cursor_req(
                        fixture.original_id,
                        query_options(
                            limit,
                            ordering,
                            Some(&cursors[&after]),
                            Some(&cursors[&before]),
                        ),
                    ),
                )
                .await
                .unwrap();
                assert_eq!(
                    listed_versions(&response),
                    expected,
                    "after {after} before {before} limit {limit:?} {ordering:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn list_package_versions_intersects_version_bounds_cursors_and_checkpoint() {
        let fixture = setup_versions_fixture().await;
        let cursors = fixture_cursors(&fixture).await;

        // Version `n` is published at checkpoint `10 * n`. Each case lists the matching versions
        // in ascending order; descending lists them in reverse.
        for (start_version, end_version, after, before, at_checkpoint, ascending) in [
            (Some(2), Some(5), None, None, None, vec![2, 3, 4]),
            (Some(2), Some(5), Some(3), None, None, vec![4]),
            (Some(2), None, None, Some(4), None, vec![2, 3]),
            (None, Some(5), Some(1), None, Some(35), vec![2, 3]),
            (Some(3), None, None, None, Some(45), vec![3, 4]),
            (None, None, None, None, Some(35), vec![1, 2, 3]),
            // The package is published, so bounds that exclude every version list nothing.
            (Some(4), None, None, None, Some(35), vec![]),
            (Some(6), None, None, None, None, vec![]),
        ] {
            for (ordering, expected) in [
                (Ordering::Ascending, ascending.clone()),
                (
                    Ordering::Descending,
                    ascending.iter().rev().copied().collect(),
                ),
            ] {
                let mut req = cursor_req(
                    fixture.original_id,
                    query_options(
                        None,
                        ordering,
                        after.map(|v| &cursors[&v]),
                        before.map(|v| &cursors[&v]),
                    ),
                );
                req.start_version = start_version;
                req.end_version = end_version;
                req.at_checkpoint = at_checkpoint;
                let response = list_versions_fixture(&fixture, req).await.unwrap();
                assert_eq!(
                    listed_versions(&response),
                    expected,
                    "versions [{start_version:?}, {end_version:?}) after {after:?} before \
                     {before:?} at_checkpoint {at_checkpoint:?} {ordering:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn list_package_versions_descending_skips_versions_after_the_checkpoint() {
        let fixture = setup_versions_fixture().await;

        // Versions 4 and 5 are published after checkpoint 35, so a descending page starts at 3
        // and still fills up.
        let mut req = cursor_req(
            fixture.original_id,
            query_options(Some(2), Ordering::Descending, None, None),
        );
        req.at_checkpoint = Some(35);
        let first = list_versions_fixture(&fixture, req).await.unwrap();
        assert_eq!(listed_versions(&first), vec![3, 2]);

        let mut req = cursor_req(
            fixture.original_id,
            query_options(
                Some(2),
                Ordering::Descending,
                None,
                first.versions[1].cursor.as_ref(),
            ),
        );
        req.at_checkpoint = Some(35);
        let second = list_versions_fixture(&fixture, req).await.unwrap();
        assert_eq!(listed_versions(&second), vec![1]);
    }

    #[tokio::test]
    async fn list_package_versions_cursor_mode_keeps_not_found_for_unpublished_packages() {
        let fixture = setup_versions_fixture().await;

        // Version 1 is published at checkpoint 10. Below it the package does not exist yet, so
        // a first page is not found even when version bounds alone would list nothing.
        let served_height = ServedHeight {
            checkpoint: 9,
            epoch: 0,
        };
        for start_version in [None, Some(2)] {
            let mut req = cursor_req(
                fixture.original_id,
                query_options(None, Ordering::Descending, None, None),
            );
            req.start_version = start_version;
            let status: tonic::Status =
                list_package_versions(fixture.client.clone(), served_height, req)
                    .await
                    .expect_err("package published above the served height")
                    .into();
            assert_eq!(status.code(), tonic::Code::NotFound, "{start_version:?}");
        }
    }

    #[tokio::test]
    async fn list_package_versions_rejects_invalid_cursor_requests() {
        let fixture = setup_versions_fixture().await;

        let mut legacy = list_req(fixture.original_id, None);
        legacy.page_size = Some(1);
        let page_token = list_versions_fixture(&fixture, legacy)
            .await
            .unwrap()
            .next_page_token
            .expect("more versions follow");
        let other_package = PackageVersionCursor::Lineage {
            original_package_id: ObjectID::from_single_byte(0xDD),
            version: 1,
        }
        .encode();
        let every_package = PackageVersionCursor::AllPackages {
            checkpoint: 10,
            original_package_id: fixture.original_id,
            version: 1,
        }
        .encode();
        let malformed = Bytes::from_static(&[0xFF, 0xFF]);
        let ascending = || query_options(None, Ordering::Ascending, None, None);

        let mut with_page_size = cursor_req(fixture.original_id, ascending());
        with_page_size.page_size = Some(1);
        let mut with_page_token = cursor_req(fixture.original_id, ascending());
        with_page_token.page_token = Some(page_token);
        let mut invalid_ordering = ascending();
        invalid_ordering.ordering = Some(7);
        let mut with_start_checkpoint = list_req(fixture.original_id, None);
        with_start_checkpoint.start_checkpoint = Some(10);
        for (req, expected) in [
            (
                with_page_size,
                "page_size and options are mutually exclusive",
            ),
            (
                with_page_token,
                "page_token and options are mutually exclusive",
            ),
            (
                cursor_req(
                    fixture.original_id,
                    query_options(None, Ordering::Ascending, Some(&malformed), None),
                ),
                "invalid cursor encoding",
            ),
            (
                cursor_req(
                    fixture.original_id,
                    query_options(None, Ordering::Descending, None, Some(&other_package)),
                ),
                "cursor package ID does not match request package ID",
            ),
            (
                cursor_req(
                    fixture.original_id,
                    query_options(None, Ordering::Ascending, Some(&every_package), None),
                ),
                "cursor is from the listing of every package",
            ),
            (
                cursor_req(fixture.original_id, invalid_ordering),
                "invalid ordering",
            ),
            (
                with_start_checkpoint,
                "start_checkpoint requires package_id to be unset",
            ),
        ] {
            let status = list_versions_fixture(&fixture, req).await.unwrap_err();
            assert_eq!(status.code(), tonic::Code::InvalidArgument, "{expected}");
            assert!(
                status.message().contains(expected),
                "expected {expected:?}, got {:?}",
                status.message()
            );
        }
    }

    #[tokio::test]
    async fn list_package_versions_page_tokens_page_as_before_with_cursors() {
        let fixture = setup_versions_fixture().await;
        let cursors = fixture_cursors(&fixture).await;

        for (start_version, end_version, pages) in [
            (None, None, vec![vec![1, 2], vec![3, 4], vec![5]]),
            (Some(2), Some(5), vec![vec![2, 3], vec![4]]),
        ] {
            let mut page_token = None;
            for (index, expected) in pages.iter().enumerate() {
                let mut req = list_req(fixture.original_id, None);
                req.page_size = Some(2);
                req.page_token = page_token.take();
                req.start_version = start_version;
                req.end_version = end_version;
                let page = list_versions_fixture(&fixture, req).await.unwrap();
                assert_eq!(&listed_versions(&page), expected, "page {index}");
                // Every version names its lineage.
                for version in &page.versions {
                    assert_eq!(
                        version.original_id,
                        Some(fixture.original_id.to_canonical_string(true))
                    );
                }
                // Page-token items carry the same cursors as cursor mode.
                for version in &page.versions {
                    assert_eq!(
                        version.cursor.as_ref(),
                        Some(&cursors[&version.version.unwrap()])
                    );
                }
                page_token = page.next_page_token;
                assert_eq!(
                    page_token.is_some(),
                    index + 1 < pages.len(),
                    "page {index}"
                );
            }
        }
    }

    /// A mock seeded with the versions of three packages, published (as `(checkpoint, package,
    /// version)`) at (10, A, 1), (10, B, 1), (20, A, 2), (30, C, 1), (30, C, 2), and (70, B, 2),
    /// above the served height.
    struct AllPackagesFixture {
        client: BigTableClient,
        _server: tokio::task::JoinHandle<()>,
    }

    const PACKAGE_A: ObjectID = ObjectID::from_single_byte(0x0A);
    const PACKAGE_B: ObjectID = ObjectID::from_single_byte(0x0B);
    const PACKAGE_C: ObjectID = ObjectID::from_single_byte(0x0C);

    /// The versions [`AllPackagesFixture`] lists at its served height, in publish order.
    const ALL_PACKAGES: [(u64, ObjectID, u64); 5] = [
        (10, PACKAGE_A, 1),
        (10, PACKAGE_B, 1),
        (20, PACKAGE_A, 2),
        (30, PACKAGE_C, 1),
        (30, PACKAGE_C, 2),
    ];

    async fn setup_all_packages_fixture() -> AllPackagesFixture {
        let mock = MockBigtableServer::new();
        let (addr, server) = mock.start().await.expect("start mock BigTable");
        let client = BigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");

        for (checkpoint, original_id, version) in
            ALL_PACKAGES.into_iter().chain([(70, PACKAGE_B, 2)])
        {
            let storage_id = storage_id_of(original_id, version);
            mock.insert_row(
                tables::packages::NAME,
                tables::packages::encode_key(original_id.as_ref(), version),
                tables::packages::encode(checkpoint, storage_id.as_ref(), false),
            )
            .await;
            mock.insert_row(
                tables::packages_by_checkpoint::NAME,
                tables::packages_by_checkpoint::encode_key(
                    checkpoint,
                    original_id.as_ref(),
                    version,
                ),
                tables::packages_by_checkpoint::encode(),
            )
            .await;
        }

        AllPackagesFixture {
            client,
            _server: server,
        }
    }

    /// The storage id the fixture gives `version` of the package originally published at
    /// `original_id`.
    fn storage_id_of(original_id: ObjectID, version: u64) -> ObjectID {
        let mut bytes = original_id.into_bytes();
        bytes[0] = version as u8;
        ObjectID::new(bytes)
    }

    fn all_packages_req(options: Option<QueryOptions>) -> ListPackageVersionsRequest {
        let mut req = ListPackageVersionsRequest::default();
        req.options = options;
        req
    }

    async fn list_all_packages_fixture(
        fixture: &AllPackagesFixture,
        req: ListPackageVersionsRequest,
    ) -> Result<ListPackageVersionsResponse, tonic::Status> {
        list_package_versions(fixture.client.clone(), VERSIONS_FIXTURE_SERVED_HEIGHT, req)
            .await
            .map_err(Into::into)
    }

    /// The `(checkpoint, package, version)` of each version a response lists, in order,
    /// checking that each version carries its storage id, original id and cursor.
    fn listed_publishes(response: &ListPackageVersionsResponse) -> Vec<(u64, ObjectID, u64)> {
        assert!(response.next_page_token.is_none());
        response
            .versions
            .iter()
            .map(|listed| {
                let checkpoint = listed.checkpoint.expect("checkpoint is set");
                let original_id: ObjectID = listed
                    .original_id
                    .as_deref()
                    .expect("original_id is set")
                    .parse()
                    .unwrap();
                let version = listed.version.expect("version is set");
                let storage_id: ObjectID = listed
                    .package_id
                    .as_deref()
                    .expect("package_id is set")
                    .parse()
                    .unwrap();
                assert_eq!(storage_id, storage_id_of(original_id, version));
                assert_eq!(
                    listed.cursor,
                    Some(
                        PackageVersionCursor::AllPackages {
                            checkpoint,
                            original_package_id: original_id,
                            version,
                        }
                        .encode()
                    )
                );
                (checkpoint, original_id, version)
            })
            .collect()
    }

    #[tokio::test]
    async fn list_all_package_versions_pages_in_publish_order_in_either_direction() {
        let fixture = setup_all_packages_fixture().await;
        let list = |options| list_all_packages_fixture(&fixture, all_packages_req(Some(options)));
        let last_cursor = |response: &ListPackageVersionsResponse| {
            response.versions.last().and_then(|v| v.cursor.clone())
        };

        // Without options, every version at or below the served height, in publish order.
        let everything = list_all_packages_fixture(&fixture, all_packages_req(None))
            .await
            .unwrap();
        assert_eq!(listed_publishes(&everything), ALL_PACKAGES.to_vec());

        // Forward in pages of two, each after the last version received, until a short page.
        let mut after = None;
        let mut forward = vec![];
        for expected in [&ALL_PACKAGES[0..2], &ALL_PACKAGES[2..4], &ALL_PACKAGES[4..]] {
            let page = list(query_options(
                Some(2),
                Ordering::Ascending,
                after.as_ref(),
                None,
            ))
            .await
            .unwrap();
            assert_eq!(listed_publishes(&page), expected.to_vec());
            after = last_cursor(&page);
            forward.push(page);
        }

        // Backward from the end in pages of two, each before the lowest version received.
        let mut before = None;
        for expected in [vec![4, 3], vec![2, 1], vec![0]] {
            let page = list(query_options(
                Some(2),
                Ordering::Descending,
                None,
                before.as_ref(),
            ))
            .await
            .unwrap();
            let expected: Vec<_> = expected.iter().map(|&i| ALL_PACKAGES[i]).collect();
            assert_eq!(listed_publishes(&page), expected);
            before = last_cursor(&page);
        }

        // A cursor from an ascending page bounds a descending one, inside checkpoint 10 and 30.
        let between = list(query_options(
            None,
            Ordering::Descending,
            forward[0].versions.first().and_then(|v| v.cursor.as_ref()),
            forward[2].versions.first().and_then(|v| v.cursor.as_ref()),
        ))
        .await
        .unwrap();
        assert_eq!(
            listed_publishes(&between),
            vec![ALL_PACKAGES[3], ALL_PACKAGES[2], ALL_PACKAGES[1]],
        );
    }

    /// The cursor of `ALL_PACKAGES[index]`.
    fn all_packages_cursor(index: usize) -> Bytes {
        let (checkpoint, original_package_id, version) = ALL_PACKAGES[index];
        PackageVersionCursor::AllPackages {
            checkpoint,
            original_package_id,
            version,
        }
        .encode()
    }

    #[tokio::test]
    async fn list_all_package_versions_bounds_by_start_and_at_checkpoint() {
        let fixture = setup_all_packages_fixture().await;
        let list = |start_checkpoint,
                    at_checkpoint,
                    after: Option<usize>,
                    before: Option<usize>,
                    ordering| {
            let (after, before) = (
                after.map(all_packages_cursor),
                before.map(all_packages_cursor),
            );
            let mut req = all_packages_req(Some(query_options(
                None,
                ordering,
                after.as_ref(),
                before.as_ref(),
            )));
            req.start_checkpoint = start_checkpoint;
            req.at_checkpoint = at_checkpoint;
            list_all_packages_fixture(&fixture, req)
        };

        // Each case's cursors are indexes into `ALL_PACKAGES`.
        for (start_checkpoint, at_checkpoint, after, before, expected) in [
            (Some(20), None, None, None, &ALL_PACKAGES[2..]),
            (None, Some(20), None, None, &ALL_PACKAGES[..3]),
            (Some(20), Some(20), None, None, &ALL_PACKAGES[2..3]),
            (Some(11), Some(29), None, None, &ALL_PACKAGES[2..3]),
            (Some(25), Some(20), None, None, &ALL_PACKAGES[..0]),
            (Some(30), Some(60), None, None, &ALL_PACKAGES[3..]),
            // Cursors inside the checkpoint bounds narrow them.
            (Some(10), None, Some(0), None, &ALL_PACKAGES[1..]),
            (None, Some(30), None, Some(4), &ALL_PACKAGES[..4]),
            // Cursors outside them leave them as they are.
            (Some(20), None, Some(0), None, &ALL_PACKAGES[2..]),
            (None, Some(20), None, Some(4), &ALL_PACKAGES[..3]),
            (Some(11), Some(29), Some(1), Some(3), &ALL_PACKAGES[2..3]),
        ] {
            let ascending = list(
                start_checkpoint,
                at_checkpoint,
                after,
                before,
                Ordering::Ascending,
            )
            .await
            .unwrap();
            assert_eq!(
                listed_publishes(&ascending),
                expected.to_vec(),
                "[{start_checkpoint:?}, {at_checkpoint:?}] after {after:?} before {before:?}"
            );
            let descending = list(
                start_checkpoint,
                at_checkpoint,
                after,
                before,
                Ordering::Descending,
            )
            .await
            .unwrap();
            let mut reversed = expected.to_vec();
            reversed.reverse();
            assert_eq!(
                listed_publishes(&descending),
                reversed,
                "[{start_checkpoint:?}, {at_checkpoint:?}] after {after:?} before {before:?} \
                 descending"
            );
        }

        let status = list(None, Some(61), None, None, Ordering::Ascending)
            .await
            .unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn list_all_package_versions_rejects_lineage_paging_and_cursors() {
        let fixture = setup_all_packages_fixture().await;
        let lineage_cursor = PackageVersionCursor::Lineage {
            original_package_id: PACKAGE_A,
            version: 1,
        }
        .encode();

        let mut with_page_size = all_packages_req(None);
        with_page_size.page_size = Some(1);
        let mut with_page_token = all_packages_req(None);
        with_page_token.page_token = Some(Bytes::from_static(&[0]));
        let mut with_start_version = all_packages_req(None);
        with_start_version.start_version = Some(1);
        let mut with_end_version = all_packages_req(None);
        with_end_version.end_version = Some(2);

        for (req, expected) in [
            (with_page_size, "page_size requires package_id"),
            (with_page_token, "page_token requires package_id"),
            (with_start_version, "start_version requires package_id"),
            (with_end_version, "end_version requires package_id"),
            (
                all_packages_req(Some(query_options(
                    None,
                    Ordering::Ascending,
                    None,
                    Some(&lineage_cursor),
                ))),
                "cursor is from the listing of one package",
            ),
        ] {
            let status = list_all_packages_fixture(&fixture, req).await.unwrap_err();
            assert_eq!(status.code(), tonic::Code::InvalidArgument, "{expected}");
            assert!(
                status.message().contains(expected),
                "expected {expected:?}, got {:?}",
                status.message()
            );
        }
    }
}
