// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    grpc::v2beta2::move_package_service::{
        conversions::{convert_error, convert_module},
        load_package,
    },
    ErrorReason, Result, RpcService,
};
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2beta2::{GetPackageRequest, GetPackageResponse, Package};

#[tracing::instrument(skip(service))]
pub fn get_package(service: &RpcService, request: GetPackageRequest) -> Result<GetPackageResponse> {
    let package_id_str = request.package_id.as_ref().ok_or_else(|| {
        FieldViolation::new("package_id")
            .with_description("missing package_id")
            .with_reason(ErrorReason::FieldMissing)
    })?;

    let package = load_package(service, package_id_str)?;
    let package_id = package.id();

    let resolved_package =
        sui_package_resolver::Package::read_from_package(&package).map_err(convert_error)?;

    let modules: Vec<_> = resolved_package
        .modules()
        .iter()
        .map(|(module_name, resolver_module)| {
            convert_module(module_name, resolver_module, &package_id)
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(GetPackageResponse {
        package: Some(Package {
            storage_id: Some(package_id.to_string()),
            original_id: Some(package.original_package_id().to_string()),
            version: Some(package.version().value()),
            modules,
            ..Default::default()
        }),
    })
}
