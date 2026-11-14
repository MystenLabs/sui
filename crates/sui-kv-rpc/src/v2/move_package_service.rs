// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_kvstore::PackageData;
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
use sui_rpc::proto::sui::rpc::v2::get_package_request::Selector;
use sui_rpc::proto::sui::rpc::v2::move_package_service_server::MovePackageService;
use sui_rpc_api::AtCheckpointUpperBound;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_rpc_api::grpc::v2::move_package_service::PageToken;
use sui_rpc_api::grpc::v2::move_package_service::get_datatype_response;
use sui_rpc_api::grpc::v2::move_package_service::get_function_response;
use sui_rpc_api::grpc::v2::move_package_service::get_package_response;
use sui_types::base_types::ObjectID;
use sui_types::move_package::MovePackage;
use sui_types::object::OBJECT_START_VERSION;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;

use crate::KvRpcServer;
use crate::bigtable_client::BigTableClient as LimitedBigTableClient;
use crate::served_height::ServedHeight;

#[tonic::async_trait]
impl MovePackageService for KvRpcServer {
    async fn get_package(
        &self,
        request: tonic::Request<GetPackageRequest>,
    ) -> Result<tonic::Response<GetPackageResponse>, tonic::Status> {
        get_package(
            self.limited_client("GetPackage"),
            ServedHeight::of(&request)?,
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
        get_datatype(
            self.limited_client("GetDatatype"),
            ServedHeight::of(&request)?,
            request.into_inner(),
        )
        .await
        .map(tonic::Response::new)
        .map_err(Into::into)
    }

    async fn get_function(
        &self,
        request: tonic::Request<GetFunctionRequest>,
    ) -> Result<tonic::Response<GetFunctionResponse>, tonic::Status> {
        get_function(
            self.limited_client("GetFunction"),
            ServedHeight::of(&request)?,
            request.into_inner(),
        )
        .await
        .map(tonic::Response::new)
        .map_err(Into::into)
    }

    async fn list_package_versions(
        &self,
        request: tonic::Request<ListPackageVersionsRequest>,
    ) -> Result<tonic::Response<ListPackageVersionsResponse>, tonic::Status> {
        list_package_versions(
            self.limited_client("ListPackageVersions"),
            ServedHeight::of(&request)?,
            request.into_inner(),
        )
        .await
        .map(tonic::Response::new)
        .map_err(Into::into)
    }
}

/// Serve a package as of `served_height`: a package published above it is `NotFound`.
async fn get_package(
    client: LimitedBigTableClient,
    served_height: ServedHeight,
    request: GetPackageRequest,
) -> Result<GetPackageResponse, RpcError> {
    let package_id_str = request.package_id.as_ref().ok_or_else(|| {
        FieldViolation::new("package_id")
            .with_description("missing package_id")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let package_id = parse_package_id(package_id_str)?;

    let data = match request.selector {
        None => {
            let package = load_package_at_served_height(client, served_height, package_id).await?;
            return get_package_response(&package);
        }
        Some(Selector::Version(version)) => {
            let original_id = resolve_original_package_id(client.clone(), package_id).await?;
            client
                .get_packages_by_version(&[(original_id, version)])
                .await?
                .pop()
                .filter(|data| data.cp_sequence_number <= served_height.checkpoint)
        }
        Some(Selector::AtCheckpoint(at_checkpoint)) => {
            // The response does not echo the checkpoint it resolved at, so a bound above the
            // served height would let a caller persist a wrong answer; fail closed instead.
            AtCheckpointUpperBound::new(served_height.checkpoint).check(at_checkpoint)?;
            let original_id = resolve_original_package_id(client.clone(), package_id).await?;
            client
                .get_package_latest(original_id, at_checkpoint)
                .await?
        }
        Some(_) => {
            return Err(FieldViolation::new("selector")
                .with_description("unknown selector variant")
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }
    }
    .ok_or_else(RpcError::not_found)?;

    let package = load_package_version(client, data).await?;
    get_package_response(&package)
}

/// Serve a datatype of the package a bare storage ID names, resolved as `GetPackage` resolves it.
async fn get_datatype(
    client: LimitedBigTableClient,
    served_height: ServedHeight,
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

    let package =
        load_package_at_served_height(client, served_height, parse_package_id(package_id_str)?)
            .await?;
    get_datatype_response(&package, module_name, datatype_name)
}

/// Serve a function of the package a bare storage ID names, resolved as `GetPackage` resolves it.
async fn get_function(
    client: LimitedBigTableClient,
    served_height: ServedHeight,
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

    let package =
        load_package_at_served_height(client, served_height, parse_package_id(package_id_str)?)
            .await?;
    get_function_response(&package, module_name, function_name)
}

/// List the versions of a package published at or below `served_height`.
async fn list_package_versions(
    client: LimitedBigTableClient,
    served_height: ServedHeight,
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
    let first_page = page_token.is_none();
    let after_version = page_token.map(|token| token.version.saturating_sub(1));

    // Versions are published in checkpoint order, so those above the served height can only
    // come at the end of this ascending scan. Cutting them here rather than passing the height
    // as `cp_bound` keeps the scan unbounded by checkpoint, which lets BigTable stop at the row
    // limit.
    let mut versions: Vec<_> = client
        .get_package_versions(
            original_package_id,
            u64::MAX,
            after_version,
            None,
            page_size + 1,
            false,
        )
        .await?
        .into_iter()
        .take_while(|pkg| pkg.cp_sequence_number <= served_height.checkpoint)
        .map(|pkg| {
            let storage_id = ObjectID::from_bytes(&pkg.package_id)
                .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
            Ok(PackageVersion::new(&storage_id.into(), pkg.package_version))
        })
        .collect::<Result<_, RpcError>>()?;

    // Every package has a first version, so a first page with nothing at or below the served
    // height means the package is not published yet as of that height.
    if first_page && versions.is_empty() {
        return Err(RpcError::not_found());
    }

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

/// Load the package a bare storage ID names as of `served_height`: a package published above it
/// is `NotFound`. A storage ID holds one package version that never changes, except for system
/// packages, which are upgraded in place: a version above the first one at the package's
/// original ID. Such a package resolves to its version current at the served height.
async fn load_package_at_served_height(
    client: LimitedBigTableClient,
    served_height: ServedHeight,
    package_id: ObjectID,
) -> Result<MovePackage, RpcError> {
    let package = load_package(client.clone(), package_id).await?;
    let published = client
        .get_packages_by_version(&[(package.original_package_id(), package.version().value())])
        .await?
        .pop()
        .is_some_and(|data| data.cp_sequence_number <= served_height.checkpoint);
    if published {
        return Ok(package);
    }
    // Upgraded in place is decided from the package, not a list of system package IDs, which a
    // kv-rpc build older than the chain would be missing entries from.
    let upgraded_in_place =
        package.version() > OBJECT_START_VERSION && package.id() == package.original_package_id();
    if !upgraded_in_place {
        return Err(RpcError::not_found());
    }
    let data = client
        .get_package_latest(package_id, served_height.checkpoint)
        .await?
        .ok_or_else(RpcError::not_found)?;
    load_package_version(client, data).await
}

/// Load the package object a `packages` row names. Callers pass a row at or below the served
/// height, and the kvstore never prunes objects, so a missing object is `Internal`.
async fn load_package_version(
    client: LimitedBigTableClient,
    data: PackageData,
) -> Result<MovePackage, RpcError> {
    let storage_id = ObjectID::from_bytes(&data.package_id).map_err(|e| {
        RpcError::new(
            tonic::Code::Internal,
            format!("invalid stored package id: {e}"),
        )
    })?;
    let object = client
        .get_objects(&[ObjectKey(storage_id, data.package_version.into())])
        .await?
        .pop()
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                format!(
                    "package {storage_id} version {} has a packages row but no stored object",
                    data.package_version
                ),
            )
        })?;
    into_package(object)
}

/// Load the newest object at a storage ID as a package, whatever checkpoint wrote it.
async fn load_package(
    client: LimitedBigTableClient,
    package_id: ObjectID,
) -> Result<MovePackage, RpcError> {
    let object = client
        .get_latest_object(package_id)
        .await?
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
    client: LimitedBigTableClient,
    package_id: ObjectID,
) -> Result<ObjectID, RpcError> {
    let pairs = client.get_package_original_ids(&[package_id]).await?;
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
    use crate::bigtable_client::Metrics;
    use crate::v2::test_utils::UNBOUNDED_SERVED_HEIGHT;
    use sui_kvstore::BigTableClient;

    /// A request-scoped limited client over `client`, as `KvRpcServer::limited_client` builds.
    fn limited(client: &BigTableClient) -> LimitedBigTableClient {
        LimitedBigTableClient::new(client.clone(), 2, Metrics::for_testing().0, "test")
    }

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
        let resp = list_package_versions(limited(&client), UNBOUNDED_SERVED_HEIGHT, req)
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
        let resp = list_package_versions(limited(&client), UNBOUNDED_SERVED_HEIGHT, req)
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
        let err = list_package_versions(limited(&client), UNBOUNDED_SERVED_HEIGHT, req)
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
        use bytes::Bytes;
        use move_binary_format::file_format::basic_test_module;
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

        // A structurally valid module (`DUMMY`, with a struct `Bar` and a function `foo`) whose
        // self-address becomes the package's id, standing in for real compiled code.
        let mut module = basic_test_module();
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
        get_package(limited(&fixture.client), FIXTURE_SERVED_HEIGHT, request)
            .await
            .expect("get_package succeeds")
            .package
            .expect("response carries a package")
    }

    async fn fetch_pkg_err(fixture: &LineageFixture, request: GetPackageRequest) -> tonic::Status {
        get_package(limited(&fixture.client), FIXTURE_SERVED_HEIGHT, request)
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

    /// The `google.rpc.ErrorInfo` reason among `status`'s details.
    fn error_reason(status: &tonic::Status) -> String {
        use prost::Message;
        use sui_rpc::proto::google::rpc::{ErrorInfo, Status};

        Status::decode(status.details())
            .expect("details decode as google.rpc.Status")
            .details
            .iter()
            .find_map(|detail| detail.to_msg::<ErrorInfo>().ok())
            .unwrap_or_else(|| panic!("no ErrorInfo in {status:?}"))
            .reason
    }

    #[tokio::test]
    async fn at_checkpoint_above_served_height_is_rejected() {
        let fixture = setup_lineage_fixture().await;
        let unknown = ObjectID::from_single_byte(0xDD);

        for package_id in [fixture.original_id, unknown] {
            for at_checkpoint in [26u64, u64::MAX] {
                let mut req = get_package_req(package_id);
                req.selector = Some(Selector::AtCheckpoint(at_checkpoint));
                let status = fetch_pkg_err(&fixture, req).await;
                assert_eq!(
                    status.code(),
                    tonic::Code::OutOfRange,
                    "{package_id} at {at_checkpoint}"
                );
                assert_eq!(
                    error_reason(&status),
                    ErrorReason::AboveLatestAvailable.as_str_name()
                );
                assert_eq!(
                    status.message(),
                    format!(
                        "requested data above latest available; at_checkpoint is \
                         {at_checkpoint}, and only checkpoints below 26 are available"
                    ),
                );
            }
        }
    }

    #[tokio::test]
    async fn bare_id_published_above_served_height_is_not_found() {
        let fixture = setup_lineage_fixture().await;

        // v1 is published at checkpoint 5, and v2 at checkpoint 20.
        for (package_id, version, published_at) in
            [(fixture.original_id, 1, 5), (fixture.upgraded_id, 2, 20)]
        {
            for (served_checkpoint, found) in [(published_at - 1, false), (published_at, true)] {
                let served_height = ServedHeight {
                    checkpoint: served_checkpoint,
                    epoch: 0,
                };
                let result = get_package(
                    limited(&fixture.client),
                    served_height,
                    get_package_req(package_id),
                )
                .await;
                let context = format!("v{version} at served height {served_checkpoint}");
                match result {
                    Ok(response) => {
                        assert!(found, "{context}: served");
                        assert_eq!(response.package.and_then(|p| p.version), Some(version));
                    }
                    Err(err) => {
                        assert!(!found, "{context}: hidden");
                        let status: tonic::Status = err.into();
                        assert_eq!(status.code(), tonic::Code::NotFound, "{context}");
                    }
                }
            }
        }
    }

    fn get_datatype_req(package_id: ObjectID) -> GetDatatypeRequest {
        let mut request = GetDatatypeRequest::default();
        request.package_id = Some(package_id.to_canonical_string(true));
        request.module_name = Some("DUMMY".to_owned());
        request.name = Some("Bar".to_owned());
        request
    }

    fn get_function_req(package_id: ObjectID) -> GetFunctionRequest {
        let mut request = GetFunctionRequest::default();
        request.package_id = Some(package_id.to_canonical_string(true));
        request.module_name = Some("DUMMY".to_owned());
        request.name = Some("foo".to_owned());
        request
    }

    /// `GetDatatype` and `GetFunction` see a package exactly when bare-ID `GetPackage` does.
    #[tokio::test]
    async fn datatype_and_function_of_package_published_above_served_height_are_not_found() {
        let fixture = setup_lineage_fixture().await;

        // v1 is published at checkpoint 5, and v2 at checkpoint 20.
        for (package_id, version, published_at) in
            [(fixture.original_id, 1, 5), (fixture.upgraded_id, 2, 20)]
        {
            for (served_checkpoint, found) in [(published_at - 1, false), (published_at, true)] {
                let served_height = ServedHeight {
                    checkpoint: served_checkpoint,
                    epoch: 0,
                };
                let context = format!("v{version} at served height {served_checkpoint}");
                let datatype = get_datatype(
                    limited(&fixture.client),
                    served_height,
                    get_datatype_req(package_id),
                )
                .await
                .map(|response| response.datatype.and_then(|datatype| datatype.name))
                .map_err(|err| tonic::Status::from(err).code());
                let function = get_function(
                    limited(&fixture.client),
                    served_height,
                    get_function_req(package_id),
                )
                .await
                .map(|response| response.function.and_then(|function| function.name))
                .map_err(|err| tonic::Status::from(err).code());
                if found {
                    assert_eq!(datatype, Ok(Some("Bar".to_owned())), "{context}");
                    assert_eq!(function, Ok(Some("foo".to_owned())), "{context}");
                } else {
                    assert_eq!(datatype, Err(tonic::Code::NotFound), "{context}");
                    assert_eq!(function, Err(tonic::Code::NotFound), "{context}");
                }
            }
        }
    }

    /// Seed a package upgraded in place, at an ID outside the binary's list of system packages, as
    /// a system package added by a protocol upgrade newer than the build would be: version 1 at
    /// checkpoint 5, upgraded in place to version 2 at checkpoint 10. Its module `DUMMY` has a
    /// struct `Bar`, with no abilities in version 1 and `drop` in version 2, and a function `foo`,
    /// private in version 1 and public in version 2.
    async fn setup_package_upgraded_in_place()
    -> (BigTableClient, ObjectID, tokio::task::JoinHandle<()>) {
        use bytes::Bytes;
        use move_binary_format::file_format::{Ability, AbilitySet, Visibility, basic_test_module};
        use move_core_types::account_address::AccountAddress;

        let mock = MockBigtableServer::new();
        let (addr, server) = mock.start().await.expect("start mock BigTable");
        let client = BigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");

        let package_id = ObjectID::from_single_byte(0xDD);
        assert!(!sui_types::is_system_package(package_id));
        let mut v1_module = basic_test_module();
        v1_module.address_identifiers[0] = AccountAddress::from(package_id);
        let mut v2_module = v1_module.clone();
        v2_module.datatype_handles[0].abilities = AbilitySet::singleton(Ability::Drop);
        v2_module.function_defs[0].visibility = Visibility::Public;
        for (version, checkpoint, module) in [(1u64, 5u64, v1_module), (2, 10, v2_module)] {
            let object = Object::new_package_from_data(
                Data::Package(MovePackage::new_system(
                    SequenceNumber::from(version),
                    std::slice::from_ref(&module),
                    [],
                )),
                TransactionDigest::genesis_marker(),
            );
            mock.insert_row(
                tables::objects::NAME,
                Bytes::from(tables::objects::encode_key(&ObjectKey(
                    object.id(),
                    object.version(),
                ))),
                tables::objects::encode(&object).expect("encode object"),
            )
            .await;
            mock.insert_row(
                tables::packages::NAME,
                Bytes::from(tables::packages::encode_key(package_id.as_ref(), version)),
                tables::packages::encode(checkpoint, package_id.as_ref(), true),
            )
            .await;
        }

        (client, package_id, server)
    }

    /// A package upgraded in place resolves to the version current at the served height, and is
    /// `NotFound` below its first version, whether or not this binary knows its ID as a system
    /// package's.
    #[tokio::test]
    async fn bare_id_of_package_upgraded_in_place_resolves_the_version_at_served_height() {
        let (client, package_id, _server) = setup_package_upgraded_in_place().await;

        for (served_checkpoint, expected_version) in [(4, None), (9, Some(1)), (10, Some(2))] {
            let served_height = ServedHeight {
                checkpoint: served_checkpoint,
                epoch: 0,
            };
            let result = get_package(limited(&client), served_height, get_package_req(package_id))
                .await
                .map(|response| response.package.and_then(|package| package.version))
                .map_err(|err| tonic::Status::from(err).code());
            let expected = expected_version.map(Some).ok_or(tonic::Code::NotFound);
            assert_eq!(result, expected, "served height {served_checkpoint}");
        }
    }

    /// `GetDatatype` and `GetFunction` on a package upgraded in place describe the version that
    /// bare-ID `GetPackage` resolves at the served height.
    #[tokio::test]
    async fn datatype_and_function_of_package_upgraded_in_place_follow_the_served_height() {
        use sui_rpc::proto::sui::rpc::v2::Ability as ProtoAbility;
        use sui_rpc::proto::sui::rpc::v2::function_descriptor::Visibility;

        let (client, package_id, _server) = setup_package_upgraded_in_place().await;

        for (served_checkpoint, expected) in [
            (4, None),
            (9, Some((vec![], Visibility::Private))),
            (
                10,
                Some((vec![ProtoAbility::Drop as i32], Visibility::Public)),
            ),
        ] {
            let served_height = ServedHeight {
                checkpoint: served_checkpoint,
                epoch: 0,
            };
            let abilities = get_datatype(
                limited(&client),
                served_height,
                get_datatype_req(package_id),
            )
            .await
            .map(|response| response.datatype.expect("datatype is set").abilities)
            .map_err(|err| tonic::Status::from(err).code());
            let visibility = get_function(
                limited(&client),
                served_height,
                get_function_req(package_id),
            )
            .await
            .map(|response| response.function.expect("function is set").visibility)
            .map_err(|err| tonic::Status::from(err).code());
            let context = format!("served height {served_checkpoint}");
            match expected {
                None => {
                    assert_eq!(abilities, Err(tonic::Code::NotFound), "{context}");
                    assert_eq!(visibility, Err(tonic::Code::NotFound), "{context}");
                }
                Some((expected_abilities, expected_visibility)) => {
                    assert_eq!(abilities, Ok(expected_abilities), "{context}");
                    assert_eq!(
                        visibility,
                        Ok(Some(expected_visibility as i32)),
                        "{context}"
                    );
                }
            }
        }
    }

    /// A `packages` row at or below the served height whose object is missing is an inconsistency
    /// of the store, not a package that does not exist: every read that resolves to it is
    /// `Internal`, never `NotFound`.
    #[tokio::test]
    async fn package_version_without_its_object_is_internal() {
        use bytes::Bytes;
        use move_binary_format::file_format::basic_test_module;
        use move_core_types::account_address::AccountAddress;

        let mock = MockBigtableServer::new();
        let (addr, _server) = mock.start().await.expect("start mock BigTable");
        let client = BigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");

        // A package upgraded in place: version 1 at checkpoint 5 and version 2 at checkpoint 10,
        // with only version 2's object stored.
        let package_id = ObjectID::from_single_byte(0xDD);
        let mut module = basic_test_module();
        module.address_identifiers[0] = AccountAddress::from(package_id);
        let v2 = Object::new_package_from_data(
            Data::Package(MovePackage::new_system(
                SequenceNumber::from(2),
                std::slice::from_ref(&module),
                [],
            )),
            TransactionDigest::genesis_marker(),
        );
        mock.insert_row(
            tables::objects::NAME,
            Bytes::from(tables::objects::encode_key(&ObjectKey(
                v2.id(),
                v2.version(),
            ))),
            tables::objects::encode(&v2).expect("encode object"),
        )
        .await;
        for (version, checkpoint) in [(1u64, 5u64), (2, 10)] {
            mock.insert_row(
                tables::packages::NAME,
                Bytes::from(tables::packages::encode_key(package_id.as_ref(), version)),
                tables::packages::encode(checkpoint, package_id.as_ref(), true),
            )
            .await;
        }

        let served_height = ServedHeight {
            checkpoint: 9,
            epoch: 0,
        };
        let mut version = get_package_req(package_id);
        version.selector = Some(Selector::Version(1));
        let mut at_checkpoint = get_package_req(package_id);
        at_checkpoint.selector = Some(Selector::AtCheckpoint(9));
        for (label, req) in [
            ("bare id", get_package_req(package_id)),
            ("version", version),
            ("at_checkpoint", at_checkpoint),
        ] {
            let status: tonic::Status = get_package(limited(&client), served_height, req)
                .await
                .expect_err("version 1's object is missing")
                .into();
            assert_eq!(status.code(), tonic::Code::Internal, "{label}");
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
            let result = get_package(limited(&fixture.client), served_height, req).await;
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
                list_package_versions(limited(&fixture.client), served_height, req)
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
            list_package_versions(limited(&fixture.client), served_height, req)
                .await
                .expect_err("package published above the served height")
                .into();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }
}
