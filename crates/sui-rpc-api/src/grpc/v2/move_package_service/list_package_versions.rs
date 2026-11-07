// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::{Bound, Range};

use super::{load_package_by_id, parse_package_id};
use crate::ledger_history::query_options::below_serving_floor;
use crate::{AtCheckpointUpperBound, ErrorReason, Result, RpcError, RpcService};
use bytes::Bytes;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::{
    ListPackageVersionsRequest, ListPackageVersionsResponse, Ordering, PackageVersion, QueryOptions,
};
use sui_types::base_types::ObjectID;
use sui_types::storage::PackagePublishKey;

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
/// mint and accept the same cursors. BCS-encoded as an enum with one variant per listing, so a
/// listing can be added without changing the encoding of existing cursors.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PackageVersionCursor {
    /// A version in the upgrade lineage of the package originally published at
    /// `original_package_id`, listed in ascending version order.
    Lineage {
        original_package_id: ObjectID,
        version: u64,
    },
    /// A version in the listing of every package's versions, in [`PackagePublishKey`] order:
    /// `version` of the package originally published at `original_package_id`, published in
    /// `checkpoint`.
    AllPackages {
        checkpoint: u64,
        original_package_id: ObjectID,
        version: u64,
    },
}

impl PackageVersionCursor {
    pub fn encode(&self) -> Bytes {
        bcs::to_bytes(self).unwrap().into()
    }
}

