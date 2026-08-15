// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Request → scan resolution for the ledger-history list endpoints.
//!
//! The pipeline is one cycle — **lookup, translate, constrain-and-account**
//! — iterated from request space down to store keys. Every item in this
//! module is one of three species:
//!
//! - **Cycle body**: [`derive_scan`], invoked with a stage's
//!   [`Contribution`]s. The checkpoint stage
//!   ([`ResolvedCheckpointRange::from_request`]) narrows by cursor hints
//!   WITHOUT attributing — collapsed windows flow through the empty-safe
//!   projection so the scan stage attributes once, with full-fidelity
//!   echo. The serving floor is the same body split into halves
//!   ([`ScanBounds::clamp_to_available_lo`] +
//!   [`TerminalRecord::absorb_raised_lo`]) because the filtered
//!   checkpoint scan pairs transaction-space bounds with a
//!   checkpoint-space record.
//! - **Translations** at representation changes:
//!   [`ResolvedCheckpointRange::with_range`] via
//!   [`ScanCoordinate::from_boundary`] into a lane's scan space;
//!   [`ScanBounds::to_range`] (and the packed event range) out to store
//!   keys — the store edge is where symbolic resume-from-excluded
//!   resolves.
//! - **Backend lookups** that force the staging: the checkpoint→tx
//!   projection, the serving-floor probe, and the scan itself.
//!
//! Vocabulary: a **position** is wire-side — a checkpoint plus a
//! coordinate; a **coordinate** is a lane's scan-space key.
//!
//! Hard invariants (wire contract):
//! - **Faithful echo**: a cursor-collapsed window's response surfaces the
//!   client's own cursor — raw coordinate and kind — so resume neither
//!   repeats nor skips.
//! - **No unscanned coverage**: watermarks never claim checkpoints the
//!   scan did not fully traverse (entry gates below, terminal pins above,
//!   the serving floor and store-edge emptiness respected).
//! - **Frontier monotonicity**: an empty response never moves the
//!   client's frontier — not forward past unscanned space (the floor
//!   rule), not backward below their cursor (the echo rule).
//!
//! Conventions (inherited wire behavior, changeable only with sign-off):
//! ties go to the cursor at every granularity (bound-key ordering), but
//! the ledger tip preempts attribution on a tip-collapsed seed; `before`
//! wins collapse precedence over `after`; entry checkpoints fold on
//! cursor presence alone.

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
    /// The wire-space half of the resolution.
    pub edges: WatermarkEdges<P>,
}

/// The wire-facing half of a resolution: the checkpoint-space shadow the
/// scan-space bounds carry because wire watermarks speak checkpoints. The
/// filtered checkpoint scan pairs this (checkpoint-space) with
/// transaction-space bounds — scan in one language, report in the other.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WatermarkEdges<T> {
    /// Checkpoint containing the interval's first position in scan
    /// direction. Checkpoint-only because its sole consumer — the
    /// covered-bound fold — claims coverage at checkpoint granularity: a
    /// claim before it proves nothing and is suppressed, keeping the wire
    /// `checkpoint` unset until the scan's first checkpoint is fully
    /// covered.
    pub entry_checkpoint: u64,
    /// The terminal edge the scan reports once it drains the interval.
    pub terminal: TerminalRecord<T>,
}

