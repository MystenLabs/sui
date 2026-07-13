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
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
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
/// `entry_checkpoint` is the checkpoint containing the effective interval's
/// first position in scan direction (fixed at range-resolution time). A
/// candidate strictly before it proves nothing — the scan is still inside its
/// first checkpoint — and is discarded, keeping the wire `checkpoint` field
/// unset until the scan's first checkpoint is fully covered, as the proto
/// contract requires.
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
    cursor_watermark(position, boundary, sui_rpc_cursor::CursorKind::Boundary)
}

/// Build a watermark whose cursor kind has been resolved by query-range
/// bookkeeping. This is needed for an ascending event interval made empty by
/// an `after` Item cursor, where changing the raw coordinate to Boundary would
/// re-include the item on resume.
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

/// Resolve the checkpoint coordinate embedded in a transaction/event/checkpoint
/// scan-frontier cursor independently from the optional completed-checkpoint
/// claim. A missing mapping is representable only at the numeric edge where
/// the frontier itself supplies the sole safe checkpoint coordinate.
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

/// Boundary watermark emitted once a scan has drained its entire resolved
/// range under natural completion. Unlike per-item watermarks it can claim
/// the range's final checkpoint complete — `end_checkpoint - 1` ascending
/// (the exclusive cp upper) or `end_checkpoint` descending (the inclusive cp
/// lower) — because no further items exist in it within the requested range.
/// The `(end_checkpoint, end_position)` cursor resumes exactly past the
/// scanned range.
fn terminal_boundary_watermark(options: &QueryOptions, end_position: Position) -> Watermark {
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
        /// The resolved interval contained no scannable positions. Natural
        /// completion of an empty interval covered nothing, so its terminal
        /// claim stays unset (the proto contract keeps `checkpoint` unset
        /// until the scan's first checkpoint is fully covered); the resume
        /// cursor is unaffected.
        interval_empty: bool,
    },
    /// The caller-provided cursor bound truncates the interval at `position`.
    CursorBound {
        position: Position,
        kind: CursorKind,
    },
}

