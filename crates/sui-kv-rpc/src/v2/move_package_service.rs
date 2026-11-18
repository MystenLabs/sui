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
use sui_rpc::proto::sui::rpc::v2::get_package_request::Selector;
use sui_rpc::proto::sui::rpc::v2::move_package_service_server::MovePackageService;
use sui_rpc_api::AtCheckpointUpperBound;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_rpc_api::grpc::v2::move_package_service::PackageVersionsQuery;
use sui_rpc_api::grpc::v2::move_package_service::check_checkpoint_range;
use sui_rpc_api::grpc::v2::move_package_service::get_datatype_response;
use sui_rpc_api::grpc::v2::move_package_service::get_function_response;
use sui_rpc_api::grpc::v2::move_package_service::get_package_response;
use sui_rpc_api::grpc::v2::move_package_service::pages_by_token;
use sui_rpc_api::grpc::v2::move_package_service::unknown_package_response;
use sui_rpc_api::grpc::v2::move_package_service::validate_request;
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

/// List the versions of a package published at or below `served_height`, within the request's
/// checkpoint range, as the fullnode lists them (see `PackageVersionsQuery`). BigTable holds
/// full history, so no checkpoint range reaches below what this server retains.
///
/// In cursor mode the package is resolved as of the served height, as the fullnode resolves it
/// as of its highest indexed checkpoint (see [`resolve_package_at_served_height`]), and a package
/// unknown there is answered by `unknown_package_response`. A legacy page-token listing resolves
/// the package whatever checkpoint published it.
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
    let package_id = parse_package_id(package_id_str)?;
    validate_request(&request)?;

    let resolved = if pages_by_token(&request) {
        resolve_original_package_id(client.clone(), package_id)
            .await
            .map(Some)
    } else {
        resolve_package_at_served_height(client.clone(), served_height, package_id).await
    };
    // A page token or cursor naming another lineage than the package's is refused before the
    // checkpoint range is checked, as malformed whatever the range. A package unknown at the
    // served height, or one that failed to resolve, has its range checked first, as on the
    // fullnode: it may have been published above the served height, and answering `NotFound`
    // for it would hide that the range is not served yet.
    let query = match &resolved {
        Ok(Some(original_package_id)) => Some(PackageVersionsQuery::new(
            &request,
            *original_package_id,
            served_height.checkpoint,
        )?),
        Ok(None) | Err(_) => None,
    };
    check_checkpoint_range(&request, served_height.checkpoint)?;
    let (mut query, original_package_id) = match (query, resolved) {
        (Some(query), Ok(Some(original_package_id))) => (query, original_package_id),
        (_, resolved) => {
            return resolved
                .and_then(|_| unknown_package_response(&request, served_height.checkpoint));
        }
    };
    if let Some(end) = query.empty_range_end() {
        return Ok(query.response(vec![], Some(end)));
    }

    // The lineage's newest version published at or before `checkpoint`.
    let version_at = |checkpoint| {
        let client = client.clone();
        async move {
            Ok::<_, RpcError>(
                client
                    .get_package_latest(original_package_id, checkpoint)
                    .await?
                    .map(|pkg| pkg.package_version),
            )
        }
    };
    if let Some(checkpoint) = query.start_checkpoint_lookup() {
        query.bound_by_start_checkpoint(version_at(checkpoint).await?);
    }

    // This server serves a version when the version has a row published at or below the served
    // height. BigTable holds every version from genesis, so a cursor for a version it does not
    // serve lies past its tip. See `PackageVersionsQuery::cursor_versions` for the cursors it
    // checks. The check runs alongside the listing, whose rows are dropped if it fails.
    let cursor_keys: Vec<_> = query
        .cursor_versions()
        .map(|version| (original_package_id, version))
        .collect();
    let check_cursors = async {
        if cursor_keys.is_empty() {
            return Ok(());
        }
        let rows = client.get_packages_by_version(&cursor_keys).await?;
        let served = cursor_keys.iter().all(|(_, version)| {
            rows.iter().any(|pkg| {
                pkg.package_version == *version
                    && pkg.cp_sequence_number <= served_height.checkpoint
            })
        });
        if !served {
            return Err(
                AtCheckpointUpperBound::new(served_height.checkpoint).unindexed_cursor_error()
            );
        }
        Ok(())
    };

    let cp_bound = query.bounding_checkpoint();

    // An ascending scan ends at the first version published after `cp_bound`; a descending scan
    // skips the versions published after it.
    let list_versions = client.get_package_versions(
        original_package_id,
        cp_bound,
        query.versions.clone(),
        query.read_limit(),
        query.descending,
    );

    let ((), rows) = tokio::try_join!(check_cursors, list_versions)?;
    let versions: Vec<_> = rows
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

    // A legacy first page that finds nothing is `NotFound` only when the package has no version
    // published at or below the served height: a package published by then but outside the
    // request's version bounds lists no versions instead. A listing in cursor mode resolved its
    // package at the served height, so the package has a version by then.
    if !query.is_cursor_mode() && query.is_first_page() && versions.is_empty() {
        let published = !client
            .get_package_versions(
                original_package_id,
                served_height.checkpoint,
                0..u64::MAX,
                1,
                false,
            )
            .await?
            .is_empty();
        if !published {
            return Err(RpcError::not_found());
        }
    }

    let end = if query.needs_end_reason(versions.len()) {
        let newest = version_at(cp_bound).await?;
        Some(query.end_reason(newest.map_or(0, |version| version.saturating_add(1))))
    } else {
        None
    };
    Ok(query.response(versions, end))
}

