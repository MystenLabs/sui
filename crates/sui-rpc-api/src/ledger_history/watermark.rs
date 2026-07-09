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
//!   item at cp `C` only proves the scan ENTERED `C` (more matches may sit
//!   at higher/lower tx_seqs / event_seqs); the last covered cp is `C ∓ 1` —
//!   see [`CheckpointBoundary::checkpoint_entered`].
//! - `list_checkpoints` dedupes cp_seq, so "cp `C` emitted" ≡ "cp `C`
//!   COVERED." It feeds `C` straight into
//!   [`CheckpointBoundary::checkpoint_covered`] for items, and translates
//!   its scan frontier into a cp-space candidate itself before doing the
//!   same.
//!
//! This module owns the shared pieces; each handler keeps only its
//! API-specific frontier-to-candidate adapter.

use sui_rpc_cursor::{CursorToken, Position};

use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;

/// Populate the completion-boundary `checkpoint` field of a `Watermark` from
/// the per-scan boundary value. The value already carries the direction-correct
/// meaning (inclusive upper bound ascending, inclusive lower bound descending);
/// the single wire field records it regardless of ordering.
fn set_checkpoint_bound(wm: &mut Watermark, boundary: Option<u64>) {
    wm.checkpoint = boundary;
}

/// Direction-aware completion-boundary accumulator: the inclusive boundary
/// checkpoint a scan has fully covered so far, advancing monotonically in the
/// request's ordering direction (max ascending, min descending). It also
/// builds the watermark frames that carry the boundary, so recording an
/// emission and stamping its frame cannot get out of order.
#[derive(Clone, Copy, Debug)]
pub struct CheckpointBoundary {
    ascending: bool,
    bound: Option<u64>,
}

impl CheckpointBoundary {
    pub fn new(ascending: bool) -> Self {
        Self {
            ascending,
            bound: None,
        }
    }

    /// Build the embedded `Watermark` for an item that only proves its
    /// checkpoint ENTERED (`list_transactions` / `list_events`: the cp may
    /// still hold further matches at other tx_seqs / event_seqs): the cursor
    /// resumes past this item; the bound excludes the item's own cp.
    pub fn item_watermark_entered(&mut self, position: Position) -> Watermark {
        self.checkpoint_entered(position.checkpoint());
        self.item_frame(position)
    }

    /// Build the embedded `Watermark` for an item that proves its checkpoint
    /// COVERED (`list_checkpoints`: cp_seq dedup makes an emitted checkpoint
    /// complete): the bound includes the item's own cp.
    pub fn item_watermark_covered(&mut self, position: Position) -> Watermark {
        self.checkpoint_covered(position.checkpoint());
        self.item_frame(position)
    }

    fn item_frame(&self, position: Position) -> Watermark {
        let mut wm = Watermark::default();
        wm.cursor = Some(CursorToken::item(position).encode());
        set_checkpoint_bound(&mut wm, self.bound);
        wm
    }

    /// Record a scan frontier (a frontier lands partway through its
    /// checkpoint regardless of endpoint, so its cp is only ENTERED) and
    /// build the standalone frontier `Watermark`. `position` carries the
    /// frontier's containing checkpoint; the emitted cursor holds the
    /// boundary-cursor-adjusted resume coordinate (see
    /// [`boundary_cursor_cp`]), which differs descending.
    pub fn frontier_watermark(&mut self, position: Position) -> Watermark {
        let cp = position.checkpoint();
        self.checkpoint_entered(cp);
        boundary_watermark(
            position.with_checkpoint(boundary_cursor_cp(cp, self.ascending)),
            self.bound,
        )
    }

    /// Record that the scan has wholly covered `candidate` (or fold an
    /// already-resolved candidate from a caller-side adapter): the checkpoint
    /// itself is complete and can be claimed.
    pub fn checkpoint_covered(&mut self, candidate: u64) {
        self.bound = Some(match self.bound {
            None => candidate,
            Some(p) if self.ascending => p.max(candidate),
            Some(p) => p.min(candidate),
        });
    }

