// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Mirrors
//! `sui-e2e-tests/tests/rpc/v2/move_package_service/get_datatype.rs`.
//! All tests target the system package at `0x3`, so we don't
//! need to publish anything.

use sui_rpc::proto::sui::rpc::v2::GetDatatypeRequest;
use sui_rpc::proto::sui::rpc::v2::move_package_service_client::MovePackageServiceClient;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;
use crate::v2::move_package_service::system_package_expectations::validate_validator_operation_cap_datatype;

async fn service(cluster: &LocalCluster) -> MovePackageServiceClient<Channel> {
    MovePackageServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap()
}

#[tokio::test]
async fn get_struct_datatype() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = GetDatatypeRequest::default();
    request.package_id = Some("0x3".to_string());
    request.module_name = Some("validator_cap".to_string());
    request.name = Some("ValidatorOperationCap".to_string());

    let datatype = svc
        .get_datatype(request)
        .await
        .unwrap()
        .into_inner()
        .datatype
        .unwrap();

    validate_validator_operation_cap_datatype(&datatype);
}

#[tokio::test]
async fn get_datatype_not_found() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = GetDatatypeRequest::default();
    request.package_id = Some("0x3".to_string());
    request.module_name = Some("validator_cap".to_string());
    request.name = Some("NonExistentType".to_string());

    let err = svc.get_datatype(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::Internal);
    assert!(
        err.message()
            .contains("Datatype 'NonExistentType' not found"),
        "unexpected message: {}",
        err.message(),
    );
}

#[tokio::test]
async fn get_datatype_invalid_package() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = GetDatatypeRequest::default();
    request.package_id = Some("invalid_id".to_string());
    request.module_name = Some("validator_cap".to_string());
    request.name = Some("ValidatorOperationCap".to_string());

    let err = svc.get_datatype(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("invalid package_id"));
}

#[tokio::test]
async fn get_datatype_module_not_found() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = GetDatatypeRequest::default();
    request.package_id = Some("0x3".to_string());
    request.module_name = Some("non_existent_module".to_string());
    request.name = Some("SomeType".to_string());

    let err = svc.get_datatype(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::Internal);
    assert!(err.message().contains("Module not found"));
}

#[tokio::test]
async fn get_datatype_missing_package_id() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = GetDatatypeRequest::default();
    request.module_name = Some("validator_cap".to_string());
    request.name = Some("ValidatorOperationCap".to_string());

    let err = svc.get_datatype(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("missing package_id"));
}

#[tokio::test]
async fn get_datatype_missing_module_name() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = GetDatatypeRequest::default();
    request.package_id = Some("0x3".to_string());
    request.name = Some("ValidatorOperationCap".to_string());

    let err = svc.get_datatype(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("missing module_name"));
}

#[tokio::test]
async fn get_datatype_missing_name() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = GetDatatypeRequest::default();
    request.package_id = Some("0x3".to_string());
    request.module_name = Some("validator_cap".to_string());

    let err = svc.get_datatype(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("missing name"));
}
