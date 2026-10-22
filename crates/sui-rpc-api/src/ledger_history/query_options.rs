// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::{Bound, Range};

use bytes::Bytes;
use sui_inverted_index::ScanDirection;
use sui_rpc::proto::sui::rpc::v2::Ordering as ProtoOrdering;
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
    pub index: u32,
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
    /// skips an item. Item is preserved for an interval made empty by an
    /// ascending scan's `after` Item cursor, and for a descending scan's
    /// `after` Item cursor at the last position, which has no next position
    /// to name as a boundary; every other site uses Boundary.
    CursorBound { kind: sui_rpc_cursor::CursorKind },
    /// A descending scan with no lower bound of its own reached this server's serving floor, the
    /// lowest checkpoint it still serves; everything below is pruned. The stream sends a watermark
    /// at the floor claiming `floor_checkpoint`, then fails with `OutOfRange` (see
    /// [`below_serving_floor`]) instead of ending with a `QueryEnd`.
    ServingFloor { floor_checkpoint: u64 },
}

/// Validated, normalized form of `QueryOptions` (the proto wire type).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryOptions {
    pub limit_items: usize,
    pub ordering: Ordering,
    after: Option<CursorToken>,
    before: Option<CursorToken>,
}

/// A request's checkpoint window resolved into the checkpoint-sequence interval to scan. Cursors
/// do not narrow it: [`ResolvedScan::resolve`] applies them by scan position. Unlike
/// [`ResolvedScan<u64>`]/[`ResolvedScan<IntraTxCoordinate>`], the scan domain here *is*
/// checkpoint space, so the entry and terminal checkpoints derive from `range` directly and need
/// no extra fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedCheckpointRange {
    /// Half-open checkpoint-sequence interval to scan (numeric order; a
    /// descending scan walks it from `end - 1` down to `start`).
    pub range: Range<u64>,
    /// Why the interval is exhausted once the scan drains it.
    pub exhaustion: RangeExhaustion,
    /// Whether `range.end` is this server's indexed tip rather than a bound the request set, so
    /// that a position at or past it is one this server has not indexed.
    pub reaches_ledger_tip: bool,
}

/// Semantic scan bounds over explicit scan positions (cursor trims need
/// exclusive bounds on either side).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScanBounds<P> {
    pub lo: Bound<P>,
    pub hi: Bound<P>,
}

pub type IntraTxScanBounds = ScanBounds<IntraTxCoordinate>;

/// A request's checkpoint bounds resolved into the scan-position interval to scan, along with the
/// checkpoint-space facts for watermark rendering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedScan<P> {
    /// Scan-position interval to scan, expressed as explicit lo/hi
    /// [`Bound`]s.
    pub bounds: ScanBounds<P>,
    /// The window's first checkpoint in scan direction, raised to the serving floor for an
    /// ascending scan. No coverage claim passes it. Cursors do not move it: a cursor vouches for
    /// every position before it in scan order, so a scan claims each checkpoint whose end it
    /// reaches, wherever its cursor started.
    pub entry_checkpoint: u64,
    /// Checkpoint containing the `end_position`.
    pub end_checkpoint: u64,
    /// Scan-direction terminal edge of the interval.
    pub end_position: P,
    /// Why the interval is exhausted once the scan drains it.
    pub exhaustion: RangeExhaustion,
    /// Whether the request's checkpoint window holds any checkpoint. Cursors do not change it: a
    /// non-empty window's natural end claims its edge even when the scan reads nothing, because
    /// the request's starting cursor vouches for every position before it.
    pub window_has_checkpoints: bool,
}

/// How a lane reads wire cursors: the token's coordinate in the lane's
/// position space.
pub trait ScanCursor<P> {
    fn coordinate(&self) -> P;
}

/// A scan position with a successor, so scan bounds can tell exactly whether they admit any
/// position.
pub trait ScanPosition: Copy + Ord {
    /// The next position in scan order, if there is one.
    fn successor(self) -> Option<Self>;
}

impl ScanPosition for u64 {
    fn successor(self) -> Option<Self> {
        self.checked_add(1)
    }
}

impl ScanPosition for IntraTxCoordinate {
    fn successor(self) -> Option<Self> {
        match self.index.checked_add(1) {
            Some(index) => Some(Self {
                tx_seq: self.tx_seq,
                index,
            }),
            None => self.tx_seq.checked_add(1).map(Self::start_of_tx),
        }
    }
}

impl IntraTxCoordinate {
    /// Fencepost at the first event slot of `tx_seq`; valid as a boundary even
    /// if the transaction has no events.
    pub fn start_of_tx(tx_seq: u64) -> Self {
        Self { tx_seq, index: 0 }
    }

    /// The event-coordinate window spanning whole transactions `[start, end)`.
    pub fn tx_window(range: Range<u64>) -> Range<Self> {
        Self::start_of_tx(range.start)..Self::start_of_tx(range.end)
    }
}

/// The error for a request that needs data below this server's serving floor, whose lowest
/// available checkpoint is `floor_checkpoint`: that data is pruned and permanently gone.
pub fn below_serving_floor(floor_checkpoint: u64) -> RpcError {
    RpcError::new(
        tonic::Code::OutOfRange,
        format!(
            "requested data below earliest available; lowest available checkpoint is {floor_checkpoint}"
        ),
    )
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
    /// `after` cursor. `apply_cursor_bounds` only ever raises `range.start` from
    /// `after` (in both orderings); `before` bounds the high end. Together with an
    /// explicit `start_checkpoint`, this lets the pruning-floor check distinguish
    /// "resume/start from here" (error if below the floor — the data is gone) from
    /// an open-ended low end (clamp up to the floor).
    pub fn has_after_cursor(&self) -> bool {
        self.after.is_some()
    }
}

