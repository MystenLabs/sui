// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::{Bound, Range};

use super::{load_package_by_id, parse_package_id};
use crate::ledger_history::query_options::{
    above_latest_available, below_oldest_retained_version, below_serving_floor,
};
use crate::reader::{AtCheckpointLowerBound, indexes_disabled};
use crate::{AtCheckpointUpperBound, ErrorReason, Result, RpcError, RpcService};
use bytes::Bytes;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::{
    ListPackageVersionsRequest, ListPackageVersionsResponse, Ordering, PackageVersion, QueryOptions,
};
use sui_types::base_types::ObjectID;
use sui_types::object::OBJECT_START_VERSION;
use sui_types::storage::IndexedPackageVersion;
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
    /// The versions the request's page token, or its `after` and `before` cursors, name.
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
                cursor_versions: [page_token.map(|token| token.version), None],
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

    /// The versions named by the request's page token or cursors, each of which the server must
    /// serve to answer the listing, in either order. A page token names the first version of the
    /// next page, a version the server that minted it served. A lineage's versions are published in
    /// version order, so a server serving such a version serves every version from it down to
    /// where its history begins. A page token or cursor naming a version the server does not
    /// serve comes from another view of the lineage, such as another replica's, and listing from
    /// this server's view would skip versions without saying so: a descending page would start
    /// below versions the server lacks, and an ascending page would end at the highest version
    /// the server serves, or find nothing from the page token's version or above an `after`
    /// cursor, which a client cannot tell from the end of the lineage.
    /// The server rejects such a page token or cursor. When it serves no version above the named
    /// one, that version lies past its tip, and it answers with
    /// [`crate::AtCheckpointUpperBound::unindexed_cursor_error`]. When it serves a version above
    /// the named one, that version lies below where its history begins, such as an old version
    /// of a system package on a node restored from a snapshot, and it answers as for
    /// pruned data (`BELOW_EARLIEST_AVAILABLE`). A backend holding every version from genesis
    /// only meets the first case. Versions below `OBJECT_START_VERSION` are left out: no lineage
    /// has one, so a page token or cursor naming one skips nothing.
    pub fn cursor_versions(&self) -> impl Iterator<Item = u64> {
        self.cursor_versions
            .into_iter()
            .flatten()
            .filter(|version| *version >= OBJECT_START_VERSION.value())
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
    /// The lowest checkpoint this server can list every publish from, when a page that ends short
    /// ends there: in a descending listing with neither `start_checkpoint` nor an `after` cursor,
    /// on a server whose floor is above genesis.
    short_page_floor: Option<u64>,
}

impl AllPackageVersionsQuery {
    /// Parse a request with `package_id` unset that lists the versions published from its
    /// `start_checkpoint` through its `at_checkpoint`, or when that is unset, through the highest
    /// checkpoint `upper_bound` admits. An unset `start_checkpoint` starts the listing at
    /// `lowest_checkpoint`, the lowest checkpoint the server can list every publish from. A
    /// `start_checkpoint` or `at_checkpoint` below it is `OUT_OF_RANGE`, and so, when
    /// `start_checkpoint` is unset, is a cursor below it. A `start_checkpoint` or `at_checkpoint`
    /// above `upper_bound` is `OUT_OF_RANGE`, and so, in either order, is a cursor above it.
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

