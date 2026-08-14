// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::{Bound, Range};

use bytes::Bytes;
use sui_inverted_index::ScanDirection;
use sui_rpc::proto::sui::rpc::v2::Ordering as ProtoOrdering;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2::QueryOptions as ProtoQueryOptions;
use sui_rpc_cursor::CursorToken;
use sui_rpc_cursor::Position;

use crate::ErrorReason;
use crate::RpcError;
use crate::proto::google::rpc::bad_request::FieldViolation;

const ORDERING_ASCENDING: i32 = ProtoOrdering::Ascending as i32;
const ORDERING_DESCENDING: i32 = ProtoOrdering::Descending as i32;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Ordering {
    Ascending,
    Descending,
}

/// Intra-transaction coordinate: a transaction and an index within it
/// (events today; any per-transaction-indexed lane). Boundary cursors may
/// point at slots with no item.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct IntraTxCoordinate {
    pub tx_seq: u64,
    pub event_index: u32,
}

/// Why a resolved scan interval is exhausted. Fixed at range-resolution time,
/// carried through scan state, and rendered by [`ScanTerminal`].
///
/// [`ScanTerminal`]: crate::ledger_history::watermark::ScanTerminal
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeExhaustion {
    /// Reached the currently indexed ledger tip.
    LedgerTip,
    /// Reached the requested (or implicit-genesis) checkpoint range bound.
    CheckpointBound,
    /// Truncated by a client `after`/`before` cursor. `kind` is the cursor
    /// kind the terminal cursor must carry so resume neither repeats nor
    /// skips an item (Item is preserved only for an ascending event interval
    /// made empty by an `after` Item cursor; every other site uses Boundary).
    CursorBound { kind: sui_rpc_cursor::CursorKind },
}

/// Validated, normalized form of `QueryOptions` (the proto wire type).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryOptions {
    pub limit_items: usize,
    pub ordering: Ordering,
    after: Option<CursorToken>,
    before: Option<CursorToken>,
}

/// A request's checkpoint bounds resolved into the checkpoint-sequence
/// interval to scan. Unlike [`ResolvedScan`], the
/// scan domain here *is* checkpoint space, so the entry and terminal
/// checkpoints derive from `range` directly and need no extra fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedCheckpointRange {
    /// Half-open checkpoint-sequence interval to scan (numeric order; a
    /// descending scan walks it from `end - 1` down to `start`).
    pub range: Range<u64>,
    /// Why the interval is exhausted once the scan drains it.
    pub exhaustion: RangeExhaustion,
}

/// Semantic scan bounds over a lane's scan coordinates, expressed as
/// explicit lo/hi [`Bound`]s (cursor trims need exclusive bounds on either
/// side).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScanBounds<P> {
    pub lo: Bound<P>,
    pub hi: Bound<P>,
}

/// Scan bounds over explicit event coordinates.
pub type IntraTxScanBounds = ScanBounds<IntraTxCoordinate>;

/// The terminal edge a scan reports once it drains its interval, as one
/// type-paired record. `T` is the terminal's coordinate space: the lane's
/// scan coordinate for the pure lanes, but independent of the scan bounds'
/// coordinate — the filtered checkpoint scan pairs a transaction-space
/// window with a checkpoint-space terminal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalRecord<T> {
    /// Checkpoint containing `end_coordinate`. Together they form the
    /// terminal-frame cursor position and the natural-completion coverage
    /// claim when the scan exhausts the interval.
    pub end_checkpoint: u64,
    /// The position the scan *reports* when the interval is exhausted (the
    /// terminal frame's cursor coordinate, paired with `end_checkpoint`).
    /// Range-derived until a cursor wins the terminal edge, after which it
    /// stores the cursor's RAW coordinate — those stamps set `CursorBound`
    /// and are never emitted as terminal frames, so the convention is not
    /// wire-visible. Stays pinned to the reported bound if the backend
    /// further clamps the scan bounds to available history.
    pub end_coordinate: T,
    /// Why the interval is exhausted once the scan drains it.
    pub exhaustion: RangeExhaustion,
}

/// A request's checkpoint bounds resolved into the interval to scan in a
/// lane's coordinate space `P`, annotated with the checkpoint-space facts
/// watermark rendering needs.
///
/// Two coordinate spaces meet here: `bounds` live in the lane's scan-key
/// space (checkpoint sequence, transaction sequence, or event coordinates),
/// while wire watermarks speak checkpoints — coverage claims are checkpoint
/// numbers and cursors are full `(checkpoint, position)` pairs. Mapping a
/// position back to its checkpoint takes a store lookup, so resolution
/// captures the two endpoint checkpoints up front.
///
/// The endpoint checkpoints are deliberately *not* a `Range`: they are
/// direction-relative (`entry_checkpoint` is numerically the high checkpoint
/// of a descending scan), they serve unrelated consumers (coverage clamping
/// vs. terminal-frame rendering), and only the terminal side carries a
/// companion position.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedScan<P> {
    /// The interval to scan (numeric order regardless of request ordering;
    /// a descending scan walks it from the high end down).
    pub bounds: ScanBounds<P>,
    /// Checkpoint containing the interval's first position in scan
    /// direction. Checkpoint-only because its sole consumer — the
    /// covered-bound fold — claims coverage at checkpoint granularity: a
    /// claim before it proves nothing and is suppressed, keeping the wire
    /// `checkpoint` unset until the scan's first checkpoint is fully
    /// covered.
    pub entry_checkpoint: u64,
    /// The terminal edge the scan reports once it drains the interval.
    pub terminal: TerminalRecord<P>,
}

/// A lane's scan coordinate: how a cursor token projects into the lane's
/// coordinate space, and where the lane's fenceposts sit. Resume from an
/// excluded coordinate is represented symbolically (`Bound::Excluded`) in
/// every lane — the item after transaction N is trivially N + 1, but the
/// representation is kept and the store edge resolves it
/// ([`ScanBounds::to_range`] for scalars, the packed event range for
/// event coordinates).
pub trait ScanCoordinate: Copy + Ord {
    fn from_cursor(cursor: &CursorToken) -> Self;
    /// The lane coordinate at scan-unit fencepost `boundary` (a checkpoint- or
    /// transaction-sequence edge): scalars are their own fenceposts; an event
    /// boundary is the first event slot of the transaction.
    fn from_boundary(boundary: u64) -> Self;
}

/// A request's checkpoint bounds, validated and clamped to the indexed tip. `start..end` is an
/// Ordering-agnostic ascending-normalized half-open interval. `high_exhaustion` records why the
/// high edge stops where it does (explicit `end_checkpoint` vs. the tip clamp). The low edge is
/// always the caller's `start_checkpoint`, so its reason needs no field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CheckpointRange {
    start: u64,
    end: u64,
    high_exhaustion: RangeExhaustion,
    indexed_tip: u64,
}

/// What the pure bounds clamp did, consumed by terminal attribution: which
/// cursor tightened its edge, and where emptiness appeared.
/// `after_left_empty` is checked before the `before` clamp applies — the
/// after echo may only claim a collapse it caused alone.
struct BoundsClampReport {
    after_won: bool,
    after_left_empty: bool,
    before_won: bool,
    empty: bool,
}

impl IntraTxCoordinate {
    /// Fencepost at the first event slot of `tx_seq`; valid as a boundary even
    /// if the transaction has no events.
    pub fn start_of_tx(tx_seq: u64) -> Self {
        Self {
            tx_seq,
            event_index: 0,
        }
    }
}

impl RangeExhaustion {
    pub fn reason(self) -> QueryEndReason {
        match self {
            Self::LedgerTip => QueryEndReason::LedgerTip,
            Self::CheckpointBound => QueryEndReason::CheckpointBound,
            Self::CursorBound { .. } => QueryEndReason::CursorBound,
        }
    }
}

