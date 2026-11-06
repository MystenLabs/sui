// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;

use super::{load_package_by_id, parse_package_id};
use crate::{ErrorReason, Result, RpcError, RpcService};
use bytes::Bytes;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::{
    ListPackageVersionsRequest, ListPackageVersionsResponse, Ordering, PackageVersion,
};
use sui_types::base_types::ObjectID;

/// Versions per page when the request sets neither `page_size` nor `options.limit`.
const DEFAULT_PAGE_SIZE: u32 = 1000;
/// Larger `page_size` and `options.limit` values are coerced down to this.
const MAX_PAGE_SIZE: u32 = 10000;

/// Page token for `ListPackageVersions`: the first version of the next page.
#[derive(serde::Serialize, serde::Deserialize)]
struct PageToken {
    original_package_id: ObjectID,
    version: u64,
}

/// The position of one listed version, set as `PackageVersion.cursor` and accepted back as
/// `options.after` and `options.before`. Public so other implementors of `MovePackageService`
/// mint and accept the same cursors. BCS-encoded as an enum so that a listing in another order
/// can add a variant without changing the encoding of existing cursors.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PackageVersionCursor {
    /// A version in the upgrade lineage of the package originally published at
    /// `original_package_id`, listed in ascending version order.
    Lineage {
        original_package_id: ObjectID,
        version: u64,
    },
}

impl PackageVersionCursor {
    pub fn encode(&self) -> Bytes {
        bcs::to_bytes(self).unwrap().into()
    }
}

/// The versions a `ListPackageVersions` request lists and how its response pages: the request's
/// version bounds, page token, and cursors intersected into one version range, validated against
/// the lineage being listed. Public so other implementors of `MovePackageService` parse requests
/// and page responses the same way.
#[derive(Clone, Debug)]
pub struct PackageVersionsQuery {
    original_package_id: ObjectID,
    /// The versions to list. An end of `u64::MAX` leaves the range unbounded above, since no
    /// version reaches it.
    pub versions: Range<u64>,
    /// Whether to list `versions` from highest to lowest.
    pub descending: bool,
    /// The number of versions in a full page.
    page_size: usize,
    /// Whether the request sets `options`, whose pages carry no `next_page_token`.
    cursor_mode: bool,
    /// Whether the request continues from a page token or cursor.
    resumed: bool,
    /// The versions the request's `after` and `before` cursors name.
    cursor_versions: [Option<u64>; 2],
}

impl PackageVersionsQuery {
    /// Parse the paging of `request`, which lists the upgrade lineage of the package originally
    /// published at `original_package_id`.
    pub fn new(
        request: &ListPackageVersionsRequest,
        original_package_id: ObjectID,
    ) -> Result<Self> {
        let mut versions =
            request.start_version.unwrap_or(0)..request.end_version.unwrap_or(u64::MAX);

        let Some(options) = &request.options else {
            let page_token = request
                .page_token
                .as_deref()
                .map(decode_page_token)
                .transpose()?;
            if let Some(token) = &page_token {
                if token.original_package_id != original_package_id {
                    return Err(FieldViolation::new("page_token")
                        .with_description("page token package ID does not match request package ID")
                        .with_reason(ErrorReason::FieldInvalid)
                        .into());
                }
                versions.start = versions.start.max(token.version);
            }

            return Ok(Self {
                original_package_id,
                versions,
                descending: false,
                page_size: page_size(request.page_size),
                cursor_mode: false,
                resumed: page_token.is_some(),
                cursor_versions: [None; 2],
            });
        };

        for (field, set) in [
            ("page_size", request.page_size.is_some()),
            ("page_token", request.page_token.is_some()),
        ] {
            if set {
                return Err(FieldViolation::new(field)
                    .with_description(format!("{field} and options are mutually exclusive"))
                    .with_reason(ErrorReason::FieldInvalid)
                    .into());
            }
        }

        let descending = match options.ordering.map(Ordering::try_from) {
            None | Some(Ok(Ordering::Ascending)) => false,
            Some(Ok(Ordering::Descending)) => true,
            Some(_) => {
                return Err(FieldViolation::new("options.ordering")
                    .with_description("invalid ordering")
                    .with_reason(ErrorReason::FieldInvalid)
                    .into());
            }
        };

        let after = options
            .after
            .as_deref()
            .map(|cursor| cursor_version("options.after", cursor, original_package_id))
            .transpose()?;
        let before = options
            .before
            .as_deref()
            .map(|cursor| cursor_version("options.before", cursor, original_package_id))
            .transpose()?;
        if let Some(after) = after {
            versions.start = versions.start.max(after.saturating_add(1));
        }
        if let Some(before) = before {
            versions.end = versions.end.min(before);
        }

        Ok(Self {
            original_package_id,
            versions,
            descending,
            page_size: page_size(options.limit),
            cursor_mode: true,
            resumed: after.is_some() || before.is_some(),
            cursor_versions: [after, before],
        })
    }

