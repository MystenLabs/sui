// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{ErrorReason, Result, RpcError, RpcService};
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::move_package_service_server::MovePackageService;
use sui_rpc::proto::sui::rpc::v2::{
    GetDatatypeRequest, GetDatatypeResponse, GetFunctionRequest, GetFunctionResponse,
    GetPackageRequest, GetPackageResponse, ListPackageVersionsRequest, ListPackageVersionsResponse,
    Package,
};
use sui_types::{base_types::ObjectID, move_package::MovePackage, object::Object};

use conversions::{convert_error, convert_module};

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

/// Full pipeline: validate the string, fetch, convert.
pub(crate) fn load_package(service: &RpcService, package_id_str: &str) -> Result<MovePackage> {
    load_package_by_id(service, parse_package_id(package_id_str)?)
}

/// Parse and validate a package ID string.
fn parse_package_id(package_id_str: &str) -> Result<ObjectID> {
    package_id_str.parse::<ObjectID>().map_err(|e| {
        FieldViolation::new("package_id")
            .with_description(format!("invalid package_id: {e}"))
            .with_reason(ErrorReason::FieldInvalid)
            .into()
    })
}

/// Fetch an object by ID and convert it into a package.
pub(crate) fn load_package_by_id(
    service: &RpcService,
    package_id: ObjectID,
) -> Result<MovePackage> {
    let object = service
        .reader
        .inner()
        .get_object(&package_id)
        .ok_or_else(RpcError::not_found)?;
    object_as_package(object)
}

fn object_as_package(object: Object) -> Result<MovePackage> {
    object
        .into_inner()
        .data
        .try_into_package()
        .ok_or_else(|| RpcError::new(tonic::Code::InvalidArgument, "object is not a package"))
}

/// Render a `MovePackage` as its proto `Package` message, including all
/// resolved modules.
pub fn package_to_proto(package: &MovePackage) -> Result<Package> {
    let package_id = package.id();

    let resolved_package =
        sui_package_resolver::Package::read_from_package(package).map_err(convert_error)?;

    let modules: Vec<_> = resolved_package
        .modules()
        .iter()
        .map(|(module_name, resolver_module)| {
            convert_module(module_name, resolver_module, &package_id)
        })
        .collect::<Result<Vec<_>>>()?;

    let mut message = Package::default();
    message.storage_id = Some(package_id.to_canonical_string(true));
    message.original_id = Some(package.original_package_id().to_canonical_string(true));
    message.version = Some(package.version().value());
    message.modules = modules;
    Ok(message)
}
