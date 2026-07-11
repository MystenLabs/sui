// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared `Watermark` construction for the v2alpha list APIs.
//!
//! Both ledger-history backends — the fullnode (`sui-rpc-api`) and bigtable
//! (`sui-kv-rpc`) — and all three list handlers (`list_transactions`,
//! `list_events`, `list_checkpoints`) emit the same wire `Watermark`: a resume
//! cursor plus a completion boundary (`checkpoint`, the inclusive boundary
//! checkpoint the scan has fully covered in the request's ordering direction).
//! The cursor encoding and the boundary bookkeeping are identical; what differs
//! per API is how a scan position resolves into a completion-boundary candidate:
//!
//! - `list_transactions` / `list_events` scan within a checkpoint, so an
//!   item at checkpoint `C` does NOT prove `C` complete (more matches may sit
//!   at higher/lower transaction or event positions). Their covered bound is
//!   advanced before `C` — see [`advance_covered_bound_before_checkpoint`].
//! - `list_checkpoints` dedupes checkpoint numbers, so "checkpoint `C`
//!   emitted" means "checkpoint `C` complete." Its item path directly records
//!   `C`; independently resolved frontier candidates are folded with
//!   [`merge_covered_checkpoint_bound`].
//!
//! This module owns the shared pieces; each handler keeps only its
//! API-specific frontier-to-candidate adapter.

use sui_inverted_index::ScanDirection;
use sui_rpc_cursor::{CursorToken, Position};

use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;

use crate::ledger_history::query_options::QueryOptions;

/// Populate the completion-boundary `checkpoint` field of a `Watermark` from
/// the per-scan boundary value. The value already carries the direction-correct
/// meaning (inclusive upper bound ascending, inclusive lower bound descending);
/// the single wire field records it regardless of ordering.
fn set_checkpoint_bound(wm: &mut Watermark, boundary: Option<u64>) {
    wm.checkpoint = boundary;
}

/// Merge a fully covered checkpoint candidate into the accumulated inclusive
/// bound. The bound advances by max in ascending scans and min in descending
/// scans.
pub fn merge_covered_checkpoint_bound(
    covered_checkpoint_bound: Option<u64>,
    candidate_bound: u64,
    options: &QueryOptions,
) -> Option<u64> {
    Some(match covered_checkpoint_bound {
        None => candidate_bound,
        Some(bound) if options.is_ascending() => bound.max(candidate_bound),
        Some(bound) => bound.min(candidate_bound),
    })
}

/// Advance the inclusive covered bound using a checkpoint that is not itself
/// proven complete. Transactions, events, and scan frontiers can leave more
/// matches within checkpoint `C`, so the candidate excludes `C`: `C - 1`
/// ascending and `C + 1` descending. The adjusted candidate is then merged by
/// max ascending or min descending.
///
/// When that adjustment would overflow (`C == 0` ascending or `u64::MAX`
/// descending), the previously covered bound is preserved.
pub fn advance_covered_bound_before_checkpoint(
    covered_checkpoint_bound: Option<u64>,
    incomplete_checkpoint: u64,
    options: &QueryOptions,
) -> Option<u64> {
    let candidate_bound = if options.is_ascending() {
        incomplete_checkpoint.checked_sub(1)
    } else {
        incomplete_checkpoint.checked_add(1)
    };
    match candidate_bound {
        Some(candidate_bound) => {
            merge_covered_checkpoint_bound(covered_checkpoint_bound, candidate_bound, options)
        }
        None => covered_checkpoint_bound,
    }
}

/// Build the embedded `Watermark` for an item: the cursor encodes this
/// item's position (so the next request's `after`/`before` resumes past it)
/// plus the current direction-matching checkpoint boundary. `cp` /
/// `position` are the item's cursor coordinates (`list_checkpoints` passes
/// its cp_seq for both).
pub fn item_watermark(position: Position, boundary: Option<u64>) -> Watermark {
    let mut wm = Watermark::default();
    wm.cursor = Some(CursorToken::item(position).encode());
    set_checkpoint_bound(&mut wm, boundary);
    wm
}

