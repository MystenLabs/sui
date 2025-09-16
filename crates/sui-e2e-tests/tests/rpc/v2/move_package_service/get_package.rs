// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc::proto::sui::rpc::v2::{
    move_package_service_client::MovePackageServiceClient, GetPackageRequest,
};
use test_cluster::TestClusterBuilder;

use crate::v2::move_package_service::system_package_expectations::validate_system_package;

#[sim_test]
async fn test_get_package_system() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut request = GetPackageRequest::default();
    request.package_id = Some("0x3".to_string());

    let response = service.get_package(request).await.unwrap();
    let package = response.into_inner().package.unwrap();

    validate_system_package(&package);
}

#[sim_test]
async fn test_get_package_not_found() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut request = GetPackageRequest::default();
    request.package_id =
        Some("0xDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEF".to_string());

    let error = service.get_package(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::NotFound);
}

#[sim_test]
async fn test_get_package_invalid_hex() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut request = GetPackageRequest::default();
    request.package_id = Some("0xINVALID".to_string());

    let error = service.get_package(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("invalid package_id"));
}

#[sim_test]
async fn test_get_package_missing_id() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetPackageRequest::default();

    let error = service.get_package(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("missing package_id"));
}