impl ResolvedCheckpointRange {
    pub fn empty_at(
        checkpoint: u64,
        exhaustion: RangeExhaustion,
        reaches_ledger_tip: bool,
    ) -> Self {
        Self {
            range: checkpoint..checkpoint,
            exhaustion,
            reaches_ledger_tip,
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
}

impl ResolvedScan<u64> {
    /// Collapse the symbolic bounds to the half-open store range.
    pub fn range(&self) -> Range<u64> {
        self.bounds.to_range()
    }
}

impl<P: Copy + Ord> ScanBounds<P> {
    pub fn from_range(range: Range<P>) -> Self {
        Self {
            lo: Bound::Included(range.start),
            hi: Bound::Excluded(range.end),
        }
    }

    pub fn empty_at(position: P) -> Self {
        Self {
            lo: Bound::Included(position),
            hi: Bound::Excluded(position),
        }
    }

    pub fn is_empty(&self) -> bool {
        match (self.lo, self.hi) {
            (Bound::Included(a), Bound::Excluded(b))
            | (Bound::Excluded(a), Bound::Excluded(b))
            | (Bound::Excluded(a), Bound::Included(b)) => a >= b,
            (Bound::Included(a), Bound::Included(b)) => a > b,
            (Bound::Unbounded, _) | (_, Bound::Unbounded) => false,
        }
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
}

impl<P: ScanPosition> ScanBounds<P> {
    /// Whether no position lies within the bounds. [`Self::is_empty`] compares the bounds
    /// symbolically, so it keeps an exclusive low bound right below an exclusive high bound;
    /// this resolves the low bound to its successor first.
    pub fn admits_no_position(&self) -> bool {
        let lo = match self.lo {
            Bound::Excluded(position) => match position.successor() {
                Some(successor) => Bound::Included(successor),
                None => return true,
            },
            lo => lo,
        };
        ScanBounds { lo, hi: self.hi }.is_empty()
    }
}

impl ScanBounds<u64> {
    pub fn to_range(&self) -> Range<u64> {
        let start = match self.lo {
            Bound::Included(position) => position,
            Bound::Excluded(position) => position.saturating_add(1),
            Bound::Unbounded => 0,
        };
        let end = match self.hi {
            Bound::Included(position) => position.saturating_add(1),
            Bound::Excluded(position) => position,
            Bound::Unbounded => u64::MAX,
        };
        start..end
    }
}

impl ScanBounds<IntraTxCoordinate> {
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
            Bound::Excluded(position) if position.index == 0 => position.tx_seq,
            Bound::Included(position) | Bound::Excluded(position) => {
                position.tx_seq.saturating_add(1)
            }
            Bound::Unbounded => u64::MAX,
        };
        (start_tx < end_tx).then_some(start_tx..end_tx)
    }
}

impl<P: ScanPosition> ResolvedScan<P>
where
    CursorToken: ScanCursor<P>,
{
    /// Record the request's checkpoint bounds, then apply cursor bounds to tighten the `range`
    /// scanning interval. Cursors bound the scan by position alone; their checkpoint fields only
    /// label the cursors this scan hands back. If the backend data source has a serving floor,
    /// `apply_serving_floor` must be called to reconcile the scanning interval.
    pub fn resolve(
        cp_range: ResolvedCheckpointRange,
        range: Range<P>,
        options: &QueryOptions,
    ) -> Result<Self, RpcError> {
        if cp_range.reaches_ledger_tip {
            reject_cursors_past_tip(options, range.end, cp_range.range.end)?;
        }

        let entry_checkpoint = if cp_range.is_empty() {
            // No checkpoint entered, pin entry to terminal boundary
            cp_range.range.end
        } else if options.is_ascending() {
            cp_range.range.start
        } else {
            cp_range.range.end - 1
        };

        let resolved = Self {
            bounds: ScanBounds::from_range(range.start..range.end),
            entry_checkpoint,
            end_checkpoint: cp_range.terminal_checkpoint(options.ordering),
            end_position: match options.ordering {
                Ordering::Ascending => range.end,
                Ordering::Descending => range.start,
            },
            exhaustion: cp_range.exhaustion,
            window_has_checkpoints: !cp_range.is_empty(),
        }
        .apply_cursor_bounds(options);

        if !cp_range.is_empty()
            && resolved.bounds.admits_no_position()
            && starting_cursor_at_window_edge(options, &range)
        {
            // The starting cursor sits at the window's edge in scan direction, and no far cursor
            // stops the scan first: the window's edge is what ends this page, as it ended the
            // page that handed the cursor back. A far cursor at that edge, or one that leaves
            // nothing to scan before it, ends the page instead.
            let (checkpoint, position) = match options.ordering {
                Ordering::Ascending => (cp_range.range.end, range.end),
                Ordering::Descending => (cp_range.range.start, range.start),
            };
            return Ok(Self {
                bounds: ScanBounds::empty_at(position),
                end_checkpoint: checkpoint,
                end_position: position,
                exhaustion: cp_range.exhaustion,
                ..resolved
            });
        }
        Ok(resolved)
    }

    pub fn is_empty(&self) -> bool {
        self.bounds.is_empty()
    }

    /// The checkpoint this scan's natural end reports as fully covered in `Watermark.checkpoint`:
    /// the checkpoint just below `end_checkpoint` ascending (the window's exclusive upper edge), or
    /// `end_checkpoint` itself descending (its inclusive lower edge). A scan that drains its window
    /// has covered every checkpoint up to that edge. `None` when the window has no checkpoint, or
    /// when an ascending scan ends at genesis.
    pub fn natural_end_claim(&self, options: &QueryOptions) -> Option<u64> {
        if !self.window_has_checkpoints {
            return None;
        }
        match options.ordering {
            Ordering::Ascending => self.end_checkpoint.checked_sub(1),
            Ordering::Descending => Some(self.end_checkpoint),
        }
    }

    fn apply_cursor_bounds(mut self, options: &QueryOptions) -> Self {
        if self.is_empty() {
            return self;
        }

        let mut cursor_terminal = self.apply_after_cursor(options);

        // Either zero, one, or two cursors emptied the bounds. If the bound is empty, before's
        // position is provably the lower coordinate. Report this minimum of the crossed positions,
        // otherwise we will skip an interval in the ascending case, or rollback scan progress in
        // the descending case.
        if let Some(recording) = self.apply_before_cursor(options) {
            cursor_terminal = Some(recording);
        }

        if let Some((checkpoint, position, kind)) = cursor_terminal {
            self.set_terminal_record(checkpoint, position, RangeExhaustion::CursorBound { kind });

            self.bounds = ScanBounds::empty_at(self.end_position);
        }

        self
    }

    fn set_terminal_record(&mut self, checkpoint: u64, position: P, exhaustion: RangeExhaustion) {
        self.end_checkpoint = checkpoint;
        self.end_position = position;
        self.exhaustion = exhaustion;
    }

    /// Tightens the low bound, and returns the terminal record if the cursor empties the interval.
    fn apply_after_cursor(
        &mut self,
        options: &QueryOptions,
    ) -> Option<(u64, P, sui_rpc_cursor::CursorKind)> {
        let cursor = options.after.as_ref()?;
        let checkpoint = cursor.position.checkpoint();
        let position: P = cursor.coordinate();

        // Symbolic resume in every lane: an Item admits strictly-after, a
        // Boundary admits from itself.
        let candidate = match cursor.kind {
            sui_rpc_cursor::CursorKind::Item => Bound::Excluded(position),
            sui_rpc_cursor::CursorKind::Boundary => Bound::Included(position),
        };

        if !lower_bound_gte(candidate, self.bounds.lo) {
            return None;
        }

        // The after cursor is the terminal bound for a descending scan. Its closing cursor names
        // the boundary just below the lowest position the scan admits, so that sent back as
        // `before` it resumes with the next position down: `after: Boundary(X)` admits X, so the
        // boundary is at X; `after: Item(X)` excludes X, so the boundary is at the position after
        // X. That boundary keeps the item's checkpoint field, the checkpoint of the position
        // before it, as an ascending scan frontier's does. With no position after X, the
        // closing cursor restates the item.
        let (descending_terminal, descending_terminal_kind) = match cursor.kind {
            sui_rpc_cursor::CursorKind::Boundary => {
                (position, sui_rpc_cursor::CursorKind::Boundary)
            }
            sui_rpc_cursor::CursorKind::Item => match position.successor() {
                Some(next) => (next, sui_rpc_cursor::CursorKind::Boundary),
                None => (position, sui_rpc_cursor::CursorKind::Item),
            },
        };
        if !options.is_ascending() {
            self.set_terminal_record(
                checkpoint,
                descending_terminal,
                RangeExhaustion::CursorBound {
                    kind: descending_terminal_kind,
                },
            );
        }

        self.bounds.lo = candidate;

        if !self.bounds.is_empty() {
            return None;
        }

        // Report the cursor that emptied the bound.
        //
        // An ascending interval made empty by an `after` Item cursor must retain Item kind.
        if options.is_ascending() {
            Some((checkpoint, position, cursor.kind))
        } else {
            Some((checkpoint, descending_terminal, descending_terminal_kind))
        }
    }

    /// Tightens the high bound, and returns the terminal record if the cursor empties the interval.
    fn apply_before_cursor(
        &mut self,
        options: &QueryOptions,
    ) -> Option<(u64, P, sui_rpc_cursor::CursorKind)> {
        let cursor = options.before.as_ref()?;
        let checkpoint = cursor.position.checkpoint();
        let position: P = cursor.coordinate();

        if !hi_admits_upper_bound(self.bounds.hi, position) {
            return None;
        }

        if options.is_ascending() {
            self.set_terminal_record(
                checkpoint,
                position,
                RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
            );
        }

        self.bounds.hi = Bound::Excluded(position);

        if !self.bounds.is_empty() {
            return None;
        }

        Some((checkpoint, position, sui_rpc_cursor::CursorKind::Boundary))
    }

    /// Raises the scan's low end to the serving floor, for a scan with no lower bound of its own
    /// whose low end lies below the floor. An ascending scan starts at the floor and claims
    /// coverage from `floor_checkpoint` on. A descending scan ends at the floor with
    /// [`RangeExhaustion::ServingFloor`]: everything below it is pruned.
    ///
    /// A floor that leaves the scan nothing to cover (its `before` cursor sits at or below the
    /// floor) is `OutOfRange`.
    pub fn apply_serving_floor(
        &mut self,
        floor: P,
        floor_checkpoint: u64,
        options: &QueryOptions,
    ) -> Result<(), RpcError> {
        let floored_lo = Bound::Included(floor);
        let floored = ScanBounds {
            lo: floored_lo,
            hi: self.bounds.hi,
        };
        if floored.is_empty() {
            return Err(below_serving_floor(floor_checkpoint));
        }
        self.bounds.lo = floored_lo;
        if options.is_ascending() {
            self.entry_checkpoint = self.entry_checkpoint.max(floor_checkpoint);
        } else {
            self.set_terminal_record(
                floor_checkpoint,
                floor,
                RangeExhaustion::ServingFloor { floor_checkpoint },
            );
        }
        Ok(())
    }
}

/// The requested checkpoint window's validity check, standalone so backends
/// can sequence it independently of options parsing (kv validates bounds
/// before the read mask and options; the fused resolution re-runs it
/// harmlessly).
pub fn validate_checkpoint_bounds(
    start_checkpoint: Option<u64>,
    end_checkpoint: Option<u64>,
) -> Result<(), RpcError> {
    let start = start_checkpoint.unwrap_or(0);
    if let Some(end) = end_checkpoint
        && end < start
    {
        return Err(FieldViolation::new("end_checkpoint")
            .with_description("end_checkpoint must be greater than or equal to start_checkpoint")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }
    Ok(())
}

impl ResolvedCheckpointRange {
    /// Validate the requested checkpoint window, clamp it to the indexed tip, and attribute the
    /// exhaustion reason the scan will report once it drains it.
    ///
    /// A descending window that reaches past the tip returns `OutOfRange`: an `end_checkpoint`
    /// above the tip, or, with `end_checkpoint` unset, a `start_checkpoint` at or above it. A
    /// descending scan starts at its window's high end, so answering from this view would skip
    /// the checkpoints between the tip and that end; like a cursor past the tip, the request
    /// becomes servable once this server catches up. An ascending window past the tip ends at the
    /// tip, where the client resumes once this server has caught up, so only an ascending scan
    /// ends with `LedgerTip`.
    ///
    /// The request's cursors do not narrow the window here. A cursor's checkpoint field is a
    /// label whose meaning depends on the scan that issued it, so [`ResolvedScan::resolve`]
    /// bounds the scan by the cursors' positions and reads the fields only to label checkpoints.
    pub fn from_request(
        start_checkpoint: Option<u64>,
        end_checkpoint: Option<u64>,
        checkpoint_hi_exclusive: u64,
        options: &QueryOptions,
    ) -> Result<Self, RpcError> {
        validate_checkpoint_bounds(start_checkpoint, end_checkpoint)?;
        let start = start_checkpoint.unwrap_or(0);
        let descending_past_tip = match end_checkpoint {
            _ if options.is_ascending() => None,
            Some(end) if end > checkpoint_hi_exclusive => Some(("end_checkpoint", end)),
            None if start >= checkpoint_hi_exclusive => Some(("start_checkpoint", start)),
            Some(_) | None => None,
        };
        if let Some((field, requested)) = descending_past_tip {
            return Err(RpcError::new(
                tonic::Code::OutOfRange,
                format!(
                    "requested data above latest available; {field} is {requested}, and only \
                     checkpoints below {checkpoint_hi_exclusive} are available"
                ),
            ));
        }
        let requested_end = end_checkpoint.unwrap_or(checkpoint_hi_exclusive);
        // Whether the request leaves its window's high end to this server's tip.
        let window_reaches_tip =
            end_checkpoint.is_none() || requested_end > checkpoint_hi_exclusive;
        let end = requested_end.min(checkpoint_hi_exclusive);
        let exhaustion = match options.ordering {
            Ordering::Ascending if window_reaches_tip => RangeExhaustion::LedgerTip,
            Ordering::Ascending | Ordering::Descending => RangeExhaustion::CheckpointBound,
        };

        if options.is_ascending() && start >= checkpoint_hi_exclusive {
            return Ok(Self::empty_at(
                checkpoint_hi_exclusive,
                RangeExhaustion::LedgerTip,
                window_reaches_tip,
            ));
        }
        if start >= end {
            let checkpoint = match options.ordering {
                Ordering::Ascending => end,
                Ordering::Descending => start,
            };
            return Ok(Self::empty_at(
                checkpoint,
                exhaustion,
                window_reaches_tip && checkpoint == checkpoint_hi_exclusive,
            ));
        }
        Ok(Self {
            range: start..end,
            exhaustion,
            reaches_ledger_tip: window_reaches_tip && end == checkpoint_hi_exclusive,
        })
    }
}

/// Whether the request's starting cursor sits exactly at the window's edge in scan direction,
/// `window.end` ascending or `window.start` descending, with no far cursor stopping the scan at
/// or before that edge. An `after` cursor starts an ascending scan at the first position it
/// admits; a `before` cursor starts a descending scan below its position. A far cursor stops the
/// scan as `apply_cursor_bounds` applies it: a `before` at or below the window's end, an `after`
/// at or above the window's start. A starting cursor past the edge sits outside the window, and
/// its page ends at the cursor.
fn starting_cursor_at_window_edge<P: ScanPosition>(
    options: &QueryOptions,
    window: &Range<P>,
) -> bool
where
    CursorToken: ScanCursor<P>,
{
    let position = |cursor: &CursorToken| -> P { cursor.coordinate() };
    match options.ordering {
        Ordering::Ascending => {
            options.after.as_ref().is_some_and(|after| {
                let first_admitted = match after.kind {
                    sui_rpc_cursor::CursorKind::Item => position(after).successor(),
                    sui_rpc_cursor::CursorKind::Boundary => Some(position(after)),
                };
                first_admitted == Some(window.end)
            }) && options
                .before
                .as_ref()
                .is_none_or(|before| position(before) > window.end)
        }
        Ordering::Descending => {
            options
                .before
                .as_ref()
                .is_some_and(|before| position(before) == window.start)
                && options
                    .after
                    .as_ref()
                    .is_none_or(|after| position(after) < window.start)
        }
    }
}

/// Rejects a request whose cursor names a position at or past `tip`, the first position this
/// server has not indexed. Such a cursor comes from a newer view of the ledger than this server's,
/// from another replica or derived from a newer tip. Answering from this view would resume an
/// ascending scan behind its `after` cursor, replaying items the client already has, start a
/// descending scan below its `before` cursor, skipping items the client has not seen, or end a
/// descending scan bounded by its `after` cursor empty, though items lie above that cursor.
/// `OutOfRange` marks the request as reading past the data this server has, the mirror of a
/// cursor below the pruned floor; unlike that one, it becomes servable once this server catches
/// up, so clients may retry it. An ascending scan's `before` cursor may lie past the tip: the
/// scan stops at the tip, and resumes from there once this server has caught up.
fn reject_cursors_past_tip<P: Ord>(
    options: &QueryOptions,
    tip: P,
    checkpoint_hi_exclusive: u64,
) -> Result<(), RpcError>
where
    CursorToken: ScanCursor<P>,
{
    let cursors = match options.ordering {
        Ordering::Ascending => [options.after.as_ref(), None],
        Ordering::Descending => [options.before.as_ref(), options.after.as_ref()],
    };
    for cursor in cursors.into_iter().flatten() {
        let position = cursor.coordinate();
        // A boundary exactly at the tip is where a caught-up scan resumes; an item there, or any
        // position beyond it, has not been indexed here.
        let past_tip = match cursor.kind {
            sui_rpc_cursor::CursorKind::Item => position >= tip,
            sui_rpc_cursor::CursorKind::Boundary => position > tip,
        };
        if past_tip {
            return Err(RpcError::new(
                tonic::Code::OutOfRange,
                format!(
                    "requested data above latest available; cursor is at checkpoint {}, and only \
                     checkpoints below {checkpoint_hi_exclusive} are available",
                    cursor.position.checkpoint(),
                ),
            ));
        }
    }
    Ok(())
}

impl ScanCursor<u64> for CursorToken {
    fn coordinate(&self) -> u64 {
        match self.position {
            Position::Checkpoints { checkpoint } => checkpoint,
            Position::Transactions { tx_seq, .. } => tx_seq,
            Position::Events { .. } => unreachable!("validated at decode"),
        }
    }
}

impl ScanCursor<IntraTxCoordinate> for CursorToken {
    fn coordinate(&self) -> IntraTxCoordinate {
        match self.position {
            Position::Events {
                tx_seq,
                event_index,
                ..
            } => IntraTxCoordinate {
                tx_seq,
                index: event_index,
            },
            _ => unreachable!("validated at decode"),
        }
    }
}

impl From<IntraTxCoordinate> for (u64, u32) {
    fn from(position: IntraTxCoordinate) -> Self {
        (position.tx_seq, position.index)
    }
}

impl From<(u64, u32)> for IntraTxCoordinate {
    fn from((tx_seq, index): (u64, u32)) -> Self {
        Self { tx_seq, index }
    }
}

fn lower_bound_gte<P: Ord + Copy>(candidate: Bound<P>, current: Bound<P>) -> bool {
    let Some(candidate) = lower_bound_key(candidate) else {
        return false;
    };
    match lower_bound_key(current) {
        Some(current) => candidate >= current,
        None => true,
    }
}

fn lower_bound_key<P: Ord + Copy>(bound: Bound<P>) -> Option<(P, u8)> {
    match bound {
        Bound::Included(position) => Some((position, 0)),
        Bound::Excluded(position) => Some((position, 1)),
        Bound::Unbounded => None,
    }
}

fn hi_admits_upper_bound<P: Ord + Copy>(current: Bound<P>, candidate: P) -> bool {
    match current {
        Bound::Included(position) | Bound::Excluded(position) => candidate <= position,
        Bound::Unbounded => true,
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
    use super::*;

    fn query_options_from_proto(
        request: Option<&ProtoQueryOptions>,
    ) -> Result<QueryOptions, RpcError> {
        QueryOptions::transactions_from_proto(request, 100, 1_000)
    }

    fn resolved_range(range: Range<u64>) -> ResolvedScan<u64> {
        ResolvedScan {
            bounds: ScanBounds::from_range(range),
            end_checkpoint: 20,
            end_position: 20,
            exhaustion: RangeExhaustion::CheckpointBound,
            entry_checkpoint: 0,
            window_has_checkpoints: true,
        }
    }

    /// The empty resolution `resolved_range` reaches when a cursor empties it.
    fn empty_resolved_range(
        end_checkpoint: u64,
        end_position: u64,
        exhaustion: RangeExhaustion,
    ) -> ResolvedScan<u64> {
        ResolvedScan {
            bounds: ScanBounds::empty_at(end_position),
            end_checkpoint,
            end_position,
            exhaustion,
            entry_checkpoint: 0,
            window_has_checkpoints: true,
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

    fn ev_item(checkpoint: u64, tx_seq: u64, index: u32) -> CursorToken {
        CursorToken::item(Position::Events {
            checkpoint,
            tx_seq,
            event_index: index,
        })
    }

    fn ev_boundary(checkpoint: u64, tx_seq: u64, index: u32) -> CursorToken {
        CursorToken::boundary(Position::Events {
            checkpoint,
            tx_seq,
            event_index: index,
        })
    }

    fn resolved_intra_tx() -> ResolvedScan<IntraTxCoordinate> {
        ResolvedScan {
            bounds: IntraTxScanBounds::from_range(
                IntraTxCoordinate::start_of_tx(0)..IntraTxCoordinate::start_of_tx(10),
            ),
            entry_checkpoint: 2,
            end_checkpoint: 9,
            end_position: IntraTxCoordinate::start_of_tx(10),
            exhaustion: RangeExhaustion::CheckpointBound,
            window_has_checkpoints: true,
        }
    }

    fn after_options(ordering: Ordering, cursor: CursorToken) -> QueryOptions {
        QueryOptions {
            limit_items: 100,
            ordering,
            after: Some(cursor),
            before: None,
        }
    }

    fn before_options(ordering: Ordering, cursor: CursorToken) -> QueryOptions {
        QueryOptions {
            limit_items: 100,
            ordering,
            after: None,
            before: Some(cursor),
        }
    }

    fn directional_options(ascending: bool) -> QueryOptions {
        let mut request = ProtoQueryOptions::default();
        if !ascending {
            request.ordering = Some(ProtoOrdering::Descending as i32);
        }
        query_options_from_proto(Some(&request)).unwrap()
    }

    /// Asserts `result` is the `OutOfRange` error for data below a serving floor at
    /// `floor_checkpoint`.
    fn assert_below_serving_floor(result: Result<(), RpcError>, floor_checkpoint: u64) {
        let status = result
            .expect_err("expected OutOfRange below the serving floor")
            .into_status_proto();
        assert_eq!(status.code, tonic::Code::OutOfRange as i32, "{status:?}");
        assert_eq!(
            status.message,
            format!(
                "requested data below earliest available; lowest available checkpoint is {floor_checkpoint}"
            )
        );
    }

    /// Serving-floor reconciliation for tx intervals: a floor inside the interval starts an
    /// ascending scan there and raises its entry claim, and ends a descending scan there with
    /// [`RangeExhaustion::ServingFloor`]. A floor at/past the high end leaves nothing to cover and
    /// is `OutOfRange` in both directions.
    #[test]
    fn tx_serving_floor_reconciles_or_rejects_consumed_scan() {
        let asc = directional_options(true);
        let desc = directional_options(false);

        // Ascending, floor inside 0..100: scan starts at the floor; the
        // entry claim rises to the floor checkpoint; terminal untouched.
        let mut resolved = ResolvedScan {
            bounds: ScanBounds::from_range(0..100),
            end_checkpoint: 20,
            end_position: 100,
            exhaustion: RangeExhaustion::CheckpointBound,
            entry_checkpoint: 0,
            window_has_checkpoints: true,
        };
        resolved.apply_serving_floor(50, 10, &asc).unwrap();
        assert_eq!(resolved.natural_end_claim(&asc), Some(19));
        assert_eq!(resolved.range(), 50..100);
        assert_eq!(resolved.entry_checkpoint, 10);
        assert_eq!(resolved.end_checkpoint, 20);
        assert_eq!(resolved.end_position, 100);
        assert_eq!(resolved.exhaustion, RangeExhaustion::CheckpointBound);

        // Descending, floor inside: entry (the high edge) untouched; the
        // scan ends at the floor, which it reaches as its serving floor.
        let mut resolved = ResolvedScan {
            bounds: ScanBounds::from_range(0..100),
            end_checkpoint: 0,
            end_position: 0,
            exhaustion: RangeExhaustion::CheckpointBound,
            entry_checkpoint: 20,
            window_has_checkpoints: true,
        };
        resolved.apply_serving_floor(50, 10, &desc).unwrap();
        assert_eq!(resolved.natural_end_claim(&desc), Some(10));
        assert_eq!(resolved.range(), 50..100);
        assert_eq!(resolved.entry_checkpoint, 20);
        assert_eq!(resolved.end_checkpoint, 10);
        assert_eq!(resolved.end_position, 50);
        assert_eq!(
            resolved.exhaustion,
            RangeExhaustion::ServingFloor {
                floor_checkpoint: 10
            }
        );

        // Floor at/past the high end (covers the == boundary), both
        // directions: nothing left to cover is `OutOfRange`.
        for floor_tx in [40, 50] {
            let mut resolved = ResolvedScan {
                bounds: ScanBounds::from_range(0..40),
                end_checkpoint: 8,
                end_position: 40,
                exhaustion: RangeExhaustion::CheckpointBound,
                entry_checkpoint: 0,
                window_has_checkpoints: true,
            };
            assert_below_serving_floor(resolved.apply_serving_floor(floor_tx, 10, &asc), 10);

            let mut resolved = ResolvedScan {
                bounds: ScanBounds::from_range(0..40),
                end_checkpoint: 0,
                end_position: 0,
                exhaustion: RangeExhaustion::CheckpointBound,
                entry_checkpoint: 8,
                window_has_checkpoints: true,
            };
            assert_below_serving_floor(resolved.apply_serving_floor(floor_tx, 10, &desc), 10);
        }
    }

    /// [`ResolvedScan::<IntraTxCoordinate>::apply_serving_floor`] mirrors the tx behavior in
    /// event coordinates.
    #[test]
    fn event_serving_floor_reconciles_or_rejects_consumed_scan() {
        let asc = directional_options(true);
        let desc = directional_options(false);

        // Ascending, floor inside: low bound moves, entry rises, terminal
        // untouched.
        let mut resolved = ResolvedScan {
            bounds: IntraTxScanBounds::from_range(
                IntraTxCoordinate::start_of_tx(0)..IntraTxCoordinate::start_of_tx(100),
            ),
            end_checkpoint: 20,
            end_position: IntraTxCoordinate::start_of_tx(100),
            exhaustion: RangeExhaustion::CheckpointBound,
            entry_checkpoint: 0,
            window_has_checkpoints: true,
        };
        resolved
            .apply_serving_floor(IntraTxCoordinate::start_of_tx(50), 10, &asc)
            .unwrap();
        assert_eq!(resolved.natural_end_claim(&asc), Some(19));
        assert_eq!(
            resolved.bounds.lo,
            Bound::Included(IntraTxCoordinate::start_of_tx(50))
        );
        assert_eq!(resolved.entry_checkpoint, 10);
        assert_eq!(resolved.end_checkpoint, 20);
        assert_eq!(resolved.end_position, IntraTxCoordinate::start_of_tx(100));

        // Descending, floor inside: the scan ends at the floor, which it reaches as its serving
        // floor.
        let mut resolved = ResolvedScan {
            bounds: IntraTxScanBounds::from_range(
                IntraTxCoordinate::start_of_tx(0)..IntraTxCoordinate::start_of_tx(100),
            ),
            end_checkpoint: 0,
            end_position: IntraTxCoordinate::start_of_tx(0),
            exhaustion: RangeExhaustion::CheckpointBound,
            entry_checkpoint: 20,
            window_has_checkpoints: true,
        };
        resolved
            .apply_serving_floor(IntraTxCoordinate::start_of_tx(50), 10, &desc)
            .unwrap();
        assert_eq!(resolved.natural_end_claim(&desc), Some(10));
        assert_eq!(
            resolved.bounds.lo,
            Bound::Included(IntraTxCoordinate::start_of_tx(50))
        );
        assert_eq!(resolved.entry_checkpoint, 20);
        assert_eq!(resolved.end_checkpoint, 10);
        assert_eq!(resolved.end_position, IntraTxCoordinate::start_of_tx(50));
        assert_eq!(
            resolved.exhaustion,
            RangeExhaustion::ServingFloor {
                floor_checkpoint: 10
            }
        );

        // Floor that empties the bounds (covers the == boundary), both
        // directions: nothing left to cover is `OutOfRange`.
        for floor_tx in [40, 50] {
            let mut resolved = ResolvedScan {
                bounds: IntraTxScanBounds::from_range(
                    IntraTxCoordinate::start_of_tx(0)..IntraTxCoordinate::start_of_tx(40),
                ),
                end_checkpoint: 8,
                end_position: IntraTxCoordinate::start_of_tx(40),
                exhaustion: RangeExhaustion::CheckpointBound,
                entry_checkpoint: 0,
                window_has_checkpoints: true,
            };
            assert_below_serving_floor(
                resolved.apply_serving_floor(IntraTxCoordinate::start_of_tx(floor_tx), 10, &asc),
                10,
            );

            let mut resolved = ResolvedScan {
                bounds: IntraTxScanBounds::from_range(
                    IntraTxCoordinate::start_of_tx(0)..IntraTxCoordinate::start_of_tx(40),
                ),
                end_checkpoint: 0,
                end_position: IntraTxCoordinate::start_of_tx(0),
                exhaustion: RangeExhaustion::CheckpointBound,
                entry_checkpoint: 8,
                window_has_checkpoints: true,
            };
            assert_below_serving_floor(
                resolved.apply_serving_floor(IntraTxCoordinate::start_of_tx(floor_tx), 10, &desc),
                10,
            );
        }
    }

    /// A natural end claims the window's edge in scan direction: `end_checkpoint - 1` ascending
    /// (the exclusive upper edge) and `end_checkpoint` descending (the inclusive lower edge). An
    /// ascending scan ending at genesis, or an empty window, claims nothing.
    #[test]
    fn natural_end_claims_the_window_edge() {
        let asc = directional_options(true);
        let desc = directional_options(false);
        let resolved = ResolvedScan {
            end_checkpoint: 10,
            ..resolved_range(0..100)
        };
        assert_eq!(resolved.natural_end_claim(&asc), Some(9));
        assert_eq!(resolved.natural_end_claim(&desc), Some(10));

        let at_genesis = ResolvedScan {
            end_checkpoint: 0,
            ..resolved_range(0..100)
        };
        assert_eq!(at_genesis.natural_end_claim(&asc), None);
        assert_eq!(at_genesis.natural_end_claim(&desc), Some(0));

        let empty_window = ResolvedScan {
            window_has_checkpoints: false,
            ..resolved
        };
        assert_eq!(empty_window.natural_end_claim(&asc), None);
        assert_eq!(empty_window.natural_end_claim(&desc), None);
    }

    /// A descending walk with no lower bound of its own ends at the serving floor with
    /// [`RangeExhaustion::ServingFloor`], claiming the floor checkpoint. Resuming it with a
    /// `before` cursor at the floor or below it, in either ordering, leaves nothing above the
    /// floor to cover and is `OutOfRange`.
    #[test]
    fn descending_walk_ends_at_serving_floor_and_resuming_below_it_is_out_of_range() {
        let floor_checkpoint = 10;
        let floor = first_tx(floor_checkpoint);

        let walk = directional_options(false);
        let mut reached_floor = resolve_tx_request((None, None), &walk).unwrap();
        reached_floor
            .apply_serving_floor(floor, floor_checkpoint, &walk)
            .unwrap();
        assert_eq!(reached_floor.range(), floor..first_tx(TIP));
        assert_eq!(
            (
                reached_floor.exhaustion,
                reached_floor.end_checkpoint,
                reached_floor.end_position,
                reached_floor.natural_end_claim(&walk),
            ),
            (
                RangeExhaustion::ServingFloor { floor_checkpoint },
                floor_checkpoint,
                floor,
                Some(floor_checkpoint),
            )
        );

        let at_floor = tx_boundary(reached_floor.end_checkpoint, reached_floor.end_position);
        let below_floor = tx_boundary(6, 33);
        for ordering in [Ordering::Descending, Ordering::Ascending] {
            for before in [at_floor.clone(), below_floor.clone()] {
                let resume = before_options(ordering, before);
                let mut resumed = resolve_tx_request((None, None), &resume).unwrap();
                assert_below_serving_floor(
                    resumed.apply_serving_floor(floor, floor_checkpoint, &resume),
                    floor_checkpoint,
                );
            }
        }
    }

    #[test]
    fn tx_range_covers_partial_endpoint_transactions() {
        let bounds = IntraTxScanBounds {
            lo: Bound::Included(IntraTxCoordinate {
                tx_seq: 10,
                index: 2,
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
                index: 1,
            }),
        };

        assert_eq!(bounds.tx_range(), Some(0..14));
    }

    #[test]
    fn tx_range_empty_bounds_yield_none() {
        let bounds = IntraTxScanBounds::from_range(
            IntraTxCoordinate::start_of_tx(10)..IntraTxCoordinate::start_of_tx(10),
        );
        assert_eq!(bounds.tx_range(), None);
    }

    /// Every after-Item cursor now resumes as an Excluded lo; the successor
    /// arithmetic and its `u64::MAX` saturation live here at the store edge.
    #[test]
    fn to_range_collapses_excluded_lo_at_successor() {
        let bounds = ScanBounds {
            lo: Bound::Excluded(14u64),
            hi: Bound::Excluded(20u64),
        };
        assert_eq!(bounds.to_range(), 15..20);

        // The saturated successor at the unoccupiable sentinel admits
        // nothing, through the ordinary emptiness of MAX..MAX.
        let bounds = ScanBounds {
            lo: Bound::Excluded(u64::MAX),
            hi: Bound::Unbounded,
        };
        assert!(bounds.to_range().is_empty());
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
            resolved_range(0..100).apply_cursor_bounds(&options).range(),
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
        let range =
            ResolvedCheckpointRange::from_request(Some(1_000), Some(1_100), 2_000, &options)
                .unwrap();

        assert_eq!(range.range, 1_000..1_100);
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
            resolved_range(10..20).apply_cursor_bounds(&options).range(),
            12..20
        );

        let options = QueryOptions {
            after: Some(tx_item(1, u64::MAX)),
            ..options
        };
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            empty_resolved_range(
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
        let bounded = resolved_range(10..20).apply_cursor_bounds(&options);
        assert_eq!(bounded.range(), 12..19);
        assert_eq!(
            bounded.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            }
        );
        // The terminal is the boundary just above the `after` item, which the scan excluded.
        assert_eq!(bounded.end_position, 12);
    }

    // (descending, after = Item 11, before = Item 12): the cursors leave the
    // empty gap (11, 12), so the fetch serves nothing and the CursorBound
    // frame comes from the descending stop side: the boundary just above the
    // after item, at 12.
    #[test]
    fn descending_before_adjacent_to_after_empties_above_the_after_item() {
        let options = QueryOptions {
            limit_items: 2,
            ordering: Ordering::Descending,
            after: Some(tx_item(1, 11)),
            before: Some(tx_item(1, 12)),
        };
        let crossed = resolved_range(10..20).apply_cursor_bounds(&options);
        assert_eq!(
            crossed,
            ResolvedScan {
                bounds: ScanBounds {
                    lo: Bound::Excluded(11),
                    hi: Bound::Excluded(12),
                },
                entry_checkpoint: 0,
                end_checkpoint: 1,
                end_position: 12,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
                window_has_checkpoints: true,
            }
        );
        assert!(crossed.range().is_empty());
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
            resolved_range(10..30).apply_cursor_bounds(&options).range(),
            20..30
        );

        let options = QueryOptions {
            ordering: Ordering::Descending,
            after: None,
            before: Some(tx_boundary(2, 20)),
            ..options
        };
        assert_eq!(
            resolved_range(10..30).apply_cursor_bounds(&options).range(),
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
            ResolvedCheckpointRange::empty_at(20, RangeExhaustion::LedgerTip, true)
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

    /// (start_checkpoint = 30, tip = 20, ascending): resolves empty at the tip — entry and
    /// terminal checkpoints 20, LedgerTip.
    #[test]
    fn ascending_window_past_tip_empties_with_ledger_tip() {
        let options = QueryOptions::events_from_proto(None, 100, 100).unwrap();
        let cp_range = ResolvedCheckpointRange::from_request(Some(30), None, 20, &options).unwrap();
        assert!(cp_range.is_empty());
        assert_eq!(
            ResolvedScan::<IntraTxCoordinate>::resolve(
                cp_range,
                IntraTxCoordinate::tx_window(100..100),
                &options
            )
            .unwrap(),
            ResolvedScan {
                bounds: IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(100)),
                entry_checkpoint: 20,
                end_checkpoint: 20,
                end_position: IntraTxCoordinate::start_of_tx(100),
                exhaustion: RangeExhaustion::LedgerTip,
                window_has_checkpoints: false,
            }
        );
    }

    /// A descending window that reaches past the tip (20) is out of range: an `end_checkpoint`
    /// above the tip, or a `start_checkpoint` at or above it with `end_checkpoint` unset. A
    /// window that ends at the tip resolves, and a zero-width window there is empty with
    /// CheckpointBound.
    #[test]
    fn descending_window_past_tip_is_out_of_range() {
        let mut request = ProtoQueryOptions::default();
        request.ordering = Some(ProtoOrdering::Descending as i32);
        let options = QueryOptions::events_from_proto(Some(&request), 100, 100).unwrap();
        for (start, end) in [
            (Some(30), None),
            (Some(20), None),
            (Some(10), Some(21)),
            (None, Some(u64::MAX)),
            (Some(25), Some(30)),
        ] {
            let error = ResolvedCheckpointRange::from_request(start, end, 20, &options)
                .expect_err("descending window past the tip");
            assert_eq!(
                error.into_status_proto().code,
                tonic::Code::OutOfRange as i32,
                "window {start:?}..{end:?}"
            );
        }

        let resolved = ResolvedCheckpointRange::from_request(Some(10), Some(20), 20, &options);
        assert_eq!(resolved.unwrap().range, 10..20);
        assert_eq!(
            ResolvedCheckpointRange::from_request(Some(20), Some(20), 20, &options).unwrap(),
            ResolvedCheckpointRange::empty_at(20, RangeExhaustion::CheckpointBound, false)
        );
    }

    /// When the resolved checkpoint range is below checkpoint_hi_exclusive and empties, the
    /// exhaustion reason is CheckpointBound, not LedgerTip.
    #[test]
    fn resolve_zero_width_window_empties_with_checkpoint_bound() {
        for ascending in [true, false] {
            let mut request = ProtoQueryOptions::default();
            if !ascending {
                request.ordering = Some(ProtoOrdering::Descending as i32);
            }
            let options = QueryOptions::events_from_proto(Some(&request), 100, 100).unwrap();
            let cp_range =
                ResolvedCheckpointRange::from_request(Some(10), Some(10), 20, &options).unwrap();
            assert!(cp_range.is_empty());
            assert_eq!(
                ResolvedScan::<IntraTxCoordinate>::resolve(
                    cp_range,
                    IntraTxCoordinate::tx_window(100..100),
                    &options
                )
                .unwrap(),
                ResolvedScan {
                    bounds: IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(100)),
                    entry_checkpoint: 10,
                    end_checkpoint: 10,
                    end_position: IntraTxCoordinate::start_of_tx(100),
                    exhaustion: RangeExhaustion::CheckpointBound,
                    window_has_checkpoints: false,
                }
            );
        }
    }

    /// If the bounds are already empty, cursors do not apply.
    #[test]
    fn cursor_bounds_pass_empty_resolution_through_unchanged() {
        let position = Position::Events {
            checkpoint: 4,
            tx_seq: 50,
            event_index: 2,
        };
        for ascending in [true, false] {
            let mut request = ProtoQueryOptions::default();
            if !ascending {
                request.ordering = Some(ProtoOrdering::Descending as i32);
            }
            request.after = Some(CursorToken::item(position).encode());
            let options = QueryOptions::events_from_proto(Some(&request), 100, 100).unwrap();
            let cp_range =
                ResolvedCheckpointRange::from_request(Some(10), Some(10), 20, &options).unwrap();
            let resolved = ResolvedScan::<IntraTxCoordinate>::resolve(
                cp_range,
                IntraTxCoordinate::tx_window(100..100),
                &options,
            )
            .unwrap();
            assert_eq!(resolved.clone().apply_cursor_bounds(&options), resolved);
        }
    }

    /// resolve sets entry_checkpoint to the scan's first checkpoint and the terminal
    /// (end_checkpoint, end_position) to its last, per ordering.
    #[test]
    fn resolve_orients_entry_and_terminal_by_ordering() {
        let options = directional_options(true);
        let cp_range =
            ResolvedCheckpointRange::from_request(Some(3), Some(10), 20, &options).unwrap();
        assert_eq!(cp_range.range, 3..10);
        let resolved = ResolvedScan::<IntraTxCoordinate>::resolve(
            cp_range.clone(),
            IntraTxCoordinate::tx_window(100..200),
            &options,
        )
        .unwrap();
        assert_eq!(
            resolved.bounds,
            IntraTxScanBounds::from_range(
                IntraTxCoordinate::start_of_tx(100)..IntraTxCoordinate::start_of_tx(200)
            )
        );
        assert_eq!(resolved.entry_checkpoint, 3);
        assert_eq!(resolved.end_checkpoint, 10);
        assert_eq!(resolved.end_position, IntraTxCoordinate::start_of_tx(200));

        let options = directional_options(false);
        let resolved = ResolvedScan::<IntraTxCoordinate>::resolve(
            cp_range,
            IntraTxCoordinate::tx_window(100..200),
            &options,
        )
        .unwrap();
        assert_eq!(resolved.entry_checkpoint, 9);
        assert_eq!(resolved.end_checkpoint, 3);
        assert_eq!(resolved.end_position, IntraTxCoordinate::start_of_tx(100));
    }

    /// (ascending, after at the window's end coordinate): empties the bounds at the
    /// cursor, and the terminal keeps the cursor's kind — an after-Item echo must stay
    /// Item so resume doesn't re-serve it.
    #[test]
    fn event_after_item_empty_interval_retains_item_kind() {
        let position = Position::Events {
            checkpoint: 1,
            tx_seq: 3,
            event_index: 0,
        };
        let resolved = ResolvedScan {
            bounds: IntraTxScanBounds::from_range(
                IntraTxCoordinate::start_of_tx(0)..IntraTxCoordinate::start_of_tx(3),
            ),
            end_checkpoint: 1,
            end_position: IntraTxCoordinate::start_of_tx(3),
            exhaustion: RangeExhaustion::CheckpointBound,
            entry_checkpoint: 0,
            window_has_checkpoints: true,
        };

        let mut request = ProtoQueryOptions::default();
        request.after = Some(CursorToken::item(position).encode());
        let options = QueryOptions::events_from_proto(Some(&request), 100, 100).unwrap();
        let item_bounded = resolved.clone().apply_cursor_bounds(&options);

        assert!(item_bounded.is_empty());
        assert_eq!(
            item_bounded.end_position,
            IntraTxCoordinate {
                tx_seq: 3,
                index: 0,
            }
        );
        assert_eq!(
            item_bounded.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Item,
            }
        );

        request.after = Some(CursorToken::boundary(position).encode());
        let options = QueryOptions::events_from_proto(Some(&request), 100, 100).unwrap();
        let boundary_bounded = resolved.apply_cursor_bounds(&options);

        assert!(boundary_bounded.is_empty());
        assert_eq!(
            boundary_bounded.end_position,
            IntraTxCoordinate {
                tx_seq: 3,
                index: 0,
            }
        );
        assert_eq!(
            boundary_bounded.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            }
        );
    }

    /// `before` cursor that empties the interval sets end_position, and exhaustion to CursorBound.
    #[test]
    fn event_before_cursor_empties_descending_interval() {
        // Start descending over tx range [100, 200) with entry at checkpoint 9 terminating at checkpoint 3
        let resolved = ResolvedScan {
            // always in ascending order
            bounds: IntraTxScanBounds::from_range(
                IntraTxCoordinate::start_of_tx(100)..IntraTxCoordinate::start_of_tx(200),
            ),
            // checkpoints reflect ordering
            entry_checkpoint: 9,
            end_checkpoint: 3,
            end_position: IntraTxCoordinate::start_of_tx(100),
            exhaustion: RangeExhaustion::CheckpointBound,
            window_has_checkpoints: true,
        };

        // This cursor empties the interval; the entry checkpoint stays at the window's.
        let cursor = Position::Events {
            checkpoint: 2,
            tx_seq: 90,
            event_index: 0,
        };

        for token in [CursorToken::item(cursor), CursorToken::boundary(cursor)] {
            let mut request = ProtoQueryOptions::default();
            request.ordering = Some(ProtoOrdering::Descending as i32);
            request.before = Some(token.encode());
            let options = QueryOptions::events_from_proto(Some(&request), 100, 100).unwrap();

            let new_coordinate = IntraTxCoordinate::start_of_tx(90);
            assert_eq!(
                resolved.clone().apply_cursor_bounds(&options),
                ResolvedScan {
                    bounds: IntraTxScanBounds::empty_at(new_coordinate),
                    entry_checkpoint: 9,
                    end_checkpoint: 2,
                    // takes on the cursor position that emptied the range
                    end_position: new_coordinate,
                    exhaustion: RangeExhaustion::CursorBound {
                        kind: sui_rpc_cursor::CursorKind::Boundary,
                    },
                    window_has_checkpoints: true,
                }
            );
        }
    }

    /// (ascending, after inside the window): Item admits strictly after its
    /// coordinate, Boundary from it; entry checkpoint and terminal untouched.
    #[test]
    fn after_cursor_tightens_ascending_lower_bound() {
        let options = after_options(Ordering::Ascending, ev_item(3, 5, 1));
        assert_eq!(
            resolved_intra_tx().apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: IntraTxScanBounds {
                    lo: Bound::Excluded(IntraTxCoordinate {
                        tx_seq: 5,
                        index: 1,
                    }),
                    hi: Bound::Excluded(IntraTxCoordinate::start_of_tx(10)),
                },
                ..resolved_intra_tx()
            }
        );

        let options = after_options(Ordering::Ascending, ev_boundary(3, 5, 1));
        assert_eq!(
            resolved_intra_tx().apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: IntraTxScanBounds {
                    lo: Bound::Included(IntraTxCoordinate {
                        tx_seq: 5,
                        index: 1,
                    }),
                    hi: Bound::Excluded(IntraTxCoordinate::start_of_tx(10)),
                },
                ..resolved_intra_tx()
            }
        );
    }

    #[test]
    fn after_cursor_below_lower_bound_changes_nothing() {
        let seed = ResolvedScan {
            bounds: IntraTxScanBounds {
                lo: Bound::Included(IntraTxCoordinate::start_of_tx(5)),
                hi: Bound::Excluded(IntraTxCoordinate::start_of_tx(10)),
            },
            ..resolved_intra_tx()
        };
        let options = after_options(Ordering::Ascending, ev_boundary(4, 2, 0));
        assert_eq!(seed.clone().apply_cursor_bounds(&options), seed);
        let options = after_options(Ordering::Ascending, ev_item(4, 2, 0));
        assert_eq!(seed.clone().apply_cursor_bounds(&options), seed);
    }

    /// Tightens the lower bound, and the terminal becomes the boundary just below the lowest
    /// admitted position, because the after cursor is where a descending scan stops: the slot
    /// after an Item cursor, which the scan excluded, and a Boundary cursor itself.
    #[test]
    fn descending_after_cursor_sets_terminal_and_tightens() {
        let options = after_options(Ordering::Descending, ev_item(3, 5, 1));
        assert_eq!(
            resolved_intra_tx().apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: IntraTxScanBounds {
                    lo: Bound::Excluded(IntraTxCoordinate {
                        tx_seq: 5,
                        index: 1,
                    }),
                    hi: Bound::Excluded(IntraTxCoordinate::start_of_tx(10)),
                },
                end_checkpoint: 3,
                end_position: IntraTxCoordinate {
                    tx_seq: 5,
                    index: 2,
                },
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
                ..resolved_intra_tx()
            }
        );

        let options = after_options(Ordering::Descending, ev_boundary(3, 5, 1));
        assert_eq!(
            resolved_intra_tx().apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: IntraTxScanBounds {
                    lo: Bound::Included(IntraTxCoordinate {
                        tx_seq: 5,
                        index: 1,
                    }),
                    hi: Bound::Excluded(IntraTxCoordinate::start_of_tx(10)),
                },
                end_checkpoint: 3,
                end_position: IntraTxCoordinate {
                    tx_seq: 5,
                    index: 1,
                },
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
                ..resolved_intra_tx()
            }
        );
    }

    /// When the after cursor empties the window, the record empties at the boundary just below
    /// what the cursor admits, at the cursor's checkpoint: the slot after an Item cursor, a
    /// Boundary cursor itself, both as Boundary.
    #[test]
    fn after_cursor_empties_descending_interval() {
        let seed = ResolvedScan {
            bounds: IntraTxScanBounds::from_range(
                IntraTxCoordinate::start_of_tx(3)..IntraTxCoordinate::start_of_tx(9),
            ),
            entry_checkpoint: 9,
            end_checkpoint: 3,
            end_position: IntraTxCoordinate::start_of_tx(3),
            exhaustion: RangeExhaustion::CheckpointBound,
            window_has_checkpoints: true,
        };

        let emptied_at = |coordinate| ResolvedScan {
            bounds: IntraTxScanBounds::empty_at(coordinate),
            entry_checkpoint: 9,
            end_checkpoint: 100,
            end_position: coordinate,
            exhaustion: RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            },
            window_has_checkpoints: true,
        };

        let options = after_options(Ordering::Descending, ev_item(100, 100, 10));
        assert_eq!(
            seed.clone().apply_cursor_bounds(&options),
            emptied_at(IntraTxCoordinate {
                tx_seq: 100,
                index: 11,
            })
        );
        let options = after_options(Ordering::Descending, ev_boundary(100, 100, 10));
        assert_eq!(
            seed.apply_cursor_bounds(&options),
            emptied_at(IntraTxCoordinate {
                tx_seq: 100,
                index: 10,
            })
        );
    }

    /// (descending, before inside the window, either cursor kind (Item or
    /// Boundary)): hi becomes exclusive at the cursor; entry checkpoint and
    /// terminal untouched.
    #[test]
    fn before_cursor_tightens_descending_upper_bound() {
        let seed = ResolvedScan {
            entry_checkpoint: 8,
            ..resolved_intra_tx()
        };
        let expected = ResolvedScan {
            bounds: IntraTxScanBounds {
                lo: Bound::Included(IntraTxCoordinate::start_of_tx(0)),
                hi: Bound::Excluded(IntraTxCoordinate {
                    tx_seq: 5,
                    index: 1,
                }),
            },
            ..seed.clone()
        };
        let options = before_options(Ordering::Descending, ev_item(6, 5, 1));
        assert_eq!(seed.clone().apply_cursor_bounds(&options), expected);
        let options = before_options(Ordering::Descending, ev_boundary(6, 5, 1));
        assert_eq!(seed.apply_cursor_bounds(&options), expected);
    }

    #[test]
    fn before_cursor_above_window_never_tightens() {
        for cursor in [ev_boundary(1, 12, 0), ev_item(1, 12, 0)] {
            let options = before_options(Ordering::Descending, cursor.clone());
            assert_eq!(
                resolved_intra_tx().apply_cursor_bounds(&options),
                resolved_intra_tx()
            );

            let options = before_options(Ordering::Ascending, cursor);
            assert_eq!(
                resolved_intra_tx().apply_cursor_bounds(&options),
                resolved_intra_tx()
            );
        }
    }

    #[test]
    fn ascending_before_cursor_sets_terminal_and_tightens() {
        let expected = ResolvedScan {
            bounds: IntraTxScanBounds {
                lo: Bound::Included(IntraTxCoordinate::start_of_tx(0)),
                hi: Bound::Excluded(IntraTxCoordinate {
                    tx_seq: 5,
                    index: 1,
                }),
            },
            entry_checkpoint: 2,
            end_checkpoint: 6,
            end_position: IntraTxCoordinate {
                tx_seq: 5,
                index: 1,
            },
            exhaustion: RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            },
            window_has_checkpoints: true,
        };
        let options = before_options(Ordering::Ascending, ev_boundary(6, 5, 1));
        assert_eq!(resolved_intra_tx().apply_cursor_bounds(&options), expected);
        let options = before_options(Ordering::Ascending, ev_item(6, 5, 1));
        assert_eq!(resolved_intra_tx().apply_cursor_bounds(&options), expected);
    }

    /// When a before cursor empties an ascending interval, the bounds are emptied at the cursor,
    /// and end_position and end_checkpoint are set to the cursor's.
    #[test]
    fn before_cursor_empties_ascending_interval() {
        let seed = ResolvedScan {
            bounds: IntraTxScanBounds::from_range(
                IntraTxCoordinate::start_of_tx(3)..IntraTxCoordinate::start_of_tx(10),
            ),
            entry_checkpoint: 2,
            end_checkpoint: 9,
            ..resolved_intra_tx()
        };
        let expected = ResolvedScan {
            bounds: IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(3)),
            end_checkpoint: 1,
            end_position: IntraTxCoordinate::start_of_tx(3),
            exhaustion: RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            },
            ..seed.clone()
        };
        let options = before_options(Ordering::Ascending, ev_boundary(1, 3, 0));
        assert_eq!(seed.clone().apply_cursor_bounds(&options), expected);
        let options = before_options(Ordering::Ascending, ev_item(1, 3, 0));
        assert_eq!(seed.apply_cursor_bounds(&options), expected);
    }

    /// (ascending, after and before both present): if only the after files, its
    /// frame stands (Item kind retained); if both file, the before wins at the min
    /// position.
    #[test]
    fn ascending_with_both_cursors_picks_the_terminal_owner() {
        // The before cursor falls out of the range so it is rejected. The after cursor empties the
        // range, and is attributed to the exhaustion reason.
        let options = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: Some(ev_item(12, 12, 0)),
            before: Some(ev_boundary(14, 14, 0)),
        };
        assert_eq!(
            resolved_intra_tx().apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(12)),
                entry_checkpoint: 2,
                end_checkpoint: 12,
                end_position: IntraTxCoordinate::start_of_tx(12),
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Item,
                },
                window_has_checkpoints: true,
            }
        );

        // When both cursors would empty, `before` cursor attribution remains.
        let options = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: Some(ev_boundary(12, 12, 0)),
            before: Some(ev_boundary(3, 3, 0)),
        };
        assert_eq!(
            resolved_intra_tx().apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(3)),
                entry_checkpoint: 2,
                end_checkpoint: 3,
                end_position: IntraTxCoordinate::start_of_tx(3),
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
                window_has_checkpoints: true,
            }
        );
    }

    /// (descending, after and before both present): the after's frame stands when
    /// only it files; when both file, the min position wins; a before that empties
    /// overrides the after's earlier terminal.
    #[test]
    fn descending_with_both_cursors_picks_the_terminal_owner() {
        // After empties, before rejected: the after-claim survives.
        let options = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Descending,
            after: Some(ev_boundary(9, 12, 0)),
            before: Some(ev_boundary(1, 15, 0)),
        };
        assert_eq!(
            resolved_intra_tx().apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(12)),
                entry_checkpoint: 2,
                end_checkpoint: 9,
                end_position: IntraTxCoordinate::start_of_tx(12),
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
                window_has_checkpoints: true,
            }
        );

        // After empties, before also files: before wins at the min position.
        let options = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Descending,
            after: Some(ev_boundary(9, 12, 0)),
            before: Some(ev_item(1, 5, 2)),
        };
        assert_eq!(
            resolved_intra_tx().apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: IntraTxScanBounds::empty_at(IntraTxCoordinate {
                    tx_seq: 5,
                    index: 2,
                }),
                entry_checkpoint: 2,
                end_checkpoint: 1,
                end_position: IntraTxCoordinate {
                    tx_seq: 5,
                    index: 2,
                },
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
                window_has_checkpoints: true,
            }
        );

        // After tightens without emptying, before empties: the before-claim
        // overrides the after-cursor's eagerly written terminal.
        let options = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Descending,
            after: Some(ev_item(8, 5, 0)),
            before: Some(ev_item(1, 5, 0)),
        };
        assert_eq!(
            resolved_intra_tx().apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(5)),
                entry_checkpoint: 2,
                end_checkpoint: 1,
                end_position: IntraTxCoordinate::start_of_tx(5),
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
                window_has_checkpoints: true,
            }
        );
    }

    /// (descending, after below the window): complete noop.
    #[test]
    fn after_cursor_rejected_descending_is_noop() {
        let seed = ResolvedScan {
            bounds: IntraTxScanBounds {
                lo: Bound::Included(IntraTxCoordinate::start_of_tx(5)),
                hi: Bound::Excluded(IntraTxCoordinate::start_of_tx(10)),
            },
            ..resolved_intra_tx()
        };
        let options = after_options(Ordering::Descending, ev_boundary(4, 2, 0));
        assert_eq!(seed.clone().apply_cursor_bounds(&options), seed);
    }

    /// A cursor whose bound coincides with the window's own is a no-op tighten, but
    /// the terminal still becomes the cursor's — CursorBound on an unchanged,
    /// non-empty record.
    #[test]
    fn cursor_at_the_window_bound_still_takes_the_terminal() {
        // Admission at the exact bound is a no-op tighten, but the stop-side
        // cursor still takes over the terminal: exhaustion flips to
        // CursorBound on a non-empty result whose bounds did not change.
        let options = before_options(Ordering::Ascending, ev_boundary(6, 10, 0));
        assert_eq!(
            resolved_intra_tx().apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: IntraTxScanBounds::from_range(
                    IntraTxCoordinate::start_of_tx(0)..IntraTxCoordinate::start_of_tx(10)
                ),
                entry_checkpoint: 2,
                end_checkpoint: 6,
                end_position: IntraTxCoordinate::start_of_tx(10),
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
                window_has_checkpoints: true,
            }
        );

        let options = after_options(Ordering::Descending, ev_boundary(6, 0, 0));
        assert_eq!(
            resolved_intra_tx().apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: IntraTxScanBounds::from_range(
                    IntraTxCoordinate::start_of_tx(0)..IntraTxCoordinate::start_of_tx(10)
                ),
                entry_checkpoint: 2,
                end_checkpoint: 6,
                end_position: IntraTxCoordinate::start_of_tx(0),
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
                window_has_checkpoints: true,
            }
        );
    }

    // An after-Item cursor makes the exclusive lower bound one past the cursor, while an
    // after-Boundary cursor makes the lower bound equal to the cursor.
    #[test]
    fn tx_after_cursor_tightens_ascending_lower_bound() {
        // Item at n admits strictly-after symbolically; the successor lives
        // at the store edge, so both forms fetch from n + 1.
        let options = after_options(Ordering::Ascending, tx_item(3, 12));
        // Resolves to `(12, 20)`
        let tightened = resolved_range(10..20).apply_cursor_bounds(&options);
        assert_eq!(
            tightened,
            ResolvedScan {
                bounds: ScanBounds {
                    lo: Bound::Excluded(12),
                    hi: Bound::Excluded(20),
                },
                ..resolved_range(10..20)
            }
        );
        // Projection into Range<u64> sets the lower bound as n + 1
        assert_eq!(tightened.range(), 13..20);

        let options = after_options(Ordering::Ascending, tx_boundary(3, 12));
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: ScanBounds {
                    lo: Bound::Included(12),
                    hi: Bound::Excluded(20),
                },
                ..resolved_range(10..20)
            }
        );
    }

    /// Tightens the lower bound, and the terminal becomes the boundary just below the lowest
    /// admitted position, because the after cursor is where a descending scan stops: the position
    /// after an Item cursor, which the scan excluded, and a Boundary cursor itself.
    #[test]
    fn tx_descending_after_cursor_sets_terminal_and_tightens() {
        // Item at 12 and Boundary at 13 scan the same exclusive edge (13..20),
        // and end on the same terminal: the boundary at 13.
        let options = after_options(Ordering::Descending, tx_item(3, 12));
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: ScanBounds {
                    lo: Bound::Excluded(12),
                    hi: Bound::Excluded(20),
                },
                entry_checkpoint: 0,
                end_checkpoint: 3,
                end_position: 13,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
                window_has_checkpoints: true,
            }
        );
        let options = after_options(Ordering::Descending, tx_boundary(3, 13));
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: ScanBounds::from_range(13..20),
                entry_checkpoint: 0,
                end_checkpoint: 3,
                end_position: 13,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
                window_has_checkpoints: true,
            }
        );
    }

    /// When the after cursor empties the window, the record empties at the boundary just below
    /// what the cursor admits, at the cursor's checkpoint, as Boundary for either cursor kind.
    #[test]
    fn tx_after_cursor_empties_descending_interval() {
        // Item at 24 and Boundary at 25 scan the same exclusive edge, and both empty the window
        // at the boundary at 25.
        let options = after_options(Ordering::Descending, tx_boundary(9, 25));
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: ScanBounds::from_range(25..25),
                entry_checkpoint: 0,
                end_checkpoint: 9,
                end_position: 25,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
                window_has_checkpoints: true,
            }
        );
        let options = after_options(Ordering::Descending, tx_item(9, 24));
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: ScanBounds::from_range(25..25),
                entry_checkpoint: 0,
                end_checkpoint: 9,
                end_position: 25,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
                window_has_checkpoints: true,
            }
        );
    }

    /// after-Item at n and after-Boundary at n + 1 admit the same interval
    /// (here both empty the window); each terminal echoes its own raw
    /// coordinate and kind.
    #[test]
    fn tx_after_item_and_successor_boundary_admit_the_same_interval() {
        // Item at 24 and Boundary at 25 scan the same exclusive edge, but the
        // terminal echoes each cursor's raw coordinate and kind: the Item echo
        // keeps Item so resume does not re-serve.
        let options = after_options(Ordering::Ascending, tx_item(5, 24));
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: ScanBounds::from_range(24..24),
                entry_checkpoint: 0,
                end_checkpoint: 5,
                end_position: 24,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Item,
                },
                window_has_checkpoints: true,
            }
        );
        let options = after_options(Ordering::Ascending, tx_boundary(5, 25));
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: ScanBounds::from_range(25..25),
                entry_checkpoint: 0,
                end_checkpoint: 5,
                end_position: 25,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
                window_has_checkpoints: true,
            }
        );
    }

    /// (ascending, after-Boundary at u64::MAX): nothing can lie at MAX; empties at
    /// (cursor checkpoint, MAX), Boundary.
    #[test]
    fn tx_after_boundary_at_max_empties_interval() {
        let options = after_options(Ordering::Ascending, tx_boundary(5, u64::MAX));
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            empty_resolved_range(
                5,
                u64::MAX,
                RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
            )
        );
    }

    /// If the bounds are already empty, cursors do not apply.
    #[test]
    fn tx_after_item_at_max_lets_before_cursor_win() {
        // An after-Item at u64::MAX is an ordinary emptying admission (the
        // saturated successor), not a short-circuit: a crossed before-cursor
        // still participates and wins the attribution at the min position.
        let options = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: Some(tx_item(5, u64::MAX)),
            before: Some(tx_boundary(3, 15)),
        };
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: ScanBounds::from_range(15..15),
                entry_checkpoint: 0,
                end_checkpoint: 3,
                end_position: 15,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
                window_has_checkpoints: true,
            }
        );

        // Descending: the seed's entry checkpoint stands.
        let seed = ResolvedScan {
            entry_checkpoint: 7,
            ..resolved_range(10..20)
        };
        let options = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Descending,
            after: Some(tx_item(5, u64::MAX)),
            before: Some(tx_boundary(3, 15)),
        };
        assert_eq!(
            seed.apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: ScanBounds::from_range(15..15),
                entry_checkpoint: 7,
                end_checkpoint: 3,
                end_position: 15,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
                window_has_checkpoints: true,
            }
        );
    }

    /// Resume-equivalence witness for the raw cursor echo: an after-Item at n
    /// admits exactly what the successor-form echo it replaced (Boundary at
    /// n + 1) admits, so feeding either token back scans the same interval.
    /// Descending, both end on the same terminal, the boundary at n + 1, as pinned by
    /// [`tx_descending_after_cursor_sets_terminal_and_tightens`].
    #[test]
    fn raw_after_item_echo_resumes_like_successor_boundary() {
        let item = after_options(Ordering::Ascending, tx_item(3, 24));
        let successor = after_options(Ordering::Ascending, tx_boundary(3, 25));

        // Item(3, 24) results in the same range as Boundary(3, 25)
        assert_eq!(
            resolved_range(10..30).apply_cursor_bounds(&item).range(),
            resolved_range(10..30)
                .apply_cursor_bounds(&successor)
                .range(),
        );

        // Drained window: both resumptions admit nothing.
        assert!(resolved_range(10..20).apply_cursor_bounds(&item).is_empty());
        assert!(
            resolved_range(10..20)
                .apply_cursor_bounds(&successor)
                .is_empty()
        );
    }

    /// For some ResolvedScan<u64>, if its bounds are Excluded(A) and Excluded(A + 1), its bounds
    /// report non-empty, but its range is empty.
    /// `test_list_transactions_after_last_item_ends_at_window_bound` checks that the result set is
    /// empty.
    #[test]
    fn tx_after_item_at_window_edge_defers_to_drain() {
        let options = after_options(Ordering::Ascending, tx_item(5, 19));
        let initial_range = resolved_range(10..20);
        let resolved = initial_range.clone().apply_cursor_bounds(&options);
        assert_eq!(
            resolved,
            ResolvedScan {
                bounds: ScanBounds {
                    lo: Bound::Excluded(19),
                    hi: Bound::Excluded(20),
                },
                ..initial_range
            }
        );
        assert!(!resolved.is_empty());
        assert!(resolved.range().is_empty());
    }

    /// Exclusive after on descending bumps the bounds, and sets the end_checkpoint, end_position
    /// (the boundary just above the item), and exhaustion from the after cursor.
    #[test]
    fn tx_after_item_at_window_edge_descending_keeps_stamped_terminal() {
        let options = after_options(Ordering::Descending, tx_item(5, 19));
        let initial_range = resolved_range(10..20);
        let resolved = initial_range.clone().apply_cursor_bounds(&options);
        assert_eq!(
            resolved,
            ResolvedScan {
                bounds: ScanBounds {
                    lo: Bound::Excluded(19),
                    hi: Bound::Excluded(20),
                },
                end_checkpoint: 5,
                end_position: 20,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
                ..initial_range
            }
        );
        assert!(resolved.range().is_empty());
    }

    /// On a descending scan, when the after-Item cursor is exactly on the terminating window, the
    /// terminal values remain unchanged; the cursor does not overwrite the original bounds as it is
    /// not additionally restrictive.
    #[test]
    fn tx_after_item_below_window_start_descending_is_noop() {
        let options = after_options(Ordering::Descending, tx_item(5, 9));
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            resolved_range(10..20)
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
            resolved_range(10..20).apply_cursor_bounds(&options).range(),
            12..20
        );

        request.after = None;
        request.before = Some(token);
        let options = query_options_from_proto(Some(&request)).unwrap();
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options).range(),
            10..11
        );
    }

    /// Tip for the cursor-past-tip tests: checkpoints below 20 are indexed, and checkpoint `c`
    /// starts at transaction `5 * c`, so the first unindexed transaction is 100.
    const TIP: u64 = 20;

    fn first_tx(checkpoint: u64) -> u64 {
        5 * checkpoint
    }

    fn resolve_checkpoints(
        end_checkpoint: Option<u64>,
        options: &QueryOptions,
    ) -> Result<ResolvedScan<u64>, RpcError> {
        let cp_range = ResolvedCheckpointRange::from_request(None, end_checkpoint, TIP, options)?;
        let range = cp_range.range.clone();
        ResolvedScan::<u64>::resolve(cp_range, range, options)
    }

    fn resolve_transactions(options: &QueryOptions) -> Result<ResolvedScan<u64>, RpcError> {
        let cp_range = ResolvedCheckpointRange::from_request(None, None, TIP, options)?;
        let tx_range = first_tx(cp_range.range.start)..first_tx(cp_range.range.end);
        ResolvedScan::<u64>::resolve(cp_range, tx_range, options)
    }

    fn resolve_events(options: &QueryOptions) -> Result<ResolvedScan<IntraTxCoordinate>, RpcError> {
        let cp_range = ResolvedCheckpointRange::from_request(None, None, TIP, options)?;
        let tx_range = first_tx(cp_range.range.start)..first_tx(cp_range.range.end);
        ResolvedScan::<IntraTxCoordinate>::resolve(
            cp_range,
            IntraTxCoordinate::tx_window(tx_range),
            options,
        )
    }

    fn is_out_of_range<P>(resolved: Result<ResolvedScan<P>, RpcError>) -> bool {
        resolved.is_err_and(|e| e.into_status_proto().code == tonic::Code::OutOfRange as i32)
    }

    #[test]
    fn ascending_after_cursor_past_the_tip_is_out_of_range() {
        let ascending = |cursor| after_options(Ordering::Ascending, cursor);

        // The last indexed checkpoint, and the boundary a caught-up scan resumes from.
        assert!(resolve_checkpoints(None, &ascending(cp_item(TIP - 1))).is_ok());
        let caught_up = resolve_checkpoints(
            None,
            &ascending(CursorToken::boundary(Position::Checkpoints {
                checkpoint: TIP,
            })),
        )
        .unwrap();
        assert!(caught_up.is_empty());
        assert_eq!(caught_up.end_position, TIP);

        // Positions this server has not indexed.
        assert!(is_out_of_range(resolve_checkpoints(
            None,
            &ascending(cp_item(TIP))
        )));
        assert!(is_out_of_range(resolve_checkpoints(
            None,
            &ascending(CursorToken::boundary(Position::Checkpoints {
                checkpoint: TIP + 1,
            })),
        )));

        // A window the request ends at or below the tip is exhausted, not rejected; one that
        // ends above it is bounded by the tip.
        let past_window = resolve_checkpoints(Some(TIP), &ascending(cp_item(TIP + 5))).unwrap();
        assert!(past_window.is_empty());
        assert!(is_out_of_range(resolve_checkpoints(
            Some(TIP + 1),
            &ascending(cp_item(TIP + 5)),
        )));
    }

    #[test]
    fn descending_cursors_past_the_tip_are_out_of_range() {
        let before = |cursor| before_options(Ordering::Descending, cursor);
        let after = |cursor| after_options(Ordering::Descending, cursor);
        let tip_boundary = || CursorToken::boundary(Position::Checkpoints { checkpoint: TIP });

        assert!(resolve_checkpoints(None, &before(cp_item(TIP - 1))).is_ok());
        assert!(resolve_checkpoints(None, &before(tip_boundary())).is_ok());
        assert!(is_out_of_range(resolve_checkpoints(
            None,
            &before(cp_item(TIP))
        )));

        // `after` bounds the far end of a descending scan. At the tip it leaves an empty page that
        // ends at that cursor; past it, it admits only positions this server has not indexed.
        let at_tip = resolve_checkpoints(None, &after(tip_boundary())).unwrap();
        assert!(at_tip.is_empty());
        assert_eq!(
            at_tip.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary
            }
        );
        assert!(is_out_of_range(resolve_checkpoints(
            None,
            &after(cp_item(TIP))
        )));
        assert!(is_out_of_range(resolve_checkpoints(
            None,
            &after(cp_item(TIP + 5))
        )));

        // A window the request ends at or below the tip is exhausted, not rejected.
        let past_window = resolve_checkpoints(Some(TIP), &after(cp_item(TIP + 5))).unwrap();
        assert!(past_window.is_empty());
    }

    /// A transaction or event cursor inside the first unindexed checkpoint is past the tip even
    /// though its checkpoint equals the tip; only the start of that checkpoint is caught up.
    #[test]
    fn cursor_inside_the_first_unindexed_checkpoint_is_out_of_range() {
        let ascending = |cursor| after_options(Ordering::Ascending, cursor);
        let tip_tx = first_tx(TIP);

        assert!(resolve_transactions(&ascending(tx_item(TIP - 1, tip_tx - 1))).is_ok());
        assert!(resolve_transactions(&ascending(tx_boundary(TIP, tip_tx))).is_ok());
        assert!(is_out_of_range(resolve_transactions(&ascending(
            tx_boundary(TIP, tip_tx + 2,)
        ))));
        assert!(is_out_of_range(resolve_transactions(&ascending(tx_item(
            TIP, tip_tx
        )))));

        assert!(resolve_events(&ascending(ev_boundary(TIP, tip_tx, 0))).is_ok());
        assert!(is_out_of_range(resolve_events(&ascending(ev_item(
            TIP, tip_tx, 0
        )))));
        assert!(is_out_of_range(resolve_events(&ascending(ev_boundary(
            TIP, tip_tx, 1
        )))));
    }

    // Boundary checkpoint fields read in either direction, on the ledger of the cursor-past-tip
    // tests (checkpoint `c` holds transactions `5c..5c + 5`, tip at checkpoint 20), where every
    // transaction emits events 0 and 1.

    /// What issued a cursor, which decides how its checkpoint field relates to its position.
    #[derive(Clone, Copy, Debug)]
    enum Issuer {
        /// An item: the checkpoint containing the position.
        Item,
        /// A range end, a subscription boundary or a restated cursor bound: the checkpoint
        /// containing the position.
        ExactBoundary,
        /// An ascending scan frontier: the checkpoint of the position before it.
        AscendingFrontier,
        /// A descending scan frontier: one past the checkpoint containing the position.
        DescendingFrontier,
    }

    const ISSUERS: [Issuer; 4] = [
        Issuer::Item,
        Issuer::ExactBoundary,
        Issuer::AscendingFrontier,
        Issuer::DescendingFrontier,
    ];

    /// A request's `start_checkpoint` and `end_checkpoint`.
    type Window = (Option<u64>, Option<u64>);

    /// Checkpoint windows: open, inside the ledger, open at one end, and one checkpoint wide.
    const WINDOWS: [Window; 5] = [
        (None, None),
        (Some(4), Some(13)),
        (Some(7), None),
        (None, Some(7)),
        (Some(5), Some(6)),
    ];

    /// Checkpoint windows reaching past the tip: an `end_checkpoint` above it, and a
    /// `start_checkpoint` at it with `end_checkpoint` unset. A descending request over either is
    /// out of range.
    const PAST_TIP_WINDOWS: [Window; 2] = [(Some(15), Some(TIP + 1)), (Some(TIP), None)];

    const ORDERINGS: [Ordering; 2] = [Ordering::Ascending, Ordering::Descending];

    fn containing_checkpoint(tx: u64) -> u64 {
        tx / 5
    }

    /// The transaction cursor `issuer` mints at `tx`, where it can mint one.
    fn issued_tx_cursor(issuer: Issuer, tx: u64) -> Option<CursorToken> {
        let indexed = tx < first_tx(TIP);
        let checkpoint = containing_checkpoint(tx);
        match issuer {
            Issuer::Item => indexed.then(|| tx_item(checkpoint, tx)),
            Issuer::ExactBoundary => Some(tx_boundary(checkpoint, tx)),
            Issuer::AscendingFrontier => Some(tx_boundary(
                tx.checked_sub(1).map_or(0, containing_checkpoint),
                tx,
            )),
            Issuer::DescendingFrontier => indexed.then(|| tx_boundary(checkpoint + 1, tx)),
        }
    }

    /// The event cursor `issuer` mints at `(tx, index)`, where it can mint one.
    fn issued_event_cursor(issuer: Issuer, tx: u64, index: u32) -> Option<CursorToken> {
        let indexed = tx < first_tx(TIP);
        let checkpoint = containing_checkpoint(tx);
        let slot_before = if index > 0 {
            Some(tx)
        } else {
            tx.checked_sub(1)
        };
        match issuer {
            Issuer::Item => (indexed && index < 2).then(|| ev_item(checkpoint, tx, index)),
            Issuer::ExactBoundary => {
                (indexed || index == 0).then(|| ev_boundary(checkpoint, tx, index))
            }
            Issuer::AscendingFrontier => (indexed || index == 0)
                .then(|| ev_boundary(slot_before.map_or(0, containing_checkpoint), tx, index)),
            Issuer::DescendingFrontier => indexed.then(|| ev_boundary(checkpoint + 1, tx, index)),
        }
    }

    fn every_tx_cursor(txs: impl Iterator<Item = u64> + Clone) -> Vec<CursorToken> {
        ISSUERS
            .iter()
            .flat_map(|&issuer| {
                txs.clone()
                    .filter_map(move |tx| issued_tx_cursor(issuer, tx))
            })
            .collect()
    }

    fn every_event_cursor(txs: impl Iterator<Item = u64> + Clone) -> Vec<CursorToken> {
        ISSUERS
            .iter()
            .flat_map(|&issuer| {
                txs.clone().flat_map(move |tx| {
                    (0..3).filter_map(move |index| issued_event_cursor(issuer, tx, index))
                })
            })
            .collect()
    }

    /// Transactions on both sides of checkpoint edges, the window edges, genesis and the tip.
    fn edge_txs() -> impl Iterator<Item = u64> + Clone {
        [
            0, 1, 4, 5, 6, 19, 20, 21, 24, 25, 26, 29, 30, 34, 35, 36, 64, 65, 66, 94, 95, 96, 99,
            100,
        ]
        .into_iter()
    }

    /// A smaller set of edge transactions for event cursor pairs, which carry three slots each.
    fn event_edge_txs() -> impl Iterator<Item = u64> + Clone {
        [0, 4, 5, 20, 24, 25, 29, 30, 34, 35, 64, 65, 95, 99, 100].into_iter()
    }

    fn options_with(
        ordering: Ordering,
        after: Option<CursorToken>,
        before: Option<CursorToken>,
    ) -> QueryOptions {
        QueryOptions {
            limit_items: 100,
            ordering,
            after,
            before,
        }
    }

    /// Every single-cursor request and, over `pair_cursors`, every two-cursor request, in each
    /// window, including those reaching past the tip, and each ordering.
    fn requests(
        cursors: &[CursorToken],
        pair_cursors: &[CursorToken],
    ) -> Vec<(Window, QueryOptions)> {
        let mut requests = Vec::new();
        for window in WINDOWS.into_iter().chain(PAST_TIP_WINDOWS) {
            for ordering in ORDERINGS {
                for cursor in cursors {
                    requests.push((window, options_with(ordering, Some(cursor.clone()), None)));
                    requests.push((window, options_with(ordering, None, Some(cursor.clone()))));
                }
                for after in pair_cursors {
                    for before in pair_cursors {
                        requests.push((
                            window,
                            options_with(ordering, Some(after.clone()), Some(before.clone())),
                        ));
                    }
                }
            }
        }
        requests
    }

    fn resolve_tx_request(
        window: Window,
        options: &QueryOptions,
    ) -> Result<ResolvedScan<u64>, RpcError> {
        let cp_range = ResolvedCheckpointRange::from_request(window.0, window.1, TIP, options)?;
        let tx_range = first_tx(cp_range.range.start)..first_tx(cp_range.range.end);
        ResolvedScan::<u64>::resolve(cp_range, tx_range, options)
    }

    fn resolve_event_request(
        window: Window,
        options: &QueryOptions,
    ) -> Result<ResolvedScan<IntraTxCoordinate>, RpcError> {
        let cp_range = ResolvedCheckpointRange::from_request(window.0, window.1, TIP, options)?;
        let tx_range = first_tx(cp_range.range.start)..first_tx(cp_range.range.end);
        ResolvedScan::<IntraTxCoordinate>::resolve(
            cp_range,
            IntraTxCoordinate::tx_window(tx_range),
            options,
        )
    }

    fn resolve_checkpoint_request(
        window: Window,
        options: &QueryOptions,
    ) -> Result<ResolvedScan<u64>, RpcError> {
        let cp_range = ResolvedCheckpointRange::from_request(window.0, window.1, TIP, options)?;
        let range = cp_range.range.clone();
        ResolvedScan::<u64>::resolve(cp_range, range, options)
    }

    fn ledger_txs() -> Vec<u64> {
        (0..first_tx(TIP)).collect()
    }

    /// Every event slot a cursor can name: events 0 and 1 of each transaction, and the empty
    /// slot after them.
    fn ledger_event_slots() -> Vec<IntraTxCoordinate> {
        (0..first_tx(TIP))
            .flat_map(|tx_seq| (0..3).map(move |index| IntraTxCoordinate { tx_seq, index }))
            .collect()
    }

    /// Whether the window and cursors of a request admit the position at `key` (scan order).
    fn admits(window: Window, options: &QueryOptions, key: (u64, u32)) -> bool {
        let start = first_tx(window.0.unwrap_or(0));
        let end = first_tx(window.1.unwrap_or(TIP).min(TIP));
        let in_window = start <= key.0 && key.0 < end;
        let after = options.after.as_ref().is_none_or(|cursor| {
            let cursor_key = scan_order(&cursor.position);
            match cursor.kind {
                sui_rpc_cursor::CursorKind::Item => key > cursor_key,
                sui_rpc_cursor::CursorKind::Boundary => key >= cursor_key,
            }
        });
        let before = options
            .before
            .as_ref()
            .is_none_or(|cursor| key < scan_order(&cursor.position));
        in_window && after && before
    }

    /// A position's place in ascending scan order on the model ledger, in transaction space:
    /// checkpoint `c` sits at its first transaction.
    fn scan_order(position: &Position) -> (u64, u32) {
        match *position {
            Position::Checkpoints { checkpoint } => (first_tx(checkpoint), 0),
            Position::Transactions { tx_seq, .. } => (tx_seq, 0),
            Position::Events {
                tx_seq,
                event_index,
                ..
            } => (tx_seq, event_index),
        }
    }

    fn status(error: RpcError) -> (i32, String) {
        let status = error.into_status_proto();
        (status.code, status.message)
    }

    /// One request on the model ledger, over `positions`, every scan position its cursors can
    /// name:
    /// - it admits exactly the positions its window and cursors admit;
    /// - it errs exactly when it is descending and its window reaches past the tip, or when its
    ///   window reaches the tip and a cursor other than an ascending scan's `before` names a
    ///   position past the tip, and then with `OutOfRange`;
    /// - in a non-empty window, its entry checkpoint is the window's first checkpoint in scan
    ///   direction, whatever its cursors' checkpoint fields;
    /// - in a non-empty window, it ends at the window's edge with the window's reason exactly
    ///   when no cursor stops the scan first: ascending, a `before` at or below the window's end,
    ///   or an `after` admitting nothing up to it; descending, an `after` at or above the
    ///   window's start, or a `before` below it.
    fn check_request<P: ScanPosition + std::fmt::Debug>(
        window: Window,
        options: &QueryOptions,
        resolve: impl Fn(Window, &QueryOptions) -> Result<ResolvedScan<P>, RpcError>,
        positions: &[P],
        key: impl Fn(P) -> (u64, u32),
    ) where
        CursorToken: ScanCursor<P>,
    {
        let context = format!("window {window:?}, {options:?}");
        let tip_checked_cursors = match options.ordering {
            Ordering::Ascending => [options.after.as_ref(), None],
            Ordering::Descending => [options.before.as_ref(), options.after.as_ref()],
        };
        let tip_key = (first_tx(TIP), 0);
        let window_reaches_tip = window.1.is_none_or(|end| end > TIP);
        let descending_window_past_tip = options.ordering == Ordering::Descending
            && match window.1 {
                Some(end) => end > TIP,
                None => window.0.unwrap_or(0) >= TIP,
            };
        let cursor_past_tip = window_reaches_tip
            && tip_checked_cursors.into_iter().flatten().any(|cursor| {
                let cursor_key = scan_order(&cursor.position);
                match cursor.kind {
                    sui_rpc_cursor::CursorKind::Item => cursor_key >= tip_key,
                    sui_rpc_cursor::CursorKind::Boundary => cursor_key > tip_key,
                }
            });
        let out_of_range = descending_window_past_tip || cursor_past_tip;
        let resolved = match resolve(window, options) {
            Ok(resolved) => resolved,
            Err(error) => {
                let (code, message) = status(error);
                assert!(out_of_range, "{code}: {message}: {context}");
                assert_eq!(code, tonic::Code::OutOfRange as i32, "{message}: {context}");
                return;
            }
        };
        assert!(!out_of_range, "a request past the tip resolves: {context}");
        let admitted: Vec<P> = positions
            .iter()
            .copied()
            .filter(|position| resolved.bounds.contains(*position))
            .collect();
        let expected: Vec<P> = positions
            .iter()
            .copied()
            .filter(|position| admits(window, options, key(*position)))
            .collect();
        assert_eq!(admitted, expected, "admitted positions: {context}");

        let window_start = window.0.unwrap_or(0);
        let window_end = window.1.unwrap_or(TIP).min(TIP);
        if window_start >= window_end {
            return;
        }
        let window_lo = (first_tx(window_start), 0);
        let window_hi = (first_tx(window_end), 0);
        let cursor_key = |cursor: &CursorToken| key(cursor.coordinate());
        let first_admitted_key = |cursor: &CursorToken| {
            let position: P = cursor.coordinate();
            match cursor.kind {
                sui_rpc_cursor::CursorKind::Item => position.successor().map(&key),
                sui_rpc_cursor::CursorKind::Boundary => Some(key(position)),
            }
        };
        let (entry, edge, reason, ends_at_edge) = match options.ordering {
            Ordering::Ascending => (
                window_start,
                window_hi,
                if window_reaches_tip {
                    RangeExhaustion::LedgerTip
                } else {
                    RangeExhaustion::CheckpointBound
                },
                options
                    .before
                    .as_ref()
                    .is_none_or(|before| cursor_key(before) > window_hi)
                    && options.after.as_ref().is_none_or(|after| {
                        first_admitted_key(after).is_some_and(|first| first <= window_hi)
                    }),
            ),
            Ordering::Descending => (
                window_end - 1,
                window_lo,
                RangeExhaustion::CheckpointBound,
                options
                    .after
                    .as_ref()
                    .is_none_or(|after| cursor_key(after) < window_lo)
                    && options
                        .before
                        .as_ref()
                        .is_none_or(|before| cursor_key(before) >= window_lo),
            ),
        };
        assert_eq!(
            resolved.entry_checkpoint, entry,
            "entry checkpoint: {context}"
        );
        if ends_at_edge {
            assert_eq!(resolved.exhaustion, reason, "end reason: {context}");
            assert_eq!(key(resolved.end_position), edge, "end position: {context}");
        } else {
            assert!(
                matches!(resolved.exhaustion, RangeExhaustion::CursorBound { .. }),
                "end reason: {context}: {:?}",
                resolved.exhaustion
            );
        }
    }

    /// Regression: an ascending scan frontier sent as `before`, or a descending one sent as
    /// `after`, carries a checkpoint field a checkpoint away from the one its position needs,
    /// and reading it as written dropped the positions in between.
    #[test]
    fn opposite_direction_frontier_admits_the_positions_next_to_it() {
        // Ascending frontier at transaction 23 (22 is in checkpoint 4), sent as `before`.
        let options = before_options(Ordering::Descending, tx_boundary(4, 23));
        let resolved = resolve_transactions(&options).unwrap();
        assert_eq!(resolved.range(), 0..23);

        // Descending frontier at transaction 23 (checkpoint 4, so the field is 5), sent as
        // `after`.
        let options = after_options(Ordering::Ascending, tx_boundary(5, 23));
        let resolved = resolve_transactions(&options).unwrap();
        assert_eq!(resolved.range(), 23..100);

        // Ascending event frontier at the start of transaction 23, sent as `before`.
        let options = before_options(Ordering::Descending, ev_boundary(4, 23, 0));
        let resolved = resolve_events(&options).unwrap();
        assert_eq!(
            resolved.bounds.hi,
            Bound::Excluded(IntraTxCoordinate::start_of_tx(23))
        );
        assert_eq!(
            resolved.bounds.lo,
            Bound::Included(IntraTxCoordinate::start_of_tx(0))
        );
    }

    #[test]
    fn transaction_cursors_admit_their_positions_in_either_direction() {
        let cursors = every_tx_cursor(0..=first_tx(TIP));
        let pair_cursors = every_tx_cursor(edge_txs());
        let positions = ledger_txs();
        for (window, options) in requests(&cursors, &pair_cursors) {
            check_request(window, &options, resolve_tx_request, &positions, |tx| {
                (tx, 0)
            });
        }
    }

    #[test]
    fn event_cursors_admit_their_positions_in_either_direction() {
        let cursors = every_event_cursor(0..=first_tx(TIP));
        let pair_cursors = every_event_cursor(event_edge_txs());
        let positions = ledger_event_slots();
        for (window, options) in requests(&cursors, &pair_cursors) {
            check_request(
                window,
                &options,
                resolve_event_request,
                &positions,
                |position| (position.tx_seq, position.index),
            );
        }
    }

    /// The cursor a resolved request's terminal frame carries.
    fn terminal_cursor<P: ScanPosition>(
        resolved: &ResolvedScan<P>,
        position_at: impl Fn(u64, P) -> Position,
    ) -> CursorToken {
        let kind = match resolved.exhaustion {
            RangeExhaustion::CursorBound { kind } => kind,
            RangeExhaustion::LedgerTip
            | RangeExhaustion::CheckpointBound
            | RangeExhaustion::ServingFloor { .. } => sui_rpc_cursor::CursorKind::Boundary,
        };
        CursorToken {
            kind,
            position: position_at(resolved.end_checkpoint, resolved.end_position),
        }
    }

    /// A request bounded only on its far side (`after` descending, `before` ascending) ends on a
    /// closing cursor. Sent back in the same ordering without the far bound, it resumes with
    /// exactly the window's positions the far bound excluded.
    fn check_closing_cursor<P: ScanPosition + std::fmt::Debug>(
        window: Window,
        options: &QueryOptions,
        resolve: impl Fn(Window, &QueryOptions) -> Result<ResolvedScan<P>, RpcError>,
        positions: &[P],
        key: impl Fn(P) -> (u64, u32),
        position_at: impl Fn(u64, P) -> Position,
    ) {
        let context = format!("window {window:?}, {options:?}");
        let resolved = resolve(window, options).unwrap();
        let closing = terminal_cursor(&resolved, position_at);
        let resume = if options.is_ascending() {
            options_with(options.ordering, Some(closing.clone()), None)
        } else {
            options_with(options.ordering, None, Some(closing.clone()))
        };
        let resumed = resolve(window, &resume)
            .unwrap_or_else(|e| panic!("{closing:?} resumes: {:?}: {context}", status(e)));
        let unbounded = options_with(options.ordering, None, None);
        let excluded: Vec<P> = positions
            .iter()
            .copied()
            .filter(|position| {
                admits(window, &unbounded, key(*position))
                    && !admits(window, options, key(*position))
            })
            .collect();
        let admitted: Vec<P> = positions
            .iter()
            .copied()
            .filter(|position| resumed.bounds.contains(*position))
            .collect();
        assert_eq!(admitted, excluded, "{closing:?} resumes: {context}");
    }

    /// Requests bounded only on their far side by each of `cursors`.
    fn far_bounded_requests(cursors: &[CursorToken]) -> Vec<(Window, QueryOptions)> {
        let mut requests = Vec::new();
        for window in WINDOWS {
            for cursor in cursors {
                requests.push((
                    window,
                    options_with(Ordering::Descending, Some(cursor.clone()), None),
                ));
                requests.push((
                    window,
                    options_with(Ordering::Ascending, None, Some(cursor.clone())),
                ));
            }
        }
        requests
    }

    #[test]
    fn transaction_closing_cursors_resume_without_their_far_bound() {
        let positions = ledger_txs();
        for (window, options) in far_bounded_requests(&every_tx_cursor(0..=first_tx(TIP))) {
            check_closing_cursor(
                window,
                &options,
                resolve_tx_request,
                &positions,
                |tx| (tx, 0),
                |checkpoint, tx_seq| Position::Transactions { checkpoint, tx_seq },
            );
        }
    }

    #[test]
    fn event_closing_cursors_resume_without_their_far_bound() {
        let positions = ledger_event_slots();
        for (window, options) in far_bounded_requests(&every_event_cursor(0..=first_tx(TIP))) {
            check_closing_cursor(
                window,
                &options,
                resolve_event_request,
                &positions,
                |position| (position.tx_seq, position.index),
                |checkpoint, position| Position::Events {
                    checkpoint,
                    tx_seq: position.tx_seq,
                    event_index: position.index,
                },
            );
        }
    }

    /// Regression: a descending scan bounded by `after: Item(X)` excludes X, so its closing
    /// cursor is the boundary at the position after X, under the item's checkpoint field. Sent
    /// back as `before`, it resumes with X.
    #[test]
    fn descending_after_item_closes_at_the_next_position() {
        // Inside a checkpoint: transaction 12 of checkpoint 2 closes at the boundary at 13.
        let resolved =
            resolve_transactions(&after_options(Ordering::Descending, tx_item(2, 12))).unwrap();
        assert_eq!(resolved.range(), 13..100);
        assert_eq!((resolved.end_checkpoint, resolved.end_position), (2, 13));
        let resumed =
            resolve_transactions(&before_options(Ordering::Descending, tx_boundary(2, 13)))
                .unwrap();
        assert_eq!(resumed.range(), 0..13);

        // Transaction 14 is the last of checkpoint 2: the next position starts checkpoint 3,
        // and the field still names checkpoint 2. The boundary's position, not its field, bounds
        // the resumed scan, so it still admits transaction 14.
        let resolved =
            resolve_transactions(&after_options(Ordering::Descending, tx_item(2, 14))).unwrap();
        assert_eq!(resolved.range(), 15..100);
        assert_eq!((resolved.end_checkpoint, resolved.end_position), (2, 15));
        let resumed =
            resolve_transactions(&before_options(Ordering::Descending, tx_boundary(2, 15)))
                .unwrap();
        assert_eq!(resumed.range(), 0..15);
        assert!(resumed.bounds.contains(14));

        let resolved =
            resolve_events(&after_options(Ordering::Descending, ev_item(2, 14, 1))).unwrap();
        assert_eq!(
            resolved.end_position,
            IntraTxCoordinate {
                tx_seq: 14,
                index: 2
            }
        );
        let resolved = resolve_events(&after_options(
            Ordering::Descending,
            ev_item(2, 14, u32::MAX),
        ))
        .unwrap();
        assert_eq!(resolved.end_position, IntraTxCoordinate::start_of_tx(15));

        // No position follows the last one, so the closing cursor restates the item.
        let resolved = resolved_range(10..20)
            .apply_cursor_bounds(&after_options(Ordering::Descending, tx_item(1, u64::MAX)));
        assert_eq!(resolved.end_position, u64::MAX);
        assert_eq!(
            resolved.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Item
            }
        );
    }

    #[test]
    fn checkpoint_cursors_admit_their_positions_in_either_direction() {
        let cursors: Vec<_> = (0..=TIP)
            .flat_map(|checkpoint| {
                [
                    cp_item(checkpoint),
                    CursorToken::boundary(Position::Checkpoints { checkpoint }),
                ]
            })
            .collect();
        let positions: Vec<u64> = (0..TIP).collect();
        for (window, options) in requests(&cursors, &cursors) {
            check_request(
                window,
                &options,
                resolve_checkpoint_request,
                &positions,
                |checkpoint| (first_tx(checkpoint), 0),
            );
        }
    }

    /// An ascending scan whose high end is the tip, with nothing left to scan, ends at the tip
    /// with `LedgerTip`, however its `after` cursor names the tip. A `before` cursor at the tip
    /// bounds the scan instead, and a descending scan ends at its `after` cursor.
    #[test]
    fn caught_up_ascending_scan_ends_with_ledger_tip() {
        let tip_tx = first_tx(TIP);
        let cursor_bound = RangeExhaustion::CursorBound {
            kind: sui_rpc_cursor::CursorKind::Boundary,
        };
        // The `LedgerTip` terminal, an ascending frontier at the tip, and the last item.
        for cursor in [
            tx_boundary(TIP, tip_tx),
            tx_boundary(TIP - 1, tip_tx),
            tx_item(TIP - 1, tip_tx - 1),
        ] {
            for window in [(None, None), (Some(3), None), (Some(3), Some(TIP + 5))] {
                let options = after_options(Ordering::Ascending, cursor.clone());
                let resolved = resolve_tx_request(window, &options).unwrap();
                let context = format!("{cursor:?} {window:?}");
                assert!(resolved.is_empty(), "{context}");
                assert_eq!(resolved.exhaustion, RangeExhaustion::LedgerTip, "{context}");
                assert_eq!(
                    (resolved.end_checkpoint, resolved.end_position),
                    (TIP, tip_tx),
                    "{context}"
                );
                assert_eq!(
                    resolved.entry_checkpoint,
                    window.0.unwrap_or(0),
                    "{context}"
                );
            }

            let before = |position| {
                options_with(
                    Ordering::Ascending,
                    Some(cursor.clone()),
                    Some(tx_boundary(TIP, position)),
                )
            };
            assert_eq!(
                resolve_transactions(&before(tip_tx + 1))
                    .unwrap()
                    .exhaustion,
                RangeExhaustion::LedgerTip,
                "a `before` past the tip leaves the tip as the high end: {cursor:?}"
            );
            assert_eq!(
                resolve_transactions(&before(tip_tx)).unwrap().exhaustion,
                cursor_bound,
                "a `before` at the tip bounds the scan: {cursor:?}"
            );
        }

        let events = resolve_events(&after_options(
            Ordering::Ascending,
            ev_boundary(TIP, tip_tx, 0),
        ))
        .unwrap();
        assert!(events.is_empty());
        assert_eq!(events.exhaustion, RangeExhaustion::LedgerTip);
        assert_eq!(events.end_position, IntraTxCoordinate::start_of_tx(tip_tx));

        let descending = resolve_transactions(&after_options(
            Ordering::Descending,
            tx_boundary(TIP, tip_tx),
        ))
        .unwrap();
        assert!(descending.is_empty());
        assert_eq!(descending.exhaustion, cursor_bound);
        assert_eq!(
            (descending.end_checkpoint, descending.end_position),
            (TIP, tip_tx)
        );
    }

    /// A descending frontier resumed descending claims coverage from the window's end: the
    /// cursor vouches for every position above it, so the scan's first item claims the
    /// checkpoint above the item's, though the scan never read that checkpoint.
    #[test]
    fn descending_resume_claims_what_its_cursor_vouches_for() {
        // Frontier at transaction 92 (checkpoint 18), so the field is 19.
        let options = before_options(Ordering::Descending, tx_boundary(19, 92));
        let resolved = resolve_transactions(&options).unwrap();
        assert_eq!(resolved.range(), 0..92);
        assert_eq!(resolved.entry_checkpoint, TIP - 1);
        assert_eq!(
            crate::ledger_history::watermark::advance_covered_bound_before_checkpoint(
                None,
                18,
                resolved.entry_checkpoint,
                &options
            ),
            Some(19),
            "the first item, in checkpoint 18, claims checkpoint 19"
        );
    }

    /// A cursor whose checkpoint field lies a checkpoint past its position, on the side the
    /// cursor bounds, still gets the positions its position bound admits, and the end reason of
    /// the scan over them.
    #[test]
    fn checkpoint_field_alone_never_empties_a_page() {
        // Descending, `before` is an ascending frontier inside the window's first checkpoint:
        // the scan reaches the window start.
        let resolved = resolve_tx_request(
            (Some(4), None),
            &before_options(Ordering::Descending, tx_boundary(4, 23)),
        )
        .unwrap();
        assert_eq!(resolved.range(), 20..23);
        assert_eq!(resolved.exhaustion, RangeExhaustion::CheckpointBound);
        assert_eq!((resolved.end_checkpoint, resolved.end_position), (4, 20));

        // Descending, `after` is a descending frontier in the top checkpoint: the scan ends at
        // the cursor.
        let resolved =
            resolve_transactions(&after_options(Ordering::Descending, tx_boundary(TIP, 97)))
                .unwrap();
        assert_eq!(resolved.range(), 97..100);
        assert_eq!(
            resolved.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary
            }
        );
        assert_eq!((resolved.end_checkpoint, resolved.end_position), (TIP, 97));

        // Ascending, both cursors are descending frontiers in the top checkpoint: the scan ends
        // at `before`.
        let options = options_with(
            Ordering::Ascending,
            Some(tx_boundary(TIP, 97)),
            Some(tx_boundary(TIP, 99)),
        );
        let resolved = resolve_transactions(&options).unwrap();
        assert_eq!(resolved.range(), 97..99);
        assert_eq!(
            resolved.exhaustion,
            RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary
            }
        );
        assert_eq!((resolved.end_checkpoint, resolved.end_position), (TIP, 99));

        // Ascending, `after` is a descending frontier in the top checkpoint: the scan reaches
        // the tip.
        let resolved =
            resolve_transactions(&after_options(Ordering::Ascending, tx_boundary(TIP, 97)))
                .unwrap();
        assert_eq!(resolved.range(), 97..100);
        assert_eq!(resolved.exhaustion, RangeExhaustion::LedgerTip);

        // Ascending, `after` is a descending frontier in the window's last checkpoint: the scan
        // reaches the window end.
        let resolved = resolve_tx_request(
            (None, Some(10)),
            &after_options(Ordering::Ascending, tx_boundary(10, 47)),
        )
        .unwrap();
        assert_eq!(resolved.range(), 47..50);
        assert_eq!(resolved.exhaustion, RangeExhaustion::CheckpointBound);
        assert_eq!((resolved.end_checkpoint, resolved.end_position), (10, 50));
    }

    /// Cursor fields and positions at the numeric edges resolve without overflow.
    #[test]
    fn cursors_at_numeric_edges_resolve_without_overflow() {
        // A boundary at genesis admits every position.
        let resolved =
            resolve_transactions(&after_options(Ordering::Ascending, tx_boundary(0, 0))).unwrap();
        assert_eq!(resolved.range(), 0..100);
        // A `before` at `u64::MAX` is past every window, so it narrows nothing.
        let resolved = resolve_transactions(&before_options(
            Ordering::Ascending,
            tx_boundary(u64::MAX, u64::MAX),
        ))
        .unwrap();
        assert_eq!(resolved.range(), 0..100);
        // An `after` at `u64::MAX` is past the tip, and past a window that ends below the tip.
        let options = after_options(Ordering::Descending, tx_boundary(u64::MAX, u64::MAX));
        assert!(is_out_of_range(resolve_transactions(&options)));
        let resolved = resolve_tx_request((None, Some(10)), &options).unwrap();
        assert!(resolved.is_empty());
    }

    /// Cursors in the decode-only legacy format resolve like current ones.
    #[test]
    fn legacy_cursors_resolve_like_current_ones() {
        #[derive(Clone, PartialEq, prost::Message)]
        struct LegacyCursorToken {
            #[prost(int32, tag = "1")]
            query_type: i32,
            #[prost(int32, tag = "2")]
            kind: i32,
            #[prost(uint64, optional, tag = "3")]
            checkpoint: Option<u64>,
            #[prost(uint64, optional, tag = "4")]
            position: Option<u64>,
        }
        // An ascending transaction frontier at transaction 23 (22 is in checkpoint 4).
        let legacy = prost::Message::encode_to_vec(&LegacyCursorToken {
            query_type: 2,
            kind: 2,
            checkpoint: Some(4),
            position: Some(23),
        });
        let mut proto = ProtoQueryOptions::default();
        proto.ordering = Some(ProtoOrdering::Descending as i32);
        proto.before = Some(legacy.into());
        let options = query_options_from_proto(Some(&proto)).unwrap();
        assert_eq!(options.before, Some(tx_boundary(4, 23)));
        assert_eq!(resolve_transactions(&options).unwrap().range(), 0..23);
    }

    /// `cursor` with its checkpoint field zeroed, as a transaction or event cursor minted without
    /// one decodes.
    fn without_checkpoint_field(cursor: &CursorToken) -> CursorToken {
        let position = match cursor.position {
            Position::Transactions { tx_seq, .. } => Position::Transactions {
                checkpoint: 0,
                tx_seq,
            },
            Position::Events {
                tx_seq,
                event_index,
                ..
            } => Position::Events {
                checkpoint: 0,
                tx_seq,
                event_index,
            },
            Position::Checkpoints { .. } => panic!("checkpoint cursors keep their checkpoint"),
        };
        CursorToken {
            kind: cursor.kind,
            position,
        }
    }

    /// Every request built from `cursors` (each alone as `after` or `before`, and paired with a
    /// sample of the others) resolves to the same scan with the cursors' checkpoint fields zeroed.
    /// Only the checkpoints labelling the terminal cursor may differ, and errors only in message.
    fn assert_resolves_without_checkpoint_fields<P: Copy + Eq + std::fmt::Debug>(
        cursors: &[CursorToken],
        resolve: impl Fn(Window, &QueryOptions) -> Result<ResolvedScan<P>, RpcError>,
    ) {
        let mut requests = Vec::new();
        for after in cursors {
            requests.push((Some(after), None));
            requests.push((None, Some(after)));
            for before in cursors.iter().step_by(7) {
                requests.push((Some(after), Some(before)));
            }
        }
        for window in WINDOWS {
            for ordering in [Ordering::Ascending, Ordering::Descending] {
                for &(after, before) in &requests {
                    let labelled = options_with(ordering, after.cloned(), before.cloned());
                    let unlabelled = options_with(
                        ordering,
                        after.map(without_checkpoint_field),
                        before.map(without_checkpoint_field),
                    );
                    let context = format!("window {window:?}, {labelled:?}");
                    match (resolve(window, &labelled), resolve(window, &unlabelled)) {
                        (Ok(labelled), Ok(unlabelled)) => {
                            assert_eq!(labelled.bounds, unlabelled.bounds, "{context}");
                            assert_eq!(labelled.end_position, unlabelled.end_position, "{context}");
                            assert_eq!(labelled.exhaustion, unlabelled.exhaustion, "{context}");
                            assert_eq!(
                                labelled.entry_checkpoint, unlabelled.entry_checkpoint,
                                "{context}"
                            );
                        }
                        (Err(labelled), Err(unlabelled)) => {
                            assert_eq!(status(labelled).0, status(unlabelled).0, "{context}")
                        }
                        (labelled, unlabelled) => panic!(
                            "{context}: resolves {:?} with its checkpoint fields, {:?} without",
                            labelled.map(|_| ()).map_err(status),
                            unlabelled.map(|_| ()).map_err(status),
                        ),
                    }
                }
            }
        }
    }

    /// A transaction or event cursor minted without a checkpoint field decodes with checkpoint 0,
    /// and must resume exactly as the cursor that carried its checkpoint.
    #[test]
    fn cursors_resolve_the_same_without_their_checkpoint_field() {
        let txs = 0..=first_tx(TIP) + 1;
        assert_resolves_without_checkpoint_fields(
            &every_tx_cursor(txs.clone()),
            resolve_tx_request,
        );
        assert_resolves_without_checkpoint_fields(&every_event_cursor(txs), resolve_event_request);
    }
}
