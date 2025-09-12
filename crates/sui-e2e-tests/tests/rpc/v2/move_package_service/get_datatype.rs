// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_rpc::proto::sui::rpc::v2::move_package_service_client::MovePackageServiceClient;
use sui_rpc::proto::sui::rpc::v2::GetDatatypeRequest;
use test_cluster::TestClusterBuilder;

use crate::v2::move_package_service::system_package_expectations::validate_validator_operation_cap_datatype;

use sui_macros::sim_test;

#[sim_test]
async fn test_get_struct_datatype() {
    let cluster = TestClusterBuilder::new().build().await;

    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut request = GetDatatypeRequest::default();
    request.package_id = Some("0x3".to_string());
    request.module_name = Some("validator_cap".to_string());
    request.name = Some("ValidatorOperationCap".to_string());

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
    let mut request = GetDatatypeRequest::default();
    request.package_id = Some("0x3".to_string());
    request.module_name = Some("validator_cap".to_string());
    request.name = Some("NonExistentType".to_string());

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
    let mut request = GetDatatypeRequest::default();
    request.package_id = Some("invalid_id".to_string());
    request.module_name = Some("validator_cap".to_string());
    request.name = Some("ValidatorOperationCap".to_string());

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
    let mut request = GetDatatypeRequest::default();
    request.package_id = Some("0x3".to_string());
    request.module_name = Some("non_existent_module".to_string());
    request.name = Some("SomeType".to_string());

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

    let mut request = GetDatatypeRequest::default();
    request.package_id = None;
    request.module_name = Some("validator_cap".to_string());
    request.name = Some("ValidatorOperationCap".to_string());

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

    let mut request = GetDatatypeRequest::default();
    request.package_id = Some("0x3".to_string());
    request.module_name = None;
    request.name = Some("ValidatorOperationCap".to_string());

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

    let mut request = GetDatatypeRequest::default();
    request.package_id = Some("0x3".to_string());
    request.module_name = Some("validator_cap".to_string());
    request.name = None;

    let error = service.get_datatype(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("missing name"));
}