impl QueryOptions {
    pub fn checkpoints_from_proto(
        request: Option<&ProtoQueryOptions>,
        default_limit_items: u32,
        max_limit_items: u32,
    ) -> Result<Self, RpcError> {
        Self::from_proto_with_position(request, default_limit_items, max_limit_items, |position| {
            matches!(position, Position::Checkpoints { .. })
        })
    }

    pub fn transactions_from_proto(
        request: Option<&ProtoQueryOptions>,
        default_limit_items: u32,
        max_limit_items: u32,
    ) -> Result<Self, RpcError> {
        Self::from_proto_with_position(request, default_limit_items, max_limit_items, |position| {
            matches!(position, Position::Transactions { .. })
        })
    }

    pub fn events_from_proto(
        request: Option<&ProtoQueryOptions>,
        default_limit_items: u32,
        max_limit_items: u32,
    ) -> Result<Self, RpcError> {
        Self::from_proto_with_position(request, default_limit_items, max_limit_items, |position| {
            matches!(position, Position::Events { .. })
        })
    }

    fn from_proto_with_position(
        request: Option<&ProtoQueryOptions>,
        default_limit_items: u32,
        max_limit_items: u32,
        position_matches: fn(&Position) -> bool,
    ) -> Result<Self, RpcError> {
        let limit_items = request
            .and_then(|options| options.limit)
            .unwrap_or(default_limit_items)
            .clamp(1, max_limit_items) as usize;

        let ordering = match request.and_then(|options| options.ordering) {
            None | Some(ORDERING_ASCENDING) => Ordering::Ascending,
            Some(ORDERING_DESCENDING) => Ordering::Descending,
            Some(_) => {
                return Err(FieldViolation::new("options.ordering")
                    .with_description("invalid ordering")
                    .with_reason(ErrorReason::FieldInvalid)
                    .into());
            }
        };

        let after = parse_cursor(
            "options.after",
            request.and_then(|options| options.after.as_ref()),
            position_matches,
        )?;
        let before = parse_cursor(
            "options.before",
            request.and_then(|options| options.before.as_ref()),
            position_matches,
        )?;

        Ok(Self {
            limit_items,
            ordering,
            after,
            before,
        })
    }

    /// Options for a subscription stream: an unbounded ascending scan with no
    /// cursor bounds. The ascending direction is consulted by the watermark
    /// builders; `limit_items` is irrelevant.
    pub fn subscription() -> Self {
        Self {
            limit_items: usize::MAX,
            ordering: Ordering::Ascending,
            after: None,
            before: None,
        }
    }

    pub fn scan_direction(&self) -> ScanDirection {
        match self.ordering {
            Ordering::Ascending => ScanDirection::Ascending,
            Ordering::Descending => ScanDirection::Descending,
        }
    }

    pub fn is_ascending(&self) -> bool {
        matches!(self.ordering, Ordering::Ascending)
    }

    /// Whether the request explicitly positioned the low end of the scan via an
    /// `after` cursor. `apply_cursor_bounds` only ever raises the low bound from
    /// `after` (in both orderings); `before` bounds the high end. Together with an
    /// explicit `start_checkpoint`, this lets the pruning-floor check distinguish
    /// "resume/start from here" (error if below the floor — the data is gone) from
    /// an open-ended low end (clamp up to the floor).
    pub fn has_after_cursor(&self) -> bool {
        self.after.is_some()
    }

    /// Tighten the resolved scan by the request's cursors. Decoding splits
    /// each cursor into its two products: the bound it imposes on the
    /// window (role-selected — resume for `after`, limit for `before`;
    /// exclusions stay symbolic for the store edge to resolve) for
    /// [`clamp_scan_bounds`], and its candidate stamp — the
    /// [`TerminalRecord`] the scan reports IF this cursor ends it — for
    /// [`attribute_cursor_terminal`] to install or discard. The stamp's
    /// kind is decidable at decode because the ordering is known: only an
    /// ascending window emptied by an `after` Item echoes the Item kind
    /// (resume must not re-include the delivered row); every other stamp is
    /// Boundary. An installed cursor stamp is always `CursorBound` — no
    /// path reports a cursor win as any other exhaustion.
    /// Project the store-space `range` under the resolved checkpoint window
    /// and apply the cursors — the endpoints' one entry point past
    /// checkpoint-range resolution, for every lane coordinate `P`.
    pub fn resolve_scan<P: ScanCoordinate>(
        &self,
        cp_range: ResolvedCheckpointRange,
        range: Range<u64>,
    ) -> ResolvedScan<P> {
        self.apply_cursor_bounds(cp_range.with_range(range, self.ordering))
    }

    fn apply_cursor_bounds<P: ScanCoordinate>(
        &self,
        mut resolved: ResolvedScan<P>,
    ) -> ResolvedScan<P> {
        if resolved.is_empty() {
            return resolved;
        }

        let ascending = matches!(self.ordering, Ordering::Ascending);
        let after = self.after.as_ref().map(|cursor| {
            let coordinate = P::from_cursor(cursor);
            let kind = if ascending {
                cursor.kind
            } else {
                sui_rpc_cursor::CursorKind::Boundary
            };
            (
                cursor.kind.resume_bound(coordinate),
                TerminalRecord {
                    end_checkpoint: cursor.position.checkpoint(),
                    end_coordinate: coordinate,
                    exhaustion: RangeExhaustion::CursorBound { kind },
                },
            )
        });
        let before = self.before.as_ref().map(|cursor| {
            let coordinate = P::from_cursor(cursor);
            (
                cursor.kind.limit_bound(coordinate),
                TerminalRecord {
                    end_checkpoint: cursor.position.checkpoint(),
                    end_coordinate: coordinate,
                    exhaustion: RangeExhaustion::CursorBound {
                        kind: sui_rpc_cursor::CursorKind::Boundary,
                    },
                },
            )
        });

        let report = clamp_scan_bounds(
            &mut resolved.bounds,
            after.as_ref().map(|(bound, _)| *bound),
            before.as_ref().map(|(bound, _)| *bound),
        );
        attribute_cursor_terminal(
            self.ordering,
            after.as_ref().map(|(_, stamp)| stamp),
            before.as_ref().map(|(_, stamp)| stamp),
            &report,
            &mut resolved.entry_checkpoint,
            &mut resolved.terminal,
        );
        if report.empty {
            // Canonical empty form everywhere in this module: the interval
            // collapses onto its reported terminal bound.
            resolved.bounds = ScanBounds::empty_at(resolved.terminal.end_coordinate);
        }
        resolved
    }
}

impl ResolvedCheckpointRange {
    /// Validate and tip-clamp the requested window, then narrow it by the
    /// cursors' checkpoint hints — request to resolved window in one call.
    pub fn from_request(
        start_checkpoint: Option<u64>,
        end_checkpoint: Option<u64>,
        checkpoint_hi_exclusive: u64,
        options: &QueryOptions,
    ) -> Result<Self, RpcError> {
        Ok(CheckpointRange::from_request(
            start_checkpoint,
            end_checkpoint,
            checkpoint_hi_exclusive,
        )?
        .resolve(options))
    }