        // A `start_checkpoint` above the highest checkpoint this server has indexed starts where
        // the index has not caught up: an empty page there would read as the end of the listing,
        // though versions published from it on are not indexed yet.
        if let Some(requested) = request.start_checkpoint
            && requested > upper_bound.highest_indexed()
        {
            return Err(above_latest_available(
                "start_checkpoint",
                requested,
                upper_bound.highest_indexed().saturating_add(1),
            ));
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

        // Without `start_checkpoint`, a descending listing runs down to `lowest_checkpoint`.
        // Above genesis, the listing continues below it on a server holding that history (at
        // least to the packages published at genesis), so a page ending there short would read
        // as the end of the listing. With an `after` cursor, which lies at or above
        // `lowest_checkpoint` (checked above), the listing ends at the cursor instead.
        let short_page_floor = (descending
            && request.start_checkpoint.is_none()
            && lowest_checkpoint > 0
            && matches!(lower, Bound::Included(_)))
        .then_some(lowest_checkpoint);

        Ok(Self {
            lower,
            upper,
            descending,
            page_size: page_size(options.limit),
            short_page_floor,
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

    /// The response listing `versions`, at most [`Self::limit`] read in this query's order. A
    /// page that ends short at the floor of a descending listing with no `start_checkpoint` is
    /// `OUT_OF_RANGE` (`BELOW_EARLIEST_AVAILABLE`) instead: this server cannot list the versions
    /// published below its floor, and a short page would say the listing ended there. A full
    /// page is answered, and the request for the page after it is refused if that one is short.
    pub fn response(&self, versions: Vec<PackageVersion>) -> Result<ListPackageVersionsResponse> {
        if let Some(floor) = self.short_page_floor
            && versions.len() < self.page_size
        {
            return Err(below_serving_floor(floor));
        }
        Ok(ListPackageVersionsResponse::new(versions, None))
    }
}

/// List the versions of a package's upgrade lineage published at or before `at_checkpoint`, or,
/// without it, at or before the highest checkpoint every rpc index has committed; or, with
/// `package_id` unset, the versions of every package.
///
/// Versions with no recorded publish checkpoint are listed with `checkpoint` unset; see the
/// `at_checkpoint` bounds below for why a bound may include them.
///
/// In cursor mode or with `at_checkpoint`, a page that reaches below the lowest version of the
/// lineage this node stores, such as a system package's versions replaced before the pruning
/// floor, is refused as `OutOfRange` (`BELOW_EARLIEST_AVAILABLE`) with that version as
/// `lowestAvailableVersion`. A page token or cursor naming a version this node has no row for,
/// below versions it serves, as on a node restored from a snapshot or a fork, is refused the same
/// way, except version 0, which no lineage has. A pruned node keeps the rows of the versions it
/// pruned, so it accepts a page token or cursor naming one, and serves the page it starts when
/// that page stays at or above the lowest stored version.
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
        .ok_or_else(indexes_disabled)?;

    // Without `at_checkpoint`, the listing stops at the highest indexed checkpoint, so it never
    // includes a version published above every checkpoint this node serves as an `at_checkpoint`.
    let cp_bound = request
        .at_checkpoint
        .unwrap_or(upper_bound.highest_indexed());
    // The refusal for a listing from `range_start` below the lowest version of the lineage this
    // node stores. The lower bound is read after the rows, as `GetPackage` reads it.
    let below_lowest_stored = |range_start: u64, lowest_stored: Option<u64>| -> Result<RpcError> {
        Ok(refuse_range_below_lowest_stored(
            service.reader.at_checkpoint_lower_bound()?,
            original_package_id,
            range_start,
            lowest_stored,
        ))
    };
    // This node serves a version when it has the version's `package_versions` row, published at
    // or below the highest indexed checkpoint or before its history begins. Recorded publish
    // checkpoints are non-decreasing in version, so the lowest version above one this node does
    // not serve tells whether it serves any higher version. See
    // `PackageVersionsQuery::cursor_versions`.
    let served = |entry: &IndexedPackageVersion| {
        entry
            .checkpoint
            .is_none_or(|published| published <= upper_bound.highest_indexed())
    };
    let is_stored = |row: &IndexedPackageVersion| {
        service
            .reader
            .inner()
            .get_object_by_key(&row.storage_id, row.version.into())
            .is_some()
    };
    // The lineage's rows from `version` up, in version order.
    let rows_from = |version: u64| {
        indexes
            .package_versions_iter(original_package_id, version, u64::MAX, false)
            .map(|rows| {
                rows.map(|row| row.map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string())))
            })
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))
    };

    for version in query.cursor_versions() {
        let mut rows = rows_from(version)?;
        let higher = match rows.next().transpose()? {
            Some(entry) if entry.version == version && served(&entry) => continue,
            Some(entry) if entry.version == version => rows.next().transpose()?,
            first_higher => first_higher,
        };
        if !higher.as_ref().is_some_and(served) {
            return Err(upper_bound.unindexed_cursor_error());
        }
        // The lineage has a row this node serves above the named version and none at it, so the
        // named version lies below where this node's history of the lineage begins.
        let lowest_stored =
            match range_start_against_stored_versions(version, rows_from(version)?, is_stored)? {
                RangeStart::BelowLowestStored { lowest_stored } => lowest_stored,
                RangeStart::Stored => None,
            };
        return Err(below_lowest_stored(version, lowest_stored)?);
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
        if let Some(published) = entry.checkpoint
            && published > cp_bound
        {
            if query.descending {
                continue;
            }
            break;
        }

        versions.push(query.listed_version(entry.storage_id, entry.version, entry.checkpoint));
    }

    // In cursor mode or under `at_checkpoint`, a page that reaches below the lowest version of the
    // lineage this node stores, such as a system package's versions replaced before the pruning
    // floor, is refused rather than answered with a history that is cut short or lists versions
    // this node cannot load: `package_versions` rows outlive pruning (see
    // `range_start_against_stored_versions`). A full descending page reaches down to its lowest
    // listed version; every other page reaches the low end of the query's range, and one that
    // ends at or below `OBJECT_START_VERSION` holds no version on any node. A legacy page-token
    // listing without `at_checkpoint` lists every row it reaches, including system-package
    // versions whose packages are pruned.
    if request.at_checkpoint.is_some() || request.options.is_some() {
        let lowest_listed = versions.last().and_then(|listed| listed.version);
        let range_start = match lowest_listed {
            Some(version) if query.descending && versions.len() == query.read_limit() => version,
            _ => query.versions.start.max(OBJECT_START_VERSION.value()),
        };
        if range_start < query.versions.end
            && let RangeStart::BelowLowestStored { lowest_stored } =
                range_start_against_stored_versions(
                    range_start,
                    rows_from(range_start)?,
                    is_stored,
                )?
        {
            // As `GetPackage` does for a version this node no longer stores.
            service.reader.require_object_version_history()?;
            return Err(below_lowest_stored(range_start, lowest_stored)?);
        }
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
    let inner = service.reader.inner();
    let indexes = inner.indexes().ok_or_else(indexes_disabled)?;

    // The index holds only versions with a recorded publish checkpoint. Once any post-restore
    // backfill has finished (checked below), the versions a store loaded from a live-object
    // snapshot (or a fork's seed) were published before the first checkpoint with index history,
    // so a listing reaching below it could miss them. On a node that backfilled after restoring
    // from a snapshot, the objects floor is at or above that point; a store that never backfills
    // reports its own through `get_lowest_checkpoint_with_index_history`. Pruning never deletes
    // these rows and the floor only rises, so the floor can be read before the rows.
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

    query.response(versions)
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

/// How a lineage listing that starts at some version stands against the versions of the lineage
/// this node stores. See [`range_start_against_stored_versions`].
#[derive(Debug, PartialEq)]
enum RangeStart {
    /// This node stores the version the range starts at, or the lineage has no version at or
    /// above it.
    Stored,
    /// The range starts below `lowest_stored`, the lowest version of the lineage this node stores,
    /// or, when it is `None`, below every version it has a row for, none of which it stores.
    BelowLowestStored { lowest_stored: Option<u64> },
}

/// Compare a lineage listing that starts at `range_start` with the versions of the lineage this
/// node stores. `rows` are the lineage's `package_versions` rows at or above `range_start`, in
/// version order, and `is_stored` reports whether this node stores a row's package.
///
/// A lineage's versions run from `OBJECT_START_VERSION` up without gaps, and this node's rows for
/// it run without gaps from the lowest version it indexed: a node restored from a snapshot, or a
/// fork, has no rows for the system-package versions replaced before its history begins. So a
/// first row above `range_start` means the versions below that row are gone. Pruning deletes a
/// version only once a newer version under the same ID has replaced it, which only happens to
/// system packages, upgraded in place, and it deletes them oldest first. So once a version is
/// stored, every later version is too, and the scan stops at the first stored version. A version
/// whose next row lives under another ID was never replaced in place, so it is stored without
/// asking `is_stored`. The last row is asked: with no next row, the scan cannot tell whether the
/// lineage is upgraded in place.
fn range_start_against_stored_versions(
    range_start: u64,
    rows: impl IntoIterator<Item = Result<IndexedPackageVersion>>,
    mut is_stored: impl FnMut(&IndexedPackageVersion) -> bool,
) -> Result<RangeStart> {
    let mut rows = rows.into_iter().peekable();
    let mut scanned_any = false;
    while let Some(row) = rows.next().transpose()? {
        scanned_any = true;
        let may_be_replaced_in_place = match rows.peek() {
            Some(Ok(next)) => next.storage_id == row.storage_id,
            Some(Err(_)) | None => true,
        };
        if !may_be_replaced_in_place || is_stored(&row) {
            // Rows run upward from `range_start`, so only the first row can be at it.
            return Ok(if row.version == range_start {
                RangeStart::Stored
            } else {
                RangeStart::BelowLowestStored {
                    lowest_stored: Some(row.version),
                }
            });
        }
    }
    Ok(if scanned_any {
        RangeStart::BelowLowestStored {
            lowest_stored: None,
        }
    } else {
        RangeStart::Stored
    })
}

/// The refusal for a listing of `original_package_id`'s lineage from `range_start`, below
/// `lowest_stored`, the lowest version of the lineage this node stores (`None` when it stores
/// none of the versions it has rows for), given the node's lower bound `lower`. It is
/// `OutOfRange` (`BELOW_EARLIEST_AVAILABLE`) when the node's object history has a floor, and an
/// internal error on a node that has pruned nothing, which stores every version.
fn refuse_range_below_lowest_stored(
    lower: AtCheckpointLowerBound,
    original_package_id: ObjectID,
    range_start: u64,
    lowest_stored: Option<u64>,
) -> RpcError {
    let Some(floor_checkpoint) = lower.pruned_floor() else {
        return RpcError::new(
            tonic::Code::Internal,
            format!(
                "version {range_start} of package {original_package_id} is not stored, although \
                 this node has pruned no object history"
            ),
        );
    };
    match lowest_stored {
        Some(version) => below_oldest_retained_version(floor_checkpoint, version),
        None => lower.pruned(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger_history::query_options::{
        assert_above_latest_available_error, assert_below_serving_floor_error,
    };
    use std::cell::Cell;

    /// The highest checkpoint the tests' server has indexed.
    const HIGHEST_INDEXED: u64 = 60;

    /// A request listing every package's versions from `start_checkpoint` through
    /// `at_checkpoint`, `limit` at a time in `ordering`, after `after`.
    fn all_packages_request(
        start_checkpoint: Option<u64>,
        at_checkpoint: Option<u64>,
        limit: u32,
        ordering: Ordering,
        after: Option<PackageVersionCursor>,
    ) -> ListPackageVersionsRequest {
        let mut options = QueryOptions::default();
        options.limit = Some(limit);
        options.ordering = Some(ordering as i32);
        options.after = after.map(|cursor| cursor.encode());
        let mut request = ListPackageVersionsRequest::default();
        request.start_checkpoint = start_checkpoint;
        request.at_checkpoint = at_checkpoint;
        request.options = Some(options);
        request
    }

    /// Parse `request` on a server that can list every publish from `lowest_checkpoint` and has
    /// indexed through [`HIGHEST_INDEXED`].
    fn parse(
        request: &ListPackageVersionsRequest,
        lowest_checkpoint: u64,
    ) -> Result<AllPackageVersionsQuery> {
        AllPackageVersionsQuery::new(
            request,
            lowest_checkpoint,
            AtCheckpointUpperBound::new(HIGHEST_INDEXED),
        )
    }

    /// `count` versions, as `query` lists them, published one per checkpoint down from 50.
    fn page(query: &AllPackageVersionsQuery, count: u64) -> Vec<PackageVersion> {
        (0..count)
            .map(|offset| {
                let key = PackagePublishKey {
                    checkpoint: 50 - offset,
                    original_id: ObjectID::ZERO,
                    version: 1,
                };
                query.listed_version(key, ObjectID::ZERO)
            })
            .collect()
    }

    /// A `start_checkpoint` above the highest indexed checkpoint is rejected rather than answered
    /// with an empty page, whatever `at_checkpoint` and the ordering are; one at it is served.
    #[test]
    fn start_checkpoint_above_highest_indexed_is_rejected() {
        for at_checkpoint in [None, Some(20), Some(HIGHEST_INDEXED)] {
            for start_checkpoint in [HIGHEST_INDEXED + 1, u64::MAX] {
                for ordering in [Ordering::Ascending, Ordering::Descending] {
                    let request = all_packages_request(
                        Some(start_checkpoint),
                        at_checkpoint,
                        10,
                        ordering,
                        None,
                    );
                    assert_above_latest_available_error(
                        parse(&request, 0).unwrap_err(),
                        "start_checkpoint",
                        start_checkpoint,
                        HIGHEST_INDEXED + 1,
                    );
                }
            }
        }

        let request =
            all_packages_request(Some(HIGHEST_INDEXED), None, 10, Ordering::Ascending, None);
        let query = parse(&request, 0).unwrap();
        assert_eq!(
            query.lower,
            Bound::Included(PackagePublishKey {
                checkpoint: HIGHEST_INDEXED,
                original_id: ObjectID::ZERO,
                version: 0,
            })
        );
        assert!(query.response(vec![]).unwrap().versions.is_empty());
    }

    /// A descending listing with no `start_checkpoint` runs down to the server's floor. Above
    /// genesis, a page that ends short there is rejected, empty or not, while a full page is
    /// answered. A short page that ends anywhere else is answered.
    #[test]
    fn descending_page_ending_short_at_the_floor_is_rejected() {
        const FLOOR: u64 = 10;

        // The first page, and a later one continuing below the previous page's last cursor:
        // `before` bounds only the top of the range, so the page still runs down to the floor.
        let before = PackageVersionCursor::AllPackages {
            checkpoint: 30,
            original_package_id: ObjectID::ZERO,
            version: 1,
        };
        let mut continuation = all_packages_request(None, None, 2, Ordering::Descending, None);
        continuation.options.as_mut().unwrap().before = Some(before.encode());
        for request in [
            all_packages_request(None, None, 2, Ordering::Descending, None),
            continuation,
        ] {
            let query = parse(&request, FLOOR).unwrap();
            assert_eq!(query.response(page(&query, 2)).unwrap().versions.len(), 2);
            for count in [0, 1] {
                assert_below_serving_floor_error(
                    query.response(page(&query, count)).unwrap_err(),
                    FLOOR,
                );
            }
        }

        // An ascending page ends at the highest checkpoint, and a descending one at an explicit
        // `start_checkpoint`, at an `after` cursor, or at genesis.
        let after = PackageVersionCursor::AllPackages {
            checkpoint: 20,
            original_package_id: ObjectID::ZERO,
            version: 1,
        };
        for (request, lowest_checkpoint) in [
            (
                all_packages_request(None, None, 2, Ordering::Ascending, None),
                FLOOR,
            ),
            (
                all_packages_request(Some(FLOOR), None, 2, Ordering::Descending, None),
                FLOOR,
            ),
            (
                all_packages_request(None, None, 2, Ordering::Descending, Some(after)),
                FLOOR,
            ),
            (
                all_packages_request(None, None, 2, Ordering::Descending, None),
                0,
            ),
        ] {
            let query = parse(&request, lowest_checkpoint).unwrap();
            let response = query.response(page(&query, 1)).unwrap();
            assert_eq!(response.versions.len(), 1, "{request:?}");
        }
    }

    /// `package_versions` rows for `versions` of a system package, upgraded in place, so every
    /// version lives under the package's own ID.
    fn in_place_rows(versions: std::ops::RangeInclusive<u64>) -> Vec<IndexedPackageVersion> {
        versions
            .map(|version| IndexedPackageVersion {
                version,
                storage_id: ObjectID::from_single_byte(0x2),
                checkpoint: Some(version * 10),
            })
            .collect()
    }

    /// Compare a listing from `range_start` over `rows` with the versions `stored` accepts, and
    /// count the packages probed.
    fn check(
        range_start: u64,
        rows: &[IndexedPackageVersion],
        stored: impl Fn(u64) -> bool,
    ) -> (RangeStart, usize) {
        let probes = Cell::new(0);
        let outcome = range_start_against_stored_versions(
            range_start,
            rows.iter()
                .filter(|row| row.version >= range_start)
                .copied()
                .map(Ok),
            |row| {
                probes.set(probes.get() + 1);
                stored(row.version)
            },
        )
        .unwrap();
        (outcome, probes.get())
    }

    fn below(lowest_stored: u64) -> RangeStart {
        RangeStart::BelowLowestStored {
            lowest_stored: Some(lowest_stored),
        }
    }

    #[test]
    fn unpruned_lineage_is_stored_from_every_start() {
        // A user package's versions each live under their own ID and are never replaced, so only
        // the last version is probed: with no next row, the scan cannot tell whether the lineage
        // is upgraded in place.
        let rows: Vec<_> = (1..=3)
            .map(|version| IndexedPackageVersion {
                version,
                storage_id: ObjectID::from_single_byte(version as u8),
                checkpoint: Some(version * 10),
            })
            .collect();
        for (range_start, probes) in [(1, 0), (2, 0), (3, 1)] {
            assert_eq!(
                check(range_start, &rows, |_| true),
                (RangeStart::Stored, probes),
                "range_start {range_start}"
            );
        }
        // Past the last version, the listing is empty and nothing is probed.
        assert_eq!(check(4, &rows, |_| true), (RangeStart::Stored, 0));
    }

    #[test]
    fn pruned_versions_refuse_ranges_starting_below_the_lowest_stored() {
        // Versions 1 and 2 were replaced before the pruning floor and their packages deleted,
        // but their rows remain.
        let rows = in_place_rows(1..=6);
        let stored = |version| version >= 3;
        // The scan probes only up to the first stored version.
        assert_eq!(check(1, &rows, stored), (below(3), 3));
        assert_eq!(check(2, &rows, stored), (below(3), 2));
        assert_eq!(check(3, &rows, stored), (RangeStart::Stored, 1));
        assert_eq!(check(6, &rows, stored), (RangeStart::Stored, 1));
        assert_eq!(check(7, &rows, stored), (RangeStart::Stored, 0));
    }

    #[test]
    fn versions_without_rows_refuse_ranges_starting_below_them() {
        // A node restored from a snapshot has no rows for the versions replaced before its
        // history begins, and stores every version it has a row for.
        let rows = in_place_rows(3..=4);
        for range_start in 1..=2 {
            assert_eq!(
                check(range_start, &rows, |_| true),
                (below(3), 1),
                "range_start {range_start}"
            );
        }
        assert_eq!(check(3, &rows, |_| true), (RangeStart::Stored, 1));
    }

    #[test]
    fn rows_with_no_stored_version_refuse_without_a_lowest_stored_version() {
        let rows = in_place_rows(1..=2);
        assert_eq!(
            check(1, &rows, |_| false),
            (
                RangeStart::BelowLowestStored {
                    lowest_stored: None
                },
                2
            )
        );
    }

    #[test]
    fn refusal_below_lowest_stored_is_internal_on_a_node_that_pruned_nothing() {
        use crate::ledger_history::query_options::{
            assert_below_oldest_retained_version_error, assert_below_serving_floor_error,
        };

        let package = ObjectID::from_single_byte(0x2);
        for lowest_stored in [Some(3), None] {
            let status: tonic::Status = refuse_range_below_lowest_stored(
                AtCheckpointLowerBound::new(0),
                package,
                1,
                lowest_stored,
            )
            .into();
            assert_eq!(
                status.code(),
                tonic::Code::Internal,
                "lowest_stored {lowest_stored:?}"
            );
        }

        // Genesis pruned: the floor is checkpoint 0, and the refusal is `OutOfRange`.
        assert_below_oldest_retained_version_error(
            refuse_range_below_lowest_stored(AtCheckpointLowerBound::new(1), package, 1, Some(3)),
            0,
            3,
        );
        assert_below_oldest_retained_version_error(
            refuse_range_below_lowest_stored(AtCheckpointLowerBound::new(10), package, 1, Some(3)),
            9,
            3,
        );
        assert_below_serving_floor_error(
            refuse_range_below_lowest_stored(AtCheckpointLowerBound::new(10), package, 1, None),
            9,
        );
    }
}
