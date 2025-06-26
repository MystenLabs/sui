// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    grpc::v2alpha::move_package::load_package,
    proto::google::rpc::bad_request::FieldViolation,
    proto::rpc::v2alpha::{
        list_package_versions_response::PackageVersion, ListPackageVersionsRequest,
        ListPackageVersionsResponse,
    },
    ErrorReason, Result, RpcError, RpcService,
};

#[tracing::instrument(skip(service))]
pub fn list_package_versions(
    service: &RpcService,
    request: ListPackageVersionsRequest,
) -> Result<ListPackageVersionsResponse> {
    let package_id_str = request.package_id.as_ref().ok_or_else(|| {
        FieldViolation::new("package_id")
            .with_description("missing package_id")
            .with_reason(ErrorReason::FieldMissing)
    })?;

    let package = load_package(service, package_id_str)?;
    let original_package_id = package.original_package_id();

    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(RpcError::not_found)?;

    let mut versions = vec![];
    let iter = indexes
        .package_versions_iter(original_package_id)
        .map_err(|e| {
            RpcError::new(
                tonic::Code::Internal,
                format!("Failed to query package versions: {}", e),
            )
        })?;

    for result in iter {
        let (version, storage_id) =
            result.map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

        versions.push(PackageVersion {
            package_id: Some(storage_id.to_string()),
            version: Some(version),
        });
    }

    Ok(ListPackageVersionsResponse { versions })
}
