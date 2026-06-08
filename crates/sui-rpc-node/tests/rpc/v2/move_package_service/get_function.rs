// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Mirrors
//! `sui-e2e-tests/tests/rpc/v2/move_package_service/get_function.rs`.
//! Like the datatype suite, every case targets the system package
//! at `0x3` so no on-disk Move build is needed.

use sui_rpc::proto::sui::rpc::v2::GetFunctionRequest;
use sui_rpc::proto::sui::rpc::v2::move_package_service_client::MovePackageServiceClient;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;
use crate::v2::move_package_service::system_package_expectations::validate_new_unverified_validator_operation_cap_and_transfer_function;

async fn service(cluster: &LocalCluster) -> MovePackageServiceClient<Channel> {
    MovePackageServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap()
}

#[tokio::test]
async fn get_function_validator_cap() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = GetFunctionRequest::default();
    request.package_id = Some("0x3".to_string());
    request.module_name = Some("validator_cap".to_string());
    request.name = Some("new_unverified_validator_operation_cap_and_transfer".to_string());

    let function = svc
        .get_function(request)
        .await
        .unwrap()
        .into_inner()
        .function
        .unwrap();

    validate_new_unverified_validator_operation_cap_and_transfer_function(&function);
}

#[tokio::test]
async fn get_function_not_found() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = GetFunctionRequest::default();
    request.package_id = Some("0x3".to_string());
    request.module_name = Some("validator_cap".to_string());
    request.name = Some("non_existent_function".to_string());

    let err = svc.get_function(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::Internal);
    assert!(err.message().contains("Function not found"));
}

#[tokio::test]
async fn get_function_invalid_hex() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = GetFunctionRequest::default();
    request.package_id = Some("0xGGGG".to_string());
    request.module_name = Some("module".to_string());
    request.name = Some("function".to_string());

    let err = svc.get_function(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("invalid package_id"));
}

#[tokio::test]
async fn get_function_missing_package_id() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = GetFunctionRequest::default();
    request.module_name = Some("module".to_string());
    request.name = Some("function".to_string());

    let err = svc.get_function(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("missing package_id"));
}

#[tokio::test]
async fn get_function_missing_module_name() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = GetFunctionRequest::default();
    request.package_id = Some("0x3".to_string());
    request.name = Some("function".to_string());

    let err = svc.get_function(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("missing module_name"));
}

#[tokio::test]
async fn get_function_missing_name() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = GetFunctionRequest::default();
    request.package_id = Some("0x3".to_string());
    request.module_name = Some("validator_cap".to_string());

    let err = svc.get_function(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("missing name"));
}