/// The versions a `ListPackageVersions` request with `package_id` set lists and how its response
/// pages: the request's version bounds, page token, and cursors intersected into one version
/// range, validated against the lineage being listed. Public so other implementors of
/// `MovePackageService` parse requests and page responses the same way.
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
        if request.start_checkpoint.is_some() {
            return Err(FieldViolation::new("start_checkpoint")
                .with_description("start_checkpoint requires package_id to be unset")
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }

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

        let descending = descending(options)?;

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
        let mut listed = PackageVersion::new(&storage_id.into(), version)
            .with_cursor(cursor.encode())
            .with_original_id(self.original_package_id.to_canonical_string(true));
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

/// The versions a `ListPackageVersions` request with `package_id` unset lists, across every
/// package in [`PackagePublishKey`] order, and how its response pages: the request's checkpoint
/// bounds and cursors intersected into one range of keys. Public so other implementors of
/// `MovePackageService` parse requests and page responses the same way.
#[derive(Clone, Debug)]
pub struct AllPackageVersionsQuery {
    /// The lower end of the keys to list.
    pub lower: Bound<PackagePublishKey>,
    /// The upper end of the keys to list.
    pub upper: Bound<PackagePublishKey>,
    /// Whether to list from the highest key to the lowest.
    pub descending: bool,
    /// The number of versions in a full page.
    page_size: usize,
}

impl AllPackageVersionsQuery {
    /// Parse a request with `package_id` unset that lists the versions published from its
    /// `start_checkpoint` through its `at_checkpoint`, or when that is unset, through the highest
    /// checkpoint `upper_bound` admits. An unset `start_checkpoint` starts the listing at
    /// `lowest_checkpoint`, the lowest checkpoint the server can list every publish from. A
    /// `start_checkpoint` or `at_checkpoint` below it is `OUT_OF_RANGE`, and so, when
    /// `start_checkpoint` is unset, is a cursor below it. An `at_checkpoint` above `upper_bound`
    /// is `OUT_OF_RANGE`, and so, in either order, is a cursor above it.
    pub fn new(
        request: &ListPackageVersionsRequest,
        lowest_checkpoint: u64,
        upper_bound: AtCheckpointUpperBound,
    ) -> Result<Self> {
        for (field, set) in [
            ("page_size", request.page_size.is_some()),
            ("page_token", request.page_token.is_some()),
            ("start_version", request.start_version.is_some()),
            ("end_version", request.end_version.is_some()),
        ] {
            if set {
                return Err(FieldViolation::new(field)
                    .with_description(format!("{field} requires package_id"))
                    .with_reason(ErrorReason::FieldInvalid)
                    .into());
            }
        }

        let start_checkpoint = request.start_checkpoint.unwrap_or(lowest_checkpoint);
        if start_checkpoint < lowest_checkpoint {
            return Err(below_serving_floor(lowest_checkpoint));
        }

        if let Some(at_checkpoint) = request.at_checkpoint {
            upper_bound.check(at_checkpoint)?;
            if at_checkpoint < lowest_checkpoint {
                return Err(below_serving_floor(lowest_checkpoint));
            }
        }
        let highest_checkpoint = request
            .at_checkpoint
            .unwrap_or(upper_bound.highest_indexed());

        let default_options = QueryOptions::default();
        let options = request.options.as_ref().unwrap_or(&default_options);
        let descending = descending(options)?;
        let after = options
            .after
            .as_deref()
            .map(|cursor| cursor_key("options.after", cursor))
            .transpose()?;
        let before = options
            .before
            .as_deref()
            .map(|cursor| cursor_key("options.before", cursor))
            .transpose()?;

        // A cursor below `lowest_checkpoint` continues a listing from where this server cannot
        // list every publish, such as one another server answered, or one this server answered
        // before its floor rose. Moving it up to the floor would skip versions without saying so.
        // An explicit `start_checkpoint` at or above the floor bounds the listing instead.
        if request.start_checkpoint.is_none() {
            for cursor in after.iter().chain(&before) {
                if cursor.checkpoint < lowest_checkpoint {
                    return Err(below_serving_floor(lowest_checkpoint));
                }
            }
        }

        // A cursor above the highest checkpoint this server has indexed comes from a newer view
        // of the ledger, such as another replica's, and listing from this server's view would
        // skip versions without saying so: a descending page would start below that cursor and
        // skip the versions published in between, and an ascending page would end at the
        // highest checkpoint, or find nothing above an `after` cursor there, which a client
        // cannot tell from the end of the listing.
        for cursor in after.iter().chain(&before) {
            upper_bound.check_cursor(cursor.checkpoint)?;
        }

        // The lowest and highest keys any version published in the checkpoint range can have.
        // A cursor inside the range narrows it; one outside leaves that end as it is.
        let first = PackagePublishKey {
            checkpoint: start_checkpoint,
            original_id: ObjectID::ZERO,
            version: 0,
        };
        let last = PackagePublishKey {
            checkpoint: highest_checkpoint,
            original_id: ObjectID::MAX,
            version: u64::MAX,
        };
        let lower = match after {
            Some(after) if after >= first => Bound::Excluded(after),
            _ => Bound::Included(first),
        };
        let upper = match before {
            Some(before) if before <= last => Bound::Excluded(before),
            _ => Bound::Included(last),
        };

        Ok(Self {
            lower,
            upper,
            descending,
            page_size: page_size(options.limit),
        })
    }

    /// The most versions to read for one response.
    pub fn limit(&self) -> usize {
        self.page_size
    }

    /// One listed version, carrying its cursor.
    pub fn listed_version(&self, key: PackagePublishKey, storage_id: ObjectID) -> PackageVersion {
        let cursor = PackageVersionCursor::AllPackages {
            checkpoint: key.checkpoint,
            original_package_id: key.original_id,
            version: key.version,
        };
        PackageVersion::new(&storage_id.into(), key.version)
            .with_checkpoint(key.checkpoint)
            .with_cursor(cursor.encode())
            .with_original_id(key.original_id.to_canonical_string(true))
    }
}

/// List the versions of a package's upgrade lineage, optionally only those published at or
/// before `at_checkpoint`, or with `package_id` unset, the versions of every package.
///
/// Versions with no recorded publish checkpoint are listed with `checkpoint` unset; see the
/// `at_checkpoint` bounds below for why a bound may include them.
#[tracing::instrument(skip(service))]
pub fn list_package_versions(
    service: &RpcService,
    request: ListPackageVersionsRequest,
) -> Result<ListPackageVersionsResponse> {
    let Some(package_id_str) = request.package_id.as_deref() else {
        return list_all_package_versions(service, &request);
    };

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

/// List the versions of every package published from the request's `start_checkpoint` through
/// its `at_checkpoint`, or through the highest indexed checkpoint when that is unset, in
/// [`PackagePublishKey`] order.
fn list_all_package_versions(
    service: &RpcService,
    request: &ListPackageVersionsRequest,
) -> Result<ListPackageVersionsResponse> {
    // The index holds only versions with a recorded publish checkpoint. Once any post-restore
    // backfill has finished (checked below), the versions a store loaded from a live-object
    // snapshot (or a fork's seed) were published before the first checkpoint with index history,
    // so a listing reaching below it could miss them. On a node that backfilled after restoring
    // from a snapshot, the objects floor is at or above that point; a store that never backfills
    // reports its own through `get_lowest_checkpoint_with_index_history`. Pruning never deletes
    // these rows and the floor only rises, so the floor can be read before the rows.
    let inner = service.reader.inner();
    let lowest_available = inner
        .get_lowest_available_checkpoint_objects()?
        .max(inner.get_lowest_checkpoint_with_index_history()?);

    // While a restore is still backfilling history, the index ends at the checkpoint the
    // backfill has reached, below the checkpoints the node serves elsewhere, and the versions
    // published after it appear only as restored rows with no publish checkpoint. Every listing
    // waits for the backfill, as checkpoint-pinned reads do. Otherwise the listing ends at
    // `at_checkpoint`, bounded like one package's listing, or at the highest checkpoint every rpc
    // index has committed: the pipeline writing the index can run ahead of it, and stopping there
    // matches the height the other reads serve. The bound only grows, so reading it before the
    // rows keeps it valid for them.
    let query = AllPackageVersionsQuery::new(
        request,
        lowest_available,
        service.reader.at_checkpoint_upper_bound()?,
    )?;

    let indexes = inner
        .indexes()
        .ok_or_else(|| RpcError::new(tonic::Code::Unavailable, "rpc indexes are disabled"))?;

    let versions = indexes
        .packages_by_checkpoint_iter(query.lower, query.upper, query.descending)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        .take(query.limit())
        .map(|row| {
            let (key, storage_id) =
                row.map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
            Ok(query.listed_version(key, storage_id))
        })
        .collect::<Result<_>>()?;

    Ok(ListPackageVersionsResponse::new(versions, None))
}

/// The cursor in request field `field`.
fn decode_cursor(field: &str, cursor: &[u8]) -> Result<PackageVersionCursor> {
    bcs::from_bytes(cursor).map_err(|e| {
        FieldViolation::new(field)
            .with_description(format!("invalid cursor encoding: {e}"))
            .with_reason(ErrorReason::FieldInvalid)
            .into()
    })
}

/// The version that the cursor in request field `field` names in the upgrade lineage of the
/// package originally published at `original_package_id`.
fn cursor_version(field: &str, cursor: &[u8], original_package_id: ObjectID) -> Result<u64> {
    match decode_cursor(field, cursor)? {
        PackageVersionCursor::Lineage {
            original_package_id: cursor_package_id,
            version,
        } if cursor_package_id == original_package_id => Ok(version),
        PackageVersionCursor::Lineage { .. } => Err(FieldViolation::new(field)
            .with_description("cursor package ID does not match request package ID")
            .with_reason(ErrorReason::FieldInvalid)
            .into()),
        PackageVersionCursor::AllPackages { .. } => Err(FieldViolation::new(field)
            .with_description(
                "cursor is from the listing of every package, which sets no package_id",
            )
            .with_reason(ErrorReason::FieldInvalid)
            .into()),
    }
}

/// The key that the cursor in request field `field` names in the listing of every package.
fn cursor_key(field: &str, cursor: &[u8]) -> Result<PackagePublishKey> {
    match decode_cursor(field, cursor)? {
        PackageVersionCursor::AllPackages {
            checkpoint,
            original_package_id,
            version,
        } => Ok(PackagePublishKey {
            checkpoint,
            original_id: original_package_id,
            version,
        }),
        PackageVersionCursor::Lineage { .. } => Err(FieldViolation::new(field)
            .with_description("cursor is from the listing of one package, which sets package_id")
            .with_reason(ErrorReason::FieldInvalid)
            .into()),
    }
}

/// Whether `options` lists in descending order.
fn descending(options: &QueryOptions) -> Result<bool> {
    match options.ordering.map(Ordering::try_from) {
        None | Some(Ok(Ordering::Ascending)) => Ok(false),
        Some(Ok(Ordering::Descending)) => Ok(true),
        Some(_) => Err(FieldViolation::new("options.ordering")
            .with_description("invalid ordering")
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
