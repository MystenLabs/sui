// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_kvstore::BigTableClient;
use sui_kvstore::KeyValueStoreReader;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::GetDatatypeRequest;
use sui_rpc::proto::sui::rpc::v2::GetDatatypeResponse;
use sui_rpc::proto::sui::rpc::v2::GetFunctionRequest;
use sui_rpc::proto::sui::rpc::v2::GetFunctionResponse;
use sui_rpc::proto::sui::rpc::v2::GetPackageRequest;
use sui_rpc::proto::sui::rpc::v2::GetPackageResponse;
use sui_rpc::proto::sui::rpc::v2::ListPackageVersionsRequest;
use sui_rpc::proto::sui::rpc::v2::ListPackageVersionsResponse;
use sui_rpc::proto::sui::rpc::v2::PackageVersion;
use sui_rpc::proto::sui::rpc::v2::move_package_service_server::MovePackageService;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_rpc_api::grpc::v2::move_package_service::PageToken;
use sui_rpc_api::grpc::v2::move_package_service::get_datatype_response;
use sui_rpc_api::grpc::v2::move_package_service::get_function_response;
use sui_rpc_api::grpc::v2::move_package_service::get_package_response;
use sui_types::base_types::ObjectID;
use sui_types::move_package::MovePackage;

use crate::KvRpcServer;

#[tonic::async_trait]
impl MovePackageService for KvRpcServer {
    async fn get_package(
        &self,
        request: tonic::Request<GetPackageRequest>,
    ) -> Result<tonic::Response<GetPackageResponse>, tonic::Status> {
        get_package(self.client.clone(), request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_datatype(
        &self,
        request: tonic::Request<GetDatatypeRequest>,
    ) -> Result<tonic::Response<GetDatatypeResponse>, tonic::Status> {
        get_datatype(self.client.clone(), request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_function(
        &self,
        request: tonic::Request<GetFunctionRequest>,
    ) -> Result<tonic::Response<GetFunctionResponse>, tonic::Status> {
        get_function(self.client.clone(), request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn list_package_versions(
        &self,
        request: tonic::Request<ListPackageVersionsRequest>,
    ) -> Result<tonic::Response<ListPackageVersionsResponse>, tonic::Status> {
        list_package_versions(self.client.clone(), request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}

async fn get_package(
    client: BigTableClient,
    request: GetPackageRequest,
) -> Result<GetPackageResponse, RpcError> {
    let package_id_str = request.package_id.as_ref().ok_or_else(|| {
        FieldViolation::new("package_id")
            .with_description("missing package_id")
            .with_reason(ErrorReason::FieldMissing)
    })?;

    let package = load_package(client, package_id_str).await?;
    get_package_response(&package)
}

async fn get_datatype(
    client: BigTableClient,
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

    let package = load_package(client, package_id_str).await?;
    get_datatype_response(&package, module_name, datatype_name)
}

async fn get_function(
    client: BigTableClient,
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

    let package = load_package(client, package_id_str).await?;
    get_function_response(&package, module_name, function_name)
}

async fn list_package_versions(
    mut client: BigTableClient,
    request: ListPackageVersionsRequest,
) -> Result<ListPackageVersionsResponse, RpcError> {
    let package_id_str = request.package_id.as_ref().ok_or_else(|| {
        FieldViolation::new("package_id")
            .with_description("missing package_id")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let package_id = parse_package_id(package_id_str)?;

    let original_package_id = client
        .get_package_original_ids(&[package_id])
        .await
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        .into_iter()
        .next()
        .map(|(_, original_id)| original_id)
        .ok_or_else(RpcError::not_found)?;

    let page_size = request
        .page_size
        .map(|s| (s as usize).clamp(1, 10000))
        .unwrap_or(1000);

    let page_token = request
        .page_token
        .map(|token| PageToken::decode(&token))
        .transpose()?;

    if let Some(token) = &page_token
        && token.original_package_id != original_package_id
    {
        return Err(FieldViolation::new("page_token")
            .with_description("page token package ID does not match request package ID")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }

    // The token's version is the first version of the next page (inclusive); the reader's
    // `after_version` bound is exclusive.
    let after_version = page_token.map(|token| token.version.saturating_sub(1));

    let mut versions: Vec<_> = client
        .get_package_versions(
            original_package_id,
            u64::MAX,
            after_version,
            None,
            page_size + 1,
            false,
        )
        .await
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        .into_iter()
        .map(|pkg| {
            let storage_id = ObjectID::from_bytes(&pkg.package_id)
                .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
            Ok(PackageVersion::new(&storage_id.into(), pkg.package_version))
        })
        .collect::<Result<_, RpcError>>()?;

    let next_page_token = if versions.len() > page_size {
        versions.pop().and_then(|next| {
            next.version.map(|version| {
                PageToken {
                    original_package_id,
                    version,
                }
                .encode()
            })
        })
    } else {
        None
    };

    Ok(ListPackageVersionsResponse::new(versions, next_page_token))
}

/// Load a package by its storage ID: a storage ID identifies exactly one immutable package
/// version, so the latest object at that ID is the package itself.
async fn load_package(
    mut client: BigTableClient,
    package_id_str: &str,
) -> Result<MovePackage, RpcError> {
    let package_id = parse_package_id(package_id_str)?;

    let object = client
        .get_latest_object(&package_id)
        .await
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        .ok_or_else(RpcError::not_found)?;

    object
        .into_inner()
        .data
        .try_into_package()
        .ok_or_else(|| RpcError::new(tonic::Code::InvalidArgument, "object is not a package"))
}

fn parse_package_id(package_id_str: &str) -> Result<ObjectID, RpcError> {
    package_id_str.parse::<ObjectID>().map_err(|e| {
        FieldViolation::new("package_id")
            .with_description(format!("invalid package_id: {}", e))
            .with_reason(ErrorReason::FieldInvalid)
            .into()
    })
}
