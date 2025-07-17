// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_rpc_api::proto::rpc::v2beta2::move_package_service_client::MovePackageServiceClient;
use sui_rpc_api::proto::rpc::v2beta2::GetDatatypeRequest;
use test_cluster::TestClusterBuilder;

use crate::v2beta2::move_package_service::system_package_expectations::validate_validator_operation_cap_datatype;

use sui_macros::sim_test;

#[sim_test]
async fn test_get_struct_datatype() {
    let cluster = TestClusterBuilder::new().build().await;

    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetDatatypeRequest {
        package_id: Some("0x3".to_string()),
        module_name: Some("validator_cap".to_string()),
        name: Some("ValidatorOperationCap".to_string()),
    };

    let response = service.get_datatype(request).await.unwrap();
    let datatype = response.into_inner().datatype.unwrap();

    validate_validator_operation_cap_datatype(&datatype);
}

#[sim_test]
async fn test_get_datatype_not_found() {
    let cluster = TestClusterBuilder::new().build().await;

    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Test non-existent datatype
    let request = GetDatatypeRequest {
        package_id: Some("0x3".to_string()),
        module_name: Some("validator_cap".to_string()),
        name: Some("NonExistentType".to_string()),
    };

    let error = service.get_datatype(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::Internal);
    assert!(error
        .message()
        .contains("Datatype 'NonExistentType' not found"));
}

#[sim_test]
async fn test_get_datatype_invalid_package() {
    let cluster = TestClusterBuilder::new().build().await;

    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Test invalid package ID
    let request = GetDatatypeRequest {
        package_id: Some("invalid_id".to_string()),
        module_name: Some("validator_cap".to_string()),
        name: Some("ValidatorOperationCap".to_string()),
    };

    let error = service.get_datatype(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("invalid package_id"));
}

#[sim_test]
async fn test_get_datatype_module_not_found() {
    let cluster = TestClusterBuilder::new().build().await;

    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Test non-existent module
    let request = GetDatatypeRequest {
        package_id: Some("0x3".to_string()),
        module_name: Some("non_existent_module".to_string()),
        name: Some("SomeType".to_string()),
    };

    let error = service.get_datatype(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::Internal);
    assert!(error.message().contains("Module not found"));
}

#[sim_test]
async fn test_get_datatype_missing_package_id() {
    let cluster = TestClusterBuilder::new().build().await;

    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetDatatypeRequest {
        package_id: None,
        module_name: Some("validator_cap".to_string()),
        name: Some("ValidatorOperationCap".to_string()),
    };

    let error = service.get_datatype(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("missing package_id"));
}

#[sim_test]
async fn test_get_datatype_missing_module_name() {
    let cluster = TestClusterBuilder::new().build().await;

    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetDatatypeRequest {
        package_id: Some("0x3".to_string()),
        module_name: None,
        name: Some("ValidatorOperationCap".to_string()),
    };

    let error = service.get_datatype(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("missing module_name"));
}

#[sim_test]
async fn test_get_datatype_missing_name() {
    let cluster = TestClusterBuilder::new().build().await;

    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetDatatypeRequest {
        package_id: Some("0x3".to_string()),
        module_name: Some("validator_cap".to_string()),
        name: None,
    };

    let error = service.get_datatype(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("missing name"));
}
