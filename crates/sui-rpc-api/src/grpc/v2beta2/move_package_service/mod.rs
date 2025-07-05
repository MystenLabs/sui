// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{ErrorReason, Result, RpcError, RpcService};
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2beta2::move_package_service_server::MovePackageService;
use sui_rpc::proto::sui::rpc::v2beta2::{
    GetDatatypeRequest, GetDatatypeResponse, GetFunctionRequest, GetFunctionResponse,
    GetPackageRequest, GetPackageResponse, ListPackageVersionsRequest, ListPackageVersionsResponse,
};
use sui_types::{base_types::ObjectID, move_package::MovePackage};

mod conversions;
mod get_datatype;
mod get_function;
mod get_package;
mod list_package_versions;

#[tonic::async_trait]
impl MovePackageService for RpcService {
    async fn get_package(
        &self,
        request: tonic::Request<GetPackageRequest>,
    ) -> Result<tonic::Response<GetPackageResponse>, tonic::Status> {
        get_package::get_package(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_datatype(
        &self,
        request: tonic::Request<GetDatatypeRequest>,
    ) -> Result<tonic::Response<GetDatatypeResponse>, tonic::Status> {
        get_datatype::get_datatype(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_function(
        &self,
        request: tonic::Request<GetFunctionRequest>,
    ) -> Result<tonic::Response<GetFunctionResponse>, tonic::Status> {
        get_function::get_function(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn list_package_versions(
        &self,
        request: tonic::Request<ListPackageVersionsRequest>,
    ) -> Result<tonic::Response<ListPackageVersionsResponse>, tonic::Status> {
        list_package_versions::list_package_versions(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}

pub(crate) fn load_package(service: &RpcService, package_id_str: &str) -> Result<MovePackage> {
    let package_id = package_id_str.parse::<ObjectID>().map_err(|e| {
        FieldViolation::new("package_id")
            .with_description(format!("invalid package_id: {}", e))
            .with_reason(ErrorReason::FieldInvalid)
    })?;

    let object = service
        .reader
        .inner()
        .get_object(&package_id)
        .ok_or_else(RpcError::not_found)?;

    let inner = object.into_inner();
    inner
        .data
        .try_into_package()
        .ok_or_else(|| RpcError::new(tonic::Code::InvalidArgument, "object is not a package"))
}