    /// The most versions to read for one response: a full page, plus in page-token mode the
    /// first version of the next page, which the next page token points at.
    pub fn read_limit(&self) -> usize {
        if self.cursor_mode {
            self.page_size
        } else {
            self.page_size + 1
        }
    }

    /// Whether the request starts a listing, rather than continuing one from a page token or
    /// cursor.
    pub fn is_first_page(&self) -> bool {
        !self.resumed
    }

    /// The versions named by the request's cursors, each of which the server must serve before
    /// listing, in either order. A lineage's versions are published in version order, so a
    /// server serving a cursor's version serves every version from it down to where its history
    /// begins. A cursor naming a version the server does not serve comes from another view of the
    /// lineage, such as another replica's, and listing from this server's view would skip
    /// versions without saying so: a descending page would start below versions the server
    /// lacks, and an ascending page would end at the highest version the server serves, or find
    /// nothing above an `after` cursor, which a client cannot tell from the end of the lineage.
    /// The server rejects such a cursor. When it serves no version above the cursor's, the cursor
    /// lies past its tip, and it answers with
    /// [`crate::AtCheckpointUpperBound::unindexed_cursor_error`]. When it serves a version above
    /// the cursor's, the cursor's version lies below where its history begins, such as an old
    /// version of a system package on a node restored from a snapshot, and it answers as for
    /// pruned data (`BELOW_EARLIEST_AVAILABLE`). A backend holding every version from genesis
    /// only meets the first case.
    pub fn cursor_versions(&self) -> impl Iterator<Item = u64> {
        self.cursor_versions.into_iter().flatten()
    }

    /// One listed version, carrying its cursor.
    pub fn listed_version(
        &self,
        storage_id: ObjectID,
        version: u64,
        checkpoint: Option<u64>,
    ) -> PackageVersion {
        let cursor = PackageVersionCursor::Lineage {
            original_package_id: self.original_package_id,
            version,
        };
        let mut listed =
            PackageVersion::new(&storage_id.into(), version).with_cursor(cursor.encode());
        listed.checkpoint = checkpoint;
        listed
    }