impl ScanTerminal {
    pub fn from_range_exhaustion(
        exhaustion: RangeExhaustion,
        position: Position,
        interval_empty: bool,
    ) -> Self {
        match exhaustion {
            RangeExhaustion::LedgerTip => Self::NaturalRange {
                end: NaturalRangeEnd::LedgerTip,
                position,
                interval_empty,
            },
            RangeExhaustion::CheckpointBound => Self::NaturalRange {
                end: NaturalRangeEnd::CheckpointBound,
                position,
                interval_empty,
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
    /// (LedgerTip/CheckpointBound) of a scanned interval claims the range's
    /// final checkpoint via `terminal_boundary_watermark` and ignores
    /// `covered_checkpoint_bound` (the range claim is always at least as
    /// strong); an empty interval covered nothing and claims nothing. A
    /// cursor bound never claims its own checkpoint: its claim is exactly
    /// the accumulated item coverage.
    pub fn into_watermark(
        self,
        options: &QueryOptions,
        covered_checkpoint_bound: Option<u64>,
    ) -> Watermark {
        match self {
            Self::ScanLimit { watermark } => watermark,
            Self::NaturalRange {
                position,
                interval_empty,
                ..
            } => {
                if interval_empty {
                    boundary_watermark(position, None)
                } else {
                    terminal_boundary_watermark(options, position)
                }
            }
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

    /// While the scan is still inside its first checkpoint of the effective
    /// interval, the fencepost candidate falls before the entry checkpoint and
    /// must be discarded: the wire `checkpoint` field stays unset until the
    /// scan's first checkpoint is fully covered (proto contract). The claim at
    /// exactly the entry checkpoint (candidate == entry) is the first legal
    /// one.
    #[test]
    fn advance_covered_bound_before_checkpoint_stays_unset_within_entry_checkpoint() {
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
    fn scan_terminal_converts_every_range_exhaustion() {
        let position = Position::Transactions {
            checkpoint: 9,
            tx_seq: 4,
        };
        let cases = [
            (
                RangeExhaustion::LedgerTip,
                false,
                ScanTerminal::NaturalRange {
                    end: NaturalRangeEnd::LedgerTip,
                    position,
                    interval_empty: false,
                },
            ),
            (
                RangeExhaustion::CheckpointBound,
                true,
                ScanTerminal::NaturalRange {
                    end: NaturalRangeEnd::CheckpointBound,
                    position,
                    interval_empty: true,
                },
            ),
            (
                RangeExhaustion::CursorBound {
                    kind: CursorKind::Item,
                },
                false,
                ScanTerminal::CursorBound {
                    position,
                    kind: CursorKind::Item,
                },
            ),
            (
                RangeExhaustion::CursorBound {
                    kind: CursorKind::Item,
                },
                true,
                ScanTerminal::CursorBound {
                    position,
                    kind: CursorKind::Item,
                },
            ),
        ];

        for (exhaustion, interval_empty, expected) in cases {
            assert_eq!(
                ScanTerminal::from_range_exhaustion(exhaustion, position, interval_empty),
                expected
            );
        }
    }

    #[test]
    fn scan_terminal_natural_range_uses_terminal_boundary_watermark() {
        let ascending = options(true);
        let descending = options(false);
        let position = Position::Transactions {
            checkpoint: 9,
            tx_seq: 4,
        };

        let checkpoint_bound = ScanTerminal::NaturalRange {
            end: NaturalRangeEnd::CheckpointBound,
            position,
            interval_empty: false,
        };
        assert_eq!(checkpoint_bound.reason(), QueryEndReason::CheckpointBound);
        let watermark = checkpoint_bound.clone().into_watermark(&ascending, None);
        assert_eq!(
            watermark.cursor,
            Some(CursorToken::boundary(position).encode())
        );
        assert_eq!(watermark.checkpoint, Some(8));
        let watermark = checkpoint_bound.into_watermark(&descending, None);
        assert_eq!(
            watermark.cursor,
            Some(CursorToken::boundary(position).encode())
        );
        assert_eq!(watermark.checkpoint, Some(9));

        let ledger_tip = ScanTerminal::NaturalRange {
            end: NaturalRangeEnd::LedgerTip,
            position,
            interval_empty: false,
        };
        assert_eq!(ledger_tip.reason(), QueryEndReason::LedgerTip);
        let watermark = ledger_tip.clone().into_watermark(&ascending, Some(6));
        assert_eq!(
            watermark.cursor,
            Some(CursorToken::boundary(position).encode())
        );
        assert_eq!(watermark.checkpoint, Some(8));
        let watermark = ledger_tip.into_watermark(&descending, Some(6));
        assert_eq!(
            watermark.cursor,
            Some(CursorToken::boundary(position).encode())
        );
        assert_eq!(watermark.checkpoint, Some(9));
    }

    /// Natural completion of an interval that resolved empty covered nothing:
    /// the terminal cursor is unchanged but the checkpoint claim stays unset,
    /// in both directions and for both natural reasons.
    #[test]
    fn scan_terminal_empty_natural_range_claims_nothing() {
        let ascending = options(true);
        let descending = options(false);
        let position = Position::Transactions {
            checkpoint: 9,
            tx_seq: 4,
        };

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
                interval_empty: true,
            };
            assert_eq!(terminal.reason(), reason);
            let watermark = terminal.clone().into_watermark(&ascending, None);
            assert_eq!(
                watermark.cursor,
                Some(CursorToken::boundary(position).encode())
            );
            assert_eq!(watermark.checkpoint, None);
            let watermark = terminal.into_watermark(&descending, None);
            assert_eq!(
                watermark.cursor,
                Some(CursorToken::boundary(position).encode())
            );
            assert_eq!(watermark.checkpoint, None);
        }
    }

    #[test]
    fn scan_terminal_cursor_bound_preserves_coverage() {
        let ascending = options(true);
        let position = Position::Transactions {
            checkpoint: 9,
            tx_seq: 4,
        };
        let terminal = ScanTerminal::CursorBound {
            position,
            kind: CursorKind::Boundary,
        };
        assert_eq!(terminal.reason(), QueryEndReason::CursorBound);

        let watermark = terminal.clone().into_watermark(&ascending, Some(6));
        assert_eq!(
            watermark.cursor,
            Some(CursorToken::boundary(position).encode())
        );
        assert_eq!(watermark.checkpoint, Some(6));

        let watermark = terminal.into_watermark(&ascending, None);
        assert_eq!(
            watermark.cursor,
            Some(CursorToken::boundary(position).encode())
        );
        assert_eq!(watermark.checkpoint, None);
    }

    #[test]
    fn scan_terminal_event_cursor_bound_preserves_item_kind() {
        let ascending = options(true);
        let position = Position::Events {
            checkpoint: 9,
            tx_seq: 4,
            event_index: 2,
        };
        let terminal = ScanTerminal::CursorBound {
            position,
            kind: CursorKind::Item,
        };
        assert_eq!(terminal.reason(), QueryEndReason::CursorBound);

        let watermark = terminal.into_watermark(&ascending, Some(6));
        assert_eq!(watermark.cursor, Some(CursorToken::item(position).encode()));
        assert_eq!(watermark.checkpoint, Some(6));
    }

    #[test]
    fn scan_terminal_scan_limit_returns_owned_watermark() {
        let ascending = options(true);
        let descending = options(false);
        let mut watermark = Watermark::default();
        watermark.cursor = Some(b"scan-limit".to_vec().into());
        watermark.checkpoint = Some(6);
        let terminal = ScanTerminal::ScanLimit {
            watermark: watermark.clone(),
        };
        assert_eq!(terminal.reason(), QueryEndReason::ScanLimit);
        assert_eq!(terminal.into_watermark(&ascending, None), watermark);

        let terminal = ScanTerminal::ScanLimit {
            watermark: watermark.clone(),
        };
        assert_eq!(terminal.into_watermark(&descending, Some(99)), watermark);
    }
}