    pub fn empty_at(checkpoint: u64, exhaustion: RangeExhaustion) -> Self {
        Self {
            range: checkpoint..checkpoint,
            exhaustion,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.range.is_empty()
    }

    pub fn terminal_checkpoint(&self, ordering: Ordering) -> u64 {
        match ordering {
            Ordering::Ascending => self.range.end,
            Ordering::Descending => self.range.start,
        }
    }

    /// Attach the checkpoint-space watermark metadata to a lane's projected
    /// scan interval. `range` is the interval in scan-unit fenceposts — not
    /// about tx_sequence_number or cp_sequence_number specifically, just a
    /// u64 fencepost space — and [`ScanCoordinate::from_boundary`] maps
    /// each fencepost into the lane's coordinates (identity for scalars, the
    /// start-of-tx event slot for events).
    fn with_range<P: ScanCoordinate>(
        self,
        range: Range<u64>,
        ordering: Ordering,
    ) -> ResolvedScan<P> {
        let entry_checkpoint = match ordering {
            Ordering::Ascending => self.range.start,
            Ordering::Descending => self.range.end.saturating_sub(1),
        };
        let terminal = self.terminal_record(&range, ordering);
        ResolvedScan {
            bounds: ScanBounds {
                lo: Bound::Included(P::from_boundary(range.start)),
                hi: Bound::Excluded(P::from_boundary(range.end)),
            },
            entry_checkpoint,
            terminal,
        }
    }

    /// The metadata attachment IS the terminal constructor: the terminal
    /// edge of a scan projected onto `range` fenceposts.
    fn terminal_record<T: ScanCoordinate>(
        &self,
        range: &Range<u64>,
        ordering: Ordering,
    ) -> TerminalRecord<T> {
        let end_coordinate = T::from_boundary(match ordering {
            Ordering::Ascending => range.end,
            Ordering::Descending => range.start,
        });
        TerminalRecord {
            end_checkpoint: self.terminal_checkpoint(ordering),
            end_coordinate,
            exhaustion: self.exhaustion,
        }
    }
}

impl CheckpointRange {
    fn from_request(
        start_checkpoint: Option<u64>,
        end_checkpoint: Option<u64>,
        checkpoint_hi_exclusive: u64,
    ) -> Result<Self, RpcError> {
        let start = start_checkpoint.unwrap_or(0);
        if let Some(end) = end_checkpoint
            && end < start
        {
            return Err(FieldViolation::new("end_checkpoint")
                .with_description(
                    "end_checkpoint must be greater than or equal to start_checkpoint",
                )
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }

        let requested_end = end_checkpoint.unwrap_or(checkpoint_hi_exclusive);
        let high_exhaustion = if end_checkpoint.is_none() || requested_end > checkpoint_hi_exclusive
        {
            RangeExhaustion::LedgerTip
        } else {
            RangeExhaustion::CheckpointBound
        };
        let end = requested_end.min(checkpoint_hi_exclusive);

        Ok(Self {
            start,
            end,
            high_exhaustion,
            indexed_tip: checkpoint_hi_exclusive,
        })
    }

    /// Clamped scan-window start, paired with the terminal reason to report if the scan drains into
    /// this edge (descending). The `after` cursor's checkpoint is treated as an inclusive start as
    /// the underlying item's cursor may be sub-checkpoint information (e.g transactions or events
    /// of a checkpoint.) Finer-granularity exclusivity should be handled downstream.
    fn clamp_start_cp(&self, options: &QueryOptions) -> (u64, RangeExhaustion) {
        match &options.after {
            Some(cursor) if cursor.position.checkpoint() >= self.start => (
                cursor.position.checkpoint(),
                RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
            ),
            _ => (self.start, RangeExhaustion::CheckpointBound),
        }
    }

    /// Clamped scan-window end, paired with the terminal reason to report if the scan drains into
    /// this edge (ascending); when the cursor doesn't win, the reason is the request-derived one
    /// from construction (explicit end vs. tip clamp). The `before` cursor always enters as an
    /// exclusive end (the window is half-open): an Item's checkpoint may still hold admissible
    /// items before it, so it stays in range (`cp + 1`); a Boundary's checkpoint is already an
    /// exclusive upper (descending frontiers are emitted pre-bumped).
    fn clamp_end_cp(&self, options: &QueryOptions) -> (u64, RangeExhaustion) {
        let upper = options
            .before
            .as_ref()
            .and_then(|cursor| match cursor.kind {
                sui_rpc_cursor::CursorKind::Item => cursor.position.checkpoint().checked_add(1),
                sui_rpc_cursor::CursorKind::Boundary => Some(cursor.position.checkpoint()),
            });
        match upper {
            Some(upper) if upper <= self.end => (
                upper,
                RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
            ),
            _ => (self.end, self.high_exhaustion),
        }
    }

    /// Tighten the interval by the request's cursors and attribute the exhaustion reason the scan
    /// will report once it drains it. Also designates the terminal edge and consequently which
    /// exhaustion reason the client sees.
    fn resolve(self, options: &QueryOptions) -> ResolvedCheckpointRange {
        let (start, low_exhaustion) = self.clamp_start_cp(options);
        let (end, high_exhaustion) = self.clamp_end_cp(options);

        if start >= self.indexed_tip {
            return ResolvedCheckpointRange::empty_at(self.indexed_tip, RangeExhaustion::LedgerTip);
        }
        if start >= end {
            // A cursor-collapsed interval reports CursorBound no matter which
            // edge is terminal: the paging itself consumed the range.
            let cursor_bound = matches!(low_exhaustion, RangeExhaustion::CursorBound { .. })
                || matches!(high_exhaustion, RangeExhaustion::CursorBound { .. });
            let exhaustion = if cursor_bound {
                RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                }
            } else {
                match options.ordering {
                    Ordering::Ascending => high_exhaustion,
                    Ordering::Descending => low_exhaustion,
                }
            };
            let checkpoint = match options.ordering {
                Ordering::Ascending => end,
                Ordering::Descending => start,
            };
            return ResolvedCheckpointRange::empty_at(checkpoint, exhaustion);
        }

        // Terminal-edge selection.
        let exhaustion = match options.ordering {
            Ordering::Ascending => high_exhaustion,
            Ordering::Descending => low_exhaustion,
        };
        ResolvedCheckpointRange {
            range: start..end,
            exhaustion,
        }
    }
}

impl ScanBounds<u64> {
    pub fn from_range(range: Range<u64>) -> Self {
        Self {
            lo: Bound::Included(range.start),
            hi: Bound::Excluded(range.end),
        }
    }

    /// Collapse symbolic scalar bounds into the store's half-open range —
    /// where resume-from-excluded resolves for the scalar lanes: the item
    /// after N is N + 1 (the scalar analogue of the packed event range's
    /// encode-plus-one). `Excluded(u64::MAX)` has no successor and yields
    /// an empty range. This is also where a cursor-collapsed dense window
    /// becomes empty: resolution's generic emptiness predicate cannot see
    /// that no integer lies strictly between N and N + 1.
    pub fn to_range(self) -> Range<u64> {
        let end = match self.hi {
            Bound::Excluded(hi) => hi,
            Bound::Included(hi) => hi.saturating_add(1),
            Bound::Unbounded => u64::MAX,
        };
        let start = match self.lo {
            Bound::Included(lo) => lo,
            Bound::Excluded(lo) => match lo.checked_add(1) {
                Some(successor) => successor,
                None => return end..end,
            },
            Bound::Unbounded => 0,
        };
        start..end
    }
}

impl ScanBounds<IntraTxCoordinate> {
    pub fn tx_span(start_tx: u64, end_tx: u64) -> Self {
        Self {
            lo: Bound::Included(IntraTxCoordinate::start_of_tx(start_tx)),
            hi: Bound::Excluded(IntraTxCoordinate::start_of_tx(end_tx)),
        }
    }

    /// Smallest half-open tx range covering every position these bounds could
    /// admit. An exclusive `hi` at the start of tx N excludes tx N entirely;
    /// any other bounded endpoint keeps its transaction, since earlier events
    /// of that tx may still be in bounds. `None` when no tx can qualify.
    pub fn tx_range(&self) -> Option<Range<u64>> {
        let start_tx = match self.lo {
            Bound::Included(position) | Bound::Excluded(position) => position.tx_seq,
            Bound::Unbounded => 0,
        };
        let end_tx = match self.hi {
            Bound::Excluded(position) if position.event_index == 0 => position.tx_seq,
            Bound::Included(position) | Bound::Excluded(position) => {
                position.tx_seq.saturating_add(1)
            }
            Bound::Unbounded => u64::MAX,
        };
        (start_tx < end_tx).then_some(start_tx..end_tx)
    }
}

impl ScanCoordinate for u64 {
    fn from_cursor(cursor: &CursorToken) -> Self {
        cursor
            .position
            .scalar()
            .unwrap_or_else(|| unreachable!("validated at decode"))
    }

