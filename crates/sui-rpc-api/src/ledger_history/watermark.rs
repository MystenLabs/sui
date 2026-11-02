// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared `Watermark` construction for the v2 list APIs.
//!
//! Both ledger-history backends — the fullnode (`sui-rpc-api`) and bigtable
//! (`sui-kv-rpc`) — and all three list handlers (`list_transactions`,
//! `list_events`, `list_checkpoints`) emit the same wire `Watermark`: a resume
//! cursor, the same resume point in ledger coordinates (`cursor_position`, see
//! [`crate::ledger_history::ledger_point`]), and a completion boundary
//! (`checkpoint`, the inclusive boundary checkpoint the scan has fully covered
//! in the request's ordering direction).
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

use crate::RpcError;
use crate::ledger_history::ledger_point::LedgerPoint;
use crate::ledger_history::ledger_point::TerminalLedgerPosition;
use crate::ledger_history::query_options::{QueryOptions, RangeExhaustion, below_serving_floor};

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
/// plus the current direction-matching checkpoint boundary. `position` is the
/// item's cursor coordinates (`list_checkpoints` passes its cp_seq), and
/// `item_key` its key in ledger coordinates, which the cursor carries as its
/// public position and from which the frame's `cursor_position` follows (see
/// [`LedgerPoint::item_resume`]).
pub fn item_watermark(
    position: Position,
    item_key: LedgerPoint,
    direction: ScanDirection,
    boundary: Option<u64>,
) -> Watermark {
    cursor_watermark(
        position,
        CursorKind::Item,
        TerminalLedgerPosition {
            cursor_position: Some(item_key.item_resume(&position, direction)),
            cursor_point: Some(item_key),
        },
        boundary,
    )
}

/// Build a standalone scan-frontier `Watermark`. `position` is the boundary
/// cursor coordinates the caller has already resolved for its scan domain (see
/// [`boundary_cursor_cp`] for the per-checkpoint scanners' direction
/// adjustment); `cursor_position` the same boundary in ledger coordinates,
/// which the cursor also carries as its public position; `boundary` the
/// accumulated completion boundary.
pub fn boundary_watermark(
    position: Position,
    cursor_position: LedgerPoint,
    boundary: Option<u64>,
) -> Watermark {
    cursor_watermark(
        position,
        CursorKind::Boundary,
        TerminalLedgerPosition::known(cursor_position),
        boundary,
    )
}

/// Build a watermark with a `cursor_kind` cursor at `position`, the one place
/// every frame's cursor is minted. The cursor carries `ledger.cursor_point` as
/// its public position, and the frame resumes at `ledger.cursor_position`. A
/// terminal's kind comes from query-range bookkeeping (see
/// [`RangeExhaustion::CursorBound`]): an `after` Item cursor at the last
/// position names no boundary after it, so its closing cursor keeps Item kind
/// where a Boundary at its position would re-include the item on resume.
fn cursor_watermark(
    position: Position,
    cursor_kind: CursorKind,
    ledger: TerminalLedgerPosition,
    boundary: Option<u64>,
) -> Watermark {
    let cursor = CursorToken {
        kind: cursor_kind,
        position,
        public_position: ledger
            .cursor_point
            .and_then(|point| point.public_position(&position)),
    };
    let mut wm = Watermark::default();
    wm.cursor = Some(cursor.encode());
    wm.cursor_position = ledger
        .cursor_position
        .map(|point| point.to_proto(&position));
    set_checkpoint_bound(&mut wm, boundary);
    wm
}

/// Resolve the boundary-cursor checkpoint coordinate for a `list_transactions`
/// / `list_events` scan frontier. The cursor encoding is asymmetric:
/// ascending `Boundary` cursors advance the cp-range start, so the frontier
/// cp is used directly; descending `Boundary` cursors treat the cp
/// coordinate as an EXCLUSIVE upper bound, so `cp + 1` is needed to keep
/// `cp` itself included on resume. Sent back in the other direction, either
/// coordinate can sit a checkpoint away from the one the position needs, so
/// [`ResolvedScan::resolve`] bounds a scan by cursor positions and reads the
/// coordinate only to label checkpoints.
///
/// [`ResolvedScan::resolve`]: crate::ledger_history::query_options::ResolvedScan::resolve
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