    /// The response listing `versions`: at most [`Self::read_limit`] versions, read in this
    /// query's order.
    pub fn response(&self, mut versions: Vec<PackageVersion>) -> ListPackageVersionsResponse {
        let next_page_token = if !self.cursor_mode && versions.len() > self.page_size {
            versions.pop().and_then(|next| next.version).map(|version| {
                encode_page_token(PageToken {
                    original_package_id: self.original_package_id,
                    version,
                })
            })
        } else {
            None
        };

        ListPackageVersionsResponse::new(versions, next_page_token)
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

    let package_id = parse_package_id(package_id_str)?;

    // While a restore is still backfilling history, a lineage upgraded inside the backfill window,
    // such as a system package's, lacks the versions published in the part of the window the
    // backfill has not reached, and its restored latest version has no recorded publish
    // checkpoint yet, so a listing could come back partial or include versions published after
    // `at_checkpoint`. Every listing waits for the backfill, with or without `at_checkpoint`, as
    // the listing of every package does: `at_checkpoint_upper_bound` is `Unavailable` until it
    // finishes. The bound only grows, so reading it before the rows keeps it valid for them.
    let upper_bound = service.reader.at_checkpoint_upper_bound()?;

    if let Some(at_checkpoint) = request.at_checkpoint {
        // Both bounds are checked before the package is loaded: a package published above the
        // highest indexed checkpoint may not exist on this node yet, and answering `NotFound`
        // for it would hide that the bound is not served yet.
        //
        // Above the highest indexed checkpoint the index may be missing versions, so a bound
        // there could include versions published after it. It is rejected before the read, like
        // `GetPackage`.
        upper_bound.check(at_checkpoint)?;

        // A version with no recorded publish checkpoint was published before the history the
        // index holds begins. On a node that backfilled after restoring from a snapshot, that is
        // by the end of the checkpoint below the lowest one available at restore time, and the
        // rpc-store floor the restore stamped there keeps the shared lower bound at or above it.
        // A store that never backfills, such as a fork, reports where its history begins through
        // `get_lowest_checkpoint_with_index_history`. So such a version belongs under any bound the
        // shared lower bound accepts, and a bound below it is rejected.
        //
        // `StateReader::at_checkpoint_lower_bound` is meant to be read after the reads it bounds,
        // as `GetPackage`'s `at_checkpoint` does (see `get_package.rs`), but this check can run
        // before the read: pruning never deletes `package_versions` rows, and the bound only
        // rises, so a bound that passes stays correct for rows read afterwards.
        service
            .reader
            .at_checkpoint_lower_bound()?
            .check(at_checkpoint)?;
    }

    let original_package_id = load_package_by_id(service, package_id)?.original_package_id();
    let query = PackageVersionsQuery::new(&request, original_package_id)?;

    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(|| RpcError::new(tonic::Code::Unavailable, "rpc indexes are disabled"))?;

    // This node serves a version when it has the version's `package_versions` row. See
    // `PackageVersionsQuery::cursor_versions`.
    for version in query.cursor_versions() {
        let served = indexes
            .get_package_version_storage_id(original_package_id, version)
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
            .is_some();
        if served {
            continue;
        }
        let serves_higher_version = indexes
            .package_versions_iter(
                original_package_id,
                version.saturating_add(1),
                u64::MAX,
                false,
            )
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
            .next()
            .transpose()
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
            .is_some();
        return Err(if serves_higher_version {
            service.reader.at_checkpoint_lower_bound()?.pruned()
        } else {
            upper_bound.unindexed_cursor_error()
        });
    }

    let mut versions = vec![];
    let mut iter = indexes
        .package_versions_iter(
            original_package_id,
            query.versions.start,
            query.versions.end,
            query.descending,
        )
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    while versions.len() < query.read_limit() {
        let Some(result) = iter.next() else {
            break;
        };
        let entry = result.map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

        // Recorded publish checkpoints are non-decreasing in version, and versions with no
        // recorded checkpoint are the lowest and were published before the floor checked above.
        // An ascending scan therefore ends at the first version published after the bound, and a
        // descending scan meets the versions published after it first and skips them.
        if let (Some(at_checkpoint), Some(published)) = (request.at_checkpoint, entry.checkpoint)
            && published > at_checkpoint
        {
            if query.descending {
                continue;
            }
            break;
        }

        versions.push(query.listed_version(entry.storage_id, entry.version, entry.checkpoint));
    }

    Ok(query.response(versions))
}

/// The version that the cursor in request field `field` names in the upgrade lineage of the
/// package originally published at `original_package_id`.
fn cursor_version(field: &str, cursor: &[u8], original_package_id: ObjectID) -> Result<u64> {
    let cursor: PackageVersionCursor = bcs::from_bytes(cursor).map_err(|e| {
        FieldViolation::new(field)
            .with_description(format!("invalid cursor encoding: {e}"))
            .with_reason(ErrorReason::FieldInvalid)
    })?;
    match cursor {
        PackageVersionCursor::Lineage {
            original_package_id: cursor_package_id,
            version,
        } if cursor_package_id == original_package_id => Ok(version),
        PackageVersionCursor::Lineage { .. } => Err(FieldViolation::new(field)
            .with_description("cursor package ID does not match request package ID")
            .with_reason(ErrorReason::FieldInvalid)
            .into()),
    }
}

fn page_size(requested: Option<u32>) -> usize {
    requested
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE) as usize
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