impl<T: Copy> WatermarkEdges<T> {
    /// The window's low edge just rose to `floor_position` (contained in
    /// `floor_checkpoint`); move the watermark field that describes the
    /// low edge along with it, so nothing claims the truncated gap.
    /// Ascending, the low edge is the entry side: the coverage gate rises.
    /// Descending, it is the terminal side: the record pins to the floor.
    /// Exhaustion is untouched — the floor corrects where, not why.
    pub fn absorb_raised_lo(&mut self, floor_checkpoint: u64, floor_position: T, ascending: bool) {
        if ascending {
            self.entry_checkpoint = self.entry_checkpoint.max(floor_checkpoint);
        } else {
            self.terminal.end_checkpoint = floor_checkpoint;
            self.terminal.end_coordinate = floor_position;
        }
    }
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

/// One cursor's offer to [`derive_scan`]: the bound it imposes on the
/// window, plus the raw token facts the install site builds a stamp from.
struct Contribution<P> {
    bound: Bound<P>,
    checkpoint: u64,
    coordinate: P,
    kind: sui_rpc_cursor::CursorKind,
}

impl<P: Copy> Contribution<P> {
    /// The stamp installed if this cursor ends the scan; `kind` is chosen
    /// at the install site. An installed stamp is always `CursorBound`.
    fn record(&self, kind: sui_rpc_cursor::CursorKind) -> TerminalRecord<P> {
        TerminalRecord {
            end_checkpoint: self.checkpoint,
            end_coordinate: self.coordinate,
            exhaustion: RangeExhaustion::CursorBound { kind },
        }
    }
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

