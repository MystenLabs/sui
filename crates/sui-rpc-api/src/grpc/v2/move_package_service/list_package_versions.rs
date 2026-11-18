// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::{Bound, Range};

use super::{load_package_by_id, parse_package_id};
use crate::ledger_history::query_options::{
    ScanBounds, above_latest_available, below_serving_floor, validate_checkpoint_bounds,
};
use crate::reader::{AtCheckpointLowerBound, indexes_disabled};
use crate::{AtCheckpointUpperBound, ErrorReason, Result, RpcError, RpcService};
use bytes::Bytes;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::{
    ListPackageVersionsRequest, ListPackageVersionsResponse, Ordering, PackageVersion, QueryEnd,
    QueryEndReason, QueryOptions,
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
///
/// Every cursor records `minted_at`, the highest checkpoint the server that minted it had
/// indexed (kv-rpc: its served height). A server that does not know a lineage cursor's package
/// as of its own highest indexed checkpoint uses it to tell a cursor from a replica ahead of it,
/// which it answers as a cursor past its tip, from one for a package that does not exist (see
/// [`unknown_package_response`]). The listing of every package loads no package: it checks its
/// cursors against the tip by the checkpoint that published their version.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PackageVersionCursor {
    /// A version in the upgrade lineage of the package originally published at
    /// `original_package_id`, listed in ascending version order.
    Lineage {
        original_package_id: ObjectID,
        version: u64,
        minted_at: u64,
    },
    /// A version in the listing of every package's versions, in [`PackagePublishKey`] order:
    /// `version` of the package originally published at `original_package_id`, published in
    /// `checkpoint`. `minted_at` is not part of the position.
    AllPackages {
        checkpoint: u64,
        original_package_id: ObjectID,
        version: u64,
        minted_at: u64,
    },
}

impl PackageVersionCursor {
    pub fn encode(&self) -> Bytes {
        bcs::to_bytes(self).unwrap().into()
    }

    /// The highest checkpoint the server that minted the cursor had indexed.
    fn minted_at(&self) -> u64 {
        match self {
            Self::Lineage { minted_at, .. } | Self::AllPackages { minted_at, .. } => *minted_at,
        }
    }
}

/// Validate a `ListPackageVersions` request before any range, tip or floor rule applies to it,
/// as the other `List` methods validate theirs, so a malformed request is `INVALID_ARGUMENT`
/// whatever its range: an `end_checkpoint` below `start_checkpoint`, an invalid
/// `options.ordering`, or a page token or cursor that does not decode, or that the other
/// listing minted (a lineage cursor with `package_id` unset, or one of the listing of every
/// package with it set). A lineage listing also refuses an `end_version` below
/// `start_version`, and `page_size` or `page_token` combined with `options` or a checkpoint
/// bound; the listing of every package refuses `page_size`, `page_token`, `start_version` and
/// `end_version`. Reads nothing, so it runs before the package is loaded; whether a page token
/// or cursor names the requested lineage is checked once the package is loaded, by
/// [`PackageVersionsQuery::new`]. Public so other implementors of `MovePackageService` refuse
/// the same requests.
pub fn validate_request(request: &ListPackageVersionsRequest) -> Result<()> {
    validate_checkpoint_bounds(request.start_checkpoint, request.end_checkpoint)?;
    let lists_lineage = request.package_id.is_some();
    if lists_lineage {
        if let (Some(start), Some(end)) = (request.start_version, request.end_version)
            && end < start
        {
            return Err(FieldViolation::new("end_version")
                .with_description("end_version must be greater than or equal to start_version")
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }

        if pages_by_token(request) {
            if let Some(page_token) = request.page_token.as_deref() {
                decode_page_token(page_token)?;
            }
            return Ok(());
        }

        for (field, set) in [
            ("page_size", request.page_size.is_some()),
            ("page_token", request.page_token.is_some()),
        ] {
            if set {
                let description = if request.options.is_some() {
                    format!("{field} and options are mutually exclusive")
                } else {
                    format!("{field} cannot be combined with start_checkpoint or end_checkpoint")
                };
                return Err(FieldViolation::new(field)
                    .with_description(description)
                    .with_reason(ErrorReason::FieldInvalid)
                    .into());
            }
        }
    } else {
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
    }

    if let Some(options) = &request.options {
        descending(options)?;
        for (field, cursor) in [
            ("options.after", &options.after),
            ("options.before", &options.before),
        ] {
            if let Some(cursor) = cursor {
                let from_lineage = matches!(
                    decode_cursor(field, cursor)?,
                    PackageVersionCursor::Lineage { .. }
                );
                if from_lineage != lists_lineage {
                    return Err(cursor_from_other_listing(field, lists_lineage));
                }
            }
        }
    }
    Ok(())
}

/// Check the checkpoint range of a `ListPackageVersions` request against `highest_indexed`, the
/// highest checkpoint the server has indexed, before the listing reads anything. Validate the
/// request with [`validate_request`] first, and, for a package the server knows, check that its
/// page token or cursors name the package's lineage with [`PackageVersionsQuery::new`], so a
/// malformed request is refused as such whatever its range. Public so other implementors of
/// `MovePackageService` refuse the same requests.
///
/// A descending listing that reaches past the highest indexed checkpoint is `OUT_OF_RANGE`
/// (`ABOVE_LATEST_AVAILABLE`), as `ListTransactions` refuses it: an `end_checkpoint` above the
/// checkpoint after it, or, with `end_checkpoint` unset, a `start_checkpoint` at or above that
/// checkpoint. A descending listing starts at its range's high end, so answering from this
/// server's view would skip the versions published between its tip and that end. An ascending
/// listing is never refused for its range: it lists through the highest indexed checkpoint and
/// ends with `LEDGER_TIP` (see [`PackageVersionsQuery::end_reason`]).
pub fn check_checkpoint_range(
    request: &ListPackageVersionsRequest,
    highest_indexed: u64,
) -> Result<()> {
    let descending = request
        .options
        .as_ref()
        .map(descending)
        .transpose()?
        .unwrap_or(false);
    if !descending {
        return Ok(());
    }
    let first_unavailable = highest_indexed.saturating_add(1);
    let past_tip = match request.end_checkpoint {
        Some(end) => (end > first_unavailable).then_some(("end_checkpoint", end)),
        None => request
            .start_checkpoint
            .filter(|start| *start >= first_unavailable)
            .map(|start| ("start_checkpoint", start)),
    };
    if let Some((field, requested)) = past_tip {
        return Err(above_latest_available(field, requested, first_unavailable));
    }
    Ok(())
}

/// The versions a `ListPackageVersions` request lists and how its response pages: the request's
/// version bounds, checkpoint range, page token, and cursors intersected into one version range,
/// validated against the lineage being listed. Public so other implementors of
/// `MovePackageService` parse requests and page responses the same way.
///
/// A lineage's versions are published in non-decreasing checkpoint order, so the checkpoint range
/// is one version range: `start_checkpoint` starts it just above the lineage's newest version at
/// the end of the checkpoint before it ([`Self::bound_by_start_checkpoint`]), and the listing
/// holds no version published after [`Self::bounding_checkpoint`]. Both read the lineage as of a
/// checkpoint, as a read pinned by `at_checkpoint` does.
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
    /// Whether the request pages by cursor: it sets `options`, `start_checkpoint` or
    /// `end_checkpoint`. Such responses set `end` and carry no `next_page_token`.
    cursor_mode: bool,
    /// The version the client holds below the listing: the one an `after` cursor names, or the
    /// one below a page token's. See [`Self::continues_listing`].
    held: Option<u64>,
    /// The versions the request's page token, or its `after` and `before` cursors, name.
    cursor_versions: [Option<u64>; 2],
    /// The request's `start_version` and `end_version`.
    version_bounds: [Option<u64>; 2],
    /// The request's `start_checkpoint` and `end_checkpoint`.
    checkpoint_bounds: [Option<u64>; 2],
    /// The first version published at or after `start_checkpoint`, or 0 without it.
    checkpoint_start_version: u64,
    /// The highest checkpoint the server has indexed.
    highest_indexed: u64,
}

