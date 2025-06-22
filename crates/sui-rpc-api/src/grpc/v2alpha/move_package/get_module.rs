// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    grpc::v2alpha::move_package::{
        conversions::{convert_error, convert_module},
        load_package,
    },
    proto::google::rpc::bad_request::FieldViolation,
    proto::rpc::v2alpha::{GetModuleRequest, GetModuleResponse},
    ErrorReason, Result, RpcService,
};

#[tracing::instrument(skip(service))]
pub fn get_module(service: &RpcService, request: GetModuleRequest) -> Result<GetModuleResponse> {
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

    let package = load_package(service, package_id_str)?;
    let package_id = package.id();

    let resolver_package =
        sui_package_resolver::Package::read_from_package(&package).map_err(convert_error)?;

    let resolver_module = resolver_package
        .module(module_name)
        .map_err(convert_error)?;

    let module = convert_module(module_name, resolver_module, &package_id)?;

    Ok(GetModuleResponse {
        module: Some(module),
    })
}