/// Build a standalone scan-frontier `Watermark`. `cursor_cp` / `position`
/// are the boundary cursor coordinates the caller has already resolved for
/// its scan domain (see [`boundary_cursor_cp`] for the per-checkpoint
/// scanners' direction adjustment); `boundary` is the accumulated
/// completion boundary.
pub fn boundary_watermark(position: Position, boundary: Option<u64>) -> Watermark {
    let mut wm = Watermark::default();
    wm.cursor = Some(CursorToken::boundary(position).encode());
    set_checkpoint_bound(&mut wm, boundary);
    wm
}

/// Resolve the boundary-cursor checkpoint coordinate for a `list_transactions`
/// / `list_events` scan frontier. The cursor encoding is asymmetric:
/// ascending `Boundary` cursors advance the cp-range start, so the frontier
/// cp is used directly; descending `Boundary` cursors treat the cp
/// coordinate as an EXCLUSIVE upper bound, so `cp + 1` is needed to keep
/// `cp` itself included on resume.
pub fn boundary_cursor_cp(cp: u64, direction: ScanDirection) -> u64 {
    if direction.is_ascending() {
        cp
    } else {
        cp.saturating_add(1)
    }
}

/// Boundary watermark emitted once a scan has drained its entire resolved
/// range under natural completion. Unlike per-item watermarks it can claim
/// the range's final checkpoint complete — `end_checkpoint - 1` ascending
/// (the exclusive cp upper) or `end_checkpoint` descending (the inclusive cp
/// lower) — because no further items exist in it within the requested range.
/// The `(end_checkpoint, end_position)` cursor resumes exactly past the
/// scanned range.
pub fn terminal_boundary_watermark(options: &QueryOptions, end_position: Position) -> Watermark {
    let end_checkpoint = end_position.checkpoint();
    let boundary = if options.is_ascending() {
        end_checkpoint.checked_sub(1)
    } else {
        Some(end_checkpoint)
    };
    let mut wm = Watermark::default();
    wm.cursor = Some(CursorToken::boundary(end_position).encode());
    set_checkpoint_bound(&mut wm, boundary);
    wm
}

