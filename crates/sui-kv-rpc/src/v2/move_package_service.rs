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
use sui_rpc::proto::sui::rpc::v2::PackageVersion;
use sui_rpc::proto::sui::rpc::v2::move_package_service_server::MovePackageService;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_rpc_api::grpc::v2::move_package_service::PageToken;
use sui_rpc_api::grpc::v2::move_package_service::get_datatype_response;
use sui_rpc_api::grpc::v2::move_package_service::get_function_response;
use sui_rpc_api::grpc::v2::move_package_service::get_package_response;
use sui_types::base_types::ObjectID;
use sui_types::move_package::MovePackage;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;

use crate::KvRpcServer;

#[tonic::async_trait]
impl MovePackageService for KvRpcServer {
    async fn get_package(
        &self,
        request: tonic::Request<GetPackageRequest>,
    ) -> Result<tonic::Response<GetPackageResponse>, tonic::Status> {
        get_package(self.client.clone(), request.into_inner())
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
        list_package_versions(self.client.clone(), request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}

async fn get_package(
    mut client: BigTableClient,
    request: GetPackageRequest,
) -> Result<GetPackageResponse, RpcError> {
    let package_id_str = request.package_id.as_ref().ok_or_else(|| {
        FieldViolation::new("package_id")
            .with_description("missing package_id")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let package_id = parse_package_id(package_id_str)?;

    if request.version.is_some() && request.at_checkpoint.is_some() {
        return Err(FieldViolation::new("at_checkpoint")
            .with_description("at most one of `version` and `at_checkpoint` may be set")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }

    if request.version.is_none() && request.at_checkpoint.is_none() {
        let package = load_package(client, package_id).await?;
        return get_package_response(&package);
    }

    let original_id = resolve_original_package_id(client.clone(), package_id).await?;

    // Expect either one of version or checkpoint but not both.
    let data = if let Some(version) = request.version {
        client
            .get_packages_by_version(&[(original_id, version)])
            .await
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
            .pop()
    } else {
        client
            .get_package_latest(original_id, request.at_checkpoint.unwrap_or(u64::MAX))
            .await
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
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

async fn list_package_versions(
    mut client: BigTableClient,
    request: ListPackageVersionsRequest,
) -> Result<ListPackageVersionsResponse, RpcError> {
    let package_id_str = request.package_id.as_ref().ok_or_else(|| {
        FieldViolation::new("package_id")
            .with_description("missing package_id")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let original_package_id =
        resolve_original_package_id(client.clone(), parse_package_id(package_id_str)?).await?;

    let page_size = request
        .page_size
        .map(|s| (s as usize).clamp(1, 10000))
        .unwrap_or(1000);

    let page_token = request
        .page_token
        .map(|token| PageToken::decode(&token))
        .transpose()?;

    if let Some(token) = &page_token
        && token.original_package_id != original_package_id
    {
        return Err(FieldViolation::new("page_token")
            .with_description("page token package ID does not match request package ID")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }

    // The token's version is the first version of the next page (inclusive); the reader's
    // `after_version` bound is exclusive.
    let after_version = page_token.map(|token| token.version.saturating_sub(1));

    let mut versions: Vec<_> = client
        .get_package_versions(
            original_package_id,
            u64::MAX,
            after_version,
            None,
            page_size + 1,
            false,
        )
        .await
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        .into_iter()
        .map(|pkg| {
            let storage_id = ObjectID::from_bytes(&pkg.package_id)
                .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
            Ok(PackageVersion::new(&storage_id.into(), pkg.package_version))
        })
        .collect::<Result<_, RpcError>>()?;

    let next_page_token = if versions.len() > page_size {
        versions.pop().and_then(|next| {
            next.version.map(|version| {
                PageToken {
                    original_package_id,
                    version,
                }
                .encode()
            })
        })
    } else {
        None
    };

    Ok(ListPackageVersionsResponse::new(versions, next_page_token))
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

    use sui_kvstore::tables;
    use sui_kvstore::testing::MockBigtableServer;
    use sui_types::base_types::{ObjectID, SequenceNumber};
    use sui_types::digests::TransactionDigest;
    use sui_types::move_package::MovePackage;
    use sui_types::object::{Data, Object};
    use sui_types::storage::ObjectKey;

    use super::*;

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
        let resp = list_package_versions(client.clone(), req).await.unwrap();
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
        let resp = list_package_versions(client.clone(), req).await.unwrap();
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
        let err = list_package_versions(client.clone(), req)
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

    /// Seed the mock with a two-version package lineage (v1 at `original_id`
    /// published at checkpoint 5, v2 at `upgraded_id` published at checkpoint
    /// 20) plus one non-package object.
    async fn setup_lineage_fixture() -> LineageFixture {
        use bytes::Bytes;
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
        get_package(fixture.client.clone(), request)
            .await
            .expect("get_package succeeds")
            .package
            .expect("response carries a package")
    }

    async fn fetch_pkg_err(fixture: &LineageFixture, request: GetPackageRequest) -> tonic::Status {
        get_package(fixture.client.clone(), request)
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
        req.version = Some(1);
        let package = fetch_pkg(&fixture, req).await;
        assert_eq!(
            package.storage_id,
            Some(fixture.original_id.to_canonical_string(true)),
        );
        assert_eq!(package.version, Some(1));

        // Exact version through the original id resolves forward to v2.
        let mut req = get_package_req(fixture.original_id);
        req.version = Some(2);
        let package = fetch_pkg(&fixture, req).await;
        assert_eq!(
            package.storage_id,
            Some(fixture.upgraded_id.to_canonical_string(true)),
        );
        assert_eq!(package.version, Some(2));

        // A checkpoint bound between the two publishes resolves v1.
        let mut req = get_package_req(fixture.upgraded_id);
        req.at_checkpoint = Some(19);
        let package = fetch_pkg(&fixture, req).await;
        assert_eq!(
            package.storage_id,
            Some(fixture.original_id.to_canonical_string(true)),
        );
        assert_eq!(package.version, Some(1));

        // A bound above the tip resolves the latest version.
        let mut req = get_package_req(fixture.original_id);
        req.at_checkpoint = Some(u64::MAX);
        let package = fetch_pkg(&fixture, req).await;
        assert_eq!(
            package.storage_id,
            Some(fixture.upgraded_id.to_canonical_string(true)),
        );
        assert_eq!(package.version, Some(2));
    }

    #[tokio::test]
    async fn missing_packages_are_not_found() {
        let fixture = setup_lineage_fixture().await;
        let unknown = ObjectID::from_single_byte(0xDD);

        let unknown_versioned = {
            let mut req = get_package_req(unknown);
            req.version = Some(1);
            req
        };
        let missing_version = {
            let mut req = get_package_req(fixture.original_id);
            req.version = Some(3);
            req
        };
        let before_first_publish = {
            let mut req = get_package_req(fixture.original_id);
            req.at_checkpoint = Some(4);
            req
        };
        for req in [
            get_package_req(unknown),
            unknown_versioned,
            missing_version,
            before_first_publish,
        ] {
            let status = fetch_pkg_err(&fixture, req).await;
            assert_eq!(status.code(), tonic::Code::NotFound);
        }
    }

    #[tokio::test]
    async fn non_package_object_is_rejected() {
        let fixture = setup_lineage_fixture().await;

        let status = fetch_pkg_err(&fixture, get_package_req(fixture.plain_object_id)).await;
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert!(status.message().contains("not a package"));

        // Setting both bounds is rejected before any lookup.
        let mut req = get_package_req(fixture.original_id);
        req.version = Some(1);
        req.at_checkpoint = Some(1);
        let status = fetch_pkg_err(&fixture, req).await;
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }
}
