// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Ports the `get_package_system`, `get_package_not_found`,
//! `get_package_invalid_hex`, and `get_package_missing_id` tests
//! from `sui-e2e-tests/tests/rpc/v2/move_package_service/get_package.rs`.
//!
//! `validate_system_package` lives in the e2e crate next to its
//! own snapshot expectations; we inline the small subset we
//! assert on here. Tests that publish custom Move packages
//! (`test_get_package_published`, `test_get_function_published`,
//! etc.) need an on-disk Move project compiled via `sui-move-build`,
//! which we don't yet wire up here.

use sui_rpc::proto::sui::rpc::v2::GetPackageRequest;
use sui_rpc::proto::sui::rpc::v2::move_package_service_client::MovePackageServiceClient;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

async fn service(cluster: &LocalCluster) -> MovePackageServiceClient<Channel> {
    MovePackageServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap()
}

#[tokio::test]
async fn get_package_system() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = GetPackageRequest::default();
    request.package_id = Some("0x3".to_string());

    let package = svc
        .get_package(request)
        .await
        .unwrap()
        .into_inner()
        .package
        .unwrap();

    // Mirror the assertions baked into the e2e helper
    // `validate_system_package`. We don't compare module name
    // sets exhaustively because protocol upgrades add/remove
    // modules over time; the framework guarantees at least
    // these.
    assert!(package.storage_id.is_some());
    assert!(package.original_id.is_some());
    assert!(package.version.is_some());
    assert!(!package.modules.is_empty());
    assert!(
        package
            .modules
            .iter()
            .any(|m| m.name.as_deref() == Some("sui_system")),
        "0x3 should expose the `sui_system` module",
    );
}

#[tokio::test]
async fn get_package_not_found() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = GetPackageRequest::default();
    request.package_id =
        Some("0xDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEF".to_string());

    let err = svc.get_package(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn get_package_invalid_hex() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = GetPackageRequest::default();
    request.package_id = Some("0xINVALID".to_string());

    let err = svc.get_package(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("invalid package_id"));
}

#[tokio::test]
async fn get_package_missing_id() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let err = svc
        .get_package(GetPackageRequest::default())
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("missing package_id"));
}
