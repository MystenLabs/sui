// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc::proto::sui::rpc::v2beta2::move_package_service_client::MovePackageServiceClient;
use sui_rpc::proto::sui::rpc::v2beta2::GetFunctionRequest;
use test_cluster::TestClusterBuilder;

use crate::v2beta2::move_package_service::system_package_expectations::validate_new_unverified_validator_operation_cap_and_transfer_function;

#[sim_test]
async fn test_get_function_validator_cap() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetFunctionRequest {
        package_id: Some("0x3".to_string()),
        module_name: Some("validator_cap".to_string()),
        name: Some("new_unverified_validator_operation_cap_and_transfer".to_string()),
    };

    let response = service.get_function(request).await.unwrap();
    let function = response.into_inner().function.unwrap();

    validate_new_unverified_validator_operation_cap_and_transfer_function(&function);
}

#[sim_test]
async fn test_get_function_not_found() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetFunctionRequest {
        package_id: Some("0x3".to_string()),
        module_name: Some("validator_cap".to_string()),
        name: Some("non_existent_function".to_string()),
    };

    let error = service.get_function(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::Internal);
    assert!(error.message().contains("Function not found"));
}

#[sim_test]
async fn test_get_function_invalid_hex() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetFunctionRequest {
        package_id: Some("0xGGGG".to_string()),
        module_name: Some("module".to_string()),
        name: Some("function".to_string()),
    };

    let error = service.get_function(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("invalid package_id"));
}

#[sim_test]
async fn test_get_function_missing_package_id() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetFunctionRequest {
        package_id: None,
        module_name: Some("module".to_string()),
        name: Some("function".to_string()),
    };

    let error = service.get_function(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("missing package_id"));
}

#[sim_test]
async fn test_get_function_missing_module_name() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetFunctionRequest {
        package_id: Some("0x3".to_string()),
        module_name: None,
        name: Some("function".to_string()),
    };

    let error = service.get_function(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("missing module_name"));
}

#[sim_test]
async fn test_get_function_missing_name() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetFunctionRequest {
        package_id: Some("0x3".to_string()),
        module_name: Some("validator_cap".to_string()),
        name: None,
    };

    let error = service.get_function(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("missing name"));
}