    /// Record that the scan has entered `cp` (an emission landed inside it):
    /// since the scan is ordered, entering `cp` proves the previous
    /// checkpoint covered — `cp - 1` ascending / `cp + 1` descending — while
    /// `cp` itself may still hold further matches.
    ///
    /// When that previous checkpoint doesn't exist (`cp == 0` ascending or
    /// `u64::MAX` descending) the accumulated boundary is preserved rather
    /// than collapsed back to `None`.
    pub fn checkpoint_entered(&mut self, cp: u64) {
        let covered = if self.ascending {
            cp.checked_sub(1)
        } else {
            cp.checked_add(1)
        };
        if let Some(covered) = covered {
            self.checkpoint_covered(covered);
        }
    }

    /// The accumulated boundary, for the `checkpoint` field of emitted
    /// watermark frames.
    pub fn bound(&self) -> Option<u64> {
        self.bound
    }
}

/// Build a standalone scan-frontier `Watermark` from already-resolved
/// boundary-cursor coordinates. Most frontier frames go through
/// [`CheckpointBoundary::frontier_watermark`]; this remains for
/// `list_checkpoints`' kv frontier adapter, which clamps its emitted cursor
/// coordinate against the last emitted checkpoint itself.
pub fn boundary_watermark(position: Position, boundary: Option<u64>) -> Watermark {
    let mut wm = Watermark::default();
    wm.cursor = Some(CursorToken::boundary(position).encode());
    set_checkpoint_bound(&mut wm, boundary);
    wm
}

