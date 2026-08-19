// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ErrorReason, Result, RpcError, RpcService,
    grpc::v2::move_package_service::{
        conversions::{convert_error, convert_module},
        load_package, load_package_by_id,
    },
};
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::{GetPackageRequest, GetPackageResponse, Package};
use sui_types::move_package::MovePackage;

#[tracing::instrument(skip(service))]
pub fn get_package(service: &RpcService, request: GetPackageRequest) -> Result<GetPackageResponse> {
    let package_id_str = request.package_id.as_ref().ok_or_else(|| {
        FieldViolation::new("package_id")
            .with_description("missing package_id")
            .with_reason(ErrorReason::FieldMissing)
    })?;

    let package = match (request.version, request.at_checkpoint) {
        (Some(_), Some(_)) => {
            return Err(FieldViolation::new("at_checkpoint")
                .with_description("at most one of `version` and `at_checkpoint` may be set")
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }
        (Some(version), None) => load_package_at_version(service, package_id_str, version)?,
        (None, Some(at_checkpoint)) => {
            load_package_at_checkpoint(service, package_id_str, at_checkpoint)?
        }
        (None, None) => load_package(service, package_id_str)?,
    };

    get_package_response(&package)
}

/// Load the package in `package_id_str`'s upgrade lineage with exactly
/// `version`.
fn load_package_at_version(
    service: &RpcService,
    package_id_str: &str,
    version: u64,
) -> Result<MovePackage> {
    let package = load_package(service, package_id_str)?;
    // Normalize the lineage member's storage id
    let original_id = package.original_package_id();
    if package.version().value() == version {
        return Ok(package);
    }

    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(RpcError::not_found)?;

    let storage_id = indexes
        .get_package_version_storage_id(original_id, version)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        .ok_or_else(RpcError::not_found)?;

    load_package_by_id(service, storage_id)
}

/// Load the latest package in `package_id_str`'s upgrade lineage that
/// existed at or before `at_checkpoint`.
fn load_package_at_checkpoint(
    service: &RpcService,
    package_id_str: &str,
    at_checkpoint: u64,
) -> Result<MovePackage> {
    let lowest_available = service.reader.get_lowest_available_checkpoint()?;
    if at_checkpoint < lowest_available {
        return Err(RpcError::new(
            tonic::Code::NotFound,
            format!(
                "requested checkpoint {at_checkpoint} has been pruned; lowest available \
                 checkpoint is {lowest_available}",
            ),
        ));
    }

    let original_id = load_package(service, package_id_str)?.original_package_id();

    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(RpcError::not_found)?;

    let (_, storage_id) = indexes
        .get_package_at_checkpoint(original_id, at_checkpoint)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        .ok_or_else(RpcError::not_found)?;

    load_package_by_id(service, storage_id)
}

/// Build a `GetPackage` response from an already-loaded package. Shared with other implementors
/// of `MovePackageService` (e.g. sui-kv-rpc) that load packages from a different backend.
pub fn get_package_response(package: &MovePackage) -> Result<GetPackageResponse> {
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

    Ok(GetPackageResponse::new(message))
}
