// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    proto::google::rpc::bad_request::FieldViolation,
    proto::rpc::v2alpha::{GetPackageRequest, GetPackageResponse, Module, Package},
    ErrorReason, Result, RpcError, RpcService,
};
use sui_types::{base_types::ObjectID, move_package::MovePackage, object::Data};

#[tracing::instrument(skip(service))]
pub fn get_package(service: &RpcService, request: GetPackageRequest) -> Result<GetPackageResponse> {
    let package_id = request
        .package_id
        .as_ref()
        .ok_or_else(|| {
            FieldViolation::new("package_id")
                .with_description("missing package_id")
                .with_reason(ErrorReason::FieldMissing)
        })?
        .parse::<ObjectID>()
        .map_err(|e| {
            FieldViolation::new("package_id")
                .with_description(format!("invalid package_id: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    let sui_object = service
        .reader
        .inner()
        .get_object(&package_id)
        .ok_or_else(RpcError::not_found)?;

    let package = match &sui_object.data {
        Data::Package(p) => p,
        _ => {
            return Err(RpcError::new(
                tonic::Code::InvalidArgument,
                "object is not a package",
            ))
        }
    };

    let modules = extract_modules(package)?;

    Ok(GetPackageResponse {
        package: Some(Package {
            storage_id: Some(package_id.to_string()),
            original_id: Some(package.original_package_id().to_string()),
            version: Some(package.version().value()),
            modules,
        }),
    })
}

fn extract_modules(package: &MovePackage) -> Result<Vec<Module>> {
    let mut modules = Vec::new();

    for module_name in package.serialized_module_map().keys() {
        // Only return module names
        // Full module data is provided by GetModule
        modules.push(Module {
            name: Some(module_name.clone()),
            data_types: vec![],
            functions: vec![],
        });
    }

    Ok(modules)
}