    /// Decode each cursor into its [`Contribution`] — pure translation,
    /// no policy — and run one [`derive_scan`] iteration. Empty seeds
    /// flow through so a checkpoint-stage collapse can echo the client's
    /// full cursor.
    fn apply_cursor_bounds<P: ScanCoordinate>(&self, resolved: ResolvedScan<P>) -> ResolvedScan<P> {
        let after = self.after.as_ref().map(|cursor| {
            let coordinate = P::from_cursor(cursor);
            Contribution {
                bound: cursor.kind.resume_bound(coordinate),
                checkpoint: cursor.position.checkpoint(),
                coordinate,
                kind: cursor.kind,
            }
        });
        let before = self.before.as_ref().map(|cursor| {
            let coordinate = P::from_cursor(cursor);
            Contribution {
                bound: cursor.kind.limit_bound(coordinate),
                checkpoint: cursor.position.checkpoint(),
                coordinate,
                kind: cursor.kind,
            }
        });
        derive_scan(self.ordering, resolved, after, before)
    }
}

impl ResolvedCheckpointRange {
    /// Validate and tip-clamp the requested checkpoint window, narrow it
    /// by the cursors' checkpoint hints (`after` inclusive — a hint may
    /// carry sub-checkpoint information; a `before` Item's checkpoint
    /// stays in range, a Boundary's is already exclusive), and carry the
    /// request-derived reason for the ordering-side terminal edge. No
    /// cursor attribution here: a collapsed window flows through the
    /// empty-safe projection and [`derive_scan`] attributes once, with
    /// the full-fidelity echo.
    pub fn from_request(
        start_checkpoint: Option<u64>,
        end_checkpoint: Option<u64>,
        checkpoint_hi_exclusive: u64,
        options: &QueryOptions,
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

        let start = match &options.after {
            Some(cursor) if cursor.position.checkpoint() >= start => cursor.position.checkpoint(),
            _ => start,
        };
        let before_upper = options
            .before
            .as_ref()
            .and_then(|cursor| match cursor.kind {
                sui_rpc_cursor::CursorKind::Item => cursor.position.checkpoint().checked_add(1),
                sui_rpc_cursor::CursorKind::Boundary => Some(cursor.position.checkpoint()),
            });
        let end = match before_upper {
            Some(upper) if upper <= end => upper,
            _ => end,
        };

        if start >= checkpoint_hi_exclusive {
            return Ok(Self::empty_at(
                checkpoint_hi_exclusive,
                RangeExhaustion::LedgerTip,
            ));
        }
        let exhaustion = match options.ordering {
            Ordering::Ascending => high_exhaustion,
            Ordering::Descending => RangeExhaustion::CheckpointBound,
        };
        if start >= end {
            let checkpoint = match options.ordering {
                Ordering::Ascending => end,
                Ordering::Descending => start,
            };
            return Ok(Self::empty_at(checkpoint, exhaustion));
        }
        Ok(Self {
            range: start..end,
            exhaustion,
        })
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
            edges: WatermarkEdges {
                entry_checkpoint,
                terminal,
            },
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

impl<P: Copy + Ord> ResolvedScan<P> {
    pub fn empty_at(end_checkpoint: u64, end_coordinate: P, exhaustion: RangeExhaustion) -> Self {
        Self {
            bounds: ScanBounds::empty_at(end_coordinate),
            edges: WatermarkEdges {
                entry_checkpoint: end_checkpoint,
                terminal: TerminalRecord {
                    end_checkpoint,
                    end_coordinate,
                    exhaustion,
                },
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
            self.bounds = ScanBounds::empty_at(self.edges.terminal.end_coordinate);
            return;
        }
        self.edges
            .absorb_raised_lo(floor_checkpoint, floor_position, options.is_ascending());
    }
}

/// The resolution cycle's constrain-and-account body: fold the cursors'
/// bounds into the seed window and account for who owns each edge — all
/// by comparison against the seed, no mutation order.
///
/// The ordering-side winner (descending `after`, ascending `before`)
/// stamps a nonempty window's terminal; a collapsed window installs
/// `before` over `after`. Ties go to the cursor; a tip-collapsed seed
/// keeps `LedgerTip` (polling back-off; no echo for unindexed space).
/// Stamps echo the cursor's raw coordinate; only a stamp resumed on the
/// lo side (an ascending response's cursor returns as `after`) keeps the
/// raw kind — on the hi side Item and Boundary denote the same
/// constraint, so Boundary is canonical. Entry advances on cursor
/// presence, win or lose.
//
// TODO: graphql-style empties (omit the cursor; the client's frontier
// stands) would retire the echo machinery — raw stamps, kind retention,
// collapse precedence. Needs client-contract sign-off.
fn derive_scan<P: Copy + Ord>(
    ordering: Ordering,
    seed: ResolvedScan<P>,
    after: Option<Contribution<P>>,
    before: Option<Contribution<P>>,
) -> ResolvedScan<P> {
    let ascending = matches!(ordering, Ordering::Ascending);

    // Account, entry side: folds on presence alone — max/min make a
    // losing cursor a no-op.
    let entry_checkpoint = match (ascending, &after, &before) {
        (true, Some(c), _) => seed.edges.entry_checkpoint.max(c.checkpoint),
        (false, _, Some(c)) => seed.edges.entry_checkpoint.min(c.checkpoint),
        _ => seed.edges.entry_checkpoint,
    };

    // A tip-collapsed seed is already decided: the tip preempts cursor
    // attribution, keeping the back-off signal for at- and beyond-tip
    // pollers.
    if seed.bounds.is_empty()
        && matches!(seed.edges.terminal.exhaustion, RangeExhaustion::LedgerTip)
    {
        return ResolvedScan {
            bounds: ScanBounds::empty_at(seed.edges.terminal.end_coordinate),
            edges: WatermarkEdges {
                entry_checkpoint,
                terminal: seed.edges.terminal,
            },
        };
    }

    // Constrain: win-or-keep on each edge (ties to the cursor).
    let after_win = after
        .as_ref()
        .filter(|c| lower_bound_gte(c.bound, seed.bounds.lo));
    let before_win = before
        .as_ref()
        .filter(|c| upper_bound_lte(c.bound, seed.bounds.hi));
    let lo = after_win.map_or(seed.bounds.lo, |c| c.bound);
    let hi = before_win.map_or(seed.bounds.hi, |c| c.bound);
    let empty = bounds_empty(lo, hi);

    // Account, terminal side: stamps are built where installed, kind
    // chosen here next to the precedence it serves.
    let after_record = after_win.map(|c| {
        c.record(if ascending {
            c.kind
        } else {
            sui_rpc_cursor::CursorKind::Boundary
        })
    });
    let before_record = before_win.map(|c| c.record(sui_rpc_cursor::CursorKind::Boundary));
    let terminal = match (empty, ascending) {
        // Collapsed window: echo precedence `before` over `after`.
        (true, _) => before_record.or(after_record),
        // Nonempty: the ordering-side exit edge owns the stop.
        (false, true) => before_record,
        (false, false) => after_record,
    }
    .unwrap_or(seed.edges.terminal);

    let bounds = if empty {
        // Canonical empty form everywhere in this module: the interval
        // collapses onto its reported terminal bound.
        ScanBounds::empty_at(terminal.end_coordinate)
    } else {
        ScanBounds { lo, hi }
    };
    ResolvedScan {
        bounds,
        edges: WatermarkEdges {
            entry_checkpoint,
            terminal,
        },
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
            edges: WatermarkEdges {
                entry_checkpoint: 0,
                terminal: TerminalRecord {
                    end_checkpoint: 20,
                    end_coordinate: 20,
                    exhaustion: RangeExhaustion::CheckpointBound,
                },
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
            edges: WatermarkEdges {
                entry_checkpoint: 0,
                terminal: TerminalRecord {
                    end_checkpoint: 20,
                    end_coordinate: 100,
                    exhaustion: RangeExhaustion::CheckpointBound,
                },
            },
        };
        resolved.apply_serving_floor(50, 10, &asc);
        assert_eq!(resolved.bounds.to_range(), 50..100);
        assert_eq!(resolved.edges.entry_checkpoint, 10);
        assert_eq!(resolved.edges.terminal.end_checkpoint, 20);
        assert_eq!(resolved.edges.terminal.end_coordinate, 100);

        // Descending, floor inside: entry (the high edge) untouched; the
        // terminal pins to the floor.
        let mut resolved = ResolvedScan {
            bounds: ScanBounds::from_range(0..100),
            edges: WatermarkEdges {
                entry_checkpoint: 20,
                terminal: TerminalRecord {
                    end_checkpoint: 0,
                    end_coordinate: 0,
                    exhaustion: RangeExhaustion::CheckpointBound,
                },
            },
        };
        resolved.apply_serving_floor(50, 10, &desc);
        assert_eq!(resolved.bounds.to_range(), 50..100);
        assert_eq!(resolved.edges.entry_checkpoint, 20);
        assert_eq!(resolved.edges.terminal.end_checkpoint, 10);
        assert_eq!(resolved.edges.terminal.end_coordinate, 50);

        // Floor at/past the high end (covers the == boundary), both
        // directions: empty intersection, canonicalized at the reported
        // terminal bound; no metadata moves. Descending, the terminal must
        // NOT move to the floor (checkpoint 10 lies outside the requested
        // interval).
        for floor_tx in [40, 50] {
            let mut resolved = ResolvedScan {
                bounds: ScanBounds::from_range(0..40),
                edges: WatermarkEdges {
                    entry_checkpoint: 0,
                    terminal: TerminalRecord {
                        end_checkpoint: 8,
                        end_coordinate: 40,
                        exhaustion: RangeExhaustion::CheckpointBound,
                    },
                },
            };
            resolved.apply_serving_floor(floor_tx, 10, &asc);
            assert!(resolved.is_empty());
            assert_eq!(resolved.bounds.to_range(), 40..40);
            assert_eq!(resolved.edges.entry_checkpoint, 0);
            assert_eq!(resolved.edges.terminal.end_checkpoint, 8);
            assert_eq!(resolved.edges.terminal.end_coordinate, 40);

            let mut resolved = ResolvedScan {
                bounds: ScanBounds::from_range(0..40),
                edges: WatermarkEdges {
                    entry_checkpoint: 8,
                    terminal: TerminalRecord {
                        end_checkpoint: 0,
                        end_coordinate: 0,
                        exhaustion: RangeExhaustion::CheckpointBound,
                    },
                },
            };
            resolved.apply_serving_floor(floor_tx, 10, &desc);
            assert!(resolved.is_empty());
            assert_eq!(resolved.bounds.to_range(), 0..0);
            assert_eq!(resolved.edges.entry_checkpoint, 8);
            assert_eq!(resolved.edges.terminal.end_checkpoint, 0);
            assert_eq!(resolved.edges.terminal.end_coordinate, 0);
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
            edges: WatermarkEdges {
                entry_checkpoint: 0,
                terminal: TerminalRecord {
                    end_checkpoint: 20,
                    end_coordinate: IntraTxCoordinate::start_of_tx(100),
                    exhaustion: RangeExhaustion::CheckpointBound,
                },
            },
        };
        resolved.apply_serving_floor(IntraTxCoordinate::start_of_tx(50), 10, &asc);
        assert_eq!(
            resolved.bounds.lo,
            Bound::Included(IntraTxCoordinate::start_of_tx(50))
        );
        assert_eq!(resolved.edges.entry_checkpoint, 10);
        assert_eq!(resolved.edges.terminal.end_checkpoint, 20);
        assert_eq!(
            resolved.edges.terminal.end_coordinate,
            IntraTxCoordinate::start_of_tx(100)
        );

        // Descending, floor inside: terminal pins to the floor.
        let mut resolved = ResolvedScan {
            bounds: IntraTxScanBounds::tx_span(0, 100),
            edges: WatermarkEdges {
                entry_checkpoint: 20,
                terminal: TerminalRecord {
                    end_checkpoint: 0,
                    end_coordinate: IntraTxCoordinate::start_of_tx(0),
                    exhaustion: RangeExhaustion::CheckpointBound,
                },
            },
        };
        resolved.apply_serving_floor(IntraTxCoordinate::start_of_tx(50), 10, &desc);
        assert_eq!(
            resolved.bounds.lo,
            Bound::Included(IntraTxCoordinate::start_of_tx(50))
        );
        assert_eq!(resolved.edges.entry_checkpoint, 20);
        assert_eq!(resolved.edges.terminal.end_checkpoint, 10);
        assert_eq!(
            resolved.edges.terminal.end_coordinate,
            IntraTxCoordinate::start_of_tx(50)
        );

        // Floor that empties the bounds (covers the == boundary), both
        // directions: canonical empty at the reported terminal bound, no
        // metadata moves.
        for floor_tx in [40, 50] {
            let mut resolved = ResolvedScan {
                bounds: IntraTxScanBounds::tx_span(0, 40),
                edges: WatermarkEdges {
                    entry_checkpoint: 0,
                    terminal: TerminalRecord {
                        end_checkpoint: 8,
                        end_coordinate: IntraTxCoordinate::start_of_tx(40),
                        exhaustion: RangeExhaustion::CheckpointBound,
                    },
                },
            };
            resolved.apply_serving_floor(IntraTxCoordinate::start_of_tx(floor_tx), 10, &asc);
            assert!(resolved.is_empty());
            assert_eq!(
                resolved.bounds,
                IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(40))
            );
            assert_eq!(resolved.edges.entry_checkpoint, 0);
            assert_eq!(resolved.edges.terminal.end_checkpoint, 8);
            assert_eq!(
                resolved.edges.terminal.end_coordinate,
                IntraTxCoordinate::start_of_tx(40)
            );

            let mut resolved = ResolvedScan {
                bounds: IntraTxScanBounds::tx_span(0, 40),
                edges: WatermarkEdges {
                    entry_checkpoint: 8,
                    terminal: TerminalRecord {
                        end_checkpoint: 0,
                        end_coordinate: IntraTxCoordinate::start_of_tx(0),
                        exhaustion: RangeExhaustion::CheckpointBound,
                    },
                },
            };
            resolved.apply_serving_floor(IntraTxCoordinate::start_of_tx(floor_tx), 10, &desc);
            assert!(resolved.is_empty());
            assert_eq!(
                resolved.bounds,
                IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(0))
            );
            assert_eq!(resolved.edges.entry_checkpoint, 8);
            assert_eq!(resolved.edges.terminal.end_checkpoint, 0);
            assert_eq!(
                resolved.edges.terminal.end_coordinate,
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
        let resolved =
            ResolvedCheckpointRange::from_request(Some(1_000), Some(1_100), 2_000, &options)
                .unwrap();

        assert_eq!(resolved.range, 1_000..1_100);
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
            bounded.edges.terminal.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            }
        );
        // The terminal stamp stores the winning cursor's RAW coordinate (the
        // Item at 11), not its resume successor; the stamp sets CursorBound
        // and is never emitted as a terminal frame.
        assert_eq!(bounded.edges.terminal.end_coordinate, 11);

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
            bounded.edges.terminal,
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
            bounded.edges.terminal.exhaustion,
            RangeExhaustion::CheckpointBound
        );

        let boundary_tie = QueryOptions {
            after: Some(tx_boundary(1, 10)),
            ..item_tie
        };
        let bounded = boundary_tie.apply_cursor_bounds(resolved_range(10..20));
        assert_eq!(bounded.bounds.to_range(), 10..20);
        assert_eq!(
            bounded.edges.terminal.exhaustion,
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
        let options = query_options_from_proto(None).unwrap();
        assert_eq!(
            ResolvedCheckpointRange::from_request(None, None, 20, &options)
                .unwrap()
                .exhaustion,
            RangeExhaustion::LedgerTip
        );
        assert!(ResolvedCheckpointRange::from_request(Some(10), Some(9), 20, &options).is_err());

        let resolved = ResolvedCheckpointRange::from_request(Some(10), None, 20, &options).unwrap();
        assert_eq!(resolved.range, 10..20);
        assert_eq!(resolved.exhaustion, RangeExhaustion::LedgerTip);

        assert_eq!(
            ResolvedCheckpointRange::from_request(Some(30), None, 20, &options).unwrap(),
            ResolvedCheckpointRange::empty_at(20, RangeExhaustion::LedgerTip)
        );
    }

    /// The cp-width clamp was removed when scan limiting moved to the runtime
    /// bucket-budget path. Whatever range the request asks for is honored at
    /// resolve time; the bitmap layer terminates scans on budget exhaustion.
    #[test]
    fn resolves_checkpoint_range_no_longer_clamped_by_width() {
        let options = query_options_from_proto(None).unwrap();
        let resolved =
            ResolvedCheckpointRange::from_request(Some(10), Some(10_000_000), 10_000_000, &options)
                .unwrap();
        assert_eq!(resolved.range, 10..10_000_000);
        assert_eq!(resolved.exhaustion, RangeExhaustion::CheckpointBound);
    }

    fn cp_boundary(checkpoint: u64) -> CursorToken {
        CursorToken::boundary(Position::Checkpoints { checkpoint })
    }

    /// Cursor checkpoint hints only NARROW at resolve: `after` keeps its
    /// checkpoint as the inclusive start, `before` keeps an Item's
    /// checkpoint in range (`cp + 1`) but takes a Boundary's as-is, and the
    /// exhaustion stays the request-derived ordering-side reason — cursor
    /// attribution belongs to the scan stage (see the flow-through pins
    /// below).
    #[test]
    fn resolves_checkpoint_range_with_cursor_clamps() {
        let resolve = |options: &QueryOptions| {
            ResolvedCheckpointRange::from_request(Some(10), Some(20), 100, options).unwrap()
        };

        let after_item = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: Some(cp_item(12)),
            before: None,
        };
        let resolved = resolve(&after_item);
        assert_eq!(resolved.range, 12..20);
        assert_eq!(resolved.exhaustion, RangeExhaustion::CheckpointBound);

        let resolved = resolve(&QueryOptions {
            ordering: Ordering::Descending,
            ..after_item.clone()
        });
        assert_eq!(resolved.range, 12..20);
        assert_eq!(resolved.exhaustion, RangeExhaustion::CheckpointBound);

        let before_item = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: None,
            before: Some(cp_item(15)),
        };
        let resolved = resolve(&before_item);
        assert_eq!(resolved.range, 10..16);
        assert_eq!(resolved.exhaustion, RangeExhaustion::CheckpointBound);