/// The original ID of the package a storage ID names, as of `served_height`, for a listing in
/// cursor mode, or `None` when the package is unknown there, as `load_package_at_served_height`
/// decides it: a storage ID holds one version, known when that version was published at or below
/// the served height, so when the lineage's newest version by then is at or above it; a package
/// upgraded in place, under its original ID, is known when any of its versions is. The original
/// ID comes from `packages_by_id`, which maps every storage ID whatever checkpoint published it,
/// and the lineage's `packages` rows decide. Only a storage ID that is neither the original ID
/// nor the lineage's newest version by the served height costs a read of its object, for its
/// version.
async fn resolve_package_at_served_height(
    client: LimitedBigTableClient,
    served_height: ServedHeight,
    package_id: ObjectID,
) -> Result<Option<ObjectID>, RpcError> {
    let original_id = match resolve_original_package_id(client.clone(), package_id).await {
        Err(error) if error.code() == tonic::Code::NotFound => return Ok(None),
        resolved => resolved?,
    };
    let Some(newest) = client
        .get_package_latest(original_id, served_height.checkpoint)
        .await?
    else {
        return Ok(None);
    };
    if package_id == original_id || newest.package_id == package_id.as_ref() {
        return Ok(Some(original_id));
    }
    let version = match load_package(client, package_id).await {
        Err(error) if error.code() == tonic::Code::NotFound => return Ok(None),
        loaded => loaded?.version().value(),
    };
    Ok((newest.package_version >= version).then_some(original_id))
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

    use bytes::Bytes;
    use sui_kvstore::tables;
    use sui_kvstore::testing::MockBigtableServer;
    use sui_rpc::proto::sui::rpc::v2::Ordering;
    use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
    use sui_rpc::proto::sui::rpc::v2::QueryOptions;
    use sui_rpc_api::grpc::v2::move_package_service::PackageVersionCursor;
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

    /// A listing in cursor mode by a storage ID reads that storage ID's version as of the served
    /// height, as bare-ID `GetPackage` does: v2's storage ID, published at checkpoint 20, is
    /// unknown below it, and a first page is `NotFound`, though the lineage's v1 is listed by
    /// its original ID. At 20, v2's storage ID lists the lineage.
    #[tokio::test]
    async fn list_package_versions_cursor_mode_by_a_storage_id_published_above_served_height() {
        let fixture = setup_lineage_fixture().await;
        let list = |package_id, served_checkpoint| {
            let mut req = list_req(package_id);
            req.options = Some(query_options(None, Ordering::Ascending, None, None));
            let served_height = ServedHeight {
                checkpoint: served_checkpoint,
                epoch: 0,
            };
            list_package_versions(limited(&fixture.client), served_height, req)
        };

        let status: tonic::Status = list(fixture.upgraded_id, 19)
            .await
            .expect_err("v2 is published above the served height")
            .into();
        assert_eq!(status.code(), tonic::Code::NotFound);
        for (package_id, served_checkpoint, expected) in [
            (fixture.original_id, 19, vec![1]),
            (fixture.upgraded_id, 20, vec![1, 2]),
        ] {
            let response = list(package_id, served_checkpoint).await.unwrap();
            assert_eq!(
                listed_versions(&response),
                expected,
                "{package_id} at {served_checkpoint}"
            );
        }
    }

    /// A `package_id` naming an object that is not a package is refused as such only after the
    /// checkpoint range, as on the fullnode: a descending range past the served height is
    /// `ABOVE_LATEST_AVAILABLE`, and an ascending one `INVALID_ARGUMENT`.
    #[tokio::test]
    async fn list_package_versions_checks_the_range_before_a_non_package_object() {
        let fixture = setup_lineage_fixture().await;
        let list = |ordering| {
            let mut req = range_req(fixture.plain_object_id, [None, Some(u64::MAX)]);
            req.options = Some(query_options(None, ordering, None, None));
            list_package_versions(limited(&fixture.client), FIXTURE_SERVED_HEIGHT, req)
        };

        let status: tonic::Status = list(Ordering::Descending).await.unwrap_err().into();
        assert_eq!(status.code(), tonic::Code::OutOfRange);
        assert_eq!(error_reason(&status), "ABOVE_LATEST_AVAILABLE");
        let status: tonic::Status = list(Ordering::Ascending).await.unwrap_err().into();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert!(status.message().contains("not a package"));
    }

    fn list_req(package_id: ObjectID) -> ListPackageVersionsRequest {
        let mut req = ListPackageVersionsRequest::default();
        req.package_id = Some(package_id.to_string());
        req
    }

    /// A request listing `package_id`'s lineage over `[start_checkpoint, end_checkpoint)`.
    fn range_req(
        package_id: ObjectID,
        [start_checkpoint, end_checkpoint]: [Option<u64>; 2],
    ) -> ListPackageVersionsRequest {
        let mut req = list_req(package_id);
        req.start_checkpoint = start_checkpoint;
        req.end_checkpoint = end_checkpoint;
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

    /// How a response's listing ends.
    fn end_reason(response: &ListPackageVersionsResponse) -> Option<QueryEndReason> {
        response.end.as_ref().map(|end| end.reason())
    }

    #[tokio::test]
    async fn list_package_versions_checkpoint_range_bounds_the_lineage() {
        let fixture = setup_lineage_fixture().await;

        // v1 is published at checkpoint 5 and v2 at 20; the served height is 25. Above it, an
        // ascending listing ends at the served height with `LEDGER_TIP`.
        let both = vec![(1, Some(5)), (2, Some(20))];
        for (range, expected, end) in [
            ([None, Some(5)], vec![], QueryEndReason::CheckpointBound),
            (
                [None, Some(6)],
                vec![(1, Some(5))],
                QueryEndReason::CheckpointBound,
            ),
            (
                [None, Some(20)],
                vec![(1, Some(5))],
                QueryEndReason::CheckpointBound,
            ),
            (
                [None, Some(21)],
                both.clone(),
                QueryEndReason::CheckpointBound,
            ),
            (
                [None, Some(26)],
                both.clone(),
                QueryEndReason::CheckpointBound,
            ),
            ([None, Some(27)], both.clone(), QueryEndReason::LedgerTip),
            (
                [None, Some(u64::MAX)],
                both.clone(),
                QueryEndReason::LedgerTip,
            ),
            ([Some(0), None], both.clone(), QueryEndReason::LedgerTip),
            ([Some(5), None], both.clone(), QueryEndReason::LedgerTip),
            (
                [Some(6), None],
                vec![(2, Some(20))],
                QueryEndReason::LedgerTip,
            ),
            (
                [Some(5), Some(20)],
                vec![(1, Some(5))],
                QueryEndReason::CheckpointBound,
            ),
            (
                [Some(21), Some(26)],
                vec![],
                QueryEndReason::CheckpointBound,
            ),
            (
                [Some(20), Some(20)],
                vec![],
                QueryEndReason::CheckpointBound,
            ),
            ([Some(26), None], vec![], QueryEndReason::LedgerTip),
            ([Some(30), Some(40)], vec![], QueryEndReason::LedgerTip),
        ] {
            let response = list_package_versions(
                limited(&fixture.client),
                FIXTURE_SERVED_HEIGHT,
                range_req(fixture.upgraded_id, range),
            )
            .await
            .expect("list_package_versions succeeds");
            assert_eq!(listed(&response), expected, "{range:?}");
            assert_eq!(end_reason(&response), Some(end), "{range:?}");
            assert!(response.next_page_token.is_none());
        }
    }

    /// A descending range past the served height is refused, as `ListTransactions` refuses it,
    /// before a package unknown at the served height is answered `NotFound`: a package published
    /// above the served height may have no rows yet, and `NotFound` would hide that the range is
    /// not served yet.
    #[tokio::test]
    async fn list_package_versions_descending_range_above_served_height_is_rejected() {
        let fixture = setup_lineage_fixture().await;
        // Stands in for a package published at checkpoint 30, above the served height of 25,
        // whose rows this replica has not written yet.
        let published_above_served_height = ObjectID::from_single_byte(0xDD);

        for package_id in [fixture.original_id, published_above_served_height] {
            for (range, bound, requested) in [
                ([None, Some(27)], "end_checkpoint", 27),
                ([Some(3), Some(u64::MAX)], "end_checkpoint", u64::MAX),
                ([Some(26), None], "start_checkpoint", 26),
            ] {
                let mut req = range_req(package_id, range);
                req.options = Some(query_options(None, Ordering::Descending, None, None));
                let status: tonic::Status =
                    list_package_versions(limited(&fixture.client), FIXTURE_SERVED_HEIGHT, req)
                        .await
                        .expect_err("range above the served height")
                        .into();
                assert_eq!(status.code(), tonic::Code::OutOfRange, "{range:?}");
                assert_eq!(error_reason(&status), "ABOVE_LATEST_AVAILABLE");
                assert_eq!(
                    status.message(),
                    format!(
                        "requested data above latest available; {bound} is {requested}, and \
                         only checkpoints below 26 are available"
                    ),
                );
            }
        }

        // The served height itself is served in either order.
        let mut req = range_req(fixture.original_id, [None, Some(26)]);
        req.options = Some(query_options(None, Ordering::Descending, None, None));
        let response = list_package_versions(limited(&fixture.client), FIXTURE_SERVED_HEIGHT, req)
            .await
            .unwrap();
        assert_eq!(listed(&response), vec![(2, Some(20)), (1, Some(5))]);
        assert_eq!(end_reason(&response), Some(QueryEndReason::CheckpointBound));
    }

    #[tokio::test]
    async fn list_package_versions_checkpoint_range_before_first_publish() {
        let fixture = setup_lineage_fixture().await;

        // Published at or below the served height: an empty list.
        let response = list_package_versions(
            limited(&fixture.client),
            FIXTURE_SERVED_HEIGHT,
            range_req(fixture.original_id, [None, Some(5)]),
        )
        .await
        .expect("package is published as of the served height");
        assert!(response.versions.is_empty());
        assert_eq!(end_reason(&response), Some(QueryEndReason::CheckpointBound));

        // An empty range lists nothing in either order, including an `end_checkpoint` of 0,
        // which needs no lookup at the checkpoint before it.
        for range in [[None, Some(0)], [Some(0), Some(0)], [Some(5), Some(5)]] {
            for ordering in [Ordering::Ascending, Ordering::Descending] {
                let mut req = range_req(fixture.original_id, range);
                req.options = Some(query_options(None, ordering, None, None));
                let response =
                    list_package_versions(limited(&fixture.client), FIXTURE_SERVED_HEIGHT, req)
                        .await
                        .unwrap();
                assert!(response.versions.is_empty(), "{range:?} {ordering:?}");
                assert_eq!(
                    end_reason(&response),
                    Some(QueryEndReason::CheckpointBound),
                    "{range:?} {ordering:?}"
                );
            }
        }

        // Not yet published as of the served height: the package is unknown there, and a first
        // page is `NotFound`, as `GetPackage` answers.
        let served_height = ServedHeight {
            checkpoint: 4,
            epoch: 0,
        };
        let status: tonic::Status = list_package_versions(
            limited(&fixture.client),
            served_height,
            range_req(fixture.original_id, [None, Some(4)]),
        )
        .await
        .expect_err("the package is unknown at the served height")
        .into();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    /// A checkpoint range pages by cursor: `page_size` and `page_token` with one are
    /// `INVALID_ARGUMENT`, as is an `end_checkpoint` below `start_checkpoint` or an `end_version`
    /// below `start_version`.
    #[tokio::test]
    async fn list_package_versions_checkpoint_range_pages_by_cursor() {
        let fixture = setup_lineage_fixture().await;
        let list =
            |req| list_package_versions(limited(&fixture.client), FIXTURE_SERVED_HEIGHT, req);

        let mut legacy = list_req(fixture.original_id);
        legacy.page_size = Some(1);
        let page_token = list(legacy).await.unwrap().next_page_token.unwrap();
        let mut with_page_size = range_req(fixture.original_id, [None, Some(21)]);
        with_page_size.page_size = Some(1);
        let mut with_page_token = range_req(fixture.original_id, [Some(3), None]);
        with_page_token.page_token = Some(page_token);
        let mut versions_backwards = range_req(fixture.original_id, [None, None]);
        versions_backwards.start_version = Some(2);
        versions_backwards.end_version = Some(1);
        for (req, expected) in [
            (
                with_page_size,
                "page_size cannot be combined with start_checkpoint or end_checkpoint",
            ),
            (
                with_page_token,
                "page_token cannot be combined with start_checkpoint or end_checkpoint",
            ),
            (
                range_req(fixture.original_id, [Some(6), Some(5)]),
                "end_checkpoint must be greater than or equal to start_checkpoint",
            ),
            (
                versions_backwards,
                "end_version must be greater than or equal to start_version",
            ),
        ] {
            let status: tonic::Status = list(req).await.unwrap_err().into();
            assert_eq!(status.code(), tonic::Code::InvalidArgument, "{expected}");
            assert!(
                status.message().contains(expected),
                "expected {expected:?}, got {:?}",
                status.message()
            );
        }

        // One version per page, from each page's cursor.
        let page = |after: Option<&Bytes>| {
            let mut req = range_req(fixture.original_id, [None, Some(21)]);
            req.options = Some(query_options(Some(1), Ordering::Ascending, after, None));
            list(req)
        };
        let first = page(None).await.unwrap();
        assert_eq!(listed(&first), vec![(1, Some(5))]);
        assert_eq!(end_reason(&first), Some(QueryEndReason::ItemLimit));
        assert!(first.next_page_token.is_none());
        let second = page(first.versions[0].cursor.as_ref()).await.unwrap();
        assert_eq!(listed(&second), vec![(2, Some(20))]);
        assert_eq!(end_reason(&second), Some(QueryEndReason::ItemLimit));
        let last = page(second.versions[0].cursor.as_ref()).await.unwrap();
        assert_eq!(listed(&last), vec![]);
        assert_eq!(end_reason(&last), Some(QueryEndReason::CheckpointBound));
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
        list_package_versions(
            limited(&fixture.client),
            VERSIONS_FIXTURE_SERVED_HEIGHT,
            req,
        )
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

    /// Page through the versions fixture's lineage from `req`, each page after (ascending) or
    /// before (descending) the last version received, until a page comes back shorter than the
    /// request's limit. Returns each page's versions and how it ends.
    async fn page_through_versions(
        fixture: &VersionsFixture,
        mut req: ListPackageVersionsRequest,
    ) -> Vec<(Vec<u64>, QueryEndReason)> {
        let mut pages = vec![];
        loop {
            let response = list_versions_fixture(fixture, req.clone()).await.unwrap();
            assert!(response.next_page_token.is_none());
            pages.push((
                listed_versions(&response),
                end_reason(&response).expect("a cursor-mode page sets end"),
            ));
            let options = req.options.as_mut().expect("paging needs options");
            if response.versions.len() < options.limit.expect("paging needs a limit") as usize {
                return pages;
            }
            let last_cursor = response.versions.last().and_then(|v| v.cursor.clone());
            if options.ordering == Some(Ordering::Ascending as i32) {
                options.after = last_cursor;
            } else {
                options.before = last_cursor;
            }
        }
    }

    /// The versions of `pages`, in order.
    fn paged_versions(pages: &[(Vec<u64>, QueryEndReason)]) -> Vec<u64> {
        pages
            .iter()
            .flat_map(|(versions, _)| versions.clone())
            .collect()
    }

    /// An ascending page that starts mid-lineage, then descending pages before its first
    /// version: they list the versions below the page, newest first, and with the ascending
    /// pages cover the lineage exactly once.
    #[tokio::test]
    async fn list_package_versions_reverses_from_ascending_to_descending() {
        let fixture = setup_versions_fixture().await;
        let cursors = fixture_cursors(&fixture).await;
        let req = |limit, ordering, after: Option<&Bytes>, before: Option<&Bytes>| {
            cursor_req(
                fixture.original_id,
                query_options(limit, ordering, after, before),
            )
        };

        let page = list_versions_fixture(
            &fixture,
            req(Some(2), Ordering::Ascending, Some(&cursors[&2]), None),
        )
        .await
        .unwrap();
        assert_eq!(listed_versions(&page), vec![3, 4]);
        assert_eq!(end_reason(&page), Some(QueryEndReason::ItemLimit));
        let first = page.versions.first().and_then(|v| v.cursor.as_ref());
        let last = page.versions.last().and_then(|v| v.cursor.as_ref());

        let backward =
            page_through_versions(&fixture, req(Some(1), Ordering::Descending, None, first)).await;
        assert_eq!(
            backward,
            vec![
                (vec![2], QueryEndReason::ItemLimit),
                (vec![1], QueryEndReason::ItemLimit),
                (vec![], QueryEndReason::CheckpointBound),
            ]
        );
        let forward =
            page_through_versions(&fixture, req(Some(2), Ordering::Ascending, last, None)).await;
        assert_eq!(forward, vec![(vec![5], QueryEndReason::LedgerTip)]);

        let mut listing = paged_versions(&backward);
        listing.reverse();
        listing.extend(listed_versions(&page));
        listing.extend(paged_versions(&forward));
        assert_eq!(listing, vec![1, 2, 3, 4, 5]);

        // A cursor carries no direction: descending before the page's last version lists the
        // rest of the page again.
        let relisted = list_versions_fixture(&fixture, req(None, Ordering::Descending, None, last))
            .await
            .unwrap();
        assert_eq!(listed_versions(&relisted), vec![3, 2, 1]);
        assert_eq!(end_reason(&relisted), Some(QueryEndReason::CheckpointBound));
    }

    /// A descending page that starts mid-lineage, then ascending pages after its first (highest)
    /// version: they list the versions above the page, oldest first, and with the descending
    /// pages cover the lineage exactly once.
    #[tokio::test]
    async fn list_package_versions_reverses_from_descending_to_ascending() {
        let fixture = setup_versions_fixture().await;
        let cursors = fixture_cursors(&fixture).await;
        let req = |limit, ordering, after: Option<&Bytes>, before: Option<&Bytes>| {
            cursor_req(
                fixture.original_id,
                query_options(limit, ordering, after, before),
            )
        };

        let page = list_versions_fixture(
            &fixture,
            req(Some(2), Ordering::Descending, None, Some(&cursors[&4])),
        )
        .await
        .unwrap();
        assert_eq!(listed_versions(&page), vec![3, 2]);
        assert_eq!(end_reason(&page), Some(QueryEndReason::ItemLimit));
        let first = page.versions.first().and_then(|v| v.cursor.as_ref());
        let last = page.versions.last().and_then(|v| v.cursor.as_ref());

        let forward =
            page_through_versions(&fixture, req(Some(1), Ordering::Ascending, first, None)).await;
        assert_eq!(
            forward,
            vec![
                (vec![4], QueryEndReason::ItemLimit),
                (vec![5], QueryEndReason::ItemLimit),
                (vec![], QueryEndReason::LedgerTip),
            ]
        );
        let backward =
            page_through_versions(&fixture, req(Some(2), Ordering::Descending, None, last)).await;
        assert_eq!(backward, vec![(vec![1], QueryEndReason::CheckpointBound)]);

        let mut listing = listed_versions(&page);
        listing.extend(paged_versions(&backward));
        listing.reverse();
        listing.extend(paged_versions(&forward));
        assert_eq!(listing, vec![1, 2, 3, 4, 5]);

        // A cursor carries no direction: ascending after the page's last (lowest) version lists
        // the rest of the page again.
        let relisted = list_versions_fixture(&fixture, req(None, Ordering::Ascending, last, None))
            .await
            .unwrap();
        assert_eq!(listed_versions(&relisted), vec![3, 4, 5]);
        assert_eq!(end_reason(&relisted), Some(QueryEndReason::LedgerTip));
    }

    /// Reversing from the first version of a page listed under one checkpoint range, under
    /// another range: the reversed page lists what both the new range and the cursor admit.
    /// Version `n` is published at checkpoint `10 * n`, and the served height is 60.
    #[tokio::test]
    async fn list_package_versions_reverses_from_cursors_minted_under_another_checkpoint_range() {
        use QueryEndReason::{CheckpointBound, ItemLimit, LedgerTip};
        let fixture = setup_versions_fixture().await;
        let cursors = fixture_cursors(&fixture).await;

        // Each case mints a page in one order (from the cursor of `from`, if set, on its paging
        // side), then lists in the other order from the minted page's first version under each
        // range, with the expected versions and end.
        for (ordering, mint_range, from, minted, reversals) in [
            (
                Ordering::Ascending,
                [None, None],
                Some(2),
                vec![3, 4],
                vec![
                    // The cursor is inside the new range.
                    ([None, Some(36)], None, vec![2, 1], CheckpointBound),
                    ([None, Some(36)], Some(1), vec![2], ItemLimit),
                    // The cursor is above the new range.
                    ([None, Some(25)], None, vec![2, 1], CheckpointBound),
                    ([None, Some(15)], None, vec![1], CheckpointBound),
                    ([Some(15), Some(61)], None, vec![2], CheckpointBound),
                    // The cursor is below the new range.
                    ([Some(35), None], None, vec![], CheckpointBound),
                ],
            ),
            (
                Ordering::Descending,
                [None, None],
                Some(5),
                vec![4, 3],
                vec![
                    ([None, Some(61)], None, vec![5], CheckpointBound),
                    // The cursor is inside the new range, which holds nothing after it.
                    ([None, Some(46)], None, vec![], CheckpointBound),
                    // The cursor is above the new range.
                    ([None, Some(36)], None, vec![], CheckpointBound),
                    ([Some(45), None], None, vec![5], LedgerTip),
                    // The cursor is below the new range, which holds nothing.
                    ([Some(55), None], None, vec![], LedgerTip),
                ],
            ),
            (
                Ordering::Ascending,
                [None, Some(46)],
                Some(1),
                vec![2, 3],
                vec![
                    ([None, None], None, vec![1], CheckpointBound),
                    ([Some(15), None], None, vec![], CheckpointBound),
                    ([Some(5), Some(26)], None, vec![1], CheckpointBound),
                ],
            ),
            (
                Ordering::Descending,
                [Some(20), Some(46)],
                None,
                vec![4, 3],
                vec![
                    ([None, None], None, vec![5], LedgerTip),
                    ([Some(20), Some(46)], None, vec![], CheckpointBound),
                    ([Some(45), Some(51)], None, vec![5], CheckpointBound),
                ],
            ),
        ] {
            let from = from.map(|v| &cursors[&v]);
            let (after, before) = match ordering {
                Ordering::Ascending => (from, None),
                _ => (None, from),
            };
            let mut req = cursor_req(
                fixture.original_id,
                query_options(Some(2), ordering, after, before),
            );
            [req.start_checkpoint, req.end_checkpoint] = mint_range;
            let page = list_versions_fixture(&fixture, req).await.unwrap();
            assert_eq!(
                listed_versions(&page),
                minted,
                "{ordering:?} {mint_range:?}"
            );
            assert_eq!(end_reason(&page), Some(QueryEndReason::ItemLimit));
            let pivot = page.versions[0].cursor.as_ref();

            for (range, limit, expected, end) in reversals {
                let options = match ordering {
                    Ordering::Ascending => query_options(limit, Ordering::Descending, None, pivot),
                    _ => query_options(limit, Ordering::Ascending, pivot, None),
                };
                let mut req = cursor_req(fixture.original_id, options);
                [req.start_checkpoint, req.end_checkpoint] = range;
                let response = list_versions_fixture(&fixture, req).await.unwrap();
                let case =
                    format!("minted {ordering:?} under {mint_range:?}, reversed under {range:?}");
                assert_eq!(listed_versions(&response), expected, "{case}");
                assert_eq!(end_reason(&response), Some(end), "{case}");
            }
        }
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
    async fn list_package_versions_intersects_version_bounds_cursors_and_checkpoint_range() {
        let fixture = setup_versions_fixture().await;
        let cursors = fixture_cursors(&fixture).await;

        // Version `n` is published at checkpoint `10 * n`. Each case lists the matching versions
        // in ascending order; descending lists them in reverse.
        for (start_version, end_version, after, before, range, ascending) in [
            (Some(2), Some(5), None, None, [None, None], vec![2, 3, 4]),
            (Some(2), Some(5), Some(3), None, [None, None], vec![4]),
            (Some(2), None, None, Some(4), [None, None], vec![2, 3]),
            (None, Some(5), Some(1), None, [None, Some(36)], vec![2, 3]),
            (Some(3), None, None, None, [None, Some(46)], vec![3, 4]),
            (None, None, None, None, [None, Some(36)], vec![1, 2, 3]),
            (None, None, None, None, [Some(15), None], vec![2, 3, 4, 5]),
            (None, None, None, None, [Some(20), Some(41)], vec![2, 3, 4]),
            (Some(3), None, None, None, [Some(20), None], vec![3, 4, 5]),
            (None, None, Some(3), None, [Some(20), Some(46)], vec![4]),
            (None, None, None, Some(3), [Some(21), None], vec![]),
            // The package is published, so bounds that exclude every version list nothing.
            (Some(4), None, None, None, [None, Some(36)], vec![]),
            (Some(6), None, None, None, [None, None], vec![]),
            (None, None, None, None, [Some(51), Some(61)], vec![]),
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
                [req.start_checkpoint, req.end_checkpoint] = range;
                let response = list_versions_fixture(&fixture, req).await.unwrap();
                assert_eq!(
                    listed_versions(&response),
                    expected,
                    "versions [{start_version:?}, {end_version:?}) after {after:?} before \
                     {before:?} checkpoints {range:?} {ordering:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn list_package_versions_descending_skips_versions_after_the_range() {
        let fixture = setup_versions_fixture().await;

        // Versions 4 and 5 are published at or after checkpoint 36, so a descending page starts
        // at 3 and still fills up.
        let mut req = cursor_req(
            fixture.original_id,
            query_options(Some(2), Ordering::Descending, None, None),
        );
        req.end_checkpoint = Some(36);
        let first = list_versions_fixture(&fixture, req).await.unwrap();
        assert_eq!(listed_versions(&first), vec![3, 2]);
        assert_eq!(end_reason(&first), Some(QueryEndReason::ItemLimit));

        let mut req = cursor_req(
            fixture.original_id,
            query_options(
                Some(2),
                Ordering::Descending,
                None,
                first.versions[1].cursor.as_ref(),
            ),
        );
        req.end_checkpoint = Some(36);
        let second = list_versions_fixture(&fixture, req).await.unwrap();
        assert_eq!(listed_versions(&second), vec![1]);
        assert_eq!(end_reason(&second), Some(QueryEndReason::CheckpointBound));
    }

    /// Each way a cursor-mode listing ends, and a legacy page-token listing, which sets no
    /// `end`. Version `n` is published at checkpoint `10 * n`, and the served height is 60.
    #[tokio::test]
    async fn list_package_versions_ends_with_the_reason_of_its_bound() {
        let fixture = setup_versions_fixture().await;
        let cursors = fixture_cursors(&fixture).await;
        let request = |limit, ordering, after: Option<u64>, before: Option<u64>, range| {
            let mut req = cursor_req(
                fixture.original_id,
                query_options(
                    limit,
                    ordering,
                    after.map(|v| &cursors[&v]),
                    before.map(|v| &cursors[&v]),
                ),
            );
            [req.start_checkpoint, req.end_checkpoint] = range;
            req
        };
        let mut end_version = request(None, Ordering::Ascending, None, None, [None, None]);
        end_version.end_version = Some(3);
        let mut start_version = request(None, Ordering::Descending, None, None, [None, None]);
        start_version.start_version = Some(3);

        for (req, expected, end) in [
            (
                request(Some(2), Ordering::Ascending, None, None, [None, None]),
                vec![1, 2],
                QueryEndReason::ItemLimit,
            ),
            (
                request(Some(2), Ordering::Descending, None, None, [None, None]),
                vec![5, 4],
                QueryEndReason::ItemLimit,
            ),
            (
                request(None, Ordering::Ascending, None, None, [None, None]),
                vec![1, 2, 3, 4, 5],
                QueryEndReason::LedgerTip,
            ),
            (
                request(None, Ordering::Ascending, None, None, [None, Some(36)]),
                vec![1, 2, 3],
                QueryEndReason::CheckpointBound,
            ),
            (
                request(None, Ordering::Ascending, None, None, [None, Some(61)]),
                vec![1, 2, 3, 4, 5],
                QueryEndReason::CheckpointBound,
            ),
            (
                request(None, Ordering::Ascending, None, None, [None, Some(62)]),
                vec![1, 2, 3, 4, 5],
                QueryEndReason::LedgerTip,
            ),
            (end_version, vec![1, 2], QueryEndReason::CheckpointBound),
            (
                request(None, Ordering::Ascending, None, Some(3), [None, None]),
                vec![1, 2],
                QueryEndReason::CursorBound,
            ),
            (
                request(None, Ordering::Ascending, Some(5), None, [None, None]),
                vec![],
                QueryEndReason::LedgerTip,
            ),
            (
                request(None, Ordering::Ascending, None, None, [Some(61), None]),
                vec![],
                QueryEndReason::LedgerTip,
            ),
            (
                request(None, Ordering::Descending, None, None, [None, None]),
                vec![5, 4, 3, 2, 1],
                QueryEndReason::CheckpointBound,
            ),
            (
                start_version,
                vec![5, 4, 3],
                QueryEndReason::CheckpointBound,
            ),
            (
                request(None, Ordering::Descending, None, None, [Some(25), None]),
                vec![5, 4, 3],
                QueryEndReason::CheckpointBound,
            ),
            (
                request(None, Ordering::Descending, Some(2), None, [None, None]),
                vec![5, 4, 3],
                QueryEndReason::CursorBound,
            ),
        ] {
            let response = list_versions_fixture(&fixture, req.clone()).await.unwrap();
            assert_eq!(listed_versions(&response), expected, "{req:?}");
            assert_eq!(end_reason(&response), Some(end), "{req:?}");
            assert!(response.next_page_token.is_none());
        }

        let mut req = list_req(fixture.original_id);
        req.page_size = Some(4);
        let first = list_versions_fixture(&fixture, req.clone()).await.unwrap();
        assert_eq!(listed_versions(&first), vec![1, 2, 3, 4]);
        assert_eq!(first.end, None);
        req.page_token = first.next_page_token;
        let last = list_versions_fixture(&fixture, req).await.unwrap();
        assert_eq!(listed_versions(&last), vec![5]);
        assert_eq!((last.end, last.next_page_token), (None, None));
    }

    #[tokio::test]
    async fn list_package_versions_cursors_past_the_served_height() {
        let fixture = setup_versions_fixture().await;
        // Cursors from a replica serving every version, and one for a version with no row.
        let cursors = fixture_cursors(&fixture).await;
        let unpublished = PackageVersionCursor::Lineage {
            original_package_id: fixture.original_id,
            version: 6,
            minted_at: VERSIONS_FIXTURE_SERVED_HEIGHT.checkpoint,
        }
        .encode();
        // Version `n` is published at checkpoint `10 * n`, so this replica serves versions 1 to 3.
        let served_height = ServedHeight {
            checkpoint: 35,
            epoch: 0,
        };
        let list = |options, end_checkpoint| {
            let mut req = cursor_req(fixture.original_id, options);
            req.end_checkpoint = end_checkpoint;
            list_package_versions(limited(&fixture.client), served_height, req)
        };

        // A descending page from a cursor this replica does not serve would skip the versions
        // it lacks below that cursor, so without `end_checkpoint` it is rejected, also when the
        // other cursor names a version it serves.
        for cursor in [&cursors[&4], &cursors[&5], &unpublished] {
            for (after, before) in [
                (Some(cursor), None),
                (None, Some(cursor)),
                (Some(&cursors[&1]), Some(cursor)),
            ] {
                let status: tonic::Status = list(
                    query_options(None, Ordering::Descending, after, before),
                    None,
                )
                .await
                .expect_err("cursor this replica does not serve")
                .into();
                assert_eq!(status.code(), tonic::Code::OutOfRange);
                assert_eq!(error_reason(&status), "ABOVE_LATEST_AVAILABLE");
                assert_eq!(
                    status.message(),
                    "requested data above latest available; cursor is at checkpoint 36, \
                     and only checkpoints below 36 are available"
                );
            }
        }

        // An ascending page, or a descending one with `end_checkpoint` indexed here, lists what
        // this replica serves. Ascending without a closer bound, it ends at the served height
        // with `LEDGER_TIP`, also when its `after` is past it. Before a cursor this replica does
        // not serve, it ends with `CURSOR_BOUND` only when this replica serves every version
        // below the cursor (before version 4, newest served version 3), and otherwise is cut at
        // the served height with `LEDGER_TIP` (before version 5).
        for (ordering, after, before, end_checkpoint, expected, end) in [
            (
                Ordering::Descending,
                None,
                Some(&cursors[&5]),
                Some(36),
                vec![3, 2, 1],
                QueryEndReason::CheckpointBound,
            ),
            (
                Ordering::Descending,
                Some(&cursors[&4]),
                None,
                Some(36),
                vec![],
                QueryEndReason::CursorBound,
            ),
            (
                Ordering::Ascending,
                Some(&cursors[&4]),
                None,
                None,
                vec![],
                QueryEndReason::LedgerTip,
            ),
            (
                Ordering::Ascending,
                Some(&unpublished),
                None,
                Some(70),
                vec![],
                QueryEndReason::LedgerTip,
            ),
            (
                Ordering::Ascending,
                Some(&cursors[&4]),
                Some(&cursors[&5]),
                None,
                vec![],
                QueryEndReason::CursorBound,
            ),
            (
                Ordering::Ascending,
                None,
                Some(&cursors[&5]),
                None,
                vec![1, 2, 3],
                QueryEndReason::LedgerTip,
            ),
            (
                Ordering::Ascending,
                None,
                Some(&cursors[&4]),
                None,
                vec![1, 2, 3],
                QueryEndReason::CursorBound,
            ),
            (
                Ordering::Ascending,
                Some(&cursors[&1]),
                Some(&unpublished),
                None,
                vec![2, 3],
                QueryEndReason::LedgerTip,
            ),
            (
                Ordering::Ascending,
                Some(&cursors[&4]),
                None,
                Some(36),
                vec![],
                QueryEndReason::CheckpointBound,
            ),
            (
                Ordering::Descending,
                None,
                Some(&cursors[&3]),
                None,
                vec![2, 1],
                QueryEndReason::CheckpointBound,
            ),
            (
                Ordering::Descending,
                Some(&cursors[&1]),
                Some(&cursors[&3]),
                None,
                vec![2],
                QueryEndReason::CursorBound,
            ),
            (
                Ordering::Ascending,
                Some(&cursors[&1]),
                None,
                None,
                vec![2, 3],
                QueryEndReason::LedgerTip,
            ),
            (
                Ordering::Ascending,
                None,
                Some(&cursors[&3]),
                None,
                vec![1, 2],
                QueryEndReason::CursorBound,
            ),
            (
                Ordering::Ascending,
                Some(&cursors[&3]),
                None,
                None,
                vec![],
                QueryEndReason::LedgerTip,
            ),
        ] {
            let response = list(query_options(None, ordering, after, before), end_checkpoint)
                .await
                .unwrap();
            assert_eq!(
                listed_versions(&response),
                expected,
                "{ordering:?} after {after:?} before {before:?} end {end_checkpoint:?}"
            );
            assert_eq!(
                end_reason(&response),
                Some(end),
                "{ordering:?} after {after:?} before {before:?} end {end_checkpoint:?}"
            );
        }

        // No lineage has a version 0, so a cursor naming it skips nothing and is not checked:
        // `after` lists from version 1, and `before` lists nothing.
        let version_zero = PackageVersionCursor::Lineage {
            original_package_id: fixture.original_id,
            version: 0,
            minted_at: served_height.checkpoint,
        }
        .encode();
        for ordering in [Ordering::Ascending, Ordering::Descending] {
            for (after, before, expected) in [
                (Some(&version_zero), None, vec![1, 2, 3]),
                (None, Some(&version_zero), vec![]),
            ] {
                let response = list(query_options(None, ordering, after, before), None)
                    .await
                    .unwrap();
                let mut expected = expected;
                if ordering == Ordering::Descending {
                    expected.reverse();
                }
                assert_eq!(
                    listed_versions(&response),
                    expected,
                    "{ordering:?} after {after:?} before {before:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn list_package_versions_page_tokens_past_the_served_height_list_nothing() {
        let fixture = setup_versions_fixture().await;
        // Page tokens from a replica serving every version, naming versions 2 to 5.
        let mut page_tokens = BTreeMap::new();
        let mut page_token = None;
        loop {
            let mut req = list_req(fixture.original_id);
            req.page_size = Some(1);
            req.page_token = page_token.take();
            let page = list_versions_fixture(&fixture, req).await.unwrap();
            let version = listed_versions(&page)[0] + 1;
            let Some(next) = page.next_page_token else {
                break;
            };
            page_tokens.insert(version, next.clone());
            page_token = Some(next);
        }
        assert_eq!(
            page_tokens.keys().copied().collect::<Vec<_>>(),
            vec![2, 3, 4, 5]
        );
        // Version `n` is published at checkpoint `10 * n`, so this replica serves versions 1 to 3.
        let served_height = ServedHeight {
            checkpoint: 35,
            epoch: 0,
        };
        let list = |page_token: &Bytes| {
            let mut req = list_req(fixture.original_id);
            req.page_size = Some(1);
            req.page_token = Some(page_token.clone());
            list_package_versions(limited(&fixture.client), served_height, req)
        };

        // A page from a version this replica does not serve lists nothing and carries no next
        // page token or `end`, as on servers that predate cursor paging.
        for version in [4, 5] {
            let page = list(&page_tokens[&version]).await.unwrap();
            assert_eq!(
                listed_versions(&page),
                Vec::<u64>::new(),
                "version {version}"
            );
            assert_eq!(
                (page.next_page_token, page.end),
                (None, None),
                "version {version}"
            );
        }

        // A page from a version it serves lists from there.
        let page = list(&page_tokens[&3]).await.unwrap();
        assert_eq!(listed_versions(&page), vec![3]);
        assert!(page.next_page_token.is_none());
    }

    /// A listing in cursor mode resolves the package as of the served height, as `GetPackage`
    /// does: a first page for a package that never existed, or one whose row is in BigTable but
    /// published above the served height, is `NotFound`; one published at or below it is listed.
    /// A legacy page-token listing keeps resolving the package whatever checkpoint published it.
    #[tokio::test]
    async fn list_package_versions_cursor_mode_resolves_packages_at_the_served_height() {
        let fixture = setup_versions_fixture().await;
        let first_page = |package_id, ordering| {
            cursor_req(package_id, query_options(None, ordering, None, None))
        };

        let status = list_versions_fixture(
            &fixture,
            first_page(ObjectID::from_single_byte(0xDD), Ordering::Ascending),
        )
        .await
        .unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);

        // Version 1 is published at checkpoint 10.
        let served_height = ServedHeight {
            checkpoint: 9,
            epoch: 0,
        };
        for ordering in [Ordering::Ascending, Ordering::Descending] {
            let status: tonic::Status = list_package_versions(
                limited(&fixture.client),
                served_height,
                first_page(fixture.original_id, ordering),
            )
            .await
            .unwrap_err()
            .into();
            assert_eq!(status.code(), tonic::Code::NotFound, "{ordering:?}");
        }
        let mut legacy = list_req(fixture.original_id);
        legacy.page_size = Some(10);
        let status: tonic::Status =
            list_package_versions(limited(&fixture.client), served_height, legacy)
                .await
                .unwrap_err()
                .into();
        assert_eq!(status.code(), tonic::Code::NotFound);

        let response = list_versions_fixture(
            &fixture,
            first_page(fixture.original_id, Ordering::Ascending),
        )
        .await
        .unwrap();
        assert_eq!(listed_versions(&response), vec![1, 2, 3, 4, 5]);
        assert_eq!(end_reason(&response), Some(QueryEndReason::LedgerTip));
    }

    /// A request with a cursor for a package unknown at the served height is decided by the
    /// cursor's `minted_at`. This replica serves checkpoint 9, below version 1's publish at 10.
    /// The fixture's cursors were minted at 60 by a replica ahead of it: ascending, the request
    /// gets an empty page ending as a listing with nothing at this height ends; descending with
    /// the range reaching past the served height, `ABOVE_LATEST_AVAILABLE` for the cursor at 60.
    /// A cursor minted at or below 9 is stale or forged: `NotFound`. A package published within
    /// the served height lists the same whatever its cursors' `minted_at`.
    #[tokio::test]
    async fn list_package_versions_cursor_for_an_unknown_package_is_decided_by_its_minting_height()
    {
        let fixture = setup_versions_fixture().await;
        let cursors = fixture_cursors(&fixture).await;
        let minted = |version, minted_at| {
            PackageVersionCursor::Lineage {
                original_package_id: fixture.original_id,
                version,
                minted_at,
            }
            .encode()
        };
        let lagging = ServedHeight {
            checkpoint: 9,
            epoch: 0,
        };
        let list = |served_height, options, end_checkpoint| {
            let mut req = cursor_req(fixture.original_id, options);
            req.end_checkpoint = end_checkpoint;
            list_package_versions(limited(&fixture.client), served_height, req)
        };

        for (after, before, end_checkpoint, expected) in [
            (Some(&cursors[&1]), None, None, QueryEndReason::LedgerTip),
            (None, Some(&cursors[&2]), None, QueryEndReason::LedgerTip),
            (
                Some(&cursors[&1]),
                Some(&cursors[&2]),
                None,
                QueryEndReason::CursorBound,
            ),
            (
                Some(&cursors[&1]),
                None,
                Some(10),
                QueryEndReason::CheckpointBound,
            ),
        ] {
            let response = list(
                lagging,
                query_options(None, Ordering::Ascending, after, before),
                end_checkpoint,
            )
            .await
            .unwrap();
            assert_eq!(listed_versions(&response), Vec::<u64>::new());
            assert_eq!(
                end_reason(&response),
                Some(expected),
                "after {after:?} before {before:?} end {end_checkpoint:?}"
            );
        }
        for (after, before) in [(Some(&cursors[&1]), None), (None, Some(&cursors[&2]))] {
            let status: tonic::Status = list(
                lagging,
                query_options(None, Ordering::Descending, after, before),
                None,
            )
            .await
            .expect_err("cursor minted above the served height")
            .into();
            assert_eq!(status.code(), tonic::Code::OutOfRange);
            assert_eq!(error_reason(&status), "ABOVE_LATEST_AVAILABLE");
            assert_eq!(
                status.message(),
                "requested data above latest available; cursor is at checkpoint 60, and only \
                 checkpoints below 10 are available"
            );
        }

        for minted_at in [0, lagging.checkpoint] {
            let stale = minted(1, minted_at);
            for ordering in [Ordering::Ascending, Ordering::Descending] {
                for (after, before) in [(Some(&stale), None), (None, Some(&stale))] {
                    let status: tonic::Status =
                        list(lagging, query_options(None, ordering, after, before), None)
                            .await
                            .expect_err("cursor minted at or below the served height")
                            .into();
                    assert_eq!(
                        status.code(),
                        tonic::Code::NotFound,
                        "{ordering:?} minted at {minted_at}"
                    );
                }
            }
        }

        for minted_at in [
            0,
            lagging.checkpoint,
            VERSIONS_FIXTURE_SERVED_HEIGHT.checkpoint,
        ] {
            for (ordering, after, before, expected, end) in [
                (
                    Ordering::Ascending,
                    Some(minted(2, minted_at)),
                    None,
                    vec![3, 4, 5],
                    QueryEndReason::LedgerTip,
                ),
                (
                    Ordering::Descending,
                    None,
                    Some(minted(4, minted_at)),
                    vec![3, 2, 1],
                    QueryEndReason::CheckpointBound,
                ),
            ] {
                let response = list(
                    VERSIONS_FIXTURE_SERVED_HEIGHT,
                    query_options(None, ordering, after.as_ref(), before.as_ref()),
                    None,
                )
                .await
                .unwrap();
                assert_eq!(
                    listed_versions(&response),
                    expected,
                    "minted at {minted_at}"
                );
                assert_eq!(end_reason(&response), Some(end), "minted at {minted_at}");
            }
        }
    }

    /// An ascending `after` past the served height ends with the tighter bound's reason when a
    /// bound leaves nothing above it, as `ListObjectVersions` ends it: `end_version` at or below
    /// the version after `after` ends it with `CHECKPOINT_BOUND`, and `before` there, at or below
    /// `end_version`, with `CURSOR_BOUND`. Otherwise it ends with `LEDGER_TIP`.
    #[tokio::test]
    async fn list_package_versions_after_past_the_served_height_ends_at_the_tighter_bound() {
        let fixture = setup_versions_fixture().await;
        let cursors = fixture_cursors(&fixture).await;
        // Version `n` is published at checkpoint `10 * n`, so this replica serves versions 1 to 3.
        let served_height = ServedHeight {
            checkpoint: 35,
            epoch: 0,
        };
        for (before, end_version, expected) in [
            (None, Some(5), QueryEndReason::CheckpointBound),
            (None, Some(6), QueryEndReason::LedgerTip),
            (Some(&cursors[&5]), Some(4), QueryEndReason::CheckpointBound),
            (Some(&cursors[&5]), Some(5), QueryEndReason::CursorBound),
            (Some(&cursors[&5]), Some(6), QueryEndReason::CursorBound),
            (Some(&cursors[&5]), None, QueryEndReason::CursorBound),
        ] {
            let mut req = cursor_req(
                fixture.original_id,
                query_options(None, Ordering::Ascending, Some(&cursors[&4]), before),
            );
            req.end_version = end_version;
            let response = list_package_versions(limited(&fixture.client), served_height, req)
                .await
                .unwrap();
            assert_eq!(listed_versions(&response), Vec::<u64>::new());
            assert_eq!(
                end_reason(&response),
                Some(expected),
                "before {before:?} end_version {end_version:?}"
            );
        }
    }

    #[tokio::test]
    async fn list_package_versions_rejects_invalid_cursor_requests() {
        let fixture = setup_versions_fixture().await;

        let mut legacy = list_req(fixture.original_id);
        legacy.page_size = Some(1);
        let page_token = list_versions_fixture(&fixture, legacy)
            .await
            .unwrap()
            .next_page_token
            .expect("more versions follow");
        let other_package = PackageVersionCursor::Lineage {
            original_package_id: ObjectID::from_single_byte(0xDD),
            version: 1,
            minted_at: VERSIONS_FIXTURE_SERVED_HEIGHT.checkpoint,
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
        // A descending range up to `u64::MAX` reaches past the served height, and is refused for
        // its cursor first: one that does not decode, or one naming another package's lineage.
        let mut malformed_past_the_tip = cursor_req(
            fixture.original_id,
            query_options(None, Ordering::Descending, Some(&malformed), None),
        );
        malformed_past_the_tip.end_checkpoint = Some(u64::MAX);
        let mut other_package_past_the_tip = cursor_req(
            fixture.original_id,
            query_options(None, Ordering::Descending, None, Some(&other_package)),
        );
        other_package_past_the_tip.end_checkpoint = Some(u64::MAX);

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
            (malformed_past_the_tip, "invalid cursor encoding"),
            (
                other_package_past_the_tip,
                "cursor package ID does not match request package ID",
            ),
            (
                cursor_req(
                    fixture.original_id,
                    query_options(None, Ordering::Descending, None, Some(&other_package)),
                ),
                "cursor package ID does not match request package ID",
            ),
            (
                cursor_req(fixture.original_id, invalid_ordering),
                "invalid ordering",
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
    async fn list_package_versions_page_tokens_resume_after_the_last_listed_version() {
        let fixture = setup_versions_fixture().await;
        let cursors = fixture_cursors(&fixture).await;

        for (start_version, end_version, pages) in [
            (None, None, vec![vec![1, 2], vec![3, 4], vec![5]]),
            (Some(2), Some(5), vec![vec![2, 3], vec![4]]),
        ] {
            let mut page_token = None;
            for (index, expected) in pages.iter().enumerate() {
                let mut req = list_req(fixture.original_id);
                req.page_size = Some(2);
                req.page_token = page_token.take();
                req.start_version = start_version;
                req.end_version = end_version;
                let page = list_versions_fixture(&fixture, req).await.unwrap();
                assert_eq!(&listed_versions(&page), expected, "page {index}");
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
}