/// Where a list scan ends once it drains its resolved range, as its terminal frame renders it
/// (see [`ResolvedScan::range_end`]).
///
/// [`ResolvedScan::range_end`]: crate::ledger_history::query_options::ResolvedScan::range_end
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RangeEnd {
    /// Why the range is exhausted.
    pub exhaustion: RangeExhaustion,
    /// The terminal cursor's coordinates.
    pub position: Position,
    /// `position` in ledger coordinates (see [`ResolvedScan::end_ledger_position`]).
    ///
    /// [`ResolvedScan::end_ledger_position`]: crate::ledger_history::query_options::ResolvedScan::end_ledger_position
    pub ledger: TerminalLedgerPosition,
    /// The checkpoint a natural end claims (see [`ResolvedScan::natural_end_claim`]).
    ///
    /// [`ResolvedScan::natural_end_claim`]: crate::ledger_history::query_options::ResolvedScan::natural_end_claim
    pub natural_end_claim: Option<u64>,
}

impl RangeEnd {
    /// The terminal a scan reaches at this end.
    pub fn terminal(self) -> ScanTerminal {
        ScanTerminal::from_range_exhaustion(
            self.exhaustion,
            self.position,
            self.ledger,
            self.natural_end_claim,
        )
    }
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
        /// The frame's resume point and its cursor's public position (see
        /// [`ResolvedScan::end_ledger_position`]).
        ///
        /// [`ResolvedScan::end_ledger_position`]: crate::ledger_history::query_options::ResolvedScan::end_ledger_position
        ledger: TerminalLedgerPosition,
        /// The checkpoint this page's natural end reports as fully covered in
        /// `Watermark.checkpoint`, or `None` when nothing remains to claim: the window has no
        /// checkpoint, or an ascending scan ends at genesis. See
        /// [`ResolvedScan::natural_end_claim`].
        ///
        /// [`ResolvedScan::natural_end_claim`]: crate::ledger_history::query_options::ResolvedScan::natural_end_claim
        claimed_checkpoint: Option<u64>,
    },
    /// The caller-provided cursor bound truncates the interval at `position`.
    CursorBound {
        position: Position,
        ledger: TerminalLedgerPosition,
        kind: CursorKind,
    },
    /// A descending scan with no lower bound of its own reached this server's serving floor at
    /// `position`; everything below is pruned. It never renders as a `QueryEnd`: below the item
    /// limit, the stream sends a payload-free frame with [`Self::into_watermark`] and no `end`,
    /// then fails with the error [`Self::reason`] returns for it.
    ServingFloor {
        position: Position,
        ledger: TerminalLedgerPosition,
        floor_checkpoint: u64,
    },
}

impl ScanTerminal {
    /// The terminal a scan reaches once it exhausts its resolved interval at `position`, placed
    /// in ledger coordinates by `ledger`.
    pub fn from_range_exhaustion(
        exhaustion: RangeExhaustion,
        position: Position,
        ledger: TerminalLedgerPosition,
        claimed_checkpoint: Option<u64>,
    ) -> Self {
        match exhaustion {
            RangeExhaustion::LedgerTip => Self::NaturalRange {
                end: NaturalRangeEnd::LedgerTip,
                position,
                ledger,
                claimed_checkpoint,
            },
            RangeExhaustion::CheckpointBound => Self::NaturalRange {
                end: NaturalRangeEnd::CheckpointBound,
                position,
                ledger,
                claimed_checkpoint,
            },
            RangeExhaustion::CursorBound { kind } => Self::CursorBound {
                position,
                ledger,
                kind,
            },
            RangeExhaustion::ServingFloor { floor_checkpoint } => Self::ServingFloor {
                position,
                ledger,
                floor_checkpoint,
            },
        }
    }

    /// The `QueryEnd` reason this terminal ends its page with. [`Self::ServingFloor`] ends its page
    /// with no `QueryEnd`, so for it this is the `OutOfRange` error its stream fails with instead,
    /// after the terminal watermark frame.
    pub fn reason(&self) -> Result<QueryEndReason, RpcError> {
        match self {
            Self::ScanLimit { .. } => Ok(QueryEndReason::ScanLimit),
            Self::NaturalRange {
                end: NaturalRangeEnd::LedgerTip,
                ..
            } => Ok(QueryEndReason::LedgerTip),
            Self::NaturalRange {
                end: NaturalRangeEnd::CheckpointBound,
                ..
            } => Ok(QueryEndReason::CheckpointBound),
            Self::CursorBound { .. } => Ok(QueryEndReason::CursorBound),
            Self::ServingFloor {
                floor_checkpoint, ..
            } => Err(below_serving_floor(*floor_checkpoint)),
        }
    }

