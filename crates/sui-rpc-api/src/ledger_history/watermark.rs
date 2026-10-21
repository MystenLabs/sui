// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared `Watermark` construction for the v2 list APIs.
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
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2::Watermark;
use sui_rpc_cursor::{CursorKind, CursorToken, Position};

use crate::ledger_history::query_options::{QueryOptions, RangeExhaustion};

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
/// `entry_checkpoint` is the window's first checkpoint in scan direction
/// (raised to the serving floor ascending). A candidate strictly before it
/// lies outside the window, and is discarded. Within the window, a candidate
/// counts even before the scan reads its checkpoint's start: the request's
/// starting cursor vouches for every position before it.
///
/// When the `C ∓ 1` adjustment would overflow (`C == 0` ascending or
/// `u64::MAX` descending), the previously covered bound is preserved.
pub fn advance_covered_bound_before_checkpoint(
    covered_checkpoint_bound: Option<u64>,
    incomplete_checkpoint: u64,
    entry_checkpoint: u64,
    options: &QueryOptions,
) -> Option<u64> {
    let candidate_bound = if options.is_ascending() {
        incomplete_checkpoint
            .checked_sub(1)
            .filter(|candidate| *candidate >= entry_checkpoint)
    } else {
        incomplete_checkpoint
            .checked_add(1)
            .filter(|candidate| *candidate <= entry_checkpoint)
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
/// plus the current direction-matching checkpoint boundary.
pub fn item_watermark(position: Position, boundary: Option<u64>) -> Watermark {
    let mut wm = Watermark::default();
    wm.cursor = Some(CursorToken::item(position).encode());
    set_checkpoint_bound(&mut wm, boundary);
    wm
}

/// Build a standalone scan-frontier `Watermark`. `position` is the boundary
/// cursor position the caller has already resolved for its scan domain (see
/// [`boundary_cursor_cp`] for the checkpoint scanners' direction adjustment);
/// `boundary` is the accumulated completion boundary.
pub fn boundary_watermark(position: Position, boundary: Option<u64>) -> Watermark {
    cursor_watermark(position, boundary, sui_rpc_cursor::CursorKind::Boundary)
}

/// Build a watermark whose cursor kind has been resolved by query-range
/// bookkeeping (see [`RangeExhaustion::CursorBound`]): an interval made empty
/// by an `after` Item cursor keeps Item kind where changing the raw coordinate
/// to Boundary would re-include the item on resume.
fn cursor_watermark(
    position: Position,
    boundary: Option<u64>,
    cursor_kind: sui_rpc_cursor::CursorKind,
) -> Watermark {
    let cursor = match cursor_kind {
        sui_rpc_cursor::CursorKind::Item => CursorToken::item(position),
        sui_rpc_cursor::CursorKind::Boundary => CursorToken::boundary(position),
    };
    let mut wm = Watermark::default();
    wm.cursor = Some(cursor.encode());
    set_checkpoint_bound(&mut wm, boundary);
    wm
}

/// Resolve the boundary-cursor checkpoint for a `list_checkpoints` scan
/// frontier that lands partway through checkpoint `cp`, so that `cp` itself
/// stays included on resume. Ascending `Boundary` cursors resume at their
/// checkpoint, so `cp` is used directly; descending `Boundary` cursors exclude
/// their checkpoint, so `cp + 1` is needed.
pub fn boundary_cursor_cp(cp: u64, direction: ScanDirection) -> u64 {
    if direction.is_ascending() {
        cp
    } else {
        cp.saturating_add(1)
    }
}

/// Resolve the checkpoint of a `list_checkpoints` scan-frontier cursor
/// independently from the optional completed-checkpoint claim. A missing
/// mapping is representable only at the numeric edge where the frontier itself
/// supplies the sole safe checkpoint.
pub fn scan_frontier_cursor_cp(
    checkpoint: Option<u64>,
    frontier: u64,
    direction: ScanDirection,
) -> Option<u64> {
    checkpoint
        .map(|cp| boundary_cursor_cp(cp, direction))
        .or_else(|| {
            ((direction.is_ascending() && frontier == 0)
                || (!direction.is_ascending() && frontier == u64::MAX))
                .then_some(frontier)
        })
}

/// Terminal of a successful list scan that renders as the trailing
/// payload-free `QueryEnd` frame. `ItemLimit` never reaches this type: the
/// drive loops fuse it onto the final item frame and suppress the trailing
/// frame. The wire reason and the watermark policy are projections of the
/// same value, so they cannot disagree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NaturalRangeEnd {
    LedgerTip,
    CheckpointBound,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScanTerminal {
    /// Scan budget exhausted. Owns the mandatory authoritative frontier
    /// watermark (its cursor is always set by the frontier constructors).
    ScanLimit { watermark: Watermark },
    /// The resolved interval is naturally exhausted at `position`.
    NaturalRange {
        end: NaturalRangeEnd,
        position: Position,
        /// The checkpoint this page's natural end reports as fully covered in
        /// `Watermark.checkpoint`, or `None` when nothing this server serves remains to claim: the
        /// window has no checkpoint, or the serving floor consumed the scan. See
        /// [`ResolvedScan::natural_end_claim`].
        ///
        /// [`ResolvedScan::natural_end_claim`]: crate::ledger_history::query_options::ResolvedScan::natural_end_claim
        claimed_checkpoint: Option<u64>,
    },
    /// The caller-provided cursor bound truncates the interval at `position`.
    CursorBound {
        position: Position,
        kind: CursorKind,
    },
}

impl ScanTerminal {
    /// Build the terminal for a resolved interval exhausted at `position`. `claimed_checkpoint`
    /// is the checkpoint natural completion claims; a cursor bound ignores it.
    pub fn from_range_exhaustion(
        exhaustion: RangeExhaustion,
        position: Position,
        claimed_checkpoint: Option<u64>,
    ) -> Self {
        match exhaustion {
            RangeExhaustion::LedgerTip => Self::NaturalRange {
                end: NaturalRangeEnd::LedgerTip,
                position,
                claimed_checkpoint,
            },
            RangeExhaustion::CheckpointBound => Self::NaturalRange {
                end: NaturalRangeEnd::CheckpointBound,
                position,
                claimed_checkpoint,
            },
            RangeExhaustion::CursorBound { kind } => Self::CursorBound { position, kind },
        }
    }

    pub fn reason(&self) -> QueryEndReason {
        match self {
            Self::ScanLimit { .. } => QueryEndReason::ScanLimit,
            Self::NaturalRange {
                end: NaturalRangeEnd::LedgerTip,
                ..
            } => QueryEndReason::LedgerTip,
            Self::NaturalRange {
                end: NaturalRangeEnd::CheckpointBound,
                ..
            } => QueryEndReason::CheckpointBound,
            Self::CursorBound { .. } => QueryEndReason::CursorBound,
        }
    }

    /// Render the trailing terminal frame's watermark. Natural completion
    /// (LedgerTip/CheckpointBound) claims its `claimed_checkpoint` and ignores
    /// `covered_checkpoint_bound` (the range claim is always at least as
    /// strong). A cursor bound never claims its own checkpoint: its claim is
    /// exactly the accumulated item coverage.
    pub fn into_watermark(self, covered_checkpoint_bound: Option<u64>) -> Watermark {
        match self {
            Self::ScanLimit { watermark } => watermark,
            Self::NaturalRange {
                position,
                claimed_checkpoint,
                ..
            } => boundary_watermark(position, claimed_checkpoint),
            Self::CursorBound { position, kind } => {
                cursor_watermark(position, covered_checkpoint_bound, kind)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_rpc_cursor::{CursorKind, Position};

    fn options(ascending: bool) -> QueryOptions {
        let mut request = sui_rpc::proto::sui::rpc::v2::QueryOptions::default();
        request.ordering = Some(if ascending {
            sui_rpc::proto::sui::rpc::v2::Ordering::Ascending as i32
        } else {
            sui_rpc::proto::sui::rpc::v2::Ordering::Descending as i32
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
            advance_covered_bound_before_checkpoint(None, 10, 5, &asc),
            Some(9)
        );
        assert_eq!(
            advance_covered_bound_before_checkpoint(Some(9), 12, 5, &asc),
            Some(11)
        );

        let desc = options(false);
        assert_eq!(
            advance_covered_bound_before_checkpoint(None, 10, 15, &desc),
            Some(11)
        );
        assert_eq!(
            advance_covered_bound_before_checkpoint(Some(11), 8, 15, &desc),
            Some(9)
        );
    }

    /// A candidate before the entry checkpoint lies outside the window and is
    /// discarded. The claim at exactly the entry checkpoint (candidate ==
    /// entry) is the first legal one.
    #[test]
    fn advance_covered_bound_before_checkpoint_discards_candidates_outside_the_window() {
        let asc = options(true);
        assert_eq!(
            advance_covered_bound_before_checkpoint(None, 10, 10, &asc),
            None
        );
        assert_eq!(
            advance_covered_bound_before_checkpoint(None, 11, 10, &asc),
            Some(10)
        );

        let desc = options(false);
        assert_eq!(
            advance_covered_bound_before_checkpoint(None, 10, 10, &desc),
            None
        );
        assert_eq!(
            advance_covered_bound_before_checkpoint(None, 9, 10, &desc),
            Some(10)
        );
    }

    /// Overflow at the range edge (`cp 0` ascending, `u64::MAX` descending)
    /// preserves the previously accumulated boundary instead of dropping it.
    #[test]
    fn advance_covered_bound_before_checkpoint_preserves_prev_on_overflow() {
        let asc = options(true);
        assert_eq!(
            advance_covered_bound_before_checkpoint(Some(4), 0, 0, &asc),
            Some(4)
        );
        assert_eq!(
            advance_covered_bound_before_checkpoint(None, 0, 0, &asc),
            None
        );

        let desc = options(false);
        assert_eq!(
            advance_covered_bound_before_checkpoint(Some(4), u64::MAX, u64::MAX, &desc),
            Some(4)
        );
        assert_eq!(
            advance_covered_bound_before_checkpoint(None, u64::MAX, u64::MAX, &desc),
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
        let pos = Position::Transactions { tx_seq: 42 };
        let wm = item_watermark(pos, Some(8));
        assert_eq!(wm.checkpoint, Some(8));
        assert_eq!(wm.cursor.as_ref(), Some(&CursorToken::item(pos).encode()));

        let wm = item_watermark(pos, None);
        assert_eq!(wm.checkpoint, None);
    }

    #[test]
    fn scan_terminal_converts_every_range_exhaustion() {
        let position = Position::Transactions { tx_seq: 4 };
        let cases = [
            (
                RangeExhaustion::LedgerTip,
                Some(8),
                ScanTerminal::NaturalRange {
                    end: NaturalRangeEnd::LedgerTip,
                    position,
                    claimed_checkpoint: Some(8),
                },
            ),
            (
                RangeExhaustion::CheckpointBound,
                None,
                ScanTerminal::NaturalRange {
                    end: NaturalRangeEnd::CheckpointBound,
                    position,
                    claimed_checkpoint: None,
                },
            ),
            (
                RangeExhaustion::CursorBound {
                    kind: CursorKind::Item,
                },
                Some(8),
                ScanTerminal::CursorBound {
                    position,
                    kind: CursorKind::Item,
                },
            ),
            (
                RangeExhaustion::CursorBound {
                    kind: CursorKind::Item,
                },
                None,
                ScanTerminal::CursorBound {
                    position,
                    kind: CursorKind::Item,
                },
            ),
        ];

        for (exhaustion, claimed_checkpoint, expected) in cases {
            assert_eq!(
                ScanTerminal::from_range_exhaustion(exhaustion, position, claimed_checkpoint),
                expected
            );
        }
    }

    /// A natural end claims its `claimed_checkpoint` behind a boundary cursor at its position,
    /// whatever coverage the items accumulated.
    #[test]
    fn scan_terminal_natural_range_claims_its_claimed_checkpoint() {
        let position = Position::Transactions { tx_seq: 4 };

        for claimed_checkpoint in [8, 9] {
            let checkpoint_bound = ScanTerminal::NaturalRange {
                end: NaturalRangeEnd::CheckpointBound,
                position,
                claimed_checkpoint: Some(claimed_checkpoint),
            };
            assert_eq!(checkpoint_bound.reason(), QueryEndReason::CheckpointBound);
            let watermark = checkpoint_bound.into_watermark(None);
            assert_eq!(
                watermark.cursor,
                Some(CursorToken::boundary(position).encode())
            );
            assert_eq!(watermark.checkpoint, Some(claimed_checkpoint));

            let ledger_tip = ScanTerminal::NaturalRange {
                end: NaturalRangeEnd::LedgerTip,
                position,
                claimed_checkpoint: Some(claimed_checkpoint),
            };
            assert_eq!(ledger_tip.reason(), QueryEndReason::LedgerTip);
            let watermark = ledger_tip.into_watermark(Some(6));
            assert_eq!(
                watermark.cursor,
                Some(CursorToken::boundary(position).encode())
            );
            assert_eq!(watermark.checkpoint, Some(claimed_checkpoint));
        }
    }

    /// A natural end with no claimed checkpoint (its window has no checkpoint this server
    /// serves) keeps its terminal cursor and leaves the checkpoint claim unset, for both natural
    /// reasons and whatever coverage the items accumulated.
    #[test]
    fn scan_terminal_unclaimed_natural_end_claims_nothing() {
        let position = Position::Transactions { tx_seq: 4 };

        for (end, reason) in [
            (
                NaturalRangeEnd::CheckpointBound,
                QueryEndReason::CheckpointBound,
            ),
            (NaturalRangeEnd::LedgerTip, QueryEndReason::LedgerTip),
        ] {
            let terminal = ScanTerminal::NaturalRange {
                end,
                position,
                claimed_checkpoint: None,
            };
            assert_eq!(terminal.reason(), reason);
            let watermark = terminal.clone().into_watermark(None);
            assert_eq!(
                watermark.cursor,
                Some(CursorToken::boundary(position).encode())
            );
            assert_eq!(watermark.checkpoint, None);
            let watermark = terminal.into_watermark(Some(6));
            assert_eq!(
                watermark.cursor,
                Some(CursorToken::boundary(position).encode())
            );
            assert_eq!(watermark.checkpoint, None);
        }
    }

    #[test]
    fn scan_terminal_cursor_bound_preserves_coverage() {
        let position = Position::Transactions { tx_seq: 4 };
        let terminal = ScanTerminal::CursorBound {
            position,
            kind: CursorKind::Boundary,
        };
        assert_eq!(terminal.reason(), QueryEndReason::CursorBound);

        let watermark = terminal.clone().into_watermark(Some(6));
        assert_eq!(
            watermark.cursor,
            Some(CursorToken::boundary(position).encode())
        );
        assert_eq!(watermark.checkpoint, Some(6));

        let watermark = terminal.into_watermark(None);
        assert_eq!(
            watermark.cursor,
            Some(CursorToken::boundary(position).encode())
        );
        assert_eq!(watermark.checkpoint, None);
    }

    #[test]
    fn scan_terminal_event_cursor_bound_preserves_item_kind() {
        let position = Position::Events {
            tx_seq: 4,
            event_index: 2,
        };
        let terminal = ScanTerminal::CursorBound {
            position,
            kind: CursorKind::Item,
        };
        assert_eq!(terminal.reason(), QueryEndReason::CursorBound);

        let watermark = terminal.into_watermark(Some(6));
        assert_eq!(watermark.cursor, Some(CursorToken::item(position).encode()));
        assert_eq!(watermark.checkpoint, Some(6));
    }

    #[test]
    fn scan_terminal_scan_limit_returns_owned_watermark() {
        let mut watermark = Watermark::default();
        watermark.cursor = Some(b"scan-limit".to_vec().into());
        watermark.checkpoint = Some(6);
        let terminal = ScanTerminal::ScanLimit {
            watermark: watermark.clone(),
        };
        assert_eq!(terminal.reason(), QueryEndReason::ScanLimit);
        assert_eq!(terminal.into_watermark(None), watermark);

        let terminal = ScanTerminal::ScanLimit {
            watermark: watermark.clone(),
        };
        assert_eq!(terminal.into_watermark(Some(99)), watermark);
    }
}
