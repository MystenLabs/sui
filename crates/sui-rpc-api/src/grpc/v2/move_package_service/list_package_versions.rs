// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::load_package;
use crate::{ErrorReason, Result, RpcError, RpcService};
use bytes::Bytes;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::{
    ListPackageVersionsRequest, ListPackageVersionsResponse, PackageVersion,
};
use sui_types::base_types::ObjectID;
use tap::Pipe;

#[derive(serde::Serialize, serde::Deserialize)]
struct PageToken {
    original_package_id: ObjectID,
    version: u64,
}

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

    let page_size = request
        .page_size
        .map(|s| (s as usize).clamp(1, 10000))
        .unwrap_or(1000);

    let page_token = request
        .page_token
        .map(|token| decode_page_token(&token))
        .transpose()?;

    if let Some(token) = &page_token {
        if token.original_package_id != original_package_id {
            return Err(FieldViolation::new("page_token")
                .with_description("page token package ID does not match request package ID")
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }
    }

    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(RpcError::not_found)?;

    let mut versions = vec![];
    let iter = indexes
        .package_versions_iter(original_package_id, page_token.map(|t| t.version))
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    for result in iter.take(page_size + 1) {
        let (version, storage_id) =
            result.map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

        versions.push(PackageVersion::new(&storage_id.into(), version));
    }

    let next_page_token = if versions.len() > page_size {
        // SAFETY: We've already verified that versions is greater than page_size, which is
        // guaranteed to be >= 1.
        versions.pop().unwrap().pipe(|v| {
            v.version.map(|version| {
                encode_page_token(PageToken {
                    original_package_id,
                    version,
                })
            })
        })
    } else {
        None
    };

    Ok(ListPackageVersionsResponse::new(versions, next_page_token))
}

fn decode_page_token(page_token: &[u8]) -> Result<PageToken> {
    bcs::from_bytes(page_token).map_err(|e| {
        FieldViolation::new("page_token")
            .with_description(format!("invalid page token encoding: {e}"))
            .with_reason(ErrorReason::FieldInvalid)
            .into()
    })
}

fn encode_page_token(page_token: PageToken) -> Bytes {
    bcs::to_bytes(&page_token).unwrap().into()
}