    /// Render the trailing terminal frame's watermark. Natural completion
    /// (LedgerTip/CheckpointBound) claims its `claimed_checkpoint` and ignores
    /// `covered_checkpoint_bound` (the range claim is always at least as
    /// strong). A cursor bound never claims its own checkpoint: its claim is
    /// exactly the accumulated item coverage. A serving floor claims the floor
    /// checkpoint behind a boundary cursor at the floor.
    pub fn into_watermark(self, covered_checkpoint_bound: Option<u64>) -> Watermark {
        match self {
            Self::ScanLimit { watermark } => watermark,
            Self::NaturalRange {
                position,
                ledger,
                claimed_checkpoint,
                ..
            } => cursor_watermark(position, CursorKind::Boundary, ledger, claimed_checkpoint),
            Self::CursorBound {
                position,
                ledger,
                kind,
            } => cursor_watermark(position, kind, ledger, covered_checkpoint_bound),
            Self::ServingFloor {
                position,
                ledger,
                floor_checkpoint,
            } => cursor_watermark(
                position,
                CursorKind::Boundary,
                ledger,
                Some(floor_checkpoint),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_rpc_cursor::{CursorKind, Position};

    use crate::ledger_history::query_options::assert_below_serving_floor_error;

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
    /// ordering. The resume point is above the item's key ascending, and the key descending; the
    /// cursor carries the key either way.
    #[test]
    fn item_watermark_sets_direction_matching_bound_and_resume_point() {
        let pos = Position::Transactions {
            checkpoint: 9,
            tx_seq: 42,
        };
        let key = LedgerPoint::transaction(9, 3);
        let cursor = CursorToken::item(pos)
            .with_public_position(key.public_position(&pos))
            .encode();
        let wm = item_watermark(pos, key, ScanDirection::Ascending, Some(8));
        assert_eq!(wm.checkpoint, Some(8));
        assert_eq!(wm.cursor.as_ref(), Some(&cursor));
        assert_eq!(
            wm.cursor_position,
            Some(LedgerPoint::transaction(9, 4).to_proto(&pos))
        );

        let wm = item_watermark(pos, key, ScanDirection::Descending, None);
        assert_eq!(wm.checkpoint, None);
        assert_eq!(wm.cursor.as_ref(), Some(&cursor));
        assert_eq!(wm.cursor_position, Some(key.to_proto(&pos)));

        // A checkpoint cursor carries no public position: its checkpoint is its point.
        let checkpoint = Position::Checkpoints { checkpoint: 9 };
        let wm = item_watermark(
            checkpoint,
            LedgerPoint::checkpoint_start(9),
            ScanDirection::Ascending,
            Some(9),
        );
        assert_eq!(
            wm.cursor.as_ref(),
            Some(&CursorToken::item(checkpoint).encode())
        );
    }

    #[test]
    fn scan_terminal_converts_every_range_exhaustion() {
        let position = Position::Transactions {
            checkpoint: 9,
            tx_seq: 4,
        };
        // A frame clamped past its cursor resumes at a point other than the cursor's own.
        let ledger = TerminalLedgerPosition {
            cursor_position: Some(LedgerPoint::transaction(9, 2)),
            cursor_point: Some(LedgerPoint::transaction(9, 1)),
        };
        let cases = [
            (
                RangeExhaustion::LedgerTip,
                Some(8),
                ScanTerminal::NaturalRange {
                    end: NaturalRangeEnd::LedgerTip,
                    position,
                    ledger,
                    claimed_checkpoint: Some(8),
                },
            ),
            (
                RangeExhaustion::CheckpointBound,
                None,
                ScanTerminal::NaturalRange {
                    end: NaturalRangeEnd::CheckpointBound,
                    position,
                    ledger,
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
                    ledger,
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
                    ledger,
                    kind: CursorKind::Item,
                },
            ),
            (
                RangeExhaustion::ServingFloor {
                    floor_checkpoint: 7,
                },
                Some(7),
                ScanTerminal::ServingFloor {
                    position,
                    ledger,
                    floor_checkpoint: 7,
                },
            ),
        ];

        for (exhaustion, claimed_checkpoint, expected) in cases {
            let terminal = ScanTerminal::from_range_exhaustion(
                exhaustion,
                position,
                ledger,
                claimed_checkpoint,
            );
            assert_eq!(terminal, expected);
            let watermark = terminal.into_watermark(Some(3));
            assert_eq!(
                watermark.cursor_position,
                ledger
                    .cursor_position
                    .map(|point| point.to_proto(&position))
            );
            let cursor = CursorToken::decode(&watermark.cursor.unwrap()).unwrap();
            assert_eq!(
                cursor.public_position,
                LedgerPoint::transaction(9, 1).public_position(&position)
            );
        }
    }

    /// A serving-floor terminal renders a boundary cursor at the floor position claiming the floor
    /// checkpoint, whatever coverage the items accumulated, and carries no `QueryEnd` reason: its
    /// stream fails with `OutOfRange` instead.
    #[test]
    fn scan_terminal_serving_floor_claims_the_floor_and_fails_the_stream() {
        let position = Position::Transactions {
            checkpoint: 7,
            tx_seq: 35,
        };
        let floor = LedgerPoint::checkpoint_start(7);
        let terminal = ScanTerminal::ServingFloor {
            position,
            ledger: TerminalLedgerPosition::known(floor),
            floor_checkpoint: 7,
        };

        let error = terminal
            .reason()
            .expect_err("a serving floor ends with no QueryEnd");
        assert_below_serving_floor_error(error, 7);

        for covered_checkpoint_bound in [None, Some(9)] {
            let watermark = terminal.clone().into_watermark(covered_checkpoint_bound);
            assert_eq!(
                watermark.cursor,
                Some(
                    CursorToken::boundary(position)
                        .with_public_position(floor.public_position(&position))
                        .encode()
                )
            );
            assert_eq!(watermark.checkpoint, Some(7));
            assert_eq!(watermark.cursor_position, Some(floor.to_proto(&position)));
        }
    }

    /// A natural end claims its `claimed_checkpoint` behind a boundary cursor at its position,
    /// whatever coverage the items accumulated.
    #[test]
    fn scan_terminal_natural_range_claims_its_claimed_checkpoint() {
        let position = Position::Transactions {
            checkpoint: 9,
            tx_seq: 4,
        };

        for claimed_checkpoint in [8, 9] {
            let checkpoint_bound = ScanTerminal::NaturalRange {
                end: NaturalRangeEnd::CheckpointBound,
                position,
                ledger: TerminalLedgerPosition::default(),
                claimed_checkpoint: Some(claimed_checkpoint),
            };
            assert_eq!(
                checkpoint_bound.reason().unwrap(),
                QueryEndReason::CheckpointBound
            );
            let watermark = checkpoint_bound.into_watermark(None);
            assert_eq!(
                watermark.cursor,
                Some(CursorToken::boundary(position).encode())
            );
            assert_eq!(watermark.checkpoint, Some(claimed_checkpoint));

            let ledger_tip = ScanTerminal::NaturalRange {
                end: NaturalRangeEnd::LedgerTip,
                position,
                ledger: TerminalLedgerPosition::default(),
                claimed_checkpoint: Some(claimed_checkpoint),
            };
            assert_eq!(ledger_tip.reason().unwrap(), QueryEndReason::LedgerTip);
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
                ledger: TerminalLedgerPosition::default(),
                claimed_checkpoint: None,
            };
            assert_eq!(terminal.reason().unwrap(), reason);
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
        let position = Position::Transactions {
            checkpoint: 9,
            tx_seq: 4,
        };
        let terminal = ScanTerminal::CursorBound {
            position,
            ledger: TerminalLedgerPosition::default(),
            kind: CursorKind::Boundary,
        };
        assert_eq!(terminal.reason().unwrap(), QueryEndReason::CursorBound);

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
            checkpoint: 9,
            tx_seq: 4,
            event_index: 2,
        };
        let terminal = ScanTerminal::CursorBound {
            position,
            ledger: TerminalLedgerPosition::default(),
            kind: CursorKind::Item,
        };
        assert_eq!(terminal.reason().unwrap(), QueryEndReason::CursorBound);

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
        assert_eq!(terminal.reason().unwrap(), QueryEndReason::ScanLimit);
        assert_eq!(terminal.into_watermark(None), watermark);

        let terminal = ScanTerminal::ScanLimit {
            watermark: watermark.clone(),
        };
        assert_eq!(terminal.into_watermark(Some(99)), watermark);
    }
}