impl PackageVersionsQuery {
    /// Parse the paging of `request`, which lists the upgrade lineage of the package originally
    /// published at `original_package_id`, on a server that has indexed every checkpoint up to
    /// `highest_indexed`, and check that its page token or cursors name that lineage. The request
    /// is validated with [`validate_request`] here as well, but callers run it first, so that a
    /// malformed request is refused before [`check_checkpoint_range`] and any read.
    pub fn new(
        request: &ListPackageVersionsRequest,
        original_package_id: ObjectID,
        highest_indexed: u64,
    ) -> Result<Self> {
        validate_request(request)?;
        let mut versions =
            request.start_version.unwrap_or(0)..request.end_version.unwrap_or(u64::MAX);
        let checkpoint_bounds = [request.start_checkpoint, request.end_checkpoint];
        let query = |versions, descending, page_size, cursor_mode, held, cursor_versions| Self {
            original_package_id,
            versions,
            descending,
            page_size,
            cursor_mode,
            held,
            cursor_versions,
            version_bounds: [request.start_version, request.end_version],
            checkpoint_bounds,
            checkpoint_start_version: 0,
            highest_indexed,
        };

        if pages_by_token(request) {
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
            // The client holds the version below the one the page token names.
            let held = page_token
                .as_ref()
                .and_then(|token| token.version.checked_sub(1));

            return Ok(query(
                versions,
                false,
                page_size(request.page_size),
                false,
                held,
                [page_token.map(|token| token.version), None],
            ));
        }

        let default_options = QueryOptions::default();
        let options = request.options.as_ref().unwrap_or(&default_options);
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

        Ok(query(
            versions,
            descending,
            page_size(options.limit),
            true,
            after,
            [after, before],
        ))
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

    /// Whether the request pages by cursor, rather than by page token.
    pub fn is_cursor_mode(&self) -> bool {
        self.cursor_mode
    }

    /// Whether the request starts a listing, rather than continuing one from a page token or
    /// cursor.
    pub fn is_first_page(&self) -> bool {
        self.cursor_versions == [None, None]
    }

    /// How the listing ends when its checkpoint range alone leaves nothing to list, before any
    /// read, as `ListTransactions` ends such a range: an ascending listing whose
    /// `start_checkpoint` is above the highest indexed checkpoint starts past the tip, with
    /// `LEDGER_TIP`, and an empty range (`end_checkpoint` equal to `start_checkpoint`, or 0 when
    /// that is unset) ends where it starts, with `CHECKPOINT_BOUND`.
    pub fn empty_range_end(&self) -> Option<QueryEndReason> {
        let [start_checkpoint, end_checkpoint] = self.checkpoint_bounds;
        if !self.descending && start_checkpoint.is_some_and(|start| start > self.highest_indexed) {
            Some(QueryEndReason::LedgerTip)
        } else if end_checkpoint == Some(start_checkpoint.unwrap_or(0)) {
            Some(QueryEndReason::CheckpointBound)
        } else {
            None
        }
    }

    /// Whether the listing's checkpoint range reaches past the highest indexed checkpoint:
    /// `end_checkpoint` is unset, or above the checkpoint after the highest indexed one, as
    /// `ListTransactions` decides it.
    fn range_reaches_tip(&self) -> bool {
        self.checkpoint_bounds[1].is_none_or(|end| end > self.highest_indexed.saturating_add(1))
    }

    /// The checkpoint whose version starts the range: the one before `start_checkpoint`. `None`
    /// when `start_checkpoint` is unset or 0, which leave the range's start as it is.
    pub fn start_checkpoint_lookup(&self) -> Option<u64> {
        self.checkpoint_bounds[0]?.checked_sub(1)
    }

    /// Start the range just above `newest_before_start`, the lineage's newest version at the
    /// end of [`Self::start_checkpoint_lookup`], or leave it if the lineage had none by then.
    /// Every version published at or after `start_checkpoint` is above it, since a lineage's
    /// versions are published in non-decreasing checkpoint order.
    pub fn bound_by_start_checkpoint(&mut self, newest_before_start: Option<u64>) {
        if let Some(newest) = newest_before_start {
            self.checkpoint_start_version = newest.saturating_add(1);
            self.versions.start = self.versions.start.max(self.checkpoint_start_version);
        }
    }

    /// The checkpoint that bounds the listing: `end_checkpoint - 1` when the server has indexed
    /// it, else the highest indexed checkpoint. The listing holds no version published after
    /// it. Not meaningful when [`Self::empty_range_end`] is set.
    pub fn bounding_checkpoint(&self) -> u64 {
        match self.checkpoint_bounds[1] {
            Some(end) => end.saturating_sub(1).min(self.highest_indexed),
            None => self.highest_indexed,
        }
    }

    /// Whether the request continues a listing whose client holds the version just below
    /// [`Self::versions`]: an `after` cursor names it, in either order, or a page token names the
    /// version above it, and no `start_version` or `start_checkpoint` above either bounds the
    /// range. Versions below `OBJECT_START_VERSION` are left out: no lineage has one, so a client
    /// holds none. A server that lists only the versions it retains refuses such a request when
    /// the range starts below the lowest version it retains: starting at that version instead
    /// would skip the versions in between without saying so.
    pub fn continues_listing(&self) -> bool {
        continues_listing(self.held, &self.versions)
    }

    /// The versions named by the request's cursors that the server checks against its tip: those
    /// of a descending listing whose range reaches past the tip. A cursor naming a version above
    /// every version the server serves comes from a newer view of the lineage, such as another
    /// replica's, and a descending page from this server's view would start below versions it
    /// lacks, skipping them without saying so. The server rejects it with
    /// [`crate::AtCheckpointUpperBound::unindexed_cursor_error`].
    ///
    /// An ascending listing serves cursors past the tip from its own view, ending with
    /// `LEDGER_TIP` (see [`Self::end_reason`]), and a descending listing whose `end_checkpoint`
    /// the server has indexed holds no version above `end_checkpoint - 1`. A legacy page token
    /// past the tip gets an empty page with no `next_page_token`, as from servers that predate
    /// cursor paging. A named version below where the server's history of the lineage begins,
    /// such as an old version of a system package on a node restored from a snapshot, is not
    /// rejected here: see [`Self::continues_listing`]. Versions below `OBJECT_START_VERSION` are
    /// left out: no lineage has one, so a cursor naming one skips nothing.
    pub fn cursor_versions(&self) -> impl Iterator<Item = u64> {
        let checked = self.cursor_mode && self.descending && self.range_reaches_tip();
        self.cursor_versions
            .into_iter()
            .flatten()
            .filter(move |version| checked && *version >= OBJECT_START_VERSION.value())
    }

    /// One listed version, carrying its cursor, minted at the highest indexed checkpoint.
    pub fn listed_version(
        &self,
        storage_id: ObjectID,
        version: u64,
        checkpoint: Option<u64>,
    ) -> PackageVersion {
        let cursor = PackageVersionCursor::Lineage {
            original_package_id: self.original_package_id,
            version,
            minted_at: self.highest_indexed,
        };
        let mut listed = PackageVersion::new(&storage_id.into(), version)
            .with_cursor(cursor.encode())
            .with_original_id(self.original_package_id.to_canonical_string(true));
        listed.checkpoint = checkpoint;
        listed
    }

    /// Whether a response listing `listed` versions ends with [`Self::end_reason`]: a page in
    /// cursor mode with fewer than the limit. A full page ends with `ITEM_LIMIT`.
    pub fn needs_end_reason(&self, listed: usize) -> bool {
        self.cursor_mode && listed < self.page_size
    }

    /// How a listing that read [`Self::versions`] to the end ends, given `visible_end`, the
    /// version after the lineage's newest one at the end of [`Self::bounding_checkpoint`] (0 if
    /// it had none), named as `ListObjectVersions` and the ledger `List` APIs name it:
    /// - An ascending listing whose window reaches past the lineage's newest version ends with
    ///   `LEDGER_TIP`, since versions published later extend it: its `end_checkpoint` is unset
    ///   or above the checkpoint after the highest indexed one, and `end_version` is unset or
    ///   above both the version after the newest one and the window's start. So does one whose
    ///   `after` cursor is past the tip, which lists nothing, unless a tighter bound leaves
    ///   nothing above `after`, as `ListObjectVersions` decides it: `before` at or below the
    ///   version after `after`, and at or below `end_version`, ends it with `CURSOR_BOUND`, and
    ///   otherwise `end_version` at or below that version ends it with `CHECKPOINT_BOUND`.
    /// - Otherwise the far cursor in listing order ends it with `CURSOR_BOUND` when the version
    ///   it names is inside the window the request's version and checkpoint bounds set, or at
    ///   its edge: `before` at or below the window's end ascending, `after` at or above its
    ///   start descending.
    /// - Every other listing ends at the edge of that window, with `CHECKPOINT_BOUND`. On a
    ///   server that prunes history, so does a descending listing that stops at the oldest
    ///   version it stores.
    pub fn end_reason(&self, visible_end: u64) -> QueryEndReason {
        let [start_version, end_version] = self.version_bounds;
        let [after, before] = if self.cursor_mode {
            self.cursor_versions
        } else {
            [None, None]
        };
        let low = start_version
            .unwrap_or(0)
            .max(self.checkpoint_start_version);
        let high = end_version.map_or(visible_end, |end| end.min(visible_end));
        let reaches_tip = self.range_reaches_tip()
            && end_version.is_none_or(|end| end > visible_end && end > low);
        if !self.descending
            && reaches_tip
            && let Some(after) = after.filter(|after| *after >= visible_end)
        {
            let resume = after.saturating_add(1);
            // The far cursor ends the listing when it is at least as tight as `end_version`.
            return if before.is_some_and(|before| {
                before <= resume && end_version.is_none_or(|end| before <= end)
            }) {
                QueryEndReason::CursorBound
            } else if end_version.is_some_and(|end| end <= resume) {
                QueryEndReason::CheckpointBound
            } else {
                QueryEndReason::LedgerTip
            };
        }
        if low < high {
            if self.descending && after.is_some_and(|after| after >= low) {
                return QueryEndReason::CursorBound;
            }
            if !self.descending && before.is_some_and(|before| before <= high) {
                return QueryEndReason::CursorBound;
            }
        }
        if reaches_tip && !self.descending {
            QueryEndReason::LedgerTip
        } else {
            QueryEndReason::CheckpointBound
        }
    }

    /// The response listing `versions`: at most [`Self::read_limit`] versions, read in this
    /// query's order. In cursor mode, it ends with `end`, the [`Self::end_reason`] of a page
    /// that [`Self::needs_end_reason`], or else with `ITEM_LIMIT`.
    pub fn response(
        &self,
        mut versions: Vec<PackageVersion>,
        end: Option<QueryEndReason>,
    ) -> ListPackageVersionsResponse {
        if self.cursor_mode {
            let end = QueryEnd::default().with_reason(end.unwrap_or(QueryEndReason::ItemLimit));
            return ListPackageVersionsResponse::new(versions, None).with_end(end);
        }
        let next_page_token = if versions.len() > self.page_size {
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

/// The answer to a request in cursor mode for a package the server does not know as of
/// `highest_indexed`, the highest checkpoint it has indexed (kv-rpc: its served height): no
/// version of it was published at or below that checkpoint. Public so other implementors of
/// `MovePackageService` answer such requests the same way. Check the request's checkpoint range
/// with [`check_checkpoint_range`] first.
///
/// A first page, with neither `after` nor `before`, is `NotFound`, as `GetPackage` answers. A
/// request with a cursor minted above `highest_indexed` (see [`PackageVersionCursor`]) continues
/// a listing from a replica ahead of this server, which may know a package published since; it
/// is answered as a listing of a lineage with no version at this server's height, in which every
/// cursor lies past the tip: a descending listing whose range reaches past the tip is
/// `OutOfRange` (`ABOVE_LATEST_AVAILABLE`, bound "cursor", at the highest checkpoint a cursor
/// was minted at), and every other listing is an empty page ending as
/// [`PackageVersionsQuery::end_reason`] ends one, `LEDGER_TIP` ascending without a closer bound.
/// The response has no cursor field, so the client resumes with its own. With every cursor
/// minted at or below `highest_indexed`, the minting server did not know the package either, or
/// the cursor is forged, and the request is `NotFound`. The cursors are validated first, as for
/// a package the server knows: they must name one lineage, though whether it is the requested
/// package's cannot be checked without the package.
pub fn unknown_package_response(
    request: &ListPackageVersionsRequest,
    highest_indexed: u64,
) -> Result<ListPackageVersionsResponse> {
    let mut cursors = vec![];
    if let Some(options) = &request.options {
        for (field, cursor) in [
            ("options.after", &options.after),
            ("options.before", &options.before),
        ] {
            if let Some(cursor) = cursor {
                cursors.push(decode_cursor(field, cursor)?);
            }
        }
    }
    let Some(first) = cursors.first() else {
        return Err(RpcError::not_found());
    };
    // `PackageVersionsQuery::new` refuses a cursor of the listing of every package.
    let original_package_id = match *first {
        PackageVersionCursor::Lineage {
            original_package_id,
            ..
        }
        | PackageVersionCursor::AllPackages {
            original_package_id,
            ..
        } => original_package_id,
    };
    let query = PackageVersionsQuery::new(request, original_package_id, highest_indexed)?;
    let minted_at = cursors
        .iter()
        .map(PackageVersionCursor::minted_at)
        .max()
        .unwrap_or_default();
    if minted_at <= highest_indexed {
        return Err(RpcError::not_found());
    }

    if let Some(end) = query.empty_range_end() {
        return Ok(query.response(vec![], Some(end)));
    }
    if query.descending && query.range_reaches_tip() {
        AtCheckpointUpperBound::new(highest_indexed).check_cursor(minted_at)?;
    }
    Ok(query.response(vec![], Some(query.end_reason(0))))
}

/// The versions a `ListPackageVersions` request with `package_id` unset lists, across every
/// package in [`PackagePublishKey`] order, and how its response pages: the request's checkpoint
/// range and cursors intersected into one range of keys. Public so other implementors of
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
    /// Whether `lower` and `upper` admit no key, so the listing reads nothing.
    reads_nothing: bool,
    /// How a page with fewer than `page_size` versions ends.
    short_page_end: QueryEndReason,
    /// The highest checkpoint the server has indexed, which the listing's cursors record as
    /// `minted_at`.
    highest_indexed: u64,
}

impl AllPackageVersionsQuery {
    /// Parse a request with `package_id` unset that lists the versions published in its
    /// checkpoint range `[start_checkpoint, end_checkpoint)`, through at most the highest
    /// checkpoint `upper_bound` admits, checked with [`check_checkpoint_range`]. An unset
    /// `start_checkpoint` starts the listing at `lowest_checkpoint`, the lowest checkpoint the
    /// server can list every publish from (0 on a server that keeps full history), so a
    /// descending listing without an `after` cursor ends there with a short last page. The
    /// request is validated with [`validate_request`] first, so a malformed one is
    /// `INVALID_ARGUMENT` whatever its range.
    ///
    /// With no `after` cursor, on a server whose `lowest_checkpoint` is above genesis, a
    /// `start_checkpoint` below it in a non-empty range is `OUT_OF_RANGE`
    /// (`BELOW_EARLIEST_AVAILABLE`, reporting `lowest_checkpoint`) first, as `ListTransactions`
    /// refuses it: no cursor can bound the listing at or above the floor. Otherwise a cursor past
    /// the highest indexed checkpoint is `OUT_OF_RANGE` (`ABOVE_LATEST_AVAILABLE`) in a
    /// descending listing whose range reaches past it, before the floor is checked; an
    /// ascending listing lists through the highest indexed checkpoint and ends with
    /// `LEDGER_TIP`. An empty range lists nothing, whatever the floor. Otherwise, on a server
    /// whose `lowest_checkpoint` is above genesis, a `start_checkpoint` below it is
    /// `OUT_OF_RANGE` (`BELOW_EARLIEST_AVAILABLE`, reporting `lowest_checkpoint`) unless an
    /// `after` cursor at or above it bounds the listing, and so, in a listing with no lower bound
    /// of its own (neither `start_checkpoint` nor an `after` cursor), is an `end_checkpoint` at or
    /// below it. Without a `start_checkpoint` at or above it, a listing after a cursor below it is
    /// refused the same way, in either order; a cursor in a checkpoint below it is refused even
    /// when it was that checkpoint's last publish, since the server can't tell.
    pub fn new(
        request: &ListPackageVersionsRequest,
        lowest_checkpoint: u64,
        upper_bound: AtCheckpointUpperBound,
    ) -> Result<Self> {
        validate_request(request)?;
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

        let highest_indexed = upper_bound.highest_indexed();
        check_checkpoint_range(request, highest_indexed)?;

        let empty_range = request.end_checkpoint == Some(request.start_checkpoint.unwrap_or(0));
        // Without an `after` cursor, nothing can bound the listing at or above the floor, so a
        // `start_checkpoint` below it can never be served, and is refused before a `before`
        // cursor's tip refusal, as `ListTransactions`' `floor_error_or` refuses it. With an
        // `after` cursor, the cursor's tip refusal comes first.
        if !empty_range
            && lowest_checkpoint > 0
            && after.is_none()
            && request
                .start_checkpoint
                .is_some_and(|start| start < lowest_checkpoint)
        {
            return Err(below_serving_floor(lowest_checkpoint));
        }

        // Whether the range reaches past the highest indexed checkpoint, as `ListTransactions`
        // decides it.
        let range_reaches_tip = request
            .end_checkpoint
            .is_none_or(|end| end > highest_indexed.saturating_add(1));
        // A cursor above the highest indexed checkpoint comes from a newer view of the ledger,
        // such as another replica's. A descending page from this server's view would start below
        // it and skip the versions published in between, so it is refused when the range
        // reaches past the tip, before the floor refusals that depend on an `after` cursor, as
        // `ListTransactions` and the listing of one package refuse it. An ascending page lists
        // through the tip, which `LEDGER_TIP` marks, and a range ending at an indexed checkpoint
        // holds no key above the cursor.
        if descending && range_reaches_tip {
            for cursor in after.iter().chain(&before) {
                upper_bound.check_cursor(cursor.checkpoint)?;
            }
        }

        if !empty_range && lowest_checkpoint > 0 {
            let below = match request.start_checkpoint {
                // A `start_checkpoint` below the floor is checked after the cursors narrow the
                // range, as `ListTransactions` checks it: an `after` cursor at or above the floor
                // leaves nothing below it to list.
                Some(start) => {
                    start < lowest_checkpoint
                        && after.is_none_or(|after| after.checkpoint < lowest_checkpoint)
                }
                // Without `start_checkpoint`, the listing covers what this server can list, from
                // `lowest_checkpoint` up. An `after` cursor below it names a version the client
                // holds, from a listing that reached below where this server can list every
                // publish, such as one another server answered, or one this server answered
                // before its floor rose. The versions published between the cursor and the floor
                // may be gone, and a listing that reaches the floor would skip them without
                // saying so, in either order. A `before` cursor below the floor leaves nothing to
                // list.
                None => match after {
                    Some(after) => after.checkpoint < lowest_checkpoint,
                    None => request
                        .end_checkpoint
                        .is_some_and(|end| end <= lowest_checkpoint),
                },
            };
            if below {
                return Err(below_serving_floor(lowest_checkpoint));
            }
        }

        let start_checkpoint = request.start_checkpoint.unwrap_or(lowest_checkpoint);
        let highest_checkpoint = match request.end_checkpoint {
            Some(end) => end.saturating_sub(1).min(highest_indexed),
            None => highest_indexed,
        };

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

        // How a short page ends, as `ListTransactions` and the listing of one package name it.
        let ascending_past_tip = !descending
            && request
                .start_checkpoint
                .is_some_and(|start| start > highest_indexed);
        let window_has_keys = !empty_range && first <= last;
        let cursor_bound = (window_has_keys
            && if descending {
                after.is_some_and(|after| after >= first)
            } else {
                before.is_some_and(|before| before <= last)
            })
            || (!descending
                && before.is_some_and(|before| after.is_some_and(|after| before <= after)));
        let short_page_end = if ascending_past_tip {
            QueryEndReason::LedgerTip
        } else if empty_range {
            QueryEndReason::CheckpointBound
        } else if cursor_bound {
            QueryEndReason::CursorBound
        } else if !descending && range_reaches_tip {
            QueryEndReason::LedgerTip
        } else {
            QueryEndReason::CheckpointBound
        };
        let reads_nothing = empty_range
            || ScanBounds {
                lo: lower,
                hi: upper,
            }
            .is_empty();

        Ok(Self {
            lower,
            upper,
            descending,
            page_size: page_size(options.limit),
            reads_nothing,
            short_page_end,
            highest_indexed,
        })
    }

    /// The most versions to read for one response.
    pub fn limit(&self) -> usize {
        self.page_size
    }

    /// Whether the listing's range admits no key, so the server reads nothing.
    pub fn reads_nothing(&self) -> bool {
        self.reads_nothing
    }

    /// One listed version, carrying its cursor, minted at the highest indexed checkpoint.
    pub fn listed_version(&self, key: PackagePublishKey, storage_id: ObjectID) -> PackageVersion {
        let cursor = PackageVersionCursor::AllPackages {
            checkpoint: key.checkpoint,
            original_package_id: key.original_id,
            version: key.version,
            minted_at: self.highest_indexed,
        };
        PackageVersion::new(&storage_id.into(), key.version)
            .with_checkpoint(key.checkpoint)
            .with_cursor(cursor.encode())
            .with_original_id(key.original_id.to_canonical_string(true))
    }

    /// The response listing `versions`, at most [`Self::limit`] of them: a full page ends with
    /// `ITEM_LIMIT`, a shorter one at the edge of its range.
    pub fn response(&self, versions: Vec<PackageVersion>) -> ListPackageVersionsResponse {
        let reason = if versions.len() >= self.page_size {
            QueryEndReason::ItemLimit
        } else {
            self.short_page_end
        };
        ListPackageVersionsResponse::new(versions, None)
            .with_end(QueryEnd::default().with_reason(reason))
    }
}

/// List the versions of a package's upgrade lineage published at or before the highest
/// checkpoint every rpc index has committed, or, with `package_id` unset, the versions of every
/// package.
///
/// Versions with no recorded publish checkpoint are listed with `checkpoint` unset: they were
/// published before the history the index holds begins, so they belong to any checkpoint range
/// this node accepts.
///
/// In cursor mode (`options`, `start_checkpoint` or `end_checkpoint` set), the package is read as
/// of the highest indexed checkpoint: a package with no version published at or below it is
/// unknown, and answered as [`unknown_package_response`] describes. A legacy page-token listing
/// loads the package from the live object set, so it lists a package this node has executed but
/// not yet indexed, finding no version of it.
///
/// In cursor mode, a listing covers only the versions of the lineage this node stores: a range
/// that starts below the lowest of them, such as a system package's versions replaced before the
/// pruning floor, starts at it, so a descending listing ends there with a short last page. A
/// pruned node keeps the rows of the versions it pruned, and a legacy page-token listing lists
/// every row, including those versions. A node restored from a snapshot, or a fork, has no rows
/// for the versions replaced before its history begins, and every listing starts at its lowest
/// row.
///
/// A malformed request is `INVALID_ARGUMENT` before any range rule applies to it, in either
/// listing (see [`validate_request`]); so is a page token or cursor naming another lineage than
/// the package's, whatever the range.
///
/// A checkpoint range covers the versions published in it, not the lineage's state at one
/// checkpoint. An empty range lists nothing. Past the highest indexed checkpoint it is handled as
/// [`check_checkpoint_range`] and [`PackageVersionsQuery::end_reason`] describe. Below the
/// history this node retains, a `start_checkpoint` below the checkpoint after the floor
/// `at_checkpoint` reads use is refused as `OutOfRange` (`BELOW_EARLIEST_AVAILABLE`) with that
/// checkpoint as `lowestAvailableCheckpoint`: the lowest `start_checkpoint` this node accepts,
/// one above the lowest `at_checkpoint` a point read accepts, since the range starts from the
/// version as of the checkpoint before `start_checkpoint`. The start is checked after an `after`
/// cursor narrows the range, as `ListTransactions` checks it, so it is served when the first
/// version after the cursor was published at or above that checkpoint. An `end_checkpoint` at or
/// below that checkpoint is refused the same way in a listing with no lower bound of its own
/// (neither `start_checkpoint` nor an `after` cursor).
///
/// A request continuing a listing whose client holds the version just below its range, in either
/// order (see [`PackageVersionsQuery::continues_listing`]), is refused the same way when that
/// range starts below where the listing would start: starting there would skip the versions in
/// between without saying so. A listing with `start_checkpoint` that pruning overtakes is
/// refused by the floor check after the reads, as `ListTransactions` refuses it: only a listing
/// without one starts at the oldest version this node stores. The floor check overrides what the
/// listing ends with, an answer or an error such as `NotFound` for a package whose history is
/// pruned, except `INVALID_ARGUMENT`, and except a cursor past the tip in a request with an
/// `after` cursor, as `ListTransactions` keeps it.
#[tracing::instrument(skip(service))]
pub fn list_package_versions(
    service: &RpcService,
    request: ListPackageVersionsRequest,
) -> Result<ListPackageVersionsResponse> {
    validate_request(&request)?;
    let Some(package_id_str) = request.package_id.as_deref() else {
        return list_all_package_versions(service, &request);
    };

    let package_id = parse_package_id(package_id_str)?;

    // While a restore is still backfilling history, a lineage upgraded inside the backfill window,
    // such as a system package's, lacks the versions published in the part of the window the
    // backfill has not reached, and its restored latest version has no recorded publish
    // checkpoint yet, so a listing could come back partial or include versions published after
    // its range. Every listing waits for the backfill, with or without a checkpoint range:
    // `at_checkpoint_upper_bound` is `Unavailable` until it finishes. The bound only grows, so
    // reading it before the rows keeps it valid for them.
    let upper_bound = service.reader.at_checkpoint_upper_bound()?;
    let highest_indexed = upper_bound.highest_indexed();

    // A page token or cursor naming another lineage than the package's is refused before the
    // checkpoint range is checked, as malformed whatever the range. A package this node does not
    // know as of its highest indexed checkpoint has its range checked first: it may have been
    // published above that checkpoint, and answering `NotFound` for it would hide that the range
    // is not served yet.
    let resolved = resolve_listed_package(service, &request, package_id, highest_indexed);
    let query = match &resolved {
        Ok(Some(original_package_id)) => Some(PackageVersionsQuery::new(
            &request,
            *original_package_id,
            highest_indexed,
        )?),
        Ok(None) | Err(_) => None,
    };
    check_checkpoint_range(&request, highest_indexed)?;
    let listing = match query {
        Some(query) => list_lineage(service, query, upper_bound),
        None => resolved.and_then(|_| unknown_package_response(&request, highest_indexed)),
    };

    // A version with no recorded publish checkpoint was published before the history the index
    // holds begins. On a node that backfilled after restoring from a snapshot, that is by the end
    // of the checkpoint below the lowest one available at restore time, and the rpc-store floor
    // the restore stamped there keeps the shared lower bound at or above it. A store that never
    // backfills, such as a fork, reports where its history begins through
    // `get_lowest_checkpoint_with_index_history`. So such a version belongs under any range the
    // shared lower bound accepts, and a range reaching below it is rejected.
    //
    // The lower bound is read after every read the listing made, as `GetPackage`'s
    // `at_checkpoint` does (see `get_package.rs`), and checked once: the pruner raises it before
    // it deletes, so a range that passes after the reads was in place while they ran, and
    // nothing they found was pruned under it. The check overrides whatever the reads concluded,
    // an answer or an error such as `NotFound` for a package whose history is pruned, so a
    // request below the floor ends here, after at most one page of reads, with no versions. Two
    // errors are not overridden. An `INVALID_ARGUMENT` error, a page token or cursor naming
    // another lineage, is refused as malformed whatever its range, as `validate_request` refuses
    // the rest. A cursor past the tip (`ABOVE_LATEST_AVAILABLE`; the range's own refusal returned
    // above) in a request with an `after` cursor is kept, as `ListTransactions` keeps a cursor's
    // tip refusal over the floor whenever an `after` cursor is present: the floor overrides it
    // only for a `start_checkpoint` below the floor with no `after` cursor. The refusal reads only
    // `package_versions` rows, which pruning never deletes.
    let has_after = request
        .options
        .as_ref()
        .is_some_and(|options| options.after.is_some());
    if listing.as_ref().is_err_and(|error| {
        error.code() == tonic::Code::InvalidArgument
            || (has_after && error.has_reason(ErrorReason::AboveLatestAvailable))
    }) {
        return listing;
    }
    if request.start_checkpoint.is_some() || request.end_checkpoint.is_some() {
        check_retained_checkpoint_range(
            &request,
            service.reader.at_checkpoint_lower_bound()?,
            |lowest_start| after_cursor_bounds_listing_from(service, &request, lowest_start),
        )?;
    }
    listing
}

/// The original ID of the package `package_id` names, for [`list_package_versions`], or `None`
/// when this node does not know the package in cursor mode.
///
/// In cursor mode the package is read as of `highest_indexed`, the highest indexed checkpoint:
/// the live object set gives the package's original ID and version, which never change, and the
/// package-version index decides whether the package was published by then. A storage ID holds
/// one version, published by then when the lineage's newest version as of `highest_indexed` is
/// at or above it, except for a system package, upgraded in place under its original ID, which
/// is published by then when any of its versions is. A package missing from the live object set
/// is unknown too.
///
/// A legacy page-token listing loads the package from the live object set, as servers that
/// predate cursor paging do, and a missing package is `NotFound`.
fn resolve_listed_package(
    service: &RpcService,
    request: &ListPackageVersionsRequest,
    package_id: ObjectID,
    highest_indexed: u64,
) -> Result<Option<ObjectID>> {
    if pages_by_token(request) {
        return Ok(Some(
            load_package_by_id(service, package_id)?.original_package_id(),
        ));
    }
    let package = match load_package_by_id(service, package_id) {
        Err(error) if error.code() == tonic::Code::NotFound => return Ok(None),
        loaded => loaded?,
    };
    let original_package_id = package.original_package_id();
    let newest = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(indexes_disabled)?
        .get_package_at_checkpoint(original_package_id, highest_indexed)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
    let published = newest.is_some_and(|(version, _)| {
        package.id() == original_package_id || version >= package.version().value()
    });
    Ok(published.then_some(original_package_id))
}

/// The listing `query` describes that [`list_package_versions`] answers, before the request's
/// checkpoint range is checked against the lower bound. `upper_bound` was read before any of the
/// listing's reads.
fn list_lineage(
    service: &RpcService,
    mut query: PackageVersionsQuery,
    upper_bound: AtCheckpointUpperBound,
) -> Result<ListPackageVersionsResponse> {
    let original_package_id = query.original_package_id;
    if let Some(end) = query.empty_range_end() {
        return Ok(query.response(vec![], Some(end)));
    }

    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(indexes_disabled)?;

    // The lineage's newest version published at or before `checkpoint`, or before the history
    // the index holds begins.
    let version_at = |checkpoint| {
        indexes
            .get_package_at_checkpoint(original_package_id, checkpoint)
            .map(|found| found.map(|(version, _)| version))
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))
    };
    if let Some(checkpoint) = query.start_checkpoint_lookup() {
        query.bound_by_start_checkpoint(version_at(checkpoint)?);
    }

    // The listing stops at its bounding checkpoint, at most the highest indexed one, so it never
    // includes a version published above every checkpoint this node serves.
    let cp_bound = query.bounding_checkpoint();
    // In cursor mode, a listing covers only the versions this node stores: `package_versions`
    // rows outlive pruning (see `range_start_against_stored_versions`), and a listing must not
    // name versions this node cannot load. A legacy page-token listing lists every row it
    // reaches, including system-package versions whose packages are pruned.
    let lists_stored_only = query.is_cursor_mode();
    // This node serves a version when it has the version's `package_versions` row, published at
    // or below the highest indexed checkpoint or before its history begins. Recorded publish
    // checkpoints are non-decreasing in version, so the lowest version at or above one tells
    // whether this node serves any version from it up. See `PackageVersionsQuery::cursor_versions`.
    let served = |entry: &IndexedPackageVersion| {
        entry
            .checkpoint
            .is_none_or(|published| published <= upper_bound.highest_indexed())
    };
    let is_stored = |row: &IndexedPackageVersion| {
        !lists_stored_only
            || service
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
        if !rows_from(version)?
            .next()
            .transpose()?
            .is_some_and(|entry| served(&entry))
        {
            return Err(upper_bound.unindexed_cursor_error());
        }
    }

    // Where the listing starts: the low end of the query's range, or when that lies below the
    // lowest version of the lineage this node lists from, that version, so a descending listing
    // ends there. A range that ends at or below `OBJECT_START_VERSION` holds no version on any
    // node. The start is found before the rows are read: rows for the versions published up to
    // `cp_bound` are already written, and pruning deletes versions oldest first, so the listing
    // is the one this node held when it found the start. Pruning may delete versions while the
    // listing reads, so the checkpoint range is checked against the lower bound only after the
    // listing (see [`list_package_versions`]). A legacy page-token listing lists every row, so
    // its start matters only when it continues a listing.
    let mut range_start = query.versions.start.max(OBJECT_START_VERSION.value());
    if range_start < query.versions.end
        && (lists_stored_only || query.continues_listing())
        && let RangeStart::BelowLowestStored { lowest_stored } =
            range_start_against_stored_versions(range_start, rows_from(range_start)?, is_stored)?
    {
        let lower = service.reader.at_checkpoint_lower_bound()?;
        require_history_above_genesis(lower, original_package_id, range_start)?;
        if query.continues_listing() {
            return Err(below_retained_history(lower));
        }
        range_start = lowest_stored.unwrap_or(query.versions.end);
    }

    let mut versions = vec![];
    let mut iter = indexes
        .package_versions_iter(
            original_package_id,
            range_start,
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
        // recorded checkpoint are the lowest and were published before the lower bound that the
        // checkpoint range is checked against after the listing.
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

    let end = if query.needs_end_reason(versions.len()) {
        let newest = version_at(cp_bound)?;
        Some(query.end_reason(newest.map_or(0, |version| version.saturating_add(1))))
    } else {
        None
    };
    Ok(query.response(versions, end))
}

/// Refuse a checkpoint range that reaches below the history this node retains, given `lower`, as
/// `ListTransactions` refuses it (see [`list_package_versions`]): a `start_checkpoint` below the
/// lowest one this node accepts, unless an `after` cursor bounds the listing at or above that
/// checkpoint (`after_bounds_from` decides it, see [`after_cursor_bounds_listing_from`]), or, in
/// a listing with no lower bound of its own (neither `start_checkpoint` nor an `after` cursor),
/// an `end_checkpoint` at or below it. An `after` cursor follows the continuation rules instead
/// (see [`PackageVersionsQuery::continues_listing`]). An empty range lists nothing, whatever the
/// floor, and a node that has pruned nothing accepts every range.
fn check_retained_checkpoint_range(
    request: &ListPackageVersionsRequest,
    lower: AtCheckpointLowerBound,
    after_bounds_from: impl FnOnce(u64) -> Result<bool>,
) -> Result<()> {
    if request.end_checkpoint == Some(request.start_checkpoint.unwrap_or(0)) {
        return Ok(());
    }
    let Some(floor) = lower.pruned_floor() else {
        return Ok(());
    };
    let lowest_start = floor + 1;
    let has_after = request
        .options
        .as_ref()
        .is_some_and(|options| options.after.is_some());
    let below = match request.start_checkpoint {
        // The start is checked after the `after` cursor narrows the range, as `ListTransactions`
        // checks it.
        Some(start) if start < lowest_start => !has_after || !after_bounds_from(lowest_start)?,
        Some(_) => false,
        None => {
            !has_after
                && request
                    .end_checkpoint
                    .is_some_and(|end| end <= lowest_start)
        }
    };
    if below {
        return Err(below_serving_floor(lowest_start));
    }
    Ok(())
}

/// Whether the request's lineage `after` cursor bounds the listing at or above `checkpoint`: the
/// first version after the cursor, where the listing starts, was published in `checkpoint` or
/// later, or there is no version after it. A version with no recorded publish checkpoint was
/// published before the history the index holds begins, below any floor. A request without a
/// lineage `after` cursor is not bounded by one.
fn after_cursor_bounds_listing_from(
    service: &RpcService,
    request: &ListPackageVersionsRequest,
    checkpoint: u64,
) -> Result<bool> {
    let after = request
        .options
        .as_ref()
        .and_then(|options| options.after.as_deref())
        .map(|cursor| decode_cursor("options.after", cursor))
        .transpose()?;
    let Some(PackageVersionCursor::Lineage {
        original_package_id,
        version,
        ..
    }) = after
    else {
        return Ok(false);
    };
    let Some(first_after) = version.checked_add(1) else {
        return Ok(true);
    };
    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(indexes_disabled)?;
    let next = indexes
        .package_versions_iter(original_package_id, first_after, u64::MAX, false)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        .next()
        .transpose()
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
    Ok(next.is_none_or(|row| {
        row.checkpoint
            .is_some_and(|published| published >= checkpoint)
    }))
}

/// The refusal of a listing that reaches below the history this node retains, given `lower`:
/// `OutOfRange` (`BELOW_EARLIEST_AVAILABLE`) naming the lowest `start_checkpoint` this node
/// accepts, the checkpoint after the floor `at_checkpoint` reads use.
fn below_retained_history(lower: AtCheckpointLowerBound) -> RpcError {
    below_serving_floor(lower.pruned_floor().map_or(0, |floor| floor + 1))
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

/// Whether `request` pages by page token, from servers that predate cursor paging: it sets
/// neither `options` nor a checkpoint bound. Public so other implementors of
/// `MovePackageService` tell the two paging modes apart the same way.
pub fn pages_by_token(request: &ListPackageVersionsRequest) -> bool {
    request.options.is_none()
        && request.start_checkpoint.is_none()
        && request.end_checkpoint.is_none()
}

/// List the versions of every package published in the request's checkpoint range
/// `[start_checkpoint, end_checkpoint)`, through at most the highest indexed checkpoint, in
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
    // waits for the backfill, as checkpoint-pinned reads do. Otherwise the listing ends before
    // `end_checkpoint`, or at the highest checkpoint every rpc index has committed if that is
    // lower: the pipeline writing the index can run ahead of it, and stopping there matches the
    // height the other reads serve. The bound only grows, so reading it before the rows keeps it
    // valid for them.
    let query = AllPackageVersionsQuery::new(
        request,
        lowest_available,
        service.reader.at_checkpoint_upper_bound()?,
    )?;
    if query.reads_nothing() {
        return Ok(query.response(vec![]));
    }

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

    Ok(query.response(versions))
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
            ..
        } if cursor_package_id == original_package_id => Ok(version),
        PackageVersionCursor::Lineage { .. } => Err(FieldViolation::new(field)
            .with_description("cursor package ID does not match request package ID")
            .with_reason(ErrorReason::FieldInvalid)
            .into()),
        PackageVersionCursor::AllPackages { .. } => Err(cursor_from_other_listing(field, true)),
    }
}