/// Whether the scan reached the natural end of the requested range (the
/// ledger tip or a requested `end_checkpoint`) rather than being truncated
/// by an item or scan limit, or bounded by a client cursor. Only natural
/// completion proves the range's final checkpoint complete.
pub fn reached_range_end(reason: QueryEndReason) -> bool {
    matches!(
        reason,
        QueryEndReason::LedgerTip | QueryEndReason::CheckpointBound
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_rpc_cursor::Position;

    fn options(ascending: bool) -> QueryOptions {
        let mut request = sui_rpc::proto::sui::rpc::v2alpha::QueryOptions::default();
        request.ordering = Some(if ascending {
            sui_rpc::proto::sui::rpc::v2alpha::Ordering::Ascending as i32
        } else {
            sui_rpc::proto::sui::rpc::v2alpha::Ordering::Descending as i32
        });
        QueryOptions::transactions_from_proto(Some(&request), 100, 100).unwrap()
    }

    #[test]
    fn merge_covered_checkpoint_bound_keeps_most_advanced_in_direction() {
        let asc = options(true);
        assert_eq!(merge_covered_checkpoint_bound(None, 5, &asc), Some(5));
        assert_eq!(merge_covered_checkpoint_bound(Some(5), 9, &asc), Some(9));
        assert_eq!(merge_covered_checkpoint_bound(Some(9), 5, &asc), Some(9));

        let desc = options(false);
        assert_eq!(merge_covered_checkpoint_bound(None, 9, &desc), Some(9));
        assert_eq!(merge_covered_checkpoint_bound(Some(9), 5, &desc), Some(5));
        assert_eq!(merge_covered_checkpoint_bound(Some(5), 9, &desc), Some(5));
    }

    /// The per-checkpoint scanners exclude the item's own cp: `C - 1`
    /// ascending, `C + 1` descending.
    #[test]
    fn advance_covered_bound_before_checkpoint_adjusts_by_one() {
        let asc = options(true);
        assert_eq!(
            advance_covered_bound_before_checkpoint(None, 10, &asc),
            Some(9)
        );
        assert_eq!(
            advance_covered_bound_before_checkpoint(Some(9), 12, &asc),
            Some(11)
        );

        let desc = options(false);
        assert_eq!(
            advance_covered_bound_before_checkpoint(None, 10, &desc),
            Some(11)
        );
        assert_eq!(
            advance_covered_bound_before_checkpoint(Some(11), 8, &desc),
            Some(9)
        );
    }

    /// Overflow at the range edge (`cp 0` ascending, `u64::MAX` descending)
    /// preserves the previously accumulated boundary instead of dropping it.
    #[test]
    fn advance_covered_bound_before_checkpoint_preserves_prev_on_overflow() {
        let asc = options(true);
        assert_eq!(
            advance_covered_bound_before_checkpoint(Some(4), 0, &asc),
            Some(4)
        );
        assert_eq!(advance_covered_bound_before_checkpoint(None, 0, &asc), None);

        let desc = options(false);
        assert_eq!(
            advance_covered_bound_before_checkpoint(Some(4), u64::MAX, &desc),
            Some(4)
        );
        assert_eq!(
            advance_covered_bound_before_checkpoint(None, u64::MAX, &desc),
            None
        );
    }

    #[test]
    fn boundary_cursor_cp_bumps_descending_only() {
        assert_eq!(boundary_cursor_cp(10, ScanDirection::Ascending), 10);
        assert_eq!(boundary_cursor_cp(10, ScanDirection::Descending), 11);
        assert_eq!(
            boundary_cursor_cp(u64::MAX, ScanDirection::Descending),
            u64::MAX
        );
    }

    /// The direction-correct boundary is recorded in the single `checkpoint` field regardless of
    /// ordering. A client reads the bound off the wire frame and interprets it per the request's
    /// ordering.
    #[test]
    fn item_watermark_sets_direction_matching_bound() {
        let pos = Position::Transactions {
            checkpoint: 9,
            tx_seq: 42,
        };
        let wm = item_watermark(pos, Some(8));
        assert_eq!(wm.checkpoint, Some(8));
        assert_eq!(wm.cursor.as_ref(), Some(&CursorToken::item(pos).encode()));

        let wm = item_watermark(pos, None);
        assert_eq!(wm.checkpoint, None);
    }

    /// On natural completion the terminal frame claims the range's final
    /// checkpoint complete: ascending uses `end_checkpoint - 1` and resumes
    /// from `(end_checkpoint, end_position)`; descending stores the range's
    /// lowest checkpoint (inclusive). Both land in the single `checkpoint`
    /// field.
    #[test]
    fn terminal_boundary_watermark_claims_final_checkpoint() {
        let asc = options(true);
        let pos = Position::Transactions {
            checkpoint: 10,
            tx_seq: 100,
        };
        let wm = terminal_boundary_watermark(&asc, pos);
        assert_eq!(wm.checkpoint, Some(9));
        assert_eq!(
            wm.cursor.as_ref(),
            Some(&CursorToken::boundary(pos).encode())
        );

        let desc = options(false);
        let wm = terminal_boundary_watermark(&desc, pos);
        assert_eq!(wm.checkpoint, Some(10));
        assert_eq!(
            wm.cursor.as_ref(),
            Some(&CursorToken::boundary(pos).encode())
        );
    }

    #[test]
    fn reached_range_end_only_for_natural_completion() {
        assert!(reached_range_end(QueryEndReason::LedgerTip));
        assert!(reached_range_end(QueryEndReason::CheckpointBound));
        assert!(!reached_range_end(QueryEndReason::ScanLimit));
        assert!(!reached_range_end(QueryEndReason::ItemLimit));
    }
}