    fn from_boundary(boundary: u64) -> Self {
        boundary
    }
}

impl ScanCoordinate for IntraTxCoordinate {
    fn from_cursor(cursor: &CursorToken) -> Self {
        cursor
            .position
            .intra_tx()
            .map(IntraTxCoordinate::from)
            .unwrap_or_else(|| unreachable!("validated at decode"))
    }

    fn from_boundary(boundary: u64) -> Self {
        Self::start_of_tx(boundary)
    }
}

impl From<IntraTxCoordinate> for (u64, u32) {
    fn from(position: IntraTxCoordinate) -> Self {
        (position.tx_seq, position.event_index)
    }
}

impl From<(u64, u32)> for IntraTxCoordinate {
    fn from((tx_seq, event_index): (u64, u32)) -> Self {
        Self {
            tx_seq,
            event_index,
        }
    }
}

impl<P: Copy + Ord> ScanBounds<P> {
    pub fn empty_at(position: P) -> Self {
        Self {
            lo: Bound::Included(position),
            hi: Bound::Excluded(position),
        }
    }

    pub fn is_empty(&self) -> bool {
        bounds_empty(self.lo, self.hi)
    }

    pub fn contains(&self, position: P) -> bool {
        let above_lo = match self.lo {
            Bound::Included(lo) => position >= lo,
            Bound::Excluded(lo) => position > lo,
            Bound::Unbounded => true,
        };
        let below_hi = match self.hi {
            Bound::Included(hi) => position <= hi,
            Bound::Excluded(hi) => position < hi,
            Bound::Unbounded => true,
        };
        above_lo && below_hi
    }

    /// Clamp the low edge to the available range's low fencepost — the max
    /// of the current low bound and `Included(floor)`, the same lattice op
    /// as an `after`-cursor clamp. Returns true when availability consumes
    /// the whole window: nothing is scannable and the bounds are untouched,
    /// so the caller canonicalizes to its own empty form.
    pub fn clamp_to_available_lo(&mut self, floor: P) -> bool {
        let floored_lo = Bound::Included(floor);
        if bounds_empty(floored_lo, self.hi) {
            return true;
        }
        if lower_bound_gte(floored_lo, self.lo) {
            self.lo = floored_lo;
        }
        false
    }
}

impl<T: Copy> TerminalRecord<T> {
    /// The watermark half of a serving-floor clamp whose floor did NOT
    /// empty the window: an ascending scan must not claim coverage below
    /// the floor (entry rises), a descending scan terminates at it
    /// (terminal pinned) — so no watermark ever claims unscanned history.
    /// The floor arrives as a (checkpoint, terminal-coordinate) pair
    /// because the scan bounds and the terminal may live in different
    /// coordinate spaces.
    pub fn reconcile_floor(
        &mut self,
        entry_checkpoint: &mut u64,
        floor_checkpoint: u64,
        floor_position: T,
        ascending: bool,
    ) {
        if ascending {
            *entry_checkpoint = (*entry_checkpoint).max(floor_checkpoint);
        } else {
            self.end_checkpoint = floor_checkpoint;
            self.end_coordinate = floor_position;
        }
    }
}

impl<P: Copy + Ord> ResolvedScan<P> {
    pub fn empty_at(end_checkpoint: u64, end_coordinate: P, exhaustion: RangeExhaustion) -> Self {
        Self {
            bounds: ScanBounds::empty_at(end_coordinate),
            entry_checkpoint: end_checkpoint,
            terminal: TerminalRecord {
                end_checkpoint,
                end_coordinate,
                exhaustion,
            },
        }
    }

    pub fn is_empty(&self) -> bool {
        self.bounds.is_empty()
    }

    /// Reconcile the interval and its watermark metadata after the backend
    /// clamped the interval's low end to the serving floor (`floor_position`
    /// = the effective first scannable position, `floor_checkpoint` = its
    /// containing checkpoint).
    ///
    /// A floor inside the interval starts the scan there: an ascending scan
    /// must not claim coverage below it (entry rises), and a descending scan
    /// terminates at it (terminal pinned), so no watermark ever claims
    /// unscanned history. A floor that empties the interval leaves an empty
    /// intersection with retained history: the interval canonicalizes to
    /// empty and the terminal metadata stays at the requested boundary — the
    /// cursor must not move outside the requested interval, and the empty
    /// interval already claims nothing.
    pub fn apply_serving_floor(
        &mut self,
        floor_position: P,
        floor_checkpoint: u64,
        options: &QueryOptions,
    ) {
        if self.bounds.clamp_to_available_lo(floor_position) {
            // Canonical empty form everywhere in this module: the interval
            // collapses onto its reported terminal bound.
            self.bounds = ScanBounds::empty_at(self.terminal.end_coordinate);
            return;
        }
        self.terminal.reconcile_floor(
            &mut self.entry_checkpoint,
            floor_checkpoint,
            floor_position,
            options.is_ascending(),
        );
    }
}

/// The pure bounds half of cursor application, shared by every lane: an
/// `after` cursor may raise the low edge, a `before` cursor may lower the
/// high edge. A cursor "wins" only when its bound is at least as tight as
/// the window's current edge — ties go to the cursor, so a cursor
/// coinciding with a range edge still claims terminal attribution.
/// Returns the report terminal attribution consumes.
fn clamp_scan_bounds<P: Copy + Ord>(
    bounds: &mut ScanBounds<P>,
    after_lo: Option<Bound<P>>,
    before_hi: Option<Bound<P>>,
) -> BoundsClampReport {
    let mut after_won = false;
    let mut after_left_empty = false;
    if let Some(lo) = after_lo
        && lower_bound_gte(lo, bounds.lo)
    {
        bounds.lo = lo;
        after_won = true;
        after_left_empty = bounds.is_empty();
    }
    let mut before_won = false;
    if let Some(hi) = before_hi
        && upper_bound_lte(hi, bounds.hi)
    {
        bounds.hi = hi;
        before_won = true;
    }
    BoundsClampReport {
        after_won,
        after_left_empty,
        before_won,
        empty: bounds.is_empty(),
    }
}

/// Turn the bounds clamp's report into watermark metadata — the policy
/// half of cursor application. Each cursor arrives as its candidate stamp
/// (built at decode with the correct kind); this function only installs or
/// discards whole records: a terminal-edge winner (descending `after`,
/// ascending `before`) installs its stamp on a nonempty window; a
/// cursor-collapsed window installs the last-recorded winner (`before`
/// over `after`). Stamps carry the cursor's RAW coordinate and kind so a
/// stamped resume cursor reproduces the client's cursor exactly — resume
/// must neither repeat nor skip an item. Entry checkpoints advance on
/// cursor presence alone, win or lose.
fn attribute_cursor_terminal<P: Copy>(
    ordering: Ordering,
    after: Option<&TerminalRecord<P>>,
    before: Option<&TerminalRecord<P>>,
    report: &BoundsClampReport,
    entry_checkpoint: &mut u64,
    terminal: &mut TerminalRecord<P>,
) {
    let ascending = matches!(ordering, Ordering::Ascending);
    let mut cursor_terminal = None;

    if let Some(stamp) = after {
        if ascending {
            *entry_checkpoint = (*entry_checkpoint).max(stamp.end_checkpoint);
        }
        if report.after_won {
            if !ascending || report.after_left_empty {
                cursor_terminal = Some(stamp);
            }
            if !ascending {
                *terminal = *stamp;
            }
        }
    }

    if let Some(stamp) = before {
        if !ascending {
            *entry_checkpoint = (*entry_checkpoint).min(stamp.end_checkpoint);
        }
        if report.before_won {
            if ascending || report.empty {
                cursor_terminal = Some(stamp);
            }
            if ascending {
                *terminal = *stamp;
            }
        }
    }

    if report.empty {
        if let Some(stamp) = cursor_terminal {
            *terminal = *stamp;
        } else if after.is_some() || before.is_some() {
            // The lone partial write: a window that was already empty when
            // the cursors lost their clamps re-attributes the reason
            // without moving the coordinates.
            terminal.exhaustion = RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            };
        }
    }
}

