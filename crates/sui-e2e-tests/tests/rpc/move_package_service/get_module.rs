// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::proto::rpc::v2alpha::move_package_service_client::MovePackageServiceClient;
use sui_rpc_api::proto::rpc::v2alpha::GetModuleRequest;
use test_cluster::TestClusterBuilder;

use crate::move_package_service::system_package_expectations::validate_validator_cap_module;

#[sim_test]
async fn test_get_module_validator_cap() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetModuleRequest {
        package_id: Some("0x3".to_string()),
        module_name: Some("validator_cap".to_string()),
    };

    let response = service.get_module(request).await.unwrap();
    let module = response.into_inner().module.unwrap();

    validate_validator_cap_module(&module);
}

#[sim_test]
async fn test_get_module_not_found() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetModuleRequest {
        package_id: Some("0x3".to_string()),
        module_name: Some("non_existent_module".to_string()),
    };

    let error = service.get_module(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::Internal);
    assert!(error.message().contains("Module not found"));
}

#[sim_test]
async fn test_get_module_invalid_package() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetModuleRequest {
        package_id: Some("0xDEADBEEF".to_string()),
        module_name: Some("module".to_string()),
    };

    let error = service.get_module(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::NotFound);
}

#[sim_test]
async fn test_get_module_missing_package_id() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetModuleRequest {
        package_id: None,
        module_name: Some("module".to_string()),
    };

    let error = service.get_module(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("missing package_id"));
}

#[sim_test]
async fn test_get_module_missing_name() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetModuleRequest {
        package_id: Some("0x3".to_string()),
        module_name: None,
    };

    let error = service.get_module(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("missing module_name"));
}