        let resolved = resolve(&QueryOptions {
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
            resolve(&collapsed),
            ResolvedCheckpointRange::empty_at(18, RangeExhaustion::CheckpointBound)
        );
    }

    /// A checkpoint-stage collapse flows through the (empty-safe)
    /// projection into the scan stage, which echoes the client's FULL
    /// cursor — raw coordinate and kind, the frontier's fixed point —
    /// instead of a Boundary at the clamped range edge (which would hand
    /// back a cursor BELOW the client's own position).
    #[test]
    fn cp_collapse_flows_through_to_the_full_cursor_echo() {
        let options = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: Some(cp_item(25)),
            before: None,
        };
        let cp_range =
            ResolvedCheckpointRange::from_request(Some(10), Some(20), 100, &options).unwrap();
        assert_eq!(
            cp_range,
            ResolvedCheckpointRange::empty_at(20, RangeExhaustion::CheckpointBound)
        );

        let range = cp_range.range.clone();
        let bounded: ResolvedScan<u64> =
            options.apply_cursor_bounds(cp_range.with_range(range, options.ordering));
        assert!(bounded.is_empty());
        assert_eq!(
            bounded.edges.terminal,
            TerminalRecord {
                end_checkpoint: 25,
                end_coordinate: 25,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Item,
                },
            }
        );
    }

    /// The tip preempts cursor attribution on a tip-collapsed seed: an
    /// at-tip polling cursor is told LedgerTip, keeping the back-off
    /// signal (same behavior as before the flow-through).
    #[test]
    fn at_tip_cursor_keeps_ledger_tip() {
        let options = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: Some(cp_boundary(20)),
            before: None,
        };
        let cp_range = ResolvedCheckpointRange::from_request(None, None, 20, &options).unwrap();
        assert_eq!(
            cp_range,
            ResolvedCheckpointRange::empty_at(20, RangeExhaustion::LedgerTip)
        );

        let range = cp_range.range.clone();
        let bounded: ResolvedScan<u64> =
            options.apply_cursor_bounds(cp_range.with_range(range, options.ordering));
        assert!(bounded.is_empty());
        assert_eq!(
            bounded.edges.terminal,
            TerminalRecord {
                end_checkpoint: 20,
                end_coordinate: 20,
                exhaustion: RangeExhaustion::LedgerTip,
            }
        );
    }

    /// A cursor strictly beyond the tip is also preempted: the server's
    /// frontier (LedgerTip at the tip edge) is the honest answer for
    /// space this server has not indexed — no echo is fabricated for it
    /// (same behavior as before the flow-through).
    #[test]
    fn beyond_tip_cursor_keeps_ledger_tip() {
        let options = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: Some(cp_item(25)),
            before: None,
        };
        let cp_range = ResolvedCheckpointRange::from_request(None, None, 20, &options).unwrap();
        assert_eq!(
            cp_range,
            ResolvedCheckpointRange::empty_at(20, RangeExhaustion::LedgerTip)
        );

        let range = cp_range.range.clone();
        let bounded: ResolvedScan<u64> =
            options.apply_cursor_bounds(cp_range.with_range(range, options.ordering));
        assert_eq!(
            bounded.edges.terminal,
            TerminalRecord {
                end_checkpoint: 20,
                end_coordinate: 20,
                exhaustion: RangeExhaustion::LedgerTip,
            }
        );
    }

    /// On a degenerate (request-empty, non-tip) window, a participating
    /// before cursor attributes CursorBound with the before's echo — ties
    /// to the cursor, as at every granularity.
    #[test]
    fn empty_seed_before_cursor_participates() {
        let options = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: None,
            before: Some(cp_item(3)),
        };
        let cp_range =
            ResolvedCheckpointRange::from_request(Some(5), Some(5), 100, &options).unwrap();
        assert_eq!(
            cp_range,
            ResolvedCheckpointRange::empty_at(4, RangeExhaustion::CheckpointBound)
        );

        let range = cp_range.range.clone();
        let bounded: ResolvedScan<u64> =
            options.apply_cursor_bounds(cp_range.with_range(range, options.ordering));
        assert_eq!(
            bounded.edges.terminal,
            TerminalRecord {
                end_checkpoint: 3,
                end_coordinate: 3,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
            }
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
            edges: WatermarkEdges {
                entry_checkpoint: 0,
                terminal: TerminalRecord {
                    end_checkpoint: 5,
                    end_coordinate: IntraTxCoordinate::start_of_tx(10),
                    exhaustion: RangeExhaustion::CheckpointBound,
                },
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
        assert_eq!(bounded.edges.terminal.end_checkpoint, 1);
        assert_eq!(
            bounded.edges.terminal.end_coordinate,
            IntraTxCoordinate {
                tx_seq: 3,
                event_index: 0,
            }
        );
        assert_eq!(
            bounded.edges.terminal.exhaustion,
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
        assert_eq!(bounded.edges.terminal.end_checkpoint, 2);
        assert_eq!(
            bounded.edges.terminal.end_coordinate,
            IntraTxCoordinate {
                tx_seq: 10,
                event_index: 0,
            }
        );
        assert_eq!(
            bounded.edges.terminal.exhaustion,
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
            edges: WatermarkEdges {
                entry_checkpoint: 50,
                terminal: TerminalRecord {
                    end_checkpoint: 99,
                    end_coordinate: IntraTxCoordinate::start_of_tx(0),
                    exhaustion: RangeExhaustion::CheckpointBound,
                },
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
        assert_eq!(bounded.edges.terminal.end_checkpoint, 2);
        assert_eq!(
            bounded.edges.terminal.end_coordinate,
            IntraTxCoordinate {
                tx_seq: 4,
                event_index: 1,
            }
        );
        assert_eq!(
            bounded.edges.terminal.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            }
        );
        // Descending entry edge is the before cursor, win or lose.
        assert_eq!(bounded.edges.entry_checkpoint, 8);

        let ascending = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: Some(event_cursor(CursorKind::Item, 2, 4, 1)),
            before: Some(event_cursor(CursorKind::Item, 8, 9, 0)),
        };
        let bounded = ascending.apply_cursor_bounds(resolved);
        assert!(!bounded.is_empty());
        assert_eq!(bounded.edges.terminal.end_checkpoint, 8);
        assert_eq!(
            bounded.edges.terminal.end_coordinate,
            IntraTxCoordinate {
                tx_seq: 9,
                event_index: 0,
            }
        );
        assert_eq!(
            bounded.edges.terminal.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            }
        );
        // Ascending entry edge is the after cursor, win or lose.
        assert_eq!(bounded.edges.entry_checkpoint, 50);
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
            edges: WatermarkEdges {
                entry_checkpoint: 0,
                terminal: TerminalRecord {
                    end_checkpoint: 1,
                    end_coordinate: IntraTxCoordinate::start_of_tx(3),
                    exhaustion: RangeExhaustion::CheckpointBound,
                },
            },
        };

        let mut request = ProtoQueryOptions::default();
        request.after = Some(CursorToken::item(position).encode());
        let options = QueryOptions::events_from_proto(Some(&request), 100, 100).unwrap();
        let item_bounded = options.apply_cursor_bounds(resolved.clone());

        assert!(item_bounded.is_empty());
        assert_eq!(
            item_bounded.edges.terminal.end_coordinate,
            IntraTxCoordinate {
                tx_seq: 3,
                event_index: 0,
            }
        );
        assert_eq!(
            item_bounded.edges.terminal.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Item,
            }
        );

        request.after = Some(CursorToken::boundary(position).encode());
        let options = QueryOptions::events_from_proto(Some(&request), 100, 100).unwrap();
        let boundary_bounded = options.apply_cursor_bounds(resolved);

        assert!(boundary_bounded.is_empty());
        assert_eq!(
            boundary_bounded.edges.terminal.end_coordinate,
            IntraTxCoordinate {
                tx_seq: 3,
                event_index: 0,
            }
        );
        assert_eq!(
            boundary_bounded.edges.terminal.exhaustion,
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
            edges: WatermarkEdges {
                entry_checkpoint: 5,
                terminal: TerminalRecord {
                    end_checkpoint: 9,
                    end_coordinate: 40,
                    exhaustion: RangeExhaustion::CheckpointBound,
                },
            },
        };
        assert_eq!(
            ascending
                .apply_cursor_bounds(resolved.clone())
                .edges
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
            edges: WatermarkEdges {
                entry_checkpoint: 9,
                ..resolved.edges.clone()
            },
            ..resolved
        };
        assert_eq!(
            descending
                .apply_cursor_bounds(resolved)
                .edges
                .entry_checkpoint,
            7
        );
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
