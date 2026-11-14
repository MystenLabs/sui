// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{load_package_by_id, parse_package_id};
use crate::ledger_history::query_options::below_oldest_retained_version;
use crate::reader::{AtCheckpointLowerBound, indexes_disabled};
use crate::{ErrorReason, Result, RpcError, RpcService};
use bytes::Bytes;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::{
    ListPackageVersionsRequest, ListPackageVersionsResponse, PackageVersion,
};
use sui_types::base_types::ObjectID;
use sui_types::object::OBJECT_START_VERSION;
use sui_types::storage::IndexedPackageVersion;
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

/// List the versions of a package's upgrade lineage published at or before `at_checkpoint`, or,
/// without it, at or before the highest checkpoint every rpc index has committed.
///
/// Versions with no recorded publish checkpoint are listed with `checkpoint` unset; see the
/// `at_checkpoint` bounds below for why a bound may include them.
///
/// With `at_checkpoint`, a listing that starts below the lowest version of the lineage this node
/// stores, such as a system package's versions replaced before the pruning floor, is refused as
/// `OutOfRange` (`BELOW_EARLIEST_AVAILABLE`) with that version as `lowestAvailableVersion`.
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
        .ok_or_else(indexes_disabled)?;

    // A listing bounded by `at_checkpoint` that starts below the lowest version of the lineage this
    // node stores is refused, rather than answered with a history that is cut short or lists
    // versions this node cannot load: `package_versions` rows outlive pruning (see
    // `range_start_against_stored_versions`). A listing without `at_checkpoint` lists every row.
    if request.at_checkpoint.is_some() {
        let range_start = page_token
            .as_ref()
            .map_or(OBJECT_START_VERSION.value(), |token| token.version)
            .max(OBJECT_START_VERSION.value());
        let rows = indexes
            .package_versions_iter(original_package_id, Some(range_start))
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
            .map(|row| row.map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string())));
        let is_stored = |row: &IndexedPackageVersion| {
            service
                .reader
                .inner()
                .get_object_by_key(&row.storage_id, row.version.into())
                .is_some()
        };
        if let RangeStart::BelowLowestStored { lowest_stored } =
            range_start_against_stored_versions(range_start, rows, is_stored)?
        {
            // As `GetPackage` does for a version this node no longer stores.
            service.reader.require_object_version_history()?;
            return Err(refuse_range_below_lowest_stored(
                service.reader.at_checkpoint_lower_bound()?,
                original_package_id,
                range_start,
                lowest_stored,
            ));
        }
    }

    // Without `at_checkpoint`, the listing stops at the highest indexed checkpoint, so it never
    // includes a version published above every checkpoint this node serves as an `at_checkpoint`.
    // It lists every row it reaches, including system-package versions whose packages are pruned.
    let cp_bound = request
        .at_checkpoint
        .unwrap_or(upper_bound.highest_indexed());
    let mut versions = vec![];
    let iter = indexes
        .package_versions_iter(original_package_id, page_token.map(|t| t.version))
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    for result in iter.take(page_size + 1) {
        let entry = result.map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

        // Recorded publish checkpoints are non-decreasing in version, so the first version
        // published after the bound ends the scan. Versions with no recorded checkpoint come
        // first and were published before the history the index holds begins, so they belong
        // under any bound this listing serves (see the floor checked above).
        if let Some(published) = entry.checkpoint
            && published > cp_bound
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
    use std::cell::Cell;

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