/// The key that the cursor in request field `field` names in the listing of every package.
fn cursor_key(field: &str, cursor: &[u8]) -> Result<PackagePublishKey> {
    match decode_cursor(field, cursor)? {
        PackageVersionCursor::AllPackages {
            checkpoint,
            original_package_id,
            version,
            ..
        } => Ok(PackagePublishKey {
            checkpoint,
            original_id: original_package_id,
            version,
        }),
        PackageVersionCursor::Lineage { .. } => Err(cursor_from_other_listing(field, false)),
    }
}

/// The refusal of a cursor in request field `field` that the other listing minted, given
/// whether the request lists one package's lineage (`package_id` set).
fn cursor_from_other_listing(field: &str, lists_lineage: bool) -> RpcError {
    let description = if lists_lineage {
        "cursor is from the listing of every package, which sets no package_id"
    } else {
        "cursor is from the listing of one package, which sets package_id"
    };
    FieldViolation::new(field)
        .with_description(description)
        .with_reason(ErrorReason::FieldInvalid)
        .into()
}

fn page_size(requested: Option<u32>) -> usize {
    requested
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE) as usize
}

/// Whether a request whose client holds version `held` continues the listing from just above
/// it, given the request's range `versions`. See [`PackageVersionsQuery::continues_listing`].
fn continues_listing(held: Option<u64>, versions: &Range<u64>) -> bool {
    held.is_some_and(|held| {
        held >= OBJECT_START_VERSION.value() && held.checked_add(1) == Some(versions.start)
    })
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

/// Check that this node's history begins above genesis, given its lower bound `lower`, when a
/// listing of `original_package_id`'s lineage from `range_start` finds that version missing. A
/// node whose object history has a floor may have pruned it, or never indexed it. A node that has
/// pruned nothing stores every version, so a missing one is an internal error.
fn require_history_above_genesis(
    lower: AtCheckpointLowerBound,
    original_package_id: ObjectID,
    range_start: u64,
) -> Result<()> {
    if lower.pruned_floor().is_none() {
        return Err(RpcError::new(
            tonic::Code::Internal,
            format!(
                "version {range_start} of package {original_package_id} is not stored, although \
                 this node has pruned no object history"
            ),
        ));
    }
    Ok(())
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

    /// A request listing every package's versions published in `[start_checkpoint,
    /// end_checkpoint)`, `limit` at a time in `ordering`, after `after` and before `before`.
    fn all_packages_request(
        start_checkpoint: Option<u64>,
        end_checkpoint: Option<u64>,
        limit: u32,
        ordering: Ordering,
        after: Option<PackageVersionCursor>,
        before: Option<PackageVersionCursor>,
    ) -> ListPackageVersionsRequest {
        let mut options = QueryOptions::default();
        options.limit = Some(limit);
        options.ordering = Some(ordering as i32);
        options.after = after.map(|cursor| cursor.encode());
        options.before = before.map(|cursor| cursor.encode());
        let mut request = ListPackageVersionsRequest::default();
        request.start_checkpoint = start_checkpoint;
        request.end_checkpoint = end_checkpoint;
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

    /// The cursor of version 1 of package `0x0`, published at checkpoint `checkpoint`.
    fn cursor_at(checkpoint: u64) -> PackageVersionCursor {
        PackageVersionCursor::AllPackages {
            checkpoint,
            original_package_id: ObjectID::ZERO,
            version: 1,
            minted_at: HIGHEST_INDEXED,
        }
    }

    /// The key at which a listing starting at checkpoint `checkpoint` starts.
    fn first_key(checkpoint: u64) -> Bound<PackagePublishKey> {
        Bound::Included(PackagePublishKey {
            checkpoint,
            original_id: ObjectID::ZERO,
            version: 0,
        })
    }

    /// The key at which a listing through checkpoint `checkpoint` ends.
    fn last_key(checkpoint: u64) -> Bound<PackagePublishKey> {
        Bound::Included(PackagePublishKey {
            checkpoint,
            original_id: ObjectID::MAX,
            version: u64::MAX,
        })
    }

    /// How the response of `query` listing `listed` versions ends.
    fn end_reason(query: &AllPackageVersionsQuery, listed: u64) -> QueryEndReason {
        let versions = (1..=listed)
            .map(|version| {
                let key = PackagePublishKey {
                    checkpoint: 20,
                    original_id: ObjectID::ZERO,
                    version,
                };
                query.listed_version(key, ObjectID::ZERO)
            })
            .collect();
        let response = query.response(versions);
        assert_eq!(response.next_page_token, None);
        response.end.unwrap().reason()
    }

    /// A descending listing starts at its range's high end, so a range reaching past the
    /// checkpoint after the highest indexed one is refused, naming the bound that reaches past it:
    /// `end_checkpoint`, or with `end_checkpoint` unset, `start_checkpoint`. A range ending at that
    /// checkpoint, or starting at the highest indexed one, is served.
    #[test]
    fn descending_range_past_the_tip_is_refused() {
        let first_unavailable = HIGHEST_INDEXED + 1;
        for start_checkpoint in [None, Some(20), Some(first_unavailable)] {
            for end_checkpoint in [first_unavailable + 1, u64::MAX] {
                let request = all_packages_request(
                    start_checkpoint,
                    Some(end_checkpoint),
                    10,
                    Ordering::Descending,
                    None,
                    None,
                );
                assert_above_latest_available_error(
                    parse(&request, 0).unwrap_err(),
                    "end_checkpoint",
                    end_checkpoint,
                    first_unavailable,
                );
            }
            let request = all_packages_request(
                start_checkpoint,
                Some(first_unavailable),
                10,
                Ordering::Descending,
                None,
                None,
            );
            parse(&request, 0).unwrap();
        }

        for start_checkpoint in [first_unavailable, u64::MAX] {
            let request = all_packages_request(
                Some(start_checkpoint),
                None,
                10,
                Ordering::Descending,
                None,
                None,
            );
            assert_above_latest_available_error(
                parse(&request, 0).unwrap_err(),
                "start_checkpoint",
                start_checkpoint,
                first_unavailable,
            );
        }

        let request = all_packages_request(
            Some(HIGHEST_INDEXED),
            None,
            10,
            Ordering::Descending,
            None,
            None,
        );
        let query = parse(&request, 0).unwrap();
        assert_eq!(
            [query.lower, query.upper],
            [first_key(HIGHEST_INDEXED), last_key(HIGHEST_INDEXED)]
        );
    }

    /// An ascending listing is never refused for reaching past the highest indexed checkpoint: it
    /// lists through that checkpoint and ends with `LEDGER_TIP`. A range starting past it, or a
    /// listing after a cursor past it, reads nothing.
    #[test]
    fn ascending_listing_past_the_tip_ends_at_the_ledger_tip() {
        for (start_checkpoint, end_checkpoint) in [
            (HIGHEST_INDEXED + 1, None),
            (HIGHEST_INDEXED + 1, Some(HIGHEST_INDEXED + 5)),
            (u64::MAX, None),
        ] {
            let request = all_packages_request(
                Some(start_checkpoint),
                end_checkpoint,
                10,
                Ordering::Ascending,
                None,
                None,
            );
            let query = parse(&request, 0).unwrap();
            assert!(query.reads_nothing(), "{request:?}");
            assert_eq!(
                end_reason(&query, 0),
                QueryEndReason::LedgerTip,
                "{request:?}"
            );
        }

        for start_checkpoint in [None, Some(20)] {
            for end_checkpoint in [HIGHEST_INDEXED + 2, u64::MAX] {
                let request = all_packages_request(
                    start_checkpoint,
                    Some(end_checkpoint),
                    10,
                    Ordering::Ascending,
                    None,
                    None,
                );
                let query = parse(&request, 0).unwrap();
                assert!(!query.reads_nothing(), "{request:?}");
                assert_eq!(query.upper, last_key(HIGHEST_INDEXED), "{request:?}");
                assert_eq!(
                    end_reason(&query, 3),
                    QueryEndReason::LedgerTip,
                    "{request:?}"
                );
            }
        }

        for after in [HIGHEST_INDEXED + 1, u64::MAX] {
            let request = all_packages_request(
                None,
                None,
                10,
                Ordering::Ascending,
                Some(cursor_at(after)),
                None,
            );
            let query = parse(&request, 0).unwrap();
            assert!(query.reads_nothing(), "after {after}");
            assert_eq!(
                end_reason(&query, 0),
                QueryEndReason::LedgerTip,
                "after {after}"
            );
        }
    }

    /// A cursor past the highest indexed checkpoint comes from a newer view of the ledger. A
    /// descending listing whose range reaches past the tip is refused after or before it, naming
    /// the cursor: a page from this server's view would skip the versions published in between.
    /// One whose range ends at or below the checkpoint after the tip holds no key above the cursor
    /// and is served, as is an ascending listing, which ends with `LEDGER_TIP`.
    #[test]
    fn descending_cursor_past_the_tip_is_refused_when_the_range_reaches_it() {
        let past_tip = HIGHEST_INDEXED + 1;
        for [after, before] in [
            [Some(cursor_at(past_tip)), None],
            [None, Some(cursor_at(past_tip))],
        ] {
            let request = all_packages_request(None, None, 10, Ordering::Descending, after, before);
            assert_above_latest_available_error(
                parse(&request, 0).unwrap_err(),
                "cursor",
                past_tip,
                past_tip,
            );

            for end_checkpoint in [20, HIGHEST_INDEXED + 1] {
                let request = all_packages_request(
                    None,
                    Some(end_checkpoint),
                    10,
                    Ordering::Descending,
                    after,
                    before,
                );
                parse(&request, 0).unwrap();
            }

            let request = all_packages_request(None, None, 10, Ordering::Ascending, after, before);
            let query = parse(&request, 0).unwrap();
            assert_eq!(
                end_reason(&query, 0),
                QueryEndReason::LedgerTip,
                "{request:?}"
            );
        }
    }

    /// On a server that can list every publish from checkpoint 10, a listing with no lower bound
    /// of its own (neither `start_checkpoint` nor an `after` cursor) covers the publishes from 10
    /// on, so one with an `end_checkpoint` at or below 10 would cover none of them: it is refused,
    /// in either order. A `start_checkpoint` at the floor, or an `after` cursor at or above it,
    /// bounds the listing, which then reads nothing. An `end_checkpoint` of 11 covers checkpoint
    /// 10.
    #[test]
    fn end_checkpoint_at_or_below_the_floor_is_refused_without_a_lower_bound() {
        const FLOOR: u64 = 10;
        for ordering in [Ordering::Ascending, Ordering::Descending] {
            for end_checkpoint in [1, 5, FLOOR] {
                let request =
                    all_packages_request(None, Some(end_checkpoint), 10, ordering, None, None);
                assert_below_serving_floor_error(parse(&request, FLOOR).unwrap_err(), FLOOR);

                for after in [FLOOR, FLOOR + 5] {
                    let request = all_packages_request(
                        None,
                        Some(end_checkpoint),
                        10,
                        ordering,
                        Some(cursor_at(after)),
                        None,
                    );
                    assert!(
                        parse(&request, FLOOR).unwrap().reads_nothing(),
                        "{request:?}"
                    );
                }
            }

            let request = all_packages_request(Some(FLOOR), Some(FLOOR), 10, ordering, None, None);
            assert!(
                parse(&request, FLOOR).unwrap().reads_nothing(),
                "{ordering:?}"
            );

            let request = all_packages_request(None, Some(FLOOR + 1), 10, ordering, None, None);
            let query = parse(&request, FLOOR).unwrap();
            assert_eq!(
                [query.lower, query.upper],
                [first_key(FLOOR), last_key(FLOOR)],
                "{ordering:?}"
            );
        }
    }

    /// An empty range lists nothing and ends with `CHECKPOINT_BOUND`, in either order, even below
    /// the floor: it holds no publish to miss.
    #[test]
    fn empty_range_reads_nothing_even_below_the_floor() {
        for ordering in [Ordering::Ascending, Ordering::Descending] {
            for (start_checkpoint, end_checkpoint) in
                [(None, 0), (Some(0), 0), (Some(3), 3), (Some(30), 30)]
            {
                for lowest_checkpoint in [0, 10] {
                    let request = all_packages_request(
                        start_checkpoint,
                        Some(end_checkpoint),
                        10,
                        ordering,
                        None,
                        None,
                    );
                    let query = parse(&request, lowest_checkpoint).unwrap();
                    assert!(query.reads_nothing(), "{request:?}");
                    assert_eq!(
                        end_reason(&query, 0),
                        QueryEndReason::CheckpointBound,
                        "{request:?}"
                    );
                }
            }
        }
    }

    /// An `end_checkpoint` below `start_checkpoint` is `INVALID_ARGUMENT`, in either order.
    #[test]
    fn end_checkpoint_below_start_checkpoint_is_invalid() {
        for ordering in [Ordering::Ascending, Ordering::Descending] {
            let request = all_packages_request(Some(20), Some(19), 10, ordering, None, None);
            let status = parse(&request, 0).unwrap_err().into_status_proto();
            assert_eq!(
                status.code,
                tonic::Code::InvalidArgument as i32,
                "{status:?}"
            );
        }
    }

    /// A malformed request is `INVALID_ARGUMENT` before any range rule applies, even when its
    /// range reaches past the tip or below the floor: an undecodable cursor, a cursor from the
    /// listing of one package, an invalid ordering, or a lineage paging field.
    #[test]
    fn malformed_requests_are_invalid_whatever_their_range() {
        const FLOOR: u64 = 10;
        let lineage_cursor = PackageVersionCursor::Lineage {
            original_package_id: ObjectID::ZERO,
            version: 1,
            minted_at: HIGHEST_INDEXED,
        };
        let undecodable = |mut request: ListPackageVersionsRequest| {
            request.options.as_mut().unwrap().after = Some(Bytes::from_static(&[0xFF; 2]));
            request
        };
        let invalid_ordering = |mut request: ListPackageVersionsRequest| {
            request.options.as_mut().unwrap().ordering = Some(7);
            request
        };
        let with_page_size = |mut request: ListPackageVersionsRequest| {
            request.page_size = Some(1);
            request
        };
        // A descending range up to `u64::MAX` reaches past the tip, and one from checkpoint 3
        // reaches below the floor.
        let past_tip = |after| {
            all_packages_request(None, Some(u64::MAX), 10, Ordering::Descending, after, None)
        };
        let below_floor =
            |after| all_packages_request(Some(3), None, 10, Ordering::Ascending, after, None);
        for request in [
            undecodable(past_tip(None)),
            past_tip(Some(lineage_cursor)),
            invalid_ordering(past_tip(None)),
            with_page_size(past_tip(None)),
            undecodable(below_floor(None)),
            below_floor(Some(lineage_cursor)),
            invalid_ordering(below_floor(None)),
            with_page_size(below_floor(None)),
        ] {
            let status = parse(&request, FLOOR).unwrap_err().into_status_proto();
            assert_eq!(
                status.code,
                tonic::Code::InvalidArgument as i32,
                "{request:?}: {status:?}"
            );
        }
    }

    /// A descending listing after a cursor below the floor and before one past the tip is refused
    /// for the cursor past the tip, as `ListTransactions` and the listing of one package refuse
    /// it: the tip is checked before the floor. An ascending listing is not refused for that
    /// cursor, so it is refused for the floor.
    #[test]
    fn descending_cursor_past_the_tip_is_refused_before_the_floor() {
        const FLOOR: u64 = 10;
        let past_tip = HIGHEST_INDEXED + 1;
        let request = |ordering| {
            all_packages_request(
                None,
                None,
                10,
                ordering,
                Some(cursor_at(FLOOR - 1)),
                Some(cursor_at(past_tip)),
            )
        };
        assert_above_latest_available_error(
            parse(&request(Ordering::Descending), FLOOR).unwrap_err(),
            "cursor",
            past_tip,
            past_tip,
        );
        assert_below_serving_floor_error(
            parse(&request(Ordering::Ascending), FLOOR).unwrap_err(),
            FLOOR,
        );
    }

    /// The order of the floor and tip refusals follows `ListTransactions`' `floor_error_or`. A
    /// descending listing from a `start_checkpoint` below the floor with no `after` cursor can
    /// never be served, so it is refused for the floor even when its `before` cursor is past the
    /// tip. With an `after` cursor below the floor too, the cursor past the tip is refused first.
    /// An empty range below the floor is refused for neither: it lists nothing.
    #[test]
    fn start_below_the_floor_is_refused_before_a_cursor_past_the_tip_without_an_after_cursor() {
        const FLOOR: u64 = 10;
        let past_tip = HIGHEST_INDEXED + 1;
        let request = |after: Option<PackageVersionCursor>| {
            all_packages_request(
                Some(FLOOR - 5),
                None,
                10,
                Ordering::Descending,
                after,
                Some(cursor_at(past_tip)),
            )
        };
        assert_below_serving_floor_error(parse(&request(None), FLOOR).unwrap_err(), FLOOR);
        assert_above_latest_available_error(
            parse(&request(Some(cursor_at(FLOOR - 2))), FLOOR).unwrap_err(),
            "cursor",
            past_tip,
            past_tip,
        );

        // On a server that keeps full history the cursor past the tip is refused.
        assert_above_latest_available_error(
            parse(&request(None), 0).unwrap_err(),
            "cursor",
            past_tip,
            past_tip,
        );
        let mut empty = request(None);
        empty.end_checkpoint = empty.start_checkpoint;
        assert!(parse(&empty, FLOOR).unwrap().reads_nothing());
    }

    /// A full page ends with `ITEM_LIMIT`, a shorter one with the reason of the bound it reached:
    /// `CURSOR_BOUND` at a cursor inside the range on the far side in scan order (a descending
    /// listing's `after`, an ascending one's `before`), `LEDGER_TIP` where an ascending range
    /// reaches past the checkpoint after the highest indexed one, and `CHECKPOINT_BOUND` at the
    /// range's edge otherwise.
    #[test]
    fn pages_end_with_the_reason_of_their_bound() {
        let inside = Some(cursor_at(20));
        for (request, reason) in [
            (
                all_packages_request(None, None, 2, Ordering::Descending, None, None),
                QueryEndReason::CheckpointBound,
            ),
            (
                all_packages_request(None, None, 2, Ordering::Descending, None, inside),
                QueryEndReason::CheckpointBound,
            ),
            (
                all_packages_request(
                    Some(10),
                    None,
                    2,
                    Ordering::Descending,
                    Some(cursor_at(5)),
                    None,
                ),
                QueryEndReason::CheckpointBound,
            ),
            (
                all_packages_request(None, Some(30), 2, Ordering::Ascending, None, None),
                QueryEndReason::CheckpointBound,
            ),
            (
                all_packages_request(
                    None,
                    Some(HIGHEST_INDEXED + 1),
                    2,
                    Ordering::Ascending,
                    None,
                    None,
                ),
                QueryEndReason::CheckpointBound,
            ),
            (
                all_packages_request(None, None, 2, Ordering::Descending, inside, None),
                QueryEndReason::CursorBound,
            ),
            (
                all_packages_request(None, None, 2, Ordering::Ascending, None, inside),
                QueryEndReason::CursorBound,
            ),
            (
                all_packages_request(None, None, 2, Ordering::Ascending, None, None),
                QueryEndReason::LedgerTip,
            ),
            (
                all_packages_request(None, None, 2, Ordering::Ascending, inside, None),
                QueryEndReason::LedgerTip,
            ),
            (
                all_packages_request(
                    None,
                    Some(HIGHEST_INDEXED + 2),
                    2,
                    Ordering::Ascending,
                    None,
                    None,
                ),
                QueryEndReason::LedgerTip,
            ),
        ] {
            let query = parse(&request, 0).unwrap();
            assert_eq!(
                end_reason(&query, 2),
                QueryEndReason::ItemLimit,
                "{request:?}"
            );
            assert_eq!(end_reason(&query, 1), reason, "{request:?}");
        }
    }

    /// An `after` cursor below the floor, with no `start_checkpoint`, names a version the client
    /// holds, and the versions published between it and the floor may be gone: a listing after
    /// it is refused, in either order, rather than reach the floor. With `start_checkpoint` at
    /// the floor, the cursor only bounds the listing.
    #[test]
    fn listing_after_a_cursor_below_the_floor_is_refused() {
        const FLOOR: u64 = 10;
        let after = Some(cursor_at(FLOOR - 1));

        for ordering in [Ordering::Ascending, Ordering::Descending] {
            let request = all_packages_request(None, None, 2, ordering, after, None);
            assert_below_serving_floor_error(parse(&request, FLOOR).unwrap_err(), FLOOR);
        }

        for ordering in [Ordering::Ascending, Ordering::Descending] {
            let request = all_packages_request(Some(FLOOR), None, 2, ordering, after, None);
            let query = parse(&request, FLOOR).unwrap();
            assert_eq!(query.lower, first_key(FLOOR), "{ordering:?}");
        }
    }

    /// A `start_checkpoint` below the floor is refused, in either order, unless an `after` cursor
    /// at or above the floor bounds the listing: the range then holds nothing below the floor,
    /// and the listing starts after the cursor. An `after` cursor below the floor bounds nothing
    /// the floor admits.
    #[test]
    fn start_checkpoint_below_the_floor_is_served_after_a_cursor_at_or_above_it() {
        const FLOOR: u64 = 10;
        for ordering in [Ordering::Ascending, Ordering::Descending] {
            for checkpoint in [FLOOR, FLOOR + 5] {
                let after = Some(cursor_at(checkpoint));
                let request = all_packages_request(Some(3), None, 2, ordering, after, None);
                let query = parse(&request, FLOOR).unwrap();
                let after = PackagePublishKey {
                    checkpoint,
                    original_id: ObjectID::ZERO,
                    version: 1,
                };
                assert_eq!(
                    query.lower,
                    Bound::Excluded(after),
                    "{ordering:?} after {checkpoint}"
                );
            }

            for after in [None, Some(cursor_at(FLOOR - 1))] {
                let request = all_packages_request(Some(3), None, 2, ordering, after, None);
                assert_below_serving_floor_error(parse(&request, FLOOR).unwrap_err(), FLOOR);
            }
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
    fn ranges_below_pruned_versions_start_at_the_lowest_stored() {
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
    fn ranges_below_versions_without_rows_start_at_the_lowest_row() {
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
    fn rows_with_no_stored_version_have_no_lowest_stored_version() {
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
    fn missing_version_is_internal_on_a_node_that_pruned_nothing() {
        let package = ObjectID::from_single_byte(0x2);
        let status: tonic::Status =
            require_history_above_genesis(AtCheckpointLowerBound::new(0), package, 1)
                .unwrap_err()
                .into();
        assert_eq!(status.code(), tonic::Code::Internal);
    }

    /// A request listing the lineage of package 0x2 from `start_version`, through a page token
    /// naming `token`, or in cursor mode after `after` and before `before`.
    fn lineage_request(
        start_version: Option<u64>,
        token: Option<u64>,
        cursors: Option<(Option<u64>, Option<u64>)>,
    ) -> ListPackageVersionsRequest {
        let package = ObjectID::from_single_byte(0x2);
        let cursor = |version| {
            PackageVersionCursor::Lineage {
                original_package_id: package,
                version,
                minted_at: 0,
            }
            .encode()
        };
        let mut request = ListPackageVersionsRequest::default();
        request.package_id = Some(package.to_string());
        request.start_version = start_version;
        request.page_token = token.map(|version| {
            encode_page_token(PageToken {
                original_package_id: package,
                version,
            })
        });
        request.options = cursors.map(|(after, before)| {
            let mut options = sui_rpc::proto::sui::rpc::v2::QueryOptions::default();
            options.after = after.map(cursor);
            options.before = before.map(cursor);
            options
        });
        request
    }

    /// A request continues a listing, in either order, when its range starts just above a version
    /// the client holds: one an `after` cursor names, or the one below a page token's, unless a
    /// higher `start_version` bounds the range or no lineage has that version.
    #[test]
    fn continuation_is_an_after_cursor_or_page_token_that_bounds_the_range() {
        let package = ObjectID::from_single_byte(0x2);
        let mut descending_after = lineage_request(None, None, Some((Some(2), None)));
        descending_after.options.as_mut().unwrap().ordering = Some(Ordering::Descending as i32);
        for (request, continues) in [
            (lineage_request(None, None, None), false),
            (lineage_request(Some(3), None, None), false),
            (lineage_request(None, None, Some((None, None))), false),
            (lineage_request(None, None, Some((None, Some(3)))), false),
            (lineage_request(None, None, Some((Some(2), None))), true),
            (lineage_request(None, None, Some((Some(2), Some(9)))), true),
            (lineage_request(Some(3), None, Some((Some(2), None))), true),
            (lineage_request(Some(4), None, Some((Some(2), None))), false),
            (descending_after, true),
            // No lineage has version 0, so a client holds none.
            (lineage_request(None, None, Some((Some(0), None))), false),
            (lineage_request(None, Some(3), None), true),
            (lineage_request(Some(3), Some(3), None), true),
            (lineage_request(Some(4), Some(3), None), false),
            (lineage_request(None, Some(1), None), false),
            (lineage_request(None, Some(0), None), false),
        ] {
            let query = PackageVersionsQuery::new(&request, package, u64::MAX).unwrap();
            assert_eq!(query.continues_listing(), continues, "{request:?}");
        }
    }
}

/// Listings through the handler over an rpc-store, whose `package_versions` rows outlive the
/// package objects pruning deletes.
#[cfg(test)]
mod store_tests {
    use std::sync::{Arc, Mutex};

    use move_binary_format::file_format::empty_module;
    use sui_consistent_store::{ChainId, Db, FrameworkSchema, PipelineTaskKey};
    use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
    use sui_rpc_store::schema::{objects, package_versions, pruning_watermark};
    use sui_rpc_store::{RpcStoreReader, RpcStoreSchema};
    use sui_types::base_types::{SequenceNumber, VersionNumber};
    use sui_types::digests::TransactionDigest;
    use sui_types::move_package::MovePackage;
    use sui_types::object::{Data, Object};
    use sui_types::storage::{ObjectStore, RpcIndexes, RpcStateReader};

    use super::*;
    use crate::ledger_history::query_options::assert_below_serving_floor_error;
    use crate::reader::tests::commit_every_pipeline;

    /// The highest checkpoint every rpc index of the store has committed.
    const HIGHEST_INDEXED: u64 = 20;

    /// A system package upgraded in place: every version lives under this ID.
    fn system() -> ObjectID {
        ObjectID::from_single_byte(0xAB)
    }

    /// An rpc-store with every rpc index committed through [`HIGHEST_INDEXED`].
    struct Store {
        _dir: tempfile::TempDir,
        db: Db,
        schema: Arc<RpcStoreSchema>,
    }

    impl Store {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let (db, schema) =
                Db::open::<RpcStoreSchema>(dir.path(), sui_consistent_store::DbOptions::default())
                    .unwrap();
            let mut batch = db.batch();
            batch
                .put(
                    &FrameworkSchema::new(db.clone()).chain_ids,
                    &PipelineTaskKey::new("test"),
                    &ChainId([1; 32]),
                )
                .unwrap();
            batch.commit().unwrap();
            commit_every_pipeline(&db, HIGHEST_INDEXED);
            Self {
                _dir: dir,
                db,
                schema: Arc::new(schema),
            }
        }

        /// The store holding versions `1..=3` of [`system`], published at checkpoints 2, 5 and 8,
        /// with their objects.
        fn with_system_package() -> Self {
            Self::with_system_versions(&[(1, 2), (2, 5), (3, 8)])
        }

        /// The store holding each `(version, checkpoint)` of [`system`] in `published`, with its
        /// object.
        fn with_system_versions(published: &[(u64, u64)]) -> Self {
            let store = Self::new();
            let mut batch = store.db.batch();
            for &(version, checkpoint) in published {
                let mut module = empty_module();
                module.address_identifiers[0] = system().into();
                let package = Object::new_package_from_data(
                    Data::Package(MovePackage::new_system(
                        SequenceNumber::from_u64(version),
                        &[module],
                        [],
                    )),
                    TransactionDigest::genesis_marker(),
                );
                let key = objects::Key {
                    id: system(),
                    version: package.version(),
                };
                batch
                    .put(&store.schema.objects, &key, &objects::store(&package))
                    .unwrap();
                let (key, value) = package_versions::store(system(), version, system(), checkpoint);
                batch
                    .put(&store.schema.package_versions, &key, &value)
                    .unwrap();
            }
            batch.commit().unwrap();
            store
        }

        /// Prune object history below `lowest_available`, deleting `versions` of [`system`], as
        /// the pruner does: the floor rises before the objects are deleted.
        fn prune(&self, lowest_available: u64, versions: &[u64]) {
            prune(&self.db, &self.schema, lowest_available, versions);
        }

        fn reader(&self) -> RpcStoreReader {
            RpcStoreReader::new(self.db.clone(), self.schema.clone())
        }

        /// The response to `request`, served from `reader`.
        fn respond_with(
            reader: impl RpcStateReader + 'static,
            request: ListPackageVersionsRequest,
        ) -> Result<ListPackageVersionsResponse> {
            let service = RpcService::new(Arc::new(reader));
            list_package_versions(&service, request)
        }

        fn respond(
            &self,
            request: ListPackageVersionsRequest,
        ) -> Result<ListPackageVersionsResponse> {
            Self::respond_with(self.reader(), request)
        }

        /// The versions `request` lists.
        fn list(&self, request: ListPackageVersionsRequest) -> Result<Vec<u64>> {
            Ok(listed(&self.respond(request)?))
        }

        /// The versions `request` lists, and how the listing ends.
        fn list_to_end(
            &self,
            request: ListPackageVersionsRequest,
        ) -> (Vec<u64>, Option<QueryEndReason>) {
            let response = self.respond(request.clone()).unwrap();
            (listed(&response), response.end.map(|end| end.reason()))
        }
    }

    /// The versions `response` lists.
    fn listed(response: &ListPackageVersionsResponse) -> Vec<u64> {
        response
            .versions
            .iter()
            .map(|version| version.version.unwrap())
            .collect()
    }

    /// Assert that `error` is `INVALID_ARGUMENT`.
    fn assert_invalid_argument(error: RpcError) {
        let status = error.into_status_proto();
        assert_eq!(
            status.code,
            tonic::Code::InvalidArgument as i32,
            "{status:?}"
        );
    }

    fn prune(db: &Db, schema: &RpcStoreSchema, lowest_available: u64, versions: &[u64]) {
        let (key, value) = pruning_watermark::store(&pruning_watermark::Watermarks {
            tx_seq_lo: 100,
            checkpoint_lo: lowest_available,
        });
        let mut batch = db.batch();
        batch.put(&schema.pruning_watermark, &key, &value).unwrap();
        batch.commit().unwrap();
        let mut batch = db.batch();
        for &version in versions {
            let key = objects::Key {
                id: system(),
                version: SequenceNumber::from_u64(version),
            };
            batch.delete(&schema.objects, &key).unwrap();
        }
        batch.commit().unwrap();
    }

    /// A request listing [`system`]'s lineage up to `end_checkpoint`, with `options` when
    /// `ordering` is set, after the version `after`, or from the page token naming `token`.
    fn lineage(
        end_checkpoint: Option<u64>,
        ordering: Option<Ordering>,
        after: Option<u64>,
        token: Option<u64>,
    ) -> ListPackageVersionsRequest {
        let mut request = ListPackageVersionsRequest::default();
        request.package_id = Some(system().to_string());
        request.end_checkpoint = end_checkpoint;
        request.options = ordering.map(|ordering| {
            let mut options = QueryOptions::default();
            options.ordering = Some(ordering as i32);
            options.after = after.map(|version| {
                PackageVersionCursor::Lineage {
                    original_package_id: system(),
                    version,
                    minted_at: HIGHEST_INDEXED,
                }
                .encode()
            });
            options
        });
        request.page_token = token.map(|version| {
            encode_page_token(PageToken {
                original_package_id: system(),
                version,
            })
        });
        request
    }

    /// Once pruning deletes versions 1 and 2 but keeps their rows, a listing in cursor mode
    /// starts at version 3, in either order, and an `after` cursor at version 2 continues to it.
    /// A continuation whose client holds version 1, from an `after` cursor at it in either
    /// order, is refused: answering it would skip version 2. The refusal names the lowest
    /// `start_checkpoint` this node accepts, the checkpoint after its `at_checkpoint` floor. A
    /// legacy page-token listing lists every row, pruned or not.
    #[test]
    fn pruned_lineage_lists_from_the_lowest_stored_version() {
        let store = Store::with_system_package();
        store.prune(10, &[1, 2]);

        for (request, expected) in [
            (
                lineage(None, Some(Ordering::Ascending), None, None),
                vec![3],
            ),
            (
                lineage(None, Some(Ordering::Descending), None, None),
                vec![3],
            ),
            (lineage(Some(16), None, None, None), vec![3]),
            (
                lineage(None, Some(Ordering::Ascending), Some(2), None),
                vec![3],
            ),
            (lineage(None, None, None, None), vec![1, 2, 3]),
            (lineage(None, None, None, Some(2)), vec![2, 3]),
        ] {
            assert_eq!(
                store.list(request.clone()).unwrap(),
                expected,
                "{request:?}"
            );
        }

        for request in [
            lineage(None, Some(Ordering::Ascending), Some(1), None),
            lineage(None, Some(Ordering::Descending), Some(1), None),
        ] {
            assert_below_serving_floor_error(store.list(request).unwrap_err(), 10);
        }
    }

    /// On a node whose `at_checkpoint` floor is 9, the lowest `start_checkpoint` it accepts is
    /// 10, and in a listing with no lower bound of its own, the lowest `end_checkpoint` is 11. A
    /// range reaching below that is refused whatever its listing finds: versions, nothing, a
    /// continuation refusal, or no package at all. With an `after` cursor and no
    /// `start_checkpoint`, the continuation rules govern instead, and an empty range lists
    /// nothing whatever the floor.
    #[test]
    fn checkpoint_range_below_the_floor_is_refused_whatever_the_listing_finds() {
        let store = Store::with_system_package();
        store.prune(10, &[1, 2]);
        let mut unknown = lineage(Some(10), None, None, None);
        unknown.package_id = Some(ObjectID::from_single_byte(0xCD).to_string());
        let mut start_below = lineage(None, Some(Ordering::Ascending), None, None);
        start_below.start_checkpoint = Some(9);
        let mut continuation_below = lineage(None, Some(Ordering::Descending), Some(1), None);
        continuation_below.start_checkpoint = Some(0);
        for request in [
            lineage(Some(10), None, None, None),
            lineage(Some(6), Some(Ordering::Descending), None, None),
            lineage(Some(5), Some(Ordering::Ascending), Some(1), None),
            start_below,
            continuation_below,
            unknown,
        ] {
            assert_below_serving_floor_error(store.list(request.clone()).unwrap_err(), 10);
        }

        let mut lowest_start = lineage(None, Some(Ordering::Ascending), None, None);
        lowest_start.start_checkpoint = Some(10);
        let mut empty_below = lineage(Some(5), None, None, None);
        empty_below.start_checkpoint = Some(5);
        for (request, expected) in [
            (lineage(Some(11), None, None, None), vec![3]),
            (
                lineage(Some(11), Some(Ordering::Descending), None, None),
                vec![3],
            ),
            (lowest_start, vec![]),
            (
                lineage(Some(9), Some(Ordering::Ascending), Some(2), None),
                vec![3],
            ),
            (lineage(Some(0), None, None, None), vec![]),
            (empty_below, vec![]),
        ] {
            assert_eq!(
                store.list(request.clone()).unwrap(),
                expected,
                "{request:?}"
            );
        }
    }

    /// On a node whose lowest accepted `start_checkpoint` is 6, a `start_checkpoint` below it is
    /// served when the first version after an `after` cursor was published at or above 6, as
    /// `ListTransactions` checks its start only after the cursors narrow it: after version 2
    /// (published at 4) the listing starts at version 3 (published at 6), and after version 3 at
    /// version 4. Without a cursor, or after version 1 (version 2 follows, published at 4), it is
    /// refused.
    #[test]
    fn lineage_start_checkpoint_below_the_floor_is_served_after_a_cursor_at_or_above_it() {
        let store = pruned_five_version_store();
        for start_checkpoint in [0, 2, 5] {
            let range = [Some(start_checkpoint), None];
            for (after, ascending) in [(2, vec![3, 4, 5]), (3, vec![4, 5])] {
                for (ordering, expected) in [
                    (Ordering::Ascending, ascending.clone()),
                    (
                        Ordering::Descending,
                        ascending.iter().rev().copied().collect(),
                    ),
                ] {
                    let request = cursor_lineage(range, ordering, 10, Some(after), None);
                    assert_eq!(
                        store.list(request.clone()).unwrap(),
                        expected,
                        "{request:?}"
                    );
                }
            }
            for ordering in [Ordering::Ascending, Ordering::Descending] {
                for after in [None, Some(1)] {
                    let request = cursor_lineage(range, ordering, 10, after, None);
                    assert_below_serving_floor_error(store.list(request.clone()).unwrap_err(), 6);
                }
            }
        }
    }

    /// A request listing [`system`]'s lineage in cursor mode over the checkpoint range
    /// `[start_checkpoint, end_checkpoint)`, `limit` versions at a time in `ordering`, after the
    /// version `after` and before the version `before`.
    fn cursor_lineage(
        [start_checkpoint, end_checkpoint]: [Option<u64>; 2],
        ordering: Ordering,
        limit: u32,
        after: Option<u64>,
        before: Option<u64>,
    ) -> ListPackageVersionsRequest {
        let cursor = |version| {
            PackageVersionCursor::Lineage {
                original_package_id: system(),
                version,
                minted_at: HIGHEST_INDEXED,
            }
            .encode()
        };
        let mut options = QueryOptions::default();
        options.ordering = Some(ordering as i32);
        options.limit = Some(limit);
        options.after = after.map(cursor);
        options.before = before.map(cursor);
        let mut request = ListPackageVersionsRequest::default();
        request.package_id = Some(system().to_string());
        request.start_checkpoint = start_checkpoint;
        request.end_checkpoint = end_checkpoint;
        request.options = Some(options);
        request
    }

    /// An `end_checkpoint` below `start_checkpoint` is `INVALID_ARGUMENT`, and equal bounds list
    /// nothing, including an `end_checkpoint` of 0, which needs no lookup at the checkpoint before
    /// it; likewise an `end_version` below `start_version`. So is a page token or page size with a
    /// checkpoint bound, which pages by cursor, and a cursor that does not decode, names another
    /// lineage, or comes from the listing of every package. A malformed request is refused as
    /// such before any range rule applies, even when its range reaches past the tip or below the
    /// floor.
    #[test]
    fn invalid_checkpoint_ranges_are_refused() {
        let store = Store::with_system_package();
        let pruned = Store::with_system_package();
        pruned.prune(10, &[1, 2]);
        for ordering in [Ordering::Ascending, Ordering::Descending] {
            assert_invalid_argument(
                store
                    .list(cursor_lineage([Some(5), Some(4)], ordering, 10, None, None))
                    .unwrap_err(),
            );
            for range in [[Some(5), Some(5)], [None, Some(0)], [Some(0), Some(0)]] {
                assert_eq!(
                    store.list_to_end(cursor_lineage(range, ordering, 10, None, None)),
                    (vec![], Some(QueryEndReason::CheckpointBound)),
                    "{range:?} {ordering:?}"
                );
            }
        }

        // Each range below reaches below the pruned store's lowest accepted `start_checkpoint`,
        // 11; the legacy listing has none.
        let mut page_size = lineage(Some(9), None, None, None);
        page_size.page_size = Some(1);
        let mut page_token = lineage(None, None, None, Some(2));
        page_token.start_checkpoint = Some(3);
        let mut versions_backwards =
            cursor_lineage([Some(3), Some(9)], Ordering::Ascending, 10, None, None);
        versions_backwards.start_version = Some(3);
        versions_backwards.end_version = Some(2);
        let mut legacy_versions_backwards = lineage(None, None, None, None);
        legacy_versions_backwards.start_version = Some(3);
        legacy_versions_backwards.end_version = Some(2);
        // A descending range up to `u64::MAX` also reaches past the tip.
        let mut undecodable_after = cursor_lineage(
            [Some(3), Some(u64::MAX)],
            Ordering::Descending,
            10,
            None,
            None,
        );
        undecodable_after.options.as_mut().unwrap().after = Some(Bytes::from_static(&[0xFF; 2]));
        let other_lineage = PackageVersionCursor::Lineage {
            original_package_id: ObjectID::from_single_byte(0xCD),
            version: 1,
            minted_at: HIGHEST_INDEXED,
        }
        .encode();
        let mut other_lineage_after =
            cursor_lineage([Some(3), None], Ordering::Ascending, 10, None, None);
        other_lineage_after.options.as_mut().unwrap().after = Some(other_lineage.clone());
        let mut other_lineage_before =
            cursor_lineage([Some(3), None], Ordering::Ascending, 10, None, None);
        other_lineage_before.options.as_mut().unwrap().before = Some(other_lineage.clone());
        // Another lineage's cursor is refused as malformed, not for the descending range past
        // the tip, and not for the start below the pruned store's floor.
        let mut other_lineage_past_the_tip = cursor_lineage(
            [Some(3), Some(u64::MAX)],
            Ordering::Descending,
            10,
            None,
            None,
        );
        other_lineage_past_the_tip.options.as_mut().unwrap().before = Some(other_lineage);
        let mut every_package_after = cursor_lineage(
            [Some(3), Some(u64::MAX)],
            Ordering::Descending,
            10,
            None,
            None,
        );
        every_package_after.options.as_mut().unwrap().after = Some(
            PackageVersionCursor::AllPackages {
                checkpoint: 5,
                original_package_id: system(),
                version: 2,
                minted_at: HIGHEST_INDEXED,
            }
            .encode(),
        );
        for request in [
            page_size,
            page_token,
            versions_backwards,
            legacy_versions_backwards,
            undecodable_after,
            other_lineage_after,
            other_lineage_before,
            other_lineage_past_the_tip,
            every_package_after,
        ] {
            assert_invalid_argument(store.list(request.clone()).unwrap_err());
            assert_invalid_argument(pruned.list(request).unwrap_err());
        }
        let mut versions_equal = lineage(None, Some(Ordering::Ascending), None, None);
        versions_equal.start_version = Some(2);
        versions_equal.end_version = Some(2);
        assert_eq!(store.list(versions_equal).unwrap(), Vec::<u64>::new());
    }

    /// The cursor naming `version` of [`system`]'s lineage, minted by a server whose highest
    /// indexed checkpoint was `minted_at`.
    fn minted_cursor(version: u64, minted_at: u64) -> Bytes {
        PackageVersionCursor::Lineage {
            original_package_id: system(),
            version,
            minted_at,
        }
        .encode()
    }

    /// A listing in cursor mode reads the package as of the highest indexed checkpoint: a first
    /// page for a package that never existed, or one published only above that checkpoint, is
    /// `NotFound`, and one published at or below it is listed. A legacy page-token listing loads
    /// the package from the live object set, so a package executed but not yet indexed gets a
    /// page with no versions.
    #[test]
    fn cursor_mode_reads_packages_as_of_the_highest_indexed_checkpoint() {
        let ascending = || cursor_lineage([None, None], Ordering::Ascending, 10, None, None);
        let assert_not_found = |result: Result<ListPackageVersionsResponse>, case: &str| {
            let status = result.unwrap_err().into_status_proto();
            assert_eq!(
                status.code,
                tonic::Code::NotFound as i32,
                "{case}: {status:?}"
            );
        };
        let mut never_existed = ascending();
        never_existed.package_id = Some(ObjectID::from_single_byte(0xCD).to_string());
        assert_not_found(
            Store::with_system_package().respond(never_existed),
            "never existed",
        );

        let unindexed = Store::with_system_versions(&[(1, HIGHEST_INDEXED + 5)]);
        for ordering in [Ordering::Ascending, Ordering::Descending] {
            assert_not_found(
                unindexed.respond(cursor_lineage([None, None], ordering, 10, None, None)),
                &format!("published above the tip, {ordering:?}"),
            );
        }
        let legacy = unindexed.respond(lineage(None, None, None, None)).unwrap();
        assert_eq!(listed(&legacy), Vec::<u64>::new());
        assert_eq!((legacy.next_page_token, legacy.end), (None, None));

        assert_eq!(
            Store::with_system_package().list_to_end(ascending()),
            (vec![1, 2, 3], Some(QueryEndReason::LedgerTip))
        );
    }

    /// A request with a cursor for a package this node does not know as of its highest indexed
    /// checkpoint, 20, is decided by the cursor's `minted_at`. Minted above 20, by a replica ahead
    /// of this node, it is a cursor past the tip: ascending, an empty page ending as a listing
    /// with nothing at this height ends; descending with the range reaching past the tip,
    /// `ABOVE_LATEST_AVAILABLE` for the cursor at its minting height. Minted at or below 20, the
    /// minting server did not know the package either, or the cursor is forged: `NotFound`. A
    /// cursor for a package this node knows lists the same whatever its `minted_at`.
    #[test]
    fn cursor_for_an_unknown_package_is_decided_by_its_minting_height() {
        let ahead = HIGHEST_INDEXED + 5;
        let request =
            |range, ordering, after: Option<Bytes>, before: Option<Bytes>, end_version| {
                let mut request = cursor_lineage(range, ordering, 10, None, None);
                let options = request.options.as_mut().unwrap();
                options.after = after;
                options.before = before;
                request.end_version = end_version;
                request
            };

        // The lagging replica: version 1 is published at 25, above this node's tip.
        let lagging = Store::with_system_versions(&[(1, ahead)]);
        for (range, after, before, end_version, expected) in [
            (
                [None, None],
                Some(minted_cursor(1, ahead)),
                None,
                None,
                QueryEndReason::LedgerTip,
            ),
            (
                [None, None],
                None,
                Some(minted_cursor(2, ahead)),
                None,
                QueryEndReason::LedgerTip,
            ),
            (
                [None, Some(HIGHEST_INDEXED + 1)],
                Some(minted_cursor(1, ahead)),
                None,
                None,
                QueryEndReason::CheckpointBound,
            ),
            (
                [None, None],
                Some(minted_cursor(1, ahead)),
                Some(minted_cursor(2, ahead)),
                None,
                QueryEndReason::CursorBound,
            ),
            (
                [None, None],
                Some(minted_cursor(1, ahead)),
                None,
                Some(2),
                QueryEndReason::CheckpointBound,
            ),
        ] {
            let request = request(range, Ordering::Ascending, after, before, end_version);
            assert_eq!(
                lagging.list_to_end(request.clone()),
                (vec![], Some(expected)),
                "{request:?}"
            );
        }
        for (after, before) in [
            (Some(minted_cursor(1, ahead)), None),
            (None, Some(minted_cursor(2, ahead))),
            // One cursor minted ahead is enough.
            (
                Some(minted_cursor(1, HIGHEST_INDEXED)),
                Some(minted_cursor(3, ahead)),
            ),
        ] {
            let request = request([None, None], Ordering::Descending, after, before, None);
            assert_eq!(
                lagging
                    .respond(request.clone())
                    .unwrap_err()
                    .into_status_proto(),
                above_latest_available("cursor", ahead, HIGHEST_INDEXED + 1).into_status_proto(),
                "{request:?}"
            );
        }
        // With an indexed `end_checkpoint`, a descending listing holds nothing published above it.
        assert_eq!(
            lagging.list_to_end(request(
                [None, Some(HIGHEST_INDEXED + 1)],
                Ordering::Descending,
                None,
                Some(minted_cursor(2, ahead)),
                None,
            )),
            (vec![], Some(QueryEndReason::CheckpointBound))
        );

        // A stale or forged cursor, minted at or below this node's tip.
        for minted_at in [0, HIGHEST_INDEXED] {
            for ordering in [Ordering::Ascending, Ordering::Descending] {
                for (after, before) in [
                    (Some(minted_cursor(1, minted_at)), None),
                    (None, Some(minted_cursor(2, minted_at))),
                ] {
                    let request = request([None, None], ordering, after, before, None);
                    let status = lagging
                        .respond(request.clone())
                        .unwrap_err()
                        .into_status_proto();
                    assert_eq!(
                        status.code,
                        tonic::Code::NotFound as i32,
                        "{request:?}: {status:?}"
                    );
                }
            }
        }

        // A package published within this node's height lists the same whatever the cursor's
        // `minted_at`.
        let store = Store::with_system_package();
        for minted_at in [0, HIGHEST_INDEXED, ahead] {
            for (ordering, after, before, expected) in [
                (
                    Ordering::Ascending,
                    Some(minted_cursor(1, minted_at)),
                    None,
                    (vec![2, 3], QueryEndReason::LedgerTip),
                ),
                (
                    Ordering::Descending,
                    None,
                    Some(minted_cursor(3, minted_at)),
                    (vec![2, 1], QueryEndReason::CheckpointBound),
                ),
            ] {
                let request = request([None, None], ordering, after, before, None);
                assert_eq!(
                    store.list_to_end(request.clone()),
                    (expected.0, Some(expected.1)),
                    "{request:?}"
                );
            }
        }
    }

    /// A listing in cursor mode by a storage ID reads that storage ID's version as of the highest
    /// indexed checkpoint, as bare-ID `GetPackage` on kv-rpc does: an upgrade whose version 2 is
    /// published above it is unknown, and a first page by its storage ID is `NotFound`, though
    /// the node stores its object and lists the lineage's version 1 by the original ID.
    #[test]
    fn storage_id_of_an_upgrade_published_above_the_tip_is_not_found() {
        let store = Store::new();
        let original = ObjectID::from_single_byte(0xC1);
        let upgrade = ObjectID::from_single_byte(0xC2);
        // An upgrade's original ID is the address of its modules.
        let mut module = empty_module();
        module.address_identifiers[0] = original.into();
        let mut bytes = vec![];
        module
            .serialize_with_version(
                move_binary_format::file_format_common::VERSION_MAX,
                &mut bytes,
            )
            .unwrap();
        let mut batch = store.db.batch();
        for (id, version, checkpoint) in [(original, 1, 5), (upgrade, 2, HIGHEST_INDEXED + 5)] {
            let package = Object::new_package_from_data(
                Data::Package(
                    MovePackage::new(
                        id,
                        SequenceNumber::from_u64(version),
                        [("m".to_owned(), bytes.clone())].into(),
                        100_000,
                        vec![],
                        Default::default(),
                    )
                    .unwrap(),
                ),
                TransactionDigest::genesis_marker(),
            );
            let key = objects::Key {
                id,
                version: package.version(),
            };
            batch
                .put(&store.schema.objects, &key, &objects::store(&package))
                .unwrap();
            let (key, value) = package_versions::store(original, version, id, checkpoint);
            batch
                .put(&store.schema.package_versions, &key, &value)
                .unwrap();
        }
        batch.commit().unwrap();
        let request = |package_id: ObjectID| {
            let mut request = cursor_lineage([None, None], Ordering::Ascending, 10, None, None);
            request.package_id = Some(package_id.to_string());
            request
        };

        let status = store
            .respond(request(upgrade))
            .unwrap_err()
            .into_status_proto();
        assert_eq!(status.code, tonic::Code::NotFound as i32, "{status:?}");
        assert_eq!(
            store.list_to_end(request(original)),
            (vec![1], Some(QueryEndReason::LedgerTip))
        );
    }

    /// On a pruned node, a descending listing with a `start_checkpoint` below the floor, an
    /// `after` cursor whose next version is below it too, and a `before` cursor past the tip is
    /// refused for the cursor past the tip, not as pruned: as `ListTransactions` keeps a cursor's
    /// tip refusal over the floor when an `after` cursor is present. Without the `after` cursor
    /// the floor refusal stands.
    #[test]
    fn cursor_past_the_tip_is_refused_before_the_floor_with_an_after_cursor() {
        let store = Store::with_system_package();
        store.prune(10, &[1, 2]);
        let status = store
            .respond(cursor_lineage(
                [Some(3), None],
                Ordering::Descending,
                10,
                Some(1),
                Some(9),
            ))
            .unwrap_err()
            .into_status_proto();
        assert_eq!(
            status,
            above_latest_available("cursor", HIGHEST_INDEXED + 1, HIGHEST_INDEXED + 1)
                .into_status_proto()
        );

        let error = store
            .respond(cursor_lineage(
                [Some(3), None],
                Ordering::Descending,
                10,
                None,
                Some(9),
            ))
            .unwrap_err();
        assert_below_serving_floor_error(error, 10);
    }

    /// A `start_checkpoint` starts the lineage at the first version published in it or later,
    /// in either order, and with `end_checkpoint` the listing covers the versions published in
    /// the range. Versions 1, 2 and 3 are published at checkpoints 2, 5 and 8.
    #[test]
    fn start_checkpoint_lists_the_versions_published_from_it() {
        let store = Store::with_system_package();
        for (range, expected) in [
            ([Some(0), None], vec![1, 2, 3]),
            ([Some(2), None], vec![1, 2, 3]),
            ([Some(3), None], vec![2, 3]),
            ([Some(5), None], vec![2, 3]),
            ([Some(6), None], vec![3]),
            ([Some(9), None], vec![]),
            ([Some(3), Some(6)], vec![2]),
            ([Some(3), Some(5)], vec![]),
            ([Some(2), Some(9)], vec![1, 2, 3]),
        ] {
            assert_eq!(
                store
                    .list(cursor_lineage(range, Ordering::Ascending, 10, None, None))
                    .unwrap(),
                expected,
                "{range:?}"
            );
            let mut descending = expected;
            descending.reverse();
            assert_eq!(
                store
                    .list(cursor_lineage(range, Ordering::Descending, 10, None, None))
                    .unwrap(),
                descending,
                "{range:?} descending"
            );
        }
    }

    /// A descending listing whose range reaches past the highest indexed checkpoint, 20, is
    /// refused, as is a descending cursor past it without `end_checkpoint`. An ascending one
    /// lists through the tip and ends with `LEDGER_TIP`, as does an ascending `after` past it,
    /// with nothing to list. A legacy page token past the tip gets an empty page with no next
    /// page token and no `end`.
    #[test]
    fn ranges_and_cursors_past_the_tip() {
        let store = Store::with_system_package();
        let status = |result: Result<_>| result.unwrap_err().into_status_proto();
        for (range, bound, requested) in [
            ([None, Some(22)], "end_checkpoint", 22),
            ([Some(3), Some(u64::MAX)], "end_checkpoint", u64::MAX),
            ([Some(21), None], "start_checkpoint", 21),
        ] {
            assert_eq!(
                status(store.list(cursor_lineage(range, Ordering::Descending, 10, None, None))),
                above_latest_available(bound, requested, 21).into_status_proto(),
                "{range:?}"
            );
        }
        // Version 9 has no row, so its cursor is past the tip.
        assert_eq!(
            status(store.list(cursor_lineage(
                [None, None],
                Ordering::Descending,
                10,
                None,
                Some(9)
            ))),
            AtCheckpointUpperBound::new(HIGHEST_INDEXED)
                .unindexed_cursor_error()
                .into_status_proto(),
        );
        let past_tip_token = store.respond(lineage(None, None, None, Some(9))).unwrap();
        assert_eq!(listed(&past_tip_token), Vec::<u64>::new());
        assert_eq!(
            (past_tip_token.next_page_token, past_tip_token.end),
            (None, None)
        );

        for (request, expected) in [
            (
                cursor_lineage([None, Some(21)], Ordering::Descending, 10, None, None),
                (vec![3, 2, 1], QueryEndReason::CheckpointBound),
            ),
            (
                cursor_lineage([Some(20), None], Ordering::Descending, 10, None, None),
                (vec![], QueryEndReason::CheckpointBound),
            ),
            (
                cursor_lineage([None, Some(21)], Ordering::Descending, 10, None, Some(9)),
                (vec![3, 2, 1], QueryEndReason::CheckpointBound),
            ),
            (
                cursor_lineage([None, Some(21)], Ordering::Ascending, 10, None, None),
                (vec![1, 2, 3], QueryEndReason::CheckpointBound),
            ),
            (
                cursor_lineage([None, Some(22)], Ordering::Ascending, 10, None, None),
                (vec![1, 2, 3], QueryEndReason::LedgerTip),
            ),
            (
                cursor_lineage(
                    [Some(3), Some(u64::MAX)],
                    Ordering::Ascending,
                    10,
                    None,
                    None,
                ),
                (vec![2, 3], QueryEndReason::LedgerTip),
            ),
            (
                cursor_lineage([Some(21), None], Ordering::Ascending, 10, None, None),
                (vec![], QueryEndReason::LedgerTip),
            ),
            (
                cursor_lineage([Some(30), Some(40)], Ordering::Ascending, 10, None, None),
                (vec![], QueryEndReason::LedgerTip),
            ),
            (
                cursor_lineage([None, None], Ordering::Ascending, 10, Some(9), None),
                (vec![], QueryEndReason::LedgerTip),
            ),
            (
                cursor_lineage([None, Some(30)], Ordering::Ascending, 10, Some(9), None),
                (vec![], QueryEndReason::LedgerTip),
            ),
            (
                cursor_lineage([None, None], Ordering::Ascending, 10, Some(9), Some(10)),
                (vec![], QueryEndReason::CursorBound),
            ),
            (
                cursor_lineage([None, None], Ordering::Ascending, 10, Some(1), Some(9)),
                (vec![2, 3], QueryEndReason::LedgerTip),
            ),
        ] {
            assert_eq!(
                store.list_to_end(request.clone()),
                (expected.0, Some(expected.1)),
                "{request:?}"
            );
        }

        // An ascending `after` past the tip ends with the tighter bound's reason when a bound
        // leaves nothing above it, as `ListObjectVersions` ends it: `end_version` at or below the
        // version after `after` ends it with `CHECKPOINT_BOUND`, and `before` there, at or below
        // `end_version`, with `CURSOR_BOUND`. Otherwise it ends with `LEDGER_TIP`.
        for (before, end_version, expected) in [
            (None, Some(10), QueryEndReason::CheckpointBound),
            (None, Some(11), QueryEndReason::LedgerTip),
            (Some(10), Some(6), QueryEndReason::CheckpointBound),
            (Some(10), Some(10), QueryEndReason::CursorBound),
            (Some(10), Some(11), QueryEndReason::CursorBound),
            (Some(11), Some(12), QueryEndReason::LedgerTip),
            (Some(10), None, QueryEndReason::CursorBound),
        ] {
            let mut request =
                cursor_lineage([None, None], Ordering::Ascending, 10, Some(9), before);
            request.end_version = end_version;
            assert_eq!(
                store.list_to_end(request.clone()),
                (vec![], Some(expected)),
                "{request:?}"
            );
        }
    }

    /// Each way a cursor-mode listing ends, and a legacy page-token listing, which sets no
    /// `end`. Versions 1, 2 and 3 are published at checkpoints 2, 5 and 8.
    #[test]
    fn listings_end_with_the_reason_of_their_bound() {
        let store = Store::with_system_package();
        let mut end_version = cursor_lineage([None, None], Ordering::Ascending, 10, None, None);
        end_version.end_version = Some(3);
        let mut start_version = cursor_lineage([None, None], Ordering::Descending, 10, None, None);
        start_version.start_version = Some(2);
        for (request, expected) in [
            (
                cursor_lineage([None, None], Ordering::Ascending, 2, None, None),
                (vec![1, 2], QueryEndReason::ItemLimit),
            ),
            (
                cursor_lineage([None, None], Ordering::Ascending, 3, None, None),
                (vec![1, 2, 3], QueryEndReason::ItemLimit),
            ),
            (
                cursor_lineage([None, None], Ordering::Ascending, 10, None, None),
                (vec![1, 2, 3], QueryEndReason::LedgerTip),
            ),
            (
                cursor_lineage([None, Some(6)], Ordering::Ascending, 10, None, None),
                (vec![1, 2], QueryEndReason::CheckpointBound),
            ),
            (end_version, (vec![1, 2], QueryEndReason::CheckpointBound)),
            (
                cursor_lineage([None, None], Ordering::Ascending, 10, None, Some(3)),
                (vec![1, 2], QueryEndReason::CursorBound),
            ),
            (
                cursor_lineage([None, None], Ordering::Ascending, 10, Some(3), None),
                (vec![], QueryEndReason::LedgerTip),
            ),
            (
                cursor_lineage([None, None], Ordering::Descending, 10, None, None),
                (vec![3, 2, 1], QueryEndReason::CheckpointBound),
            ),
            (start_version, (vec![3, 2], QueryEndReason::CheckpointBound)),
            (
                cursor_lineage([Some(3), None], Ordering::Descending, 10, None, None),
                (vec![3, 2], QueryEndReason::CheckpointBound),
            ),
            (
                cursor_lineage([None, None], Ordering::Descending, 10, Some(1), None),
                (vec![3, 2], QueryEndReason::CursorBound),
            ),
        ] {
            assert_eq!(
                store.list_to_end(request.clone()),
                (expected.0, Some(expected.1)),
                "{request:?}"
            );
        }

        let mut request = lineage(None, None, None, None);
        request.page_size = Some(2);
        let first = store.respond(request.clone()).unwrap();
        assert_eq!(listed(&first), vec![1, 2]);
        assert_eq!(first.end, None);
        request.page_token = first.next_page_token;
        let last = store.respond(request).unwrap();
        assert_eq!(listed(&last), vec![3]);
        assert_eq!((last.end, last.next_page_token), (None, None));
    }

    /// Versions 1 to 5 of [`system`], published at checkpoints 2, 4, 6, 8 and 10, on a node that
    /// pruned object history below checkpoint 6, deleting versions 1 and 2 but keeping their rows.
    fn pruned_five_version_store() -> Store {
        let store = Store::with_system_versions(&[(1, 2), (2, 4), (3, 6), (4, 8), (5, 10)]);
        store.prune(6, &[1, 2]);
        store
    }

    /// A descending walk that sends each page's last version back as the next `before` lists
    /// every stored version once and ends at the lowest of them, although pruned rows remain
    /// below it: every page but the last is full.
    #[test]
    fn descending_pages_end_at_the_lowest_stored_version() {
        let store = pruned_five_version_store();
        for (limit, expected) in [
            (1, vec![vec![5], vec![4], vec![3], vec![]]),
            (2, vec![vec![5, 4], vec![3]]),
            (3, vec![vec![5, 4, 3], vec![]]),
        ] {
            let mut pages = vec![];
            let mut before = None;
            loop {
                let (page, end) = store.list_to_end(cursor_lineage(
                    [None, None],
                    Ordering::Descending,
                    limit,
                    None,
                    before,
                ));
                let full = page.len() == limit as usize;
                before = page.last().copied();
                pages.push(page);
                if !full {
                    assert_eq!(end, Some(QueryEndReason::CheckpointBound));
                    break;
                }
                assert_eq!(end, Some(QueryEndReason::ItemLimit));
            }
            assert_eq!(pages, expected, "limit {limit}");
        }
    }

    /// An `after` cursor at the version just below the lowest stored one skips nothing, so the
    /// listing continues from the lowest stored version in either order. One further below would
    /// skip a pruned version, so it is refused in either order.
    #[test]
    fn after_cursor_just_below_the_lowest_stored_version_continues_in_either_order() {
        let store = pruned_five_version_store();
        for (ordering, expected) in [
            (Ordering::Ascending, vec![3, 4, 5]),
            (Ordering::Descending, vec![5, 4, 3]),
        ] {
            assert_eq!(
                store
                    .list(cursor_lineage([None, None], ordering, 10, Some(2), None))
                    .unwrap(),
                expected,
                "{ordering:?}"
            );
            assert_below_serving_floor_error(
                store
                    .list(cursor_lineage([None, None], ordering, 10, Some(1), None))
                    .unwrap_err(),
                6,
            );
        }
    }

    /// With `end_checkpoint`, a cursor-mode continuation lists only the versions published
    /// before that checkpoint, and is refused over a pruned version as without it. Before
    /// checkpoint 8, version 3 is the only stored version published.
    #[test]
    fn end_checkpoint_cursor_continuation_is_bounded_and_refused_over_pruned_versions() {
        let store = pruned_five_version_store();
        for ordering in [Ordering::Ascending, Ordering::Descending] {
            for (after, expected) in [(None, vec![3]), (Some(2), vec![3]), (Some(3), vec![])] {
                assert_eq!(
                    store
                        .list(cursor_lineage([None, Some(8)], ordering, 10, after, None))
                        .unwrap(),
                    expected,
                    "{ordering:?} after {after:?}"
                );
            }
            assert_below_serving_floor_error(
                store
                    .list(cursor_lineage([None, Some(8)], ordering, 10, Some(1), None))
                    .unwrap_err(),
                6,
            );
        }

        // Paging one version at a time before checkpoint 10, from each page's cursor.
        for (ordering, after, before, expected) in [
            (Ordering::Ascending, Some(3), None, vec![4]),
            (Ordering::Ascending, Some(4), None, vec![]),
            (Ordering::Descending, None, Some(4), vec![3]),
            (Ordering::Descending, None, Some(3), vec![]),
        ] {
            assert_eq!(
                store
                    .list(cursor_lineage([None, Some(10)], ordering, 1, after, before))
                    .unwrap(),
                expected,
                "{ordering:?} after {after:?} before {before:?}"
            );
        }
    }

    /// One page of a cursor-mode listing whose items are keys of type `K`: the keys that a request
    /// over the checkpoint range `[start_checkpoint, end_checkpoint)`, listing `limit` at a time in
    /// `ordering` after the key `after` and before the key `before`, lists, each named by its
    /// cursor, and how the page ends.
    type ListPage<'a, K> = dyn Fn(
            [Option<u64>; 2],
            Ordering,
            u32,
            Option<K>,
            Option<K>,
        ) -> (Vec<K>, Option<QueryEndReason>)
        + 'a;

    /// The keys of `published` (each with the checkpoint it was published in, in listing order)
    /// that a listing over the checkpoint range `[start_checkpoint, end_checkpoint)` in `ordering`
    /// lists from the key `from`: the keys above it ascending (an `after` cursor), below it
    /// descending (a `before` cursor).
    fn keys_from<K: Ord + Copy>(
        published: &[(K, u64)],
        [start_checkpoint, end_checkpoint]: [Option<u64>; 2],
        ordering: Ordering,
        from: Option<K>,
    ) -> Vec<K> {
        let descending = ordering == Ordering::Descending;
        let mut keys: Vec<K> = published
            .iter()
            .filter(|(_, checkpoint)| {
                start_checkpoint.is_none_or(|start| *checkpoint >= start)
                    && end_checkpoint.is_none_or(|end| *checkpoint < end)
            })
            .map(|(key, _)| *key)
            .filter(|key| {
                from.is_none_or(|from| if descending { *key < from } else { *key > from })
            })
            .collect();
        if descending {
            keys.reverse();
        }
        keys
    }

    /// The pages of a walk through the checkpoint range `range` in `ordering`, `limit` keys at a
    /// time, that starts from the key `from` and sends each page's last key back as the next
    /// cursor: `after` ascending, `before` descending. Every page but the last is full and ends
    /// with `ITEM_LIMIT`. The last ends at the edge of the range: with `LEDGER_TIP` when it is
    /// ascending through a range that reaches past the highest indexed checkpoint, else with
    /// `CHECKPOINT_BOUND`.
    fn walk<K: Copy + std::fmt::Debug>(
        list: &ListPage<K>,
        range: [Option<u64>; 2],
        ordering: Ordering,
        limit: u32,
        mut from: Option<K>,
    ) -> Vec<Vec<K>> {
        let descending = ordering == Ordering::Descending;
        let reaches_tip = range[1].is_none_or(|end| end > HIGHEST_INDEXED + 1);
        let mut pages = vec![];
        loop {
            let [after, before] = if descending {
                [None, from]
            } else {
                [from, None]
            };
            let (page, end) = list(range, ordering, limit, after, before);
            let full = page.len() == limit as usize;
            let expected_end = if full {
                QueryEndReason::ItemLimit
            } else if reaches_tip && !descending {
                QueryEndReason::LedgerTip
            } else {
                QueryEndReason::CheckpointBound
            };
            assert_eq!(
                end,
                Some(expected_end),
                "{range:?} {ordering:?} limit {limit} from {from:?}: {page:?}"
            );
            from = page.last().copied().or(from);
            pages.push(page);
            if !full {
                return pages;
            }
        }
    }

    /// For each page of 1 and of 2 keys that an ascending listing of `published` over the
    /// checkpoint range `mint_range` lists from an `after` cursor (so the page starts
    /// mid-listing), a descending walk over `reverse_range` from `before` = the page's first key
    /// lists exactly the keys of `reverse_range` below that key, newest first. When the ranges
    /// match, those are the keys before the page, and together with the ascending walk on from
    /// the page they list every key of the range exactly once.
    fn check_reversing_ascending_pages<K: Ord + Copy + std::fmt::Debug>(
        list: &ListPage<K>,
        published: &[(K, u64)],
        mint_range: [Option<u64>; 2],
        reverse_range: [Option<u64>; 2],
    ) {
        let minted = keys_from(published, mint_range, Ordering::Ascending, None);
        for limit in [1, 2] {
            for start in 1..minted.len() {
                let context = format!(
                    "limit {limit}, page from {:?} under {mint_range:?}, reversed under \
                     {reverse_range:?}",
                    minted[start]
                );
                let forward = walk(
                    list,
                    mint_range,
                    Ordering::Ascending,
                    limit,
                    Some(minted[start - 1]),
                );
                let page = &forward[0];
                let page_end = minted.len().min(start + limit as usize);
                assert_eq!(page[..], minted[start..page_end], "{context}");

                let backward = walk(
                    list,
                    reverse_range,
                    Ordering::Descending,
                    limit,
                    Some(page[0]),
                )
                .concat();
                assert_eq!(
                    backward,
                    keys_from(
                        published,
                        reverse_range,
                        Ordering::Descending,
                        Some(page[0])
                    ),
                    "{context}"
                );
                if mint_range == reverse_range {
                    let before_page: Vec<K> = minted[..start].iter().rev().copied().collect();
                    assert_eq!(backward, before_page, "{context}");
                    let mut union = [backward, forward.concat()].concat();
                    union.sort();
                    assert_eq!(union, minted, "{context}");
                }
            }
        }
    }

    /// For each page of 1 and of 2 keys that a descending listing of `published` over the
    /// checkpoint range `mint_range` lists from a `before` cursor (so the page starts
    /// mid-listing), an ascending walk over `reverse_range` from `after` = the page's first
    /// (highest) key lists exactly the keys of `reverse_range` above that key, oldest first; when
    /// the ranges match, those are the keys above the page, and together with the descending
    /// walk on from the page they list every key of the range exactly once. A cursor names a
    /// position, not a direction: ascending from `after` = the page's last (lowest) key lists the
    /// keys above that key, so with matching ranges it lists the rest of the page again.
    fn check_reversing_descending_pages<K: Ord + Copy + std::fmt::Debug>(
        list: &ListPage<K>,
        published: &[(K, u64)],
        mint_range: [Option<u64>; 2],
        reverse_range: [Option<u64>; 2],
    ) {
        let minted = keys_from(published, mint_range, Ordering::Ascending, None);
        for limit in [1, 2] {
            // `first` is the position of the page's first key in the ascending listing; the
            // page starts below the `before` cursor at the key after it.
            for first in 0..minted.len().saturating_sub(1) {
                let context = format!(
                    "limit {limit}, page from {:?} under {mint_range:?}, reversed under \
                     {reverse_range:?}",
                    minted[first]
                );
                let downward = walk(
                    list,
                    mint_range,
                    Ordering::Descending,
                    limit,
                    Some(minted[first + 1]),
                );
                let page = &downward[0];
                let expected_page: Vec<K> = minted[..=first]
                    .iter()
                    .rev()
                    .take(limit as usize)
                    .copied()
                    .collect();
                assert_eq!(page, &expected_page, "{context}");

                let upward = walk(
                    list,
                    reverse_range,
                    Ordering::Ascending,
                    limit,
                    Some(page[0]),
                )
                .concat();
                assert_eq!(
                    upward,
                    keys_from(published, reverse_range, Ordering::Ascending, Some(page[0])),
                    "{context}"
                );
                let last = *page.last().unwrap();
                let from_last =
                    walk(list, reverse_range, Ordering::Ascending, limit, Some(last)).concat();
                assert_eq!(
                    from_last,
                    keys_from(published, reverse_range, Ordering::Ascending, Some(last)),
                    "{context}"
                );
                if mint_range == reverse_range {
                    assert_eq!(upward[..], minted[first + 1..], "{context}");
                    let rest_of_page: Vec<K> =
                        page[..page.len() - 1].iter().rev().copied().collect();
                    assert_eq!(
                        from_last,
                        [rest_of_page, upward.clone()].concat(),
                        "{context}"
                    );
                    let mut union = [downward.concat(), upward].concat();
                    union.sort();
                    assert_eq!(union, minted, "{context}");
                }
            }
        }
    }

    /// Versions 1 to 6 of [`system`] as `(version, checkpoint)`: version 1 published at
    /// checkpoint 2, versions 2 to 4 at checkpoint 4, version 5 at 7 and version 6 at 9.
    const REVERSAL_LINEAGE: [(u64, u64); 6] = [(1, 2), (2, 4), (3, 4), (4, 4), (5, 7), (6, 9)];

    /// Checkpoint ranges over [`REVERSAL_LINEAGE`]: the whole lineage, reaching past the tip and
    /// ending at the checkpoint after it; from checkpoint 4 (versions 2 to 6); before checkpoint
    /// 8 (versions 1 to 5); `[3, 8)` (versions 2 to 5); and checkpoint 4 alone (versions 2 to 4).
    const LINEAGE_REVERSAL_RANGES: [[Option<u64>; 2]; 6] = [
        [None, None],
        [None, Some(HIGHEST_INDEXED + 1)],
        [Some(4), None],
        [None, Some(8)],
        [Some(3), Some(8)],
        [Some(4), Some(5)],
    ];

    /// Reversing an ascending lineage page from mid-listing to descending, with `before` = the
    /// page's first version, lists exactly the versions before the page, newest first, and the
    /// two walks list every version once: on an unpruned store, under each checkpoint range,
    /// with the pivot inside checkpoint 4's three versions or across a checkpoint, and on a
    /// store that pruned versions 1 and 2.
    #[test]
    fn reversing_an_ascending_lineage_page_lists_the_versions_before_it_once() {
        let store = Store::with_system_versions(&REVERSAL_LINEAGE);
        let list = |range, ordering, limit, after, before| {
            store.list_to_end(cursor_lineage(range, ordering, limit, after, before))
        };
        for range in LINEAGE_REVERSAL_RANGES {
            check_reversing_ascending_pages(&list, &REVERSAL_LINEAGE, range, range);
        }

        // The pivot inside checkpoint 4, and across checkpoints 2 and 4.
        for (after, page, before_page) in [(2, vec![3, 4], vec![2, 1]), (1, vec![2, 3], vec![1])] {
            let forward = cursor_lineage([None, None], Ordering::Ascending, 2, Some(after), None);
            assert_eq!(
                store.list_to_end(forward),
                (page.clone(), Some(QueryEndReason::ItemLimit))
            );
            let backward =
                cursor_lineage([None, None], Ordering::Descending, 2, None, Some(page[0]));
            assert_eq!(store.list(backward).unwrap(), before_page, "after {after}");
        }

        let pruned = pruned_five_version_store();
        let list_pruned = |range, ordering, limit, after, before| {
            pruned.list_to_end(cursor_lineage(range, ordering, limit, after, before))
        };
        let stored = [(3, 6), (4, 8), (5, 10)];
        check_reversing_ascending_pages(&list_pruned, &stored, [None, None], [None, None]);
    }

    /// Reversing a descending lineage page from mid-listing to ascending, with `after` = the
    /// page's first (highest) version, lists exactly the versions above the page, oldest first,
    /// and the two walks list every version once; `after` = the page's last version lists the
    /// rest of the page again, since a cursor carries no direction. On an unpruned store, under
    /// each checkpoint range, and on a store that pruned versions 1 and 2.
    #[test]
    fn reversing_a_descending_lineage_page_lists_the_versions_above_it_once() {
        let store = Store::with_system_versions(&REVERSAL_LINEAGE);
        let list = |range, ordering, limit, after, before| {
            store.list_to_end(cursor_lineage(range, ordering, limit, after, before))
        };
        for range in LINEAGE_REVERSAL_RANGES {
            check_reversing_descending_pages(&list, &REVERSAL_LINEAGE, range, range);
        }

        // A descending page inside checkpoint 4, then ascending from either end of it.
        let downward = cursor_lineage([None, None], Ordering::Descending, 2, None, Some(5));
        assert_eq!(
            store.list_to_end(downward),
            (vec![4, 3], Some(QueryEndReason::ItemLimit))
        );
        for (after, expected) in [(4, vec![5, 6]), (3, vec![4, 5, 6])] {
            let upward = cursor_lineage([None, None], Ordering::Ascending, 10, Some(after), None);
            assert_eq!(
                store.list_to_end(upward),
                (expected, Some(QueryEndReason::LedgerTip)),
                "after {after}"
            );
        }

        let pruned = pruned_five_version_store();
        let list_pruned = |range, ordering, limit, after, before| {
            pruned.list_to_end(cursor_lineage(range, ordering, limit, after, before))
        };
        let stored = [(3, 6), (4, 8), (5, 10)];
        check_reversing_descending_pages(&list_pruned, &stored, [None, None], [None, None]);
    }

    /// A lineage page's cursors, reversed under another checkpoint range than the one they were
    /// listed under, list the versions on the far side of the cursor that the new range holds,
    /// whether the cursor is inside the new range or outside it: for every pair of distinct
    /// ranges, in both directions.
    #[test]
    fn reversing_a_lineage_page_under_another_checkpoint_range_lists_the_intersection() {
        let store = Store::with_system_versions(&REVERSAL_LINEAGE);
        let list = |range, ordering, limit, after, before| {
            store.list_to_end(cursor_lineage(range, ordering, limit, after, before))
        };
        for mint_range in LINEAGE_REVERSAL_RANGES {
            for reverse_range in LINEAGE_REVERSAL_RANGES {
                if mint_range != reverse_range {
                    check_reversing_ascending_pages(
                        &list,
                        &REVERSAL_LINEAGE,
                        mint_range,
                        reverse_range,
                    );
                    check_reversing_descending_pages(
                        &list,
                        &REVERSAL_LINEAGE,
                        mint_range,
                        reverse_range,
                    );
                }
            }
        }

        // Version 5 (checkpoint 7), listed with no range, outside and inside the new range.
        for (range, ordering, after, before, expected, end) in [
            (
                [None, Some(5)],
                Ordering::Descending,
                None,
                Some(5),
                vec![4, 3],
                QueryEndReason::ItemLimit,
            ),
            (
                [None, Some(5)],
                Ordering::Ascending,
                Some(5),
                None,
                vec![],
                QueryEndReason::CheckpointBound,
            ),
            (
                [Some(4), Some(8)],
                Ordering::Descending,
                None,
                Some(3),
                vec![2],
                QueryEndReason::CheckpointBound,
            ),
            (
                [Some(7), None],
                Ordering::Descending,
                None,
                Some(5),
                vec![],
                QueryEndReason::CheckpointBound,
            ),
            (
                [Some(7), None],
                Ordering::Ascending,
                Some(3),
                None,
                vec![5, 6],
                QueryEndReason::ItemLimit,
            ),
        ] {
            assert_eq!(
                store.list_to_end(cursor_lineage(range, ordering, 2, after, before)),
                (expected, Some(end)),
                "{range:?} {ordering:?} after {after:?} before {before:?}"
            );
        }
    }

    /// An rpc-store reader that runs `on_probe` the first time the handler asks whether it
    /// stores `probe`, before answering.
    struct PruneOnProbe {
        inner: RpcStoreReader,
        probe: (ObjectID, VersionNumber),
        on_probe: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    }

    /// A listing with a checkpoint range above the floor when the request arrives, that pruning
    /// overtakes before the handler finds where the listing starts, is refused against the risen
    /// floor rather than answered with the versions left in the range, if any.
    #[test]
    fn checkpoint_range_listing_overtaken_by_pruning_is_refused() {
        let mut start_checkpoint = lineage(None, Some(Ordering::Ascending), None, None);
        start_checkpoint.start_checkpoint = Some(3);
        // Without `end_checkpoint`, the listing starts at version 1; from checkpoint 3, at 2.
        for (request, probed) in [
            (lineage(Some(7), None, None, None), 1),
            (start_checkpoint, 2),
        ] {
            let store = Store::with_system_package();
            store.prune(3, &[]);
            let (db, schema) = (store.db.clone(), store.schema.clone());
            let reader = PruneOnProbe {
                inner: store.reader(),
                probe: (system(), SequenceNumber::from_u64(probed)),
                on_probe: Mutex::new(Some(Box::new(move || prune(&db, &schema, 9, &[1, 2])))),
            };
            let error = Store::respond_with(reader, request.clone()).unwrap_err();
            assert_below_serving_floor_error(error, 9);
        }
    }

    impl ObjectStore for PruneOnProbe {
        fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
            self.inner.get_object(object_id)
        }

        fn get_object_by_key(
            &self,
            object_id: &ObjectID,
            version: VersionNumber,
        ) -> Option<Object> {
            if (*object_id, version) == self.probe
                && let Some(on_probe) = self.on_probe.lock().unwrap().take()
            {
                on_probe();
            }
            self.inner.get_object_by_key(object_id, version)
        }
    }

    impl sui_types::storage::ReadStore for PruneOnProbe {
        fn get_committee(
            &self,
            epoch: sui_types::committee::EpochId,
        ) -> Option<Arc<sui_types::committee::Committee>> {
            self.inner.get_committee(epoch)
        }

        fn get_latest_checkpoint(
            &self,
        ) -> sui_types::storage::error::Result<sui_types::messages_checkpoint::VerifiedCheckpoint>
        {
            self.inner.get_latest_checkpoint()
        }

        fn get_highest_verified_checkpoint(
            &self,
        ) -> sui_types::storage::error::Result<sui_types::messages_checkpoint::VerifiedCheckpoint>
        {
            self.inner.get_highest_verified_checkpoint()
        }

        fn get_highest_synced_checkpoint(
            &self,
        ) -> sui_types::storage::error::Result<sui_types::messages_checkpoint::VerifiedCheckpoint>
        {
            self.inner.get_highest_synced_checkpoint()
        }

        fn get_lowest_available_checkpoint(&self) -> sui_types::storage::error::Result<u64> {
            self.inner.get_lowest_available_checkpoint()
        }

        fn get_checkpoint_by_digest(
            &self,
            digest: &sui_types::digests::CheckpointDigest,
        ) -> Option<sui_types::messages_checkpoint::VerifiedCheckpoint> {
            self.inner.get_checkpoint_by_digest(digest)
        }

        fn get_checkpoint_by_sequence_number(
            &self,
            sequence_number: u64,
        ) -> Option<sui_types::messages_checkpoint::VerifiedCheckpoint> {
            self.inner
                .get_checkpoint_by_sequence_number(sequence_number)
        }

        fn get_checkpoint_contents_by_digest(
            &self,
            digest: &sui_types::digests::CheckpointContentsDigest,
        ) -> Option<sui_types::messages_checkpoint::CheckpointContents> {
            self.inner.get_checkpoint_contents_by_digest(digest)
        }

        fn get_checkpoint_contents_by_sequence_number(
            &self,
            sequence_number: u64,
        ) -> Option<sui_types::messages_checkpoint::CheckpointContents> {
            self.inner
                .get_checkpoint_contents_by_sequence_number(sequence_number)
        }

        fn get_transaction(
            &self,
            tx_digest: &TransactionDigest,
        ) -> Option<Arc<sui_types::transaction::VerifiedTransaction>> {
            self.inner.get_transaction(tx_digest)
        }

        fn get_transaction_effects(
            &self,
            tx_digest: &TransactionDigest,
        ) -> Option<sui_types::effects::TransactionEffects> {
            self.inner.get_transaction_effects(tx_digest)
        }

        fn get_events(
            &self,
            event_digest: &TransactionDigest,
        ) -> Option<sui_types::effects::TransactionEvents> {
            self.inner.get_events(event_digest)
        }

        fn get_unchanged_loaded_runtime_objects(
            &self,
            digest: &TransactionDigest,
        ) -> Option<Vec<sui_types::storage::ObjectKey>> {
            self.inner.get_unchanged_loaded_runtime_objects(digest)
        }

        fn get_transaction_checkpoint(&self, digest: &TransactionDigest) -> Option<u64> {
            self.inner.get_transaction_checkpoint(digest)
        }

        fn get_full_checkpoint_contents(
            &self,
            sequence_number: Option<u64>,
            digest: &sui_types::digests::CheckpointContentsDigest,
        ) -> Option<sui_types::messages_checkpoint::VersionedFullCheckpointContents> {
            self.inner
                .get_full_checkpoint_contents(sequence_number, digest)
        }
    }

    impl sui_types::storage::BackingPackageStore for PruneOnProbe {
        fn get_package_object(
            &self,
            package_id: &ObjectID,
        ) -> sui_types::error::SuiResult<Option<sui_types::storage::PackageObject>> {
            self.inner.get_package_object(package_id)
        }
    }

    impl sui_types::storage::RuntimeObjectResolver for PruneOnProbe {
        fn read_child_object(
            &self,
            parent: &ObjectID,
            child: &ObjectID,
            child_version_upper_bound: SequenceNumber,
        ) -> sui_types::error::SuiResult<Option<Object>> {
            self.inner
                .read_child_object(parent, child, child_version_upper_bound)
        }

        fn get_object_received_at_version(
            &self,
            owner: &ObjectID,
            receiving_object_id: &ObjectID,
            receive_object_at_version: SequenceNumber,
            epoch_id: sui_types::committee::EpochId,
        ) -> sui_types::error::SuiResult<Option<Object>> {
            self.inner.get_object_received_at_version(
                owner,
                receiving_object_id,
                receive_object_at_version,
                epoch_id,
            )
        }
    }

    impl RpcStateReader for PruneOnProbe {
        fn get_lowest_available_checkpoint_objects(
            &self,
        ) -> sui_types::storage::error::Result<u64> {
            self.inner.get_lowest_available_checkpoint_objects()
        }

        fn get_chain_identifier(
            &self,
        ) -> sui_types::storage::error::Result<sui_types::digests::ChainIdentifier> {
            self.inner.get_chain_identifier()
        }

        fn indexes(&self) -> Option<&dyn RpcIndexes> {
            self.inner.indexes()
        }
    }

    /// A store whose `packages_by_checkpoint` index holds version 1 of the package with original
    /// ID `id`, published at `checkpoint`, for each `(checkpoint, id)` in `published`, on a node
    /// whose lowest available checkpoint is `lowest_available`.
    fn store_with_publishes(published: &[(u64, u8)], lowest_available: u64) -> Store {
        let store = Store::new();
        let mut batch = store.db.batch();
        for &(checkpoint, id) in published {
            let id = ObjectID::from_single_byte(id);
            let (key, value) =
                sui_rpc_store::schema::packages_by_checkpoint::store(checkpoint, id, 1, id);
            batch
                .put(&store.schema.packages_by_checkpoint, &key, &value)
                .unwrap();
        }
        batch.commit().unwrap();
        store.prune(lowest_available, &[]);
        store
    }

    /// A request listing every package's versions in `ordering`, after and before the
    /// publishes at `(checkpoint, original ID)`.
    fn all_packages(
        ordering: Ordering,
        after: Option<(u64, u8)>,
        before: Option<(u64, u8)>,
    ) -> ListPackageVersionsRequest {
        let cursor = |(checkpoint, id): (u64, u8)| {
            PackageVersionCursor::AllPackages {
                checkpoint,
                original_package_id: ObjectID::from_single_byte(id),
                version: 1,
                minted_at: HIGHEST_INDEXED,
            }
            .encode()
        };
        let mut options = QueryOptions::default();
        options.ordering = Some(ordering as i32);
        options.after = after.map(cursor);
        options.before = before.map(cursor);
        let mut request = ListPackageVersionsRequest::default();
        request.options = Some(options);
        request
    }

    /// The publish checkpoints `request` lists.
    fn publish_checkpoints(store: &Store, request: ListPackageVersionsRequest) -> Result<Vec<u64>> {
        let service = RpcService::new(Arc::new(store.reader()));
        Ok(list_package_versions(&service, request)?
            .versions
            .iter()
            .map(|version| version.checkpoint.unwrap())
            .collect())
    }

    /// On a node whose floor is checkpoint 10, the listing of every package covers the
    /// publishes from checkpoint 10 on, in either order: a descending listing ends there with a
    /// short last page. An `after` cursor at the floor bounds the listing; a listing after one
    /// below it is refused in either order, rather than skip the publishes in between.
    #[test]
    fn listing_of_every_package_covers_the_publishes_from_the_floor() {
        let store = store_with_publishes(&[(5, 0xA), (10, 0xB), (12, 0xC)], 10);
        for (request, expected) in [
            (all_packages(Ordering::Ascending, None, None), vec![10, 12]),
            (all_packages(Ordering::Descending, None, None), vec![12, 10]),
            (
                all_packages(Ordering::Ascending, Some((10, 0xB)), None),
                vec![12],
            ),
            (
                all_packages(Ordering::Descending, Some((10, 0xB)), None),
                vec![12],
            ),
        ] {
            assert_eq!(
                publish_checkpoints(&store, request.clone()).unwrap(),
                expected,
                "{request:?}"
            );
        }
        for ordering in [Ordering::Ascending, Ordering::Descending] {
            assert_below_serving_floor_error(
                publish_checkpoints(&store, all_packages(ordering, Some((5, 0xA)), None))
                    .unwrap_err(),
                10,
            );
        }
    }

    /// On a node whose floor is checkpoint 10, a `before` cursor below the floor leaves nothing to
    /// list in either order, although publishes below the floor remain in the index. A `before`
    /// cursor above the floor bounds the listing.
    #[test]
    fn before_cursor_below_the_floor_lists_nothing() {
        let store = store_with_publishes(&[(5, 0xA), (10, 0xB), (12, 0xC)], 10);
        for ordering in [Ordering::Ascending, Ordering::Descending] {
            for (before, expected) in [
                ((5, 0xA), vec![]),
                ((9, 0xF), vec![]),
                ((12, 0xC), vec![10]),
            ] {
                assert_eq!(
                    publish_checkpoints(&store, all_packages(ordering, None, Some(before)))
                        .unwrap(),
                    expected,
                    "{ordering:?} before {before:?}"
                );
            }
        }
    }

    /// Publishes as `(checkpoint, original ID)`, in listing order: one at checkpoint 3, three at
    /// 5, one at 8, two at 12 and one at 15.
    const REVERSAL_PUBLISHES: [(u64, u8); 8] = [
        (3, 0xA),
        (5, 0x1),
        (5, 0x2),
        (5, 0x3),
        (8, 0xB),
        (12, 0x4),
        (12, 0x5),
        (15, 0xC),
    ];

    /// Checkpoint ranges over [`REVERSAL_PUBLISHES`]: every publish, reaching past the tip and
    /// ending at the checkpoint after it; from checkpoint 5; before checkpoint 13; `[5, 13)`;
    /// checkpoint 5 alone; and `[4, 12)`.
    const ALL_PACKAGES_REVERSAL_RANGES: [[Option<u64>; 2]; 7] = [
        [None, None],
        [None, Some(HIGHEST_INDEXED + 1)],
        [Some(5), None],
        [None, Some(13)],
        [Some(5), Some(13)],
        [Some(5), Some(6)],
        [Some(4), Some(12)],
    ];

    /// [`REVERSAL_PUBLISHES`], each with the checkpoint it was published in.
    fn reversal_publishes() -> Vec<((u64, u8), u64)> {
        REVERSAL_PUBLISHES
            .iter()
            .map(|&publish| (publish, publish.0))
            .collect()
    }

    /// A request listing every package's versions over the checkpoint range
    /// `[start_checkpoint, end_checkpoint)`, `limit` at a time in `ordering`, after and before the
    /// publishes at `(checkpoint, original ID)`.
    fn ranged_all_packages(
        [start_checkpoint, end_checkpoint]: [Option<u64>; 2],
        ordering: Ordering,
        limit: u32,
        after: Option<(u64, u8)>,
        before: Option<(u64, u8)>,
    ) -> ListPackageVersionsRequest {
        let mut request = all_packages(ordering, after, before);
        request.start_checkpoint = start_checkpoint;
        request.end_checkpoint = end_checkpoint;
        request.options.as_mut().unwrap().limit = Some(limit);
        request
    }

    /// The publishes `(checkpoint, original ID)` that `request` lists, read from the cursors it
    /// returns, and how the listing ends.
    fn list_publishes(
        store: &Store,
        request: ListPackageVersionsRequest,
    ) -> (Vec<(u64, u8)>, Option<QueryEndReason>) {
        let response = store.respond(request).unwrap();
        let publishes = response
            .versions
            .iter()
            .map(|listed| {
                let cursor = decode_cursor("cursor", listed.cursor.as_deref().unwrap()).unwrap();
                // Every cursor records the highest indexed checkpoint as its `minted_at`.
                let PackageVersionCursor::AllPackages {
                    checkpoint,
                    original_package_id,
                    version: 1,
                    minted_at: HIGHEST_INDEXED,
                } = cursor
                else {
                    panic!("unexpected cursor {cursor:?}");
                };
                let id = original_package_id.into_bytes()[ObjectID::LENGTH - 1];
                assert_eq!(ObjectID::from_single_byte(id), original_package_id);
                (checkpoint, id)
            })
            .collect();
        (publishes, response.end.map(|end| end.reason()))
    }

    /// Reversing an ascending page of every package from mid-listing to descending, with
    /// `before` = the page's first publish, lists exactly the publishes before the page, newest
    /// first, and the two walks list every publish once: under each checkpoint range, with the
    /// pivot inside checkpoint 5's three publishes or across a checkpoint.
    #[test]
    fn reversing_an_ascending_page_of_every_package_lists_the_publishes_before_it_once() {
        let store = store_with_publishes(&REVERSAL_PUBLISHES, 0);
        let list = |range, ordering, limit, after, before| {
            list_publishes(
                &store,
                ranged_all_packages(range, ordering, limit, after, before),
            )
        };
        let published = reversal_publishes();
        for range in ALL_PACKAGES_REVERSAL_RANGES {
            check_reversing_ascending_pages(&list, &published, range, range);
        }

        // The pivot is the first publish of checkpoint 5 (crossing back into checkpoint 3), the
        // middle publish of checkpoint 5, and, within `[5, 13)`, the only publish of checkpoint
        // 8 (crossing back into checkpoint 5).
        for (range, limit, after, page, before_page, before_end) in [
            (
                [None, None],
                2,
                (3, 0xA),
                vec![(5, 0x1), (5, 0x2)],
                vec![(3, 0xA)],
                QueryEndReason::CheckpointBound,
            ),
            (
                [None, None],
                1,
                (5, 0x1),
                vec![(5, 0x2)],
                vec![(5, 0x1)],
                QueryEndReason::ItemLimit,
            ),
            (
                [Some(5), Some(13)],
                2,
                (5, 0x3),
                vec![(8, 0xB), (12, 0x4)],
                vec![(5, 0x3), (5, 0x2)],
                QueryEndReason::ItemLimit,
            ),
        ] {
            assert_eq!(
                list(range, Ordering::Ascending, limit, Some(after), None),
                (page.clone(), Some(QueryEndReason::ItemLimit)),
                "{range:?} after {after:?}"
            );
            assert_eq!(
                list(range, Ordering::Descending, limit, None, Some(page[0])),
                (before_page, Some(before_end)),
                "{range:?} before {:?}",
                page[0]
            );
        }
    }

    /// Reversing a descending page of every package from mid-listing to ascending, with `after` =
    /// the page's first (highest) publish, lists exactly the publishes above the page, oldest
    /// first, and the two walks list every publish once; `after` = the page's last publish lists
    /// the rest of the page again, since a cursor carries no direction. Under each checkpoint
    /// range, with the pivot inside checkpoint 5's three publishes or across a checkpoint.
    #[test]
    fn reversing_a_descending_page_of_every_package_lists_the_publishes_above_it_once() {
        let store = store_with_publishes(&REVERSAL_PUBLISHES, 0);
        let list = |range, ordering, limit, after, before| {
            list_publishes(
                &store,
                ranged_all_packages(range, ordering, limit, after, before),
            )
        };
        let published = reversal_publishes();
        for range in ALL_PACKAGES_REVERSAL_RANGES {
            check_reversing_descending_pages(&list, &published, range, range);
        }

        // A descending page of checkpoint 5's last two publishes, then ascending from its first
        // publish (crossing into checkpoint 8) and from its last (inside checkpoint 5).
        assert_eq!(
            list([None, None], Ordering::Descending, 2, None, Some((8, 0xB))),
            (vec![(5, 0x3), (5, 0x2)], Some(QueryEndReason::ItemLimit))
        );
        for (after, expected) in [
            ((5, 0x3), vec![(8, 0xB), (12, 0x4), (12, 0x5), (15, 0xC)]),
            (
                (5, 0x2),
                vec![(5, 0x3), (8, 0xB), (12, 0x4), (12, 0x5), (15, 0xC)],
            ),
        ] {
            assert_eq!(
                list([None, None], Ordering::Ascending, 10, Some(after), None),
                (expected, Some(QueryEndReason::LedgerTip)),
                "after {after:?}"
            );
        }
    }

    /// A page of every package's cursors, reversed under another checkpoint range than the one
    /// they were listed under, list the publishes on the far side of the cursor that the new
    /// range holds, whether the cursor is inside the new range or outside it: for every pair of
    /// distinct ranges, in both directions.
    #[test]
    fn reversing_a_page_of_every_package_under_another_checkpoint_range_lists_the_intersection() {
        let store = store_with_publishes(&REVERSAL_PUBLISHES, 0);
        let list = |range, ordering, limit, after, before| {
            list_publishes(
                &store,
                ranged_all_packages(range, ordering, limit, after, before),
            )
        };
        let published = reversal_publishes();
        for mint_range in ALL_PACKAGES_REVERSAL_RANGES {
            for reverse_range in ALL_PACKAGES_REVERSAL_RANGES {
                if mint_range != reverse_range {
                    check_reversing_ascending_pages(&list, &published, mint_range, reverse_range);
                    check_reversing_descending_pages(&list, &published, mint_range, reverse_range);
                }
            }
        }

        // Cursors listed with no range, reversed under one that holds them or not.
        for (range, ordering, after, before, expected) in [
            (
                [None, Some(10)],
                Ordering::Descending,
                None,
                Some((12, 0x4)),
                vec![(8, 0xB), (5, 0x3), (5, 0x2), (5, 0x1), (3, 0xA)],
            ),
            (
                [Some(5), Some(10)],
                Ordering::Ascending,
                Some((12, 0x4)),
                None,
                vec![],
            ),
            (
                [Some(5), Some(6)],
                Ordering::Ascending,
                Some((3, 0xA)),
                None,
                vec![(5, 0x1), (5, 0x2), (5, 0x3)],
            ),
            (
                [Some(5), Some(6)],
                Ordering::Descending,
                None,
                Some((5, 0x3)),
                vec![(5, 0x2), (5, 0x1)],
            ),
            (
                [Some(13), None],
                Ordering::Descending,
                None,
                Some((12, 0x4)),
                vec![],
            ),
        ] {
            assert_eq!(
                list(range, ordering, 10, after, before),
                (expected, Some(QueryEndReason::CheckpointBound)),
                "{range:?} {ordering:?} after {after:?} before {before:?}"
            );
        }
    }
}
