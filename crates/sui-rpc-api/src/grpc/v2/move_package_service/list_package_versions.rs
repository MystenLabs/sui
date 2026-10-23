// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::{Bound, Range};

use super::load_package;
use crate::{ErrorReason, Result, RpcError, RpcService};
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

    /// Whether the request's version bounds, page token, or cursors exclude any version.
    pub fn is_version_bounded(&self) -> bool {
        self.versions != (0..u64::MAX)
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
    /// `start_checkpoint` through `highest_checkpoint`. An unset `start_checkpoint` starts the
    /// listing at `lowest_checkpoint`, the lowest checkpoint the server can list every publish
    /// from. A `start_checkpoint` below it is `OUT_OF_RANGE`, and so, when `start_checkpoint` is
    /// unset, is a cursor below it.
    pub fn new(
        request: &ListPackageVersionsRequest,
        lowest_checkpoint: u64,
        highest_checkpoint: u64,
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
            return Err(checkpoint_pruned_error(start_checkpoint, lowest_checkpoint));
        }

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
                    return Err(checkpoint_pruned_error(
                        cursor.checkpoint,
                        lowest_checkpoint,
                    ));
                }
            }
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

    let package = load_package(service, package_id_str)?;
    let original_package_id = package.original_package_id();
    let query = PackageVersionsQuery::new(&request, original_package_id)?;

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
            return Err(checkpoint_pruned_error(at_checkpoint, lowest_available));
        }
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
/// its `at_checkpoint`, in [`PackagePublishKey`] order.
fn list_all_package_versions(
    service: &RpcService,
    request: &ListPackageVersionsRequest,
) -> Result<ListPackageVersionsResponse> {
    // The index holds only versions with a recorded publish checkpoint. Once any post-restore
    // backfill has finished (checked below), the versions a store loaded from a live-object
    // snapshot (or a fork's seed) were published before the lowest checkpoint the indexes record
    // every publish from, so a listing reaching below it could miss them. On a node that
    // backfilled after restoring from a snapshot, the objects floor is at or above that point; a
    // store that never backfills reports its own through
    // `get_lowest_checkpoint_for_publish_listings`. Pruning never deletes these rows and the
    // floor only rises, so the floor can be read before the rows.
    let inner = service.reader.inner();
    let lowest_available = inner
        .get_lowest_available_checkpoint_objects()?
        .max(inner.get_lowest_checkpoint_for_publish_listings()?);

    // Without `at_checkpoint`, the listing runs to the end of the index. The pipeline writing it
    // commits checkpoints in order, so the index never has a gap a later page could miss.
    let query = AllPackageVersionsQuery::new(
        request,
        lowest_available,
        request.at_checkpoint.unwrap_or(u64::MAX),
    )?;

    // While a restore is still backfilling history, the index ends at the checkpoint the
    // backfill has reached, below the checkpoints the node serves elsewhere, and the versions
    // published after it appear only as restored rows with no publish checkpoint. Every listing
    // waits for the backfill, as checkpoint-pinned reads do, and `at_checkpoint` is bounded like
    // one package's listing.
    let upper_bound = service.reader.at_checkpoint_upper_bound()?;
    if let Some(at_checkpoint) = request.at_checkpoint {
        upper_bound.check(at_checkpoint)?;
        if at_checkpoint < lowest_available {
            return Err(checkpoint_pruned_error(at_checkpoint, lowest_available));
        }
    }

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

/// The error for a listing bounded below the lowest checkpoint the server can answer it at.
fn checkpoint_pruned_error(checkpoint: u64, lowest_available: u64) -> RpcError {
    RpcError::new(
        tonic::Code::OutOfRange,
        format!(
            "requested checkpoint {checkpoint} has been pruned; lowest available checkpoint is \
             {lowest_available}",
        ),
    )
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
