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

/// Cursor for `ListPackageVersions` pagination. Public so other implementors of
/// `MovePackageService` mint wire-compatible page tokens.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PageToken {
    pub original_package_id: ObjectID,
    pub version: u64,
}

impl PageToken {
    pub fn decode(page_token: &[u8]) -> Result<Self> {
        decode_page_token(page_token)
    }

    pub fn encode(self) -> Bytes {
        encode_page_token(self)
    }
}

/// List the versions of a package's upgrade lineage, optionally only those published at or
/// before `at_checkpoint`.
///
/// Versions with no recorded publish checkpoint are listed with `checkpoint` unset; see the
/// `at_checkpoint` bounds below for why a bound may include them.
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

    if let Some(token) = &page_token
        && token.original_package_id != original_package_id
    {
        return Err(FieldViolation::new("page_token")
            .with_description("page token package ID does not match request package ID")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }

    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(|| RpcError::new(tonic::Code::Unavailable, "rpc indexes are disabled"))?;

    if let Some(at_checkpoint) = request.at_checkpoint {
        // Above the highest indexed checkpoint the index may be missing versions, and while a
        // restore is still backfilling history, versions published in the part of its window it
        // has not reached still have no recorded checkpoint, so a bound anywhere could include
        // versions published after it. Both are rejected before the read, like `GetPackage`.
        service
            .reader
            .at_checkpoint_upper_bound()?
            .check(at_checkpoint)?;

        // A version with no recorded publish checkpoint was published before the history the
        // index holds begins, so it belongs under any bound at or above that point. On a node
        // that backfilled after restoring from a snapshot, that point is the lowest checkpoint
        // available at restore time, and the rpc-store floor the restore stamped there keeps the
        // objects floor at or above it. A store that never backfills, such as a fork, reports
        // its own point through `get_lowest_checkpoint_for_pinned_reads`. Below the floor, an
        // unset version may have been published after the bound, so the request is rejected.
        //
        // Unlike `GetObject`'s `at_checkpoint`, this check can run before the read: pruning never
        // deletes `package_versions` rows, and the floor only rises, so a bound that passes stays
        // correct for rows read afterwards.
        let inner = service.reader.inner();
        let lowest_available = inner
            .get_lowest_available_checkpoint_objects()?
            .max(inner.get_lowest_checkpoint_for_pinned_reads()?);
        if at_checkpoint < lowest_available {
            return Err(RpcError::new(
                tonic::Code::OutOfRange,
                format!(
                    "requested checkpoint {at_checkpoint} has been pruned; lowest available \
                     checkpoint is {lowest_available}",
                ),
            ));
        }
    }

    let mut versions = vec![];
    let iter = indexes
        .package_versions_iter(original_package_id, page_token.map(|t| t.version))
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    for result in iter.take(page_size + 1) {
        let entry = result.map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

        // Recorded publish checkpoints are non-decreasing in version, so the first version
        // published after the bound ends the scan. Versions with no recorded checkpoint come
        // first and were published before the floor checked above.
        if let (Some(at_checkpoint), Some(published)) = (request.at_checkpoint, entry.checkpoint)
            && published > at_checkpoint
        {
            break;
        }

        let mut version = PackageVersion::new(&entry.storage_id.into(), entry.version);
        version.checkpoint = entry.checkpoint;
        versions.push(version);
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
