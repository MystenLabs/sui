// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_kvstore::{BigTableClient, KeyValueStoreReader};
use sui_rpc::proto::sui::rpc::v2::{GetPackageRequest, GetPackageResponse};
use sui_rpc_api::grpc::v2::move_package_service::package_to_proto;
use sui_rpc_api::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc_api::{ErrorReason, RpcError};
use sui_types::base_types::ObjectID;
use sui_types::move_package::MovePackage;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;

pub(crate) async fn get_package(
    mut client: BigTableClient,
    request: GetPackageRequest,
) -> Result<GetPackageResponse, RpcError> {
    let package_id = request
        .package_id
        .as_ref()
        .ok_or_else(|| {
            FieldViolation::new("package_id")
                .with_description("missing package_id")
                .with_reason(ErrorReason::FieldMissing)
        })?
        .parse::<ObjectID>()
        .map_err(|e| {
            FieldViolation::new("package_id")
                .with_description(format!("invalid package_id: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    if request.version.is_some() && request.at_checkpoint.is_some() {
        return Err(FieldViolation::new("at_checkpoint")
            .with_description("at most one of `version` and `at_checkpoint` may be set")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }

    if request.version.is_none() && request.at_checkpoint.is_none() {
        let object = client
            .get_latest_object(&package_id)
            .await?
            .ok_or_else(RpcError::not_found)?;
        let package = into_package(object)?;
        return Ok(GetPackageResponse::new(package_to_proto(&package)?));
    }

    let (_, original_id) = client
        .get_package_original_ids(&[package_id])
        .await?
        .pop()
        .ok_or_else(RpcError::not_found)?;

    // Expect either one of version or checkpoint but not both.
    let data = if let Some(version) = request.version {
        client
            .get_packages_by_version(&[(original_id, version)])
            .await?
            .pop()
    } else {
        client
            .get_package_latest(original_id, request.at_checkpoint.unwrap_or(u64::MAX))
            .await?
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
        .await?
        .pop()
        .ok_or_else(RpcError::not_found)?;
    let package = into_package(object)?;

    Ok(GetPackageResponse::new(package_to_proto(&package)?))
}

fn into_package(object: Object) -> Result<MovePackage, RpcError> {
    object
        .into_inner()
        .data
        .try_into_package()
        .ok_or_else(|| RpcError::new(tonic::Code::InvalidArgument, "object is not a package"))
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use move_binary_format::file_format::empty_module;
    use move_core_types::account_address::AccountAddress;
    use sui_kvstore::tables;
    use sui_kvstore::testing::MockBigtableServer;
    use sui_protocol_config::ProtocolConfig;
    use sui_types::base_types::SuiAddress;
    use sui_types::digests::TransactionDigest;
    use sui_types::object::Data;

    use super::*;

    struct Fixture {
        client: BigTableClient,
        original_id: ObjectID,
        upgraded_id: ObjectID,
        plain_object_id: ObjectID,
        _server: tokio::task::JoinHandle<()>,
    }

    /// Seed the mock with a two-version package lineage (v1 at `original_id`
    /// published at checkpoint 5, v2 at `upgraded_id` published at checkpoint
    /// 20) plus one non-package object.
    async fn setup() -> Fixture {
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

        Fixture {
            client,
            original_id,
            upgraded_id,
            plain_object_id,
            _server: server,
        }
    }

    fn request(package_id: ObjectID) -> GetPackageRequest {
        let mut request = GetPackageRequest::default();
        request.package_id = Some(package_id.to_canonical_string(true));
        request
    }

    async fn fetch(
        fixture: &Fixture,
        request: GetPackageRequest,
    ) -> sui_rpc::proto::sui::rpc::v2::Package {
        get_package(fixture.client.clone(), request)
            .await
            .expect("get_package succeeds")
            .package
            .expect("response carries a package")
    }

    async fn fetch_err(fixture: &Fixture, request: GetPackageRequest) -> tonic::Status {
        get_package(fixture.client.clone(), request)
            .await
            .expect_err("get_package fails")
            .into()
    }

    #[tokio::test]
    async fn bare_id_is_an_exact_storage_lookup() {
        let fixture = setup().await;

        let package = fetch(&fixture, request(fixture.original_id)).await;
        assert_eq!(
            package.storage_id,
            Some(fixture.original_id.to_canonical_string(true)),
        );
        assert_eq!(
            package.original_id,
            Some(fixture.original_id.to_canonical_string(true)),
        );
        assert_eq!(package.version, Some(1));

        let package = fetch(&fixture, request(fixture.upgraded_id)).await;
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
        let fixture = setup().await;

        // Exact version through the upgraded id resolves back to v1.
        let mut req = request(fixture.upgraded_id);
        req.version = Some(1);
        let package = fetch(&fixture, req).await;
        assert_eq!(
            package.storage_id,
            Some(fixture.original_id.to_canonical_string(true)),
        );
        assert_eq!(package.version, Some(1));

        // Exact version through the original id resolves forward to v2.
        let mut req = request(fixture.original_id);
        req.version = Some(2);
        let package = fetch(&fixture, req).await;
        assert_eq!(
            package.storage_id,
            Some(fixture.upgraded_id.to_canonical_string(true)),
        );
        assert_eq!(package.version, Some(2));

        // A checkpoint bound between the two publishes resolves v1.
        let mut req = request(fixture.upgraded_id);
        req.at_checkpoint = Some(19);
        let package = fetch(&fixture, req).await;
        assert_eq!(
            package.storage_id,
            Some(fixture.original_id.to_canonical_string(true)),
        );
        assert_eq!(package.version, Some(1));

        // A bound above the tip resolves the latest version.
        let mut req = request(fixture.original_id);
        req.at_checkpoint = Some(u64::MAX);
        let package = fetch(&fixture, req).await;
        assert_eq!(
            package.storage_id,
            Some(fixture.upgraded_id.to_canonical_string(true)),
        );
        assert_eq!(package.version, Some(2));
    }

    #[tokio::test]
    async fn missing_packages_are_not_found() {
        let fixture = setup().await;
        let unknown = ObjectID::from_single_byte(0xDD);

        let unknown_versioned = {
            let mut req = request(unknown);
            req.version = Some(1);
            req
        };
        let missing_version = {
            let mut req = request(fixture.original_id);
            req.version = Some(3);
            req
        };
        let before_first_publish = {
            let mut req = request(fixture.original_id);
            req.at_checkpoint = Some(4);
            req
        };
        for req in [
            request(unknown),
            unknown_versioned,
            missing_version,
            before_first_publish,
        ] {
            let status = fetch_err(&fixture, req).await;
            assert_eq!(status.code(), tonic::Code::NotFound);
        }
    }

    #[tokio::test]
    async fn non_package_object_is_rejected() {
        let fixture = setup().await;

        let status = fetch_err(&fixture, request(fixture.plain_object_id)).await;
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert!(status.message().contains("not a package"));

        // Setting both bounds is rejected before any lookup.
        let mut req = request(fixture.original_id);
        req.version = Some(1);
        req.at_checkpoint = Some(1);
        let status = fetch_err(&fixture, req).await;
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }
}