/// Resolve the boundary-cursor checkpoint coordinate for a scan frontier.
/// The cursor encoding is asymmetric: ascending `Boundary` cursors advance
/// the cp-range start, so the frontier cp is used directly; descending
/// `Boundary` cursors treat the cp coordinate as an EXCLUSIVE upper bound,
/// so `cp + 1` is needed to keep `cp` itself included on resume.
fn boundary_cursor_cp(cp: u64, ascending: bool) -> u64 {
    if ascending { cp } else { cp.saturating_add(1) }
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

/// Boundary watermark emitted once a scan has drained its entire resolved
/// range under natural completion. Unlike per-item watermarks it can claim
/// the range's final checkpoint complete — `end_checkpoint - 1` ascending
/// (the exclusive cp upper) or `end_checkpoint` descending (the inclusive cp
/// lower) — because no further items exist in it within the requested range.
/// The `(end_checkpoint, end_position)` cursor resumes exactly past the
/// scanned range.
pub fn terminal_boundary_watermark(end_position: Position, ascending: bool) -> Watermark {
    let end_checkpoint = end_position.checkpoint();
    let boundary = if ascending {
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

    #[test]
    fn checkpoint_covered_keeps_most_advanced_in_direction() {
        let mut boundary = CheckpointBoundary::new(true);
        assert_eq!(boundary.bound(), None);
        boundary.checkpoint_covered(5);
        assert_eq!(boundary.bound(), Some(5));
        boundary.checkpoint_covered(9);
        assert_eq!(boundary.bound(), Some(9));
        boundary.checkpoint_covered(5);
        assert_eq!(boundary.bound(), Some(9));

        let mut boundary = CheckpointBoundary::new(false);
        boundary.checkpoint_covered(9);
        assert_eq!(boundary.bound(), Some(9));
        boundary.checkpoint_covered(5);
        assert_eq!(boundary.bound(), Some(5));
        boundary.checkpoint_covered(9);
        assert_eq!(boundary.bound(), Some(5));
    }

    /// The per-checkpoint scanners exclude the item's own cp: `C - 1`
    /// ascending, `C + 1` descending.
    #[test]
    fn checkpoint_entered_excludes_own_cp() {
        let mut boundary = CheckpointBoundary::new(true);
        boundary.checkpoint_entered(10);
        assert_eq!(boundary.bound(), Some(9));
        boundary.checkpoint_entered(12);
        assert_eq!(boundary.bound(), Some(11));

        let mut boundary = CheckpointBoundary::new(false);
        boundary.checkpoint_entered(10);
        assert_eq!(boundary.bound(), Some(11));
        boundary.checkpoint_entered(8);
        assert_eq!(boundary.bound(), Some(9));
    }

    /// Overflow at the range edge (`cp 0` ascending, `u64::MAX` descending)
    /// preserves the previously accumulated boundary instead of dropping it.
    #[test]
    fn checkpoint_entered_preserves_bound_on_overflow() {
        let mut boundary = CheckpointBoundary::new(true);
        boundary.checkpoint_entered(0);
        assert_eq!(boundary.bound(), None);
        boundary.checkpoint_covered(4);
        boundary.checkpoint_entered(0);
        assert_eq!(boundary.bound(), Some(4));

        let mut boundary = CheckpointBoundary::new(false);
        boundary.checkpoint_entered(u64::MAX);
        assert_eq!(boundary.bound(), None);
        boundary.checkpoint_covered(4);
        boundary.checkpoint_entered(u64::MAX);
        assert_eq!(boundary.bound(), Some(4));
    }

    #[test]
    fn boundary_cursor_cp_bumps_descending_only() {
        assert_eq!(boundary_cursor_cp(10, true), 10);
        assert_eq!(boundary_cursor_cp(10, false), 11);
        assert_eq!(boundary_cursor_cp(u64::MAX, false), u64::MAX);
    }

    /// An item frame records its own position's checkpoint before stamping
    /// the bound: partial items exclude their own cp, complete items include
    /// it.
    #[test]
    fn item_watermark_records_per_item_coverage() {
        let pos = Position::Transactions {
            checkpoint: 9,
            tx_seq: 42,
        };
        let mut boundary = CheckpointBoundary::new(true);
        let wm = boundary.item_watermark_entered(pos);
        assert_eq!(wm.checkpoint, Some(8));
        assert_eq!(wm.cursor.as_ref(), Some(&CursorToken::item(pos).encode()));

        let cp_pos = Position::Checkpoints { checkpoint: 9 };
        let mut boundary = CheckpointBoundary::new(true);
        let wm = boundary.item_watermark_covered(cp_pos);
        assert_eq!(wm.checkpoint, Some(9));
        assert_eq!(
            wm.cursor.as_ref(),
            Some(&CursorToken::item(cp_pos).encode())
        );

        // The genesis partial item claims nothing yet.
        let mut boundary = CheckpointBoundary::new(true);
        let wm = boundary.item_watermark_entered(Position::Transactions {
            checkpoint: 0,
            tx_seq: 0,
        });
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
        );        let desc = options(false);
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

    /// A frontier frame records its position's containing cp as partial; the
    /// emitted cursor carries the boundary-cursor-adjusted resume coordinate
    /// (`cp` ascending, `cp + 1` descending).
    #[test]
    fn frontier_watermark_adjusts_resume_coordinate() {
        let pos = Position::Transactions {
            checkpoint: 10,
            tx_seq: 100,
        };

        let mut boundary = CheckpointBoundary::new(true);
        let wm = boundary.frontier_watermark(pos);
        assert_eq!(wm.checkpoint, Some(9));
        assert_eq!(
            wm.cursor.as_ref(),
            Some(&CursorToken::boundary(pos).encode())
        );

        let mut boundary = CheckpointBoundary::new(false);
        let wm = boundary.frontier_watermark(pos);
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
        let pos = Position::Transactions {
            checkpoint: 10,
            tx_seq: 100,
        };
        let wm = terminal_boundary_watermark(pos, true);
        assert_eq!(wm.checkpoint, Some(9));
        assert_eq!(
            wm.cursor.as_ref(),
            Some(&CursorToken::boundary(pos).encode())
        );

        let wm = terminal_boundary_watermark(pos, false);
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
