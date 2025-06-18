// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    grpc::v2beta2::move_package_service::{
        conversions::{convert_error, convert_function},
        load_package,
    },
    proto::google::rpc::bad_request::FieldViolation,
    proto::rpc::v2beta2::{GetFunctionRequest, GetFunctionResponse},
    ErrorReason, Result, RpcService,
};

#[tracing::instrument(skip(service))]
pub fn get_function(
    service: &RpcService,
    request: GetFunctionRequest,
) -> Result<GetFunctionResponse> {
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

    let package = load_package(service, package_id_str)?;

    let resolver_package =
        sui_package_resolver::Package::read_from_package(&package).map_err(convert_error)?;

    let resolver_module = resolver_package
        .module(module_name)
        .map_err(convert_error)?;

    let func_def = resolver_module
        .function_def(function_name)
        .map_err(convert_error)?
        .ok_or_else(|| {
            crate::RpcError::new(
                tonic::Code::Internal,
                format!("Function not found: {}", function_name),
            )
        })?;

    let descriptor = convert_function(function_name, &func_def);

    Ok(GetFunctionResponse {
        function: Some(descriptor),
    })
}
