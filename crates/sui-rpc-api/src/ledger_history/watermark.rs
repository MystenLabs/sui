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
//!   item at cp `C` does NOT prove `C` complete (more matches may sit at
//!   higher/lower tx_seqs / event_seqs). Their boundary candidate is
//!   `C ∓ 1` — see [`advance_boundary_excluding_cp`].
//! - `list_checkpoints` dedupes cp_seq, so "cp `C` emitted" ≡ "cp `C`
//!   complete." It feeds `C` straight into [`advance_checkpoint_boundary`]
//!   for items, and translates its scan frontier into a cp-space candidate
//!   itself before doing the same.
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

/// Fold an already-resolved completion-boundary `candidate` into the
/// accumulated boundary, keeping the most-advanced value in scan direction:
/// the max ascending, the min descending.
///
/// Callers resolve `candidate` for their scan domain first — `list_checkpoints`
/// passes the item's cp directly (dedup makes it complete), while the
/// per-checkpoint scanners use [`advance_boundary_excluding_cp`].
pub fn advance_checkpoint_boundary(
    prev: Option<u64>,
    candidate: u64,
    options: &QueryOptions,
) -> Option<u64> {
    Some(match prev {
        None => candidate,
        Some(p) if options.is_ascending() => p.max(candidate),
        Some(p) => p.min(candidate),
    })
}

/// Fold a cp whose own checkpoint is NOT proven complete into the
/// accumulated boundary (`list_transactions` / `list_events`: cp `C` may
/// still hold further matches at other tx_seqs / event_seqs; and any scan
/// frontier, which lands partway through the checkpoint it resolves to). The
/// boundary excludes `C` itself: `C - 1` ascending / `C + 1` descending.
///
/// When that adjusted candidate would overflow (`C == 0` ascending or
/// `u64::MAX` descending) the previously accumulated boundary is preserved
/// rather than collapsed back to `None`.
pub fn advance_boundary_excluding_cp(
    prev: Option<u64>,
    cp: u64,
    options: &QueryOptions,
) -> Option<u64> {
    let candidate = if options.is_ascending() {
        cp.checked_sub(1)
    } else {
        cp.checked_add(1)
    };
    match candidate {
        Some(c) => advance_checkpoint_boundary(prev, c, options),
        None => prev,
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

/// Build the standalone scan-frontier `Watermark` from a frontier position
/// carrying its raw containing checkpoint. The frontier lands partway
/// through that checkpoint, so the frame's two fields project it in
/// opposite directions: the completion claim excludes it looking backward
/// ([`advance_boundary_excluding_cp`], `cp ∓ 1`), while the resume cursor
/// keeps it included looking forward ([`boundary_cursor_cp`] rewrites the
/// position's cp coordinate; the scalar coordinates pass through).
pub fn frontier_boundary_watermark(options: &QueryOptions, position: Position) -> Watermark {
    let cp = position.checkpoint();
    let boundary = advance_boundary_excluding_cp(None, cp, options);
    boundary_watermark(
        position.with_checkpoint(boundary_cursor_cp(cp, options.scan_direction())),
        boundary,
    )
}

/// Boundary watermark emitted once a scan has drained its entire resolved range under natural
/// completion. Unlike per-item watermarks it can claim the range's final checkpoint complete —
/// `end_checkpoint - 1` ascending (the exclusive cp upper) or `end_checkpoint` descending (the
/// inclusive cp lower) — because no further items exist in it within the requested range. The
/// `(end_checkpoint, end_position)` cursor resumes exactly past the scanned range.
pub fn terminal_boundary_watermark(options: &QueryOptions, end_position: Position) -> Watermark {
    let end_checkpoint = end_position.checkpoint();
    let boundary = if options.is_ascending() {
        end_checkpoint.checked_sub(1)
    } else {
        Some(end_checkpoint)
    };
    boundary_watermark(end_position, boundary)
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
    fn advance_checkpoint_boundary_keeps_most_advanced_in_direction() {
        let asc = options(true);
        assert_eq!(advance_checkpoint_boundary(None, 5, &asc), Some(5));
        assert_eq!(advance_checkpoint_boundary(Some(5), 9, &asc), Some(9));
        assert_eq!(advance_checkpoint_boundary(Some(9), 5, &asc), Some(9));

        let desc = options(false);
        assert_eq!(advance_checkpoint_boundary(None, 9, &desc), Some(9));
        assert_eq!(advance_checkpoint_boundary(Some(9), 5, &desc), Some(5));
        assert_eq!(advance_checkpoint_boundary(Some(5), 9, &desc), Some(5));
    }

    /// The per-checkpoint scanners exclude the item's own cp: `C - 1`
    /// ascending, `C + 1` descending.
    #[test]
    fn advance_boundary_excluding_cp_adjusts_by_one() {
        let asc = options(true);
        assert_eq!(advance_boundary_excluding_cp(None, 10, &asc), Some(9));
        assert_eq!(advance_boundary_excluding_cp(Some(9), 12, &asc), Some(11));

        let desc = options(false);
        assert_eq!(advance_boundary_excluding_cp(None, 10, &desc), Some(11));
        assert_eq!(advance_boundary_excluding_cp(Some(11), 8, &desc), Some(9));
    }

    /// Overflow at the range edge (`cp 0` ascending, `u64::MAX` descending)
    /// preserves the previously accumulated boundary instead of dropping it.
    #[test]
    fn advance_boundary_excluding_cp_preserves_prev_on_overflow() {
        let asc = options(true);
        assert_eq!(advance_boundary_excluding_cp(Some(4), 0, &asc), Some(4));
        assert_eq!(advance_boundary_excluding_cp(None, 0, &asc), None);

        let desc = options(false);
        assert_eq!(
            advance_boundary_excluding_cp(Some(4), u64::MAX, &desc),
            Some(4)
        );
        assert_eq!(advance_boundary_excluding_cp(None, u64::MAX, &desc), None);
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

    /// A frontier frame projects its raw containing cp in opposite
    /// directions: the claim excludes it looking backward (`cp ∓ 1`), the
    /// resume cursor keeps it included looking forward (`cp` ascending,
    /// `cp + 1` descending); scalar coordinates pass through unchanged.
    #[test]
    fn frontier_boundary_watermark_splits_claim_and_resume() {
        let pos = Position::Transactions {
            checkpoint: 10,
            tx_seq: 100,
        };

        let asc = options(true);
        let wm = frontier_boundary_watermark(&asc, pos);
        assert_eq!(wm.checkpoint, Some(9));
        assert_eq!(
            wm.cursor.as_ref(),
            Some(&CursorToken::boundary(pos).encode())
        );

        let desc = options(false);
        let wm = frontier_boundary_watermark(&desc, pos);
        assert_eq!(wm.checkpoint, Some(11));
        assert_eq!(
            wm.cursor.as_ref(),
            Some(
                &CursorToken::boundary(Position::Transactions {
                    checkpoint: 11,
                    tx_seq: 100,
                })
                .encode()
            )
        );
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