/// Whether an explicit lo/hi bound pair admits no position.
fn bounds_empty<P: Copy + Ord>(lo: Bound<P>, hi: Bound<P>) -> bool {
    match (lo, hi) {
        (Bound::Included(a), Bound::Excluded(b))
        | (Bound::Excluded(a), Bound::Excluded(b))
        | (Bound::Excluded(a), Bound::Included(b)) => a >= b,
        (Bound::Included(a), Bound::Included(b)) => a > b,
        (Bound::Unbounded, _) | (_, Bound::Unbounded) => false,
    }
}

/// Whether `candidate` is at least as tight a lower bound as `current`
/// (Unbounded is loosest; at equal positions Excluded is the tighter LOWER
/// bound, hence its higher key rank).
fn lower_bound_gte<P: Copy + Ord>(candidate: Bound<P>, current: Bound<P>) -> bool {
    let Some(candidate) = lower_bound_key(candidate) else {
        return false;
    };
    match lower_bound_key(current) {
        Some(current) => candidate >= current,
        None => true,
    }
}

fn lower_bound_key<P: Copy + Ord>(bound: Bound<P>) -> Option<(P, u8)> {
    match bound {
        Bound::Included(position) => Some((position, 0)),
        Bound::Excluded(position) => Some((position, 1)),
        Bound::Unbounded => None,
    }
}

/// Whether `candidate` is at least as tight an upper bound as `current` —
/// the dual of [`lower_bound_gte`]: Unbounded is loosest; at equal
/// positions Excluded is the tighter UPPER bound, hence its lower key rank
/// and the flipped comparison.
fn upper_bound_lte<P: Copy + Ord>(candidate: Bound<P>, current: Bound<P>) -> bool {
    let Some(candidate) = upper_bound_key(candidate) else {
        return false;
    };
    match upper_bound_key(current) {
        Some(current) => candidate <= current,
        None => true,
    }
}

fn upper_bound_key<P: Copy + Ord>(bound: Bound<P>) -> Option<(P, u8)> {
    match bound {
        Bound::Excluded(position) => Some((position, 0)),
        Bound::Included(position) => Some((position, 1)),
        Bound::Unbounded => None,
    }
}

fn parse_cursor(
    field: &'static str,
    cursor: Option<&Bytes>,
    position_matches: fn(&Position) -> bool,
) -> Result<Option<CursorToken>, RpcError> {
    cursor
        .map(|cursor| {
            CursorToken::decode(cursor).map_err(|_| invalid_cursor(field, "invalid cursor"))
        })
        .transpose()?
        .map(|token| {
            if position_matches(&token.position) {
                Ok(token)
            } else {
                Err(invalid_cursor(field, "invalid cursor"))
            }
        })
        .transpose()
}

fn invalid_cursor(field: &'static str, description: impl Into<String>) -> RpcError {
    FieldViolation::new(field)
        .with_description(description)
        .with_reason(ErrorReason::FieldInvalid)
        .into()
}

#[cfg(test)]
mod tests {
    use sui_rpc_cursor::CursorKind;

    use super::*;

    fn query_options_from_proto(
        request: Option<&ProtoQueryOptions>,
    ) -> Result<QueryOptions, RpcError> {
        QueryOptions::transactions_from_proto(request, 100, 1_000)
    }

    fn resolved_range(range: Range<u64>) -> ResolvedScan<u64> {
        ResolvedScan {
            bounds: ScanBounds::from_range(range),
            entry_checkpoint: 0,
            terminal: TerminalRecord {
                end_checkpoint: 20,
                end_coordinate: 20,
                exhaustion: RangeExhaustion::CheckpointBound,
            },
        }
    }

    fn tx_item(checkpoint: u64, tx_seq: u64) -> CursorToken {
        CursorToken::item(Position::Transactions { checkpoint, tx_seq })
    }

    fn tx_boundary(checkpoint: u64, tx_seq: u64) -> CursorToken {
        CursorToken::boundary(Position::Transactions { checkpoint, tx_seq })
    }

    fn cp_item(checkpoint: u64) -> CursorToken {
        CursorToken::item(Position::Checkpoints { checkpoint })
    }

    fn directional_options(ascending: bool) -> QueryOptions {
        let mut request = ProtoQueryOptions::default();
        if !ascending {
            request.ordering = Some(ProtoOrdering::Descending as i32);
        }
        query_options_from_proto(Some(&request)).unwrap()
    }

    /// Serving-floor reconciliation for tx intervals: a floor inside the
    /// interval moves the scan start and the direction-relevant watermark
    /// metadata; a floor at/past the high end empties the intersection, and
    /// the terminal metadata stays at the requested boundary instead of
    /// moving to a floor outside the requested interval.
    #[test]
    fn tx_serving_floor_reconciles_or_canonicalizes_empty() {
        let asc = directional_options(true);
        let desc = directional_options(false);

        // Ascending, floor inside 0..100: scan starts at the floor; the
        // entry claim rises to the floor checkpoint; terminal untouched.
        let mut resolved = ResolvedScan {
            bounds: ScanBounds::from_range(0..100),
            entry_checkpoint: 0,
            terminal: TerminalRecord {
                end_checkpoint: 20,
                end_coordinate: 100,
                exhaustion: RangeExhaustion::CheckpointBound,
            },
        };
        resolved.apply_serving_floor(50, 10, &asc);
        assert_eq!(resolved.bounds.to_range(), 50..100);
        assert_eq!(resolved.entry_checkpoint, 10);
        assert_eq!(resolved.terminal.end_checkpoint, 20);
        assert_eq!(resolved.terminal.end_coordinate, 100);

        // Descending, floor inside: entry (the high edge) untouched; the
        // terminal pins to the floor.
        let mut resolved = ResolvedScan {
            bounds: ScanBounds::from_range(0..100),
            entry_checkpoint: 20,
            terminal: TerminalRecord {
                end_checkpoint: 0,
                end_coordinate: 0,
                exhaustion: RangeExhaustion::CheckpointBound,
            },
        };
        resolved.apply_serving_floor(50, 10, &desc);
        assert_eq!(resolved.bounds.to_range(), 50..100);
        assert_eq!(resolved.entry_checkpoint, 20);
        assert_eq!(resolved.terminal.end_checkpoint, 10);
        assert_eq!(resolved.terminal.end_coordinate, 50);

        // Floor at/past the high end (covers the == boundary), both
        // directions: empty intersection, canonicalized at the reported
        // terminal bound; no metadata moves. Descending, the terminal must
        // NOT move to the floor (checkpoint 10 lies outside the requested
        // interval).
        for floor_tx in [40, 50] {
            let mut resolved = ResolvedScan {
                bounds: ScanBounds::from_range(0..40),
                entry_checkpoint: 0,
                terminal: TerminalRecord {
                    end_checkpoint: 8,
                    end_coordinate: 40,
                    exhaustion: RangeExhaustion::CheckpointBound,
                },
            };
            resolved.apply_serving_floor(floor_tx, 10, &asc);
            assert!(resolved.is_empty());
            assert_eq!(resolved.bounds.to_range(), 40..40);
            assert_eq!(resolved.entry_checkpoint, 0);
            assert_eq!(resolved.terminal.end_checkpoint, 8);
            assert_eq!(resolved.terminal.end_coordinate, 40);

            let mut resolved = ResolvedScan {
                bounds: ScanBounds::from_range(0..40),
                entry_checkpoint: 8,
                terminal: TerminalRecord {
                    end_checkpoint: 0,
                    end_coordinate: 0,
                    exhaustion: RangeExhaustion::CheckpointBound,
                },
            };
            resolved.apply_serving_floor(floor_tx, 10, &desc);
            assert!(resolved.is_empty());
            assert_eq!(resolved.bounds.to_range(), 0..0);
            assert_eq!(resolved.entry_checkpoint, 8);
            assert_eq!(resolved.terminal.end_checkpoint, 0);
            assert_eq!(resolved.terminal.end_coordinate, 0);
        }
    }

