// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{ErrorReason, Result, RpcService};
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2beta2::{GetDatatypeRequest, GetDatatypeResponse};

use super::{
    conversions::{convert_datatype, convert_error},
    load_package,
};

#[tracing::instrument(skip(service))]
pub fn get_datatype(
    service: &RpcService,
    request: GetDatatypeRequest,
) -> Result<GetDatatypeResponse> {
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

    let package = load_package(service, package_id_str)?;
    let package_id = package.id();

    let resolver_package =
        sui_package_resolver::Package::read_from_package(&package).map_err(convert_error)?;

    let resolver_module = resolver_package
        .module(module_name)
        .map_err(convert_error)?;

    let data_def = resolver_module
        .data_def(datatype_name)
        .map_err(convert_error)?
        .ok_or_else(|| {
            crate::RpcError::new(
                tonic::Code::Internal,
                format!("Datatype '{}' not found", datatype_name),
            )
        })?;

    let datatype = convert_datatype(datatype_name, &data_def, &package_id, module_name);

    Ok(GetDatatypeResponse::new(datatype))
}