    /// The event-lane serving floor mirrors the tx behavior in
    /// event coordinates.
    #[test]
    fn event_serving_floor_reconciles_or_canonicalizes_empty() {
        let asc = directional_options(true);
        let desc = directional_options(false);

        // Ascending, floor inside: low bound moves, entry rises, terminal
        // untouched.
        let mut resolved = ResolvedScan {
            bounds: IntraTxScanBounds::tx_span(0, 100),
            entry_checkpoint: 0,
            terminal: TerminalRecord {
                end_checkpoint: 20,
                end_coordinate: IntraTxCoordinate::start_of_tx(100),
                exhaustion: RangeExhaustion::CheckpointBound,
            },
        };
        resolved.apply_serving_floor(IntraTxCoordinate::start_of_tx(50), 10, &asc);
        assert_eq!(
            resolved.bounds.lo,
            Bound::Included(IntraTxCoordinate::start_of_tx(50))
        );
        assert_eq!(resolved.entry_checkpoint, 10);
        assert_eq!(resolved.terminal.end_checkpoint, 20);
        assert_eq!(
            resolved.terminal.end_coordinate,
            IntraTxCoordinate::start_of_tx(100)
        );

        // Descending, floor inside: terminal pins to the floor.
        let mut resolved = ResolvedScan {
            bounds: IntraTxScanBounds::tx_span(0, 100),
            entry_checkpoint: 20,
            terminal: TerminalRecord {
                end_checkpoint: 0,
                end_coordinate: IntraTxCoordinate::start_of_tx(0),
                exhaustion: RangeExhaustion::CheckpointBound,
            },
        };
        resolved.apply_serving_floor(IntraTxCoordinate::start_of_tx(50), 10, &desc);
        assert_eq!(
            resolved.bounds.lo,
            Bound::Included(IntraTxCoordinate::start_of_tx(50))
        );
        assert_eq!(resolved.entry_checkpoint, 20);
        assert_eq!(resolved.terminal.end_checkpoint, 10);
        assert_eq!(
            resolved.terminal.end_coordinate,
            IntraTxCoordinate::start_of_tx(50)
        );

        // Floor that empties the bounds (covers the == boundary), both
        // directions: canonical empty at the reported terminal bound, no
        // metadata moves.
        for floor_tx in [40, 50] {
            let mut resolved = ResolvedScan {
                bounds: IntraTxScanBounds::tx_span(0, 40),
                entry_checkpoint: 0,
                terminal: TerminalRecord {
                    end_checkpoint: 8,
                    end_coordinate: IntraTxCoordinate::start_of_tx(40),
                    exhaustion: RangeExhaustion::CheckpointBound,
                },
            };
            resolved.apply_serving_floor(IntraTxCoordinate::start_of_tx(floor_tx), 10, &asc);
            assert!(resolved.is_empty());
            assert_eq!(
                resolved.bounds,
                IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(40))
            );
            assert_eq!(resolved.entry_checkpoint, 0);
            assert_eq!(resolved.terminal.end_checkpoint, 8);
            assert_eq!(
                resolved.terminal.end_coordinate,
                IntraTxCoordinate::start_of_tx(40)
            );

            let mut resolved = ResolvedScan {
                bounds: IntraTxScanBounds::tx_span(0, 40),
                entry_checkpoint: 8,
                terminal: TerminalRecord {
                    end_checkpoint: 0,
                    end_coordinate: IntraTxCoordinate::start_of_tx(0),
                    exhaustion: RangeExhaustion::CheckpointBound,
                },
            };
            resolved.apply_serving_floor(IntraTxCoordinate::start_of_tx(floor_tx), 10, &desc);
            assert!(resolved.is_empty());
            assert_eq!(
                resolved.bounds,
                IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(0))
            );
            assert_eq!(resolved.entry_checkpoint, 8);
            assert_eq!(resolved.terminal.end_checkpoint, 0);
            assert_eq!(
                resolved.terminal.end_coordinate,
                IntraTxCoordinate::start_of_tx(0)
            );
        }
    }

    #[test]
    fn tx_range_covers_partial_endpoint_transactions() {
        let bounds = IntraTxScanBounds {
            lo: Bound::Included(IntraTxCoordinate {
                tx_seq: 10,
                event_index: 2,
            }),
            hi: Bound::Excluded(IntraTxCoordinate::start_of_tx(13)),
        };

        assert_eq!(bounds.tx_range(), Some(10..13));
    }

    #[test]
    fn tx_range_keeps_tx_of_nonzero_exclusive_hi() {
        let bounds = IntraTxScanBounds {
            lo: Bound::Unbounded,
            hi: Bound::Excluded(IntraTxCoordinate {
                tx_seq: 13,
                event_index: 1,
            }),
        };

        assert_eq!(bounds.tx_range(), Some(0..14));
    }

    #[test]
    fn tx_range_empty_bounds_yield_none() {
        let bounds = IntraTxScanBounds::tx_span(10, 10);
        assert_eq!(bounds.tx_range(), None);
    }

    #[test]
    fn parses_cursors_and_ordering() {
        let after = tx_item(2, 20).encode();
        let before = tx_item(3, 30).encode();
        let mut request = ProtoQueryOptions::default();
        request.limit = Some(500);
        request.after = Some(after);
        request.before = Some(before);
        request.ordering = Some(ProtoOrdering::Descending as i32);

        let options = query_options_from_proto(Some(&request)).unwrap();

        assert_eq!(options.limit_items, 500);
        assert_eq!(options.ordering, Ordering::Descending);
        assert_eq!(options.scan_direction(), ScanDirection::Descending);
        assert_eq!(
            options
                .apply_cursor_bounds(resolved_range(0..100))
                .bounds
                .to_range(),
            21..30
        );
    }

    #[test]
    fn has_after_cursor_reflects_only_the_after_field() {
        // No cursors → open-ended low end.
        let options = query_options_from_proto(Some(&ProtoQueryOptions::default())).unwrap();
        assert!(!options.has_after_cursor());

        // `before` bounds the high end, so it must not count as an explicit low end.
        let mut request = ProtoQueryOptions::default();
        request.before = Some(tx_item(3, 30).encode());
        let options = query_options_from_proto(Some(&request)).unwrap();
        assert!(!options.has_after_cursor());

        // `after` raises the low end → explicit.
        let mut request = ProtoQueryOptions::default();
        request.after = Some(tx_item(2, 20).encode());
        let options = query_options_from_proto(Some(&request)).unwrap();
        assert!(options.has_after_cursor());
    }

    #[test]
    fn clamps_limit_items_and_defaults_to_ascending() {
        let mut request = ProtoQueryOptions::default();
        request.limit = Some(5_000);

        let options = query_options_from_proto(Some(&request)).unwrap();

        assert_eq!(options.limit_items, 1_000);
        assert_eq!(options.ordering, Ordering::Ascending);
        assert_eq!(options.scan_direction(), ScanDirection::Ascending);
    }

    #[test]
    fn rejects_malformed_cursors_and_unknown_ordering() {
        let mut request = ProtoQueryOptions::default();
        request.after = Some(Bytes::from_static(b"short"));
        assert!(query_options_from_proto(Some(&request)).is_err());

        let mut request = ProtoQueryOptions::default();
        request.before = Some(Bytes::from_static(b"short"));
        assert!(query_options_from_proto(Some(&request)).is_err());

        let mut request = ProtoQueryOptions::default();
        request.ordering = Some(99);
        assert!(query_options_from_proto(Some(&request)).is_err());
    }

    #[test]
    fn rejects_cursor_for_different_position_variant() {
        let token = cp_item(9).encode();
        let mut request = ProtoQueryOptions::default();
        request.after = Some(token);
        assert!(query_options_from_proto(Some(&request)).is_err());
    }

    #[test]
    fn accepts_cursor_regardless_of_filter_scope() {
        // Cursors are portable across filters: a Transactions cursor must be
        // accepted by a Transactions query even though `query_options_from_proto`
        // applies no filter. Position is an absolute, filter-independent
        // coordinate, so resuming under a different filter is correct.
        let after = tx_item(1, 9).encode();
        let before = tx_item(3, 30).encode();
        let mut request = ProtoQueryOptions::default();
        request.after = Some(after);
        request.before = Some(before);
        assert!(query_options_from_proto(Some(&request)).is_ok());
    }

    #[test]
    fn accepts_cursors_for_different_checkpoint_range_and_ordering() {
        let token = tx_item(9, 9).encode();
        let mut request = ProtoQueryOptions::default();
        request.after = Some(token);
        request.ordering = Some(ProtoOrdering::Descending as i32);

        let options = query_options_from_proto(Some(&request)).unwrap();
        let range = CheckpointRange::from_request(Some(1_000), Some(1_100), 2_000).unwrap();

        assert_eq!(range.resolve(&options).range, 1_000..1_100);
    }

    #[test]
    fn applies_canonical_cursor_bounds() {
        let options = QueryOptions {
            limit_items: 2,
            ordering: Ordering::Ascending,
            after: Some(tx_item(1, 11)),
            before: None,
        };
        assert_eq!(
            options
                .apply_cursor_bounds(resolved_range(10..20))
                .bounds
                .to_range(),
            12..20
        );

        // Every after Item stays symbolic; at `u64::MAX` the exclusion
        // already empties the window AT RESOLUTION (Excluded(MAX) admits
        // nothing below any exclusive end), so the collapse machinery still
        // fires and echoes the cursor back unchanged — Item kind, raw
        // coordinate.
        let options = QueryOptions {
            after: Some(tx_item(1, u64::MAX)),
            ..options
        };
        assert_eq!(
            options.apply_cursor_bounds(resolved_range(10..20)),
            ResolvedScan::empty_at(
                1,
                u64::MAX,
                RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Item,
                },
            )
        );

        let options = QueryOptions {
            ordering: Ordering::Descending,
            after: Some(tx_item(1, 11)),
            before: Some(tx_item(1, 19)),
            ..options
        };
        let bounded = options.apply_cursor_bounds(resolved_range(10..20));
        assert_eq!(bounded.bounds.to_range(), 12..19);
        assert_eq!(
            bounded.terminal.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            }
        );
        // The terminal stamp stores the winning cursor's RAW coordinate (the
        // Item at 11), not its resume successor; the stamp sets CursorBound
        // and is never emitted as a terminal frame.
        assert_eq!(bounded.terminal.end_coordinate, 11);

        // A dense window collapsed by cursors is a store-edge fact under
        // symbolic resume: resolution's emptiness predicate cannot see that
        // no integer lies strictly between 11 and 12, so the window is NOT
        // resolution-empty, the terminal reports the last terminal-edge
        // winner (the descending after's Boundary stamp at its raw
        // coordinate), and `to_range` discovers the emptiness.
        let options = QueryOptions {
            before: Some(tx_item(1, 12)),
            ..options
        };
        let bounded = options.apply_cursor_bounds(resolved_range(10..20));
        assert!(!bounded.is_empty());
        assert_eq!(bounded.bounds.to_range(), 12..12);
        assert_eq!(
            bounded.terminal,
            TerminalRecord {
                end_checkpoint: 1,
                end_coordinate: 11,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
            }
        );
    }

    /// Low-edge ties flip under symbolic resume: an after Item at N is
    /// `Excluded(N)`, strictly looser as a lower bound than the range's
    /// `Included(N + 1)`, so an Item coinciding with the range edge loses
    /// terminal attribution (eager resolution used to award it the tie).
    /// Boundary cursors resolve to `Included` and still win their ties.
    #[test]
    fn after_item_at_range_edge_loses_the_tie() {
        let item_tie = QueryOptions {
            limit_items: 2,
            ordering: Ordering::Descending,
            after: Some(tx_item(1, 9)),
            before: None,
        };
        let bounded = item_tie.apply_cursor_bounds(resolved_range(10..20));
        assert_eq!(bounded.bounds.to_range(), 10..20);
        assert_eq!(
            bounded.terminal.exhaustion,
            RangeExhaustion::CheckpointBound
        );

        let boundary_tie = QueryOptions {
            after: Some(tx_boundary(1, 10)),
            ..item_tie
        };
        let bounded = boundary_tie.apply_cursor_bounds(resolved_range(10..20));
        assert_eq!(bounded.bounds.to_range(), 10..20);
        assert_eq!(
            bounded.terminal.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            }
        );
    }

    #[test]
    fn applies_boundary_cursor_bounds_without_item_offset() {
        let options = QueryOptions {
            limit_items: 2,
            ordering: Ordering::Ascending,
            after: Some(tx_boundary(2, 20)),
            before: None,
        };
        assert_eq!(
            options
                .apply_cursor_bounds(resolved_range(10..30))
                .bounds
                .to_range(),
            20..30
        );

        let options = QueryOptions {
            ordering: Ordering::Descending,
            after: None,
            before: Some(tx_boundary(2, 20)),
            ..options
        };
        assert_eq!(
            options
                .apply_cursor_bounds(resolved_range(10..30))
                .bounds
                .to_range(),
            10..20
        );
    }

    #[test]
    fn resolves_checkpoint_range_with_terminal_reason() {
        assert_eq!(
            CheckpointRange::from_request(None, None, 20)
                .unwrap()
                .resolve(&query_options_from_proto(None).unwrap())
                .exhaustion,
            RangeExhaustion::LedgerTip
        );
        assert!(CheckpointRange::from_request(Some(10), Some(9), 20).is_err());

        let range = CheckpointRange::from_request(Some(10), None, 20).unwrap();
        let resolved = range.resolve(&query_options_from_proto(None).unwrap());
        assert_eq!(resolved.range, 10..20);
        assert_eq!(resolved.exhaustion, RangeExhaustion::LedgerTip);

        let range = CheckpointRange::from_request(Some(30), None, 20).unwrap();
        assert_eq!(
            range.resolve(&query_options_from_proto(None).unwrap()),
            ResolvedCheckpointRange::empty_at(20, RangeExhaustion::LedgerTip)
        );
    }

    /// The cp-width clamp was removed when scan limiting moved to the runtime
    /// bucket-budget path. Whatever range the request asks for is honored at
    /// resolve time; the bitmap layer terminates scans on budget exhaustion.
    #[test]
    fn resolves_checkpoint_range_no_longer_clamped_by_width() {
        let options = query_options_from_proto(None).unwrap();
        let range = CheckpointRange::from_request(Some(10), Some(10_000_000), 10_000_000).unwrap();
        let resolved = range.resolve(&options);
        assert_eq!(resolved.range, 10..10_000_000);
        assert_eq!(resolved.exhaustion, RangeExhaustion::CheckpointBound);
    }

    fn cp_boundary(checkpoint: u64) -> CursorToken {
        CursorToken::boundary(Position::Checkpoints { checkpoint })
    }

    /// Cursor clamps at checkpoint granularity: `after` keeps its checkpoint
    /// as the inclusive start, `before` keeps an Item's checkpoint in range
    /// (`cp + 1`) but takes a Boundary's as-is, the exhaustion reason follows
    /// the ordering-side terminal edge, and a cursor-collapsed interval
    /// reports CursorBound.
    #[test]
    fn resolves_checkpoint_range_with_cursor_clamps() {
        let range = || CheckpointRange::from_request(Some(10), Some(20), 100).unwrap();

        let after_item = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: Some(cp_item(12)),
            before: None,
        };
        let resolved = range().resolve(&after_item);
        assert_eq!(resolved.range, 12..20);
        assert_eq!(resolved.exhaustion, RangeExhaustion::CheckpointBound);

        let resolved = range().resolve(&QueryOptions {
            ordering: Ordering::Descending,
            ..after_item.clone()
        });
        assert_eq!(resolved.range, 12..20);
        assert_eq!(
            resolved.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            }
        );

        let before_item = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: None,
            before: Some(cp_item(15)),
        };
        let resolved = range().resolve(&before_item);
        assert_eq!(resolved.range, 10..16);
        assert_eq!(
            resolved.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            }
        );

        let resolved = range().resolve(&QueryOptions {
            before: Some(cp_boundary(15)),
            ..before_item.clone()
        });
        assert_eq!(resolved.range, 10..15);

        let collapsed = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: Some(cp_item(18)),
            before: Some(cp_boundary(18)),
        };
        assert_eq!(
            range().resolve(&collapsed),
            ResolvedCheckpointRange::empty_at(
                18,
                RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                }
            )
        );
    }

    fn event_cursor(
        kind: CursorKind,
        checkpoint: u64,
        tx_seq: u64,
        event_index: u32,
    ) -> CursorToken {
        CursorToken {
            kind,
            position: Position::Events {
                checkpoint,
                tx_seq,
                event_index,
            },
        }
    }

    /// When both cursors collapse the interval, the reported terminal is the
    /// `before` record (always Boundary), not the `after` echo — even when
    /// the `after` Item alone would have emptied the interval and echoed
    /// Item kind.
    #[test]
    fn event_collapse_prefers_before_record_over_after_echo() {
        let resolved = ResolvedScan {
            bounds: IntraTxScanBounds::tx_span(0, 10),
            entry_checkpoint: 0,
            terminal: TerminalRecord {
                end_checkpoint: 5,
                end_coordinate: IntraTxCoordinate::start_of_tx(10),
                exhaustion: RangeExhaustion::CheckpointBound,
            },
        };

        let options = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: Some(event_cursor(CursorKind::Item, 2, 10, 0)),
            before: Some(event_cursor(CursorKind::Item, 1, 3, 0)),
        };
        let bounded = options.apply_cursor_bounds(resolved.clone());
        assert!(bounded.is_empty());
        assert_eq!(bounded.terminal.end_checkpoint, 1);
        assert_eq!(
            bounded.terminal.end_coordinate,
            IntraTxCoordinate {
                tx_seq: 3,
                event_index: 0,
            }
        );
        assert_eq!(
            bounded.terminal.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            }
        );

        // Without the before cursor, the same after Item echoes back as-is.
        let options = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: Some(event_cursor(CursorKind::Item, 2, 10, 0)),
            before: None,
        };
        let bounded = options.apply_cursor_bounds(resolved);
        assert!(bounded.is_empty());
        assert_eq!(bounded.terminal.end_checkpoint, 2);
        assert_eq!(
            bounded.terminal.end_coordinate,
            IntraTxCoordinate {
                tx_seq: 10,
                event_index: 0,
            }
        );
        assert_eq!(
            bounded.terminal.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Item,
            }
        );
    }

    /// Nonempty intervals terminate at the ordering-side cursor edge: a
    /// winning `after` sets the terminal metadata for descending scans, a
    /// winning `before` for ascending scans, and the opposite-edge cursor
    /// only advances the entry checkpoint.
    #[test]
    fn event_terminal_edge_winner_sets_end_metadata() {
        let resolved = ResolvedScan {
            bounds: IntraTxScanBounds::tx_span(0, 10),
            entry_checkpoint: 50,
            terminal: TerminalRecord {
                end_checkpoint: 99,
                end_coordinate: IntraTxCoordinate::start_of_tx(0),
                exhaustion: RangeExhaustion::CheckpointBound,
            },
        };

        let descending = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Descending,
            after: Some(event_cursor(CursorKind::Item, 2, 4, 1)),
            before: Some(event_cursor(CursorKind::Item, 8, 9, 0)),
        };
        let bounded = descending.apply_cursor_bounds(resolved.clone());
        assert!(!bounded.is_empty());
        assert_eq!(bounded.terminal.end_checkpoint, 2);
        assert_eq!(
            bounded.terminal.end_coordinate,
            IntraTxCoordinate {
                tx_seq: 4,
                event_index: 1,
            }
        );
        assert_eq!(
            bounded.terminal.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            }
        );
        // Descending entry edge is the before cursor, win or lose.
        assert_eq!(bounded.entry_checkpoint, 8);

        let ascending = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: Some(event_cursor(CursorKind::Item, 2, 4, 1)),
            before: Some(event_cursor(CursorKind::Item, 8, 9, 0)),
        };
        let bounded = ascending.apply_cursor_bounds(resolved);
        assert!(!bounded.is_empty());
        assert_eq!(bounded.terminal.end_checkpoint, 8);
        assert_eq!(
            bounded.terminal.end_coordinate,
            IntraTxCoordinate {
                tx_seq: 9,
                event_index: 0,
            }
        );
        assert_eq!(
            bounded.terminal.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            }
        );
        // Ascending entry edge is the after cursor, win or lose.
        assert_eq!(bounded.entry_checkpoint, 50);
    }

    #[test]
    fn event_after_item_empty_interval_retains_item_kind() {
        let position = Position::Events {
            checkpoint: 1,
            tx_seq: 3,
            event_index: 0,
        };
        let resolved = ResolvedScan {
            bounds: IntraTxScanBounds::tx_span(0, 3),
            entry_checkpoint: 0,
            terminal: TerminalRecord {
                end_checkpoint: 1,
                end_coordinate: IntraTxCoordinate::start_of_tx(3),
                exhaustion: RangeExhaustion::CheckpointBound,
            },
        };

        let mut request = ProtoQueryOptions::default();
        request.after = Some(CursorToken::item(position).encode());
        let options = QueryOptions::events_from_proto(Some(&request), 100, 100).unwrap();
        let item_bounded = options.apply_cursor_bounds(resolved.clone());

        assert!(item_bounded.is_empty());
        assert_eq!(
            item_bounded.terminal.end_coordinate,
            IntraTxCoordinate {
                tx_seq: 3,
                event_index: 0,
            }
        );
        assert_eq!(
            item_bounded.terminal.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Item,
            }
        );

        request.after = Some(CursorToken::boundary(position).encode());
        let options = QueryOptions::events_from_proto(Some(&request), 100, 100).unwrap();
        let boundary_bounded = options.apply_cursor_bounds(resolved);

        assert!(boundary_bounded.is_empty());
        assert_eq!(
            boundary_bounded.terminal.end_coordinate,
            IntraTxCoordinate {
                tx_seq: 3,
                event_index: 0,
            }
        );
        assert_eq!(
            boundary_bounded.terminal.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            }
        );
    }

    #[test]
    fn cursor_fold_advances_entry_checkpoint() {
        let ascending = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: Some(tx_item(7, 30)),
            before: None,
        };
        let resolved = ResolvedScan {
            bounds: ScanBounds::from_range(20..40),
            entry_checkpoint: 5,
            terminal: TerminalRecord {
                end_checkpoint: 9,
                end_coordinate: 40,
                exhaustion: RangeExhaustion::CheckpointBound,
            },
        };
        assert_eq!(
            ascending
                .apply_cursor_bounds(resolved.clone())
                .entry_checkpoint,
            7
        );

        let descending = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Descending,
            after: None,
            before: Some(tx_boundary(7, 30)),
        };
        let resolved = ResolvedScan {
            entry_checkpoint: 9,
            ..resolved
        };
        assert_eq!(descending.apply_cursor_bounds(resolved).entry_checkpoint, 7);
    }

    #[test]
    fn item_cursor_can_be_used_as_after_or_before() {
        let token = CursorToken::item(Position::Transactions {
            checkpoint: 1,
            tx_seq: 11,
        })
        .encode();

        let mut request = ProtoQueryOptions::default();
        request.after = Some(token.clone());
        let options = query_options_from_proto(Some(&request)).unwrap();
        assert_eq!(
            options
                .apply_cursor_bounds(resolved_range(10..20))
                .bounds
                .to_range(),
            12..20
        );

        request.after = None;
        request.before = Some(token);
        let options = query_options_from_proto(Some(&request)).unwrap();
        assert_eq!(
            options
                .apply_cursor_bounds(resolved_range(10..20))
                .bounds
                .to_range(),
            10..11
        );
    }
}
