// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::{Bound, Range};

use bytes::Bytes;
use sui_inverted_index::ScanDirection;
use sui_rpc::proto::sui::rpc::v2::Ordering as ProtoOrdering;
use sui_rpc::proto::sui::rpc::v2::QueryOptions as ProtoQueryOptions;

use crate::ErrorDetails;
use crate::ErrorReason;
use crate::RpcError;
use crate::ledger_history::cursor::CursorKind;
use crate::ledger_history::cursor::CursorToken;
use crate::ledger_history::cursor::LedgerPoint;
use crate::ledger_history::cursor::Position;
use crate::ledger_history::watermark::RangeEnd;
use crate::proto::google::rpc::ErrorInfo;
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
    /// Truncated by a client `after`/`before` cursor. `kind` is the cursor kind the terminal
    /// cursor must carry so resume neither repeats nor skips an item: Item only for an `after`
    /// Item cursor at the last position, which has no next position to name as a boundary;
    /// Boundary everywhere else.
    CursorBound { kind: CursorKind },
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

/// A request's checkpoint window in a lane's scan-position space, narrowed to the request's start
/// and end positions inside the window's edge checkpoints (see
/// [`CheckpointPositions`](crate::ledger_history::checkpoint_positions::CheckpointPositions)).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanWindow<P> {
    /// Half-open scan-position interval. A start position past the end position leaves it
    /// inverted.
    pub range: Range<P>,
    /// Whether `range.start` is not the first position of the window's first checkpoint, so the
    /// window may leave part of that checkpoint out. An event index past its transaction's events
    /// sets it even where it leaves nothing out, which only makes coverage claims conservative.
    pub start_cuts_checkpoint: bool,
    /// Whether `range.end` is not the end of the window's last checkpoint, so the window may leave
    /// part of that checkpoint out. As for `start_cuts_checkpoint`, an event index past its
    /// transaction's events sets it even where it leaves nothing out.
    pub end_cuts_checkpoint: bool,
    /// The same window in ledger coordinates.
    pub ledger: LedgerWindow,
}

/// A request's window in ledger coordinates, for placing the resume point of a scan's terminal
/// frame, and the public position of its cursor, where the window names it (see
/// [`ResolvedScan::end_resume_position`] and [`ResolvedScan::end_cursor_public_position`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LedgerWindow {
    /// The point of [`ScanWindow::range`]'s start.
    pub start: LedgerPoint,
    /// The point of [`ScanWindow::range`]'s end.
    pub end: LedgerPoint,
    /// For a window that holds no checkpoint (the request's checkpoint window, clamped to this
    /// server's tip, holds none), the points the request's own start and end fields name, where
    /// its terminal frame resumes ascending and descending: the start not clamped to this
    /// server's tip (an unset field counting as 0), and the end when the request set
    /// `end_checkpoint`, else `end`. `None` for a window that holds a checkpoint.
    pub requested: Option<(LedgerPoint, LedgerPoint)>,
}

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
    /// The public position of `end_scan_position`, unless a request cursor names that position
    /// (see [`ResolvedScan::end_cursor_public_position`]): the window's edge, the serving floor,
    /// or where the request cursor that ends the scan resumes.
    pub end_public_position: LedgerPoint,
    /// Scan-direction terminal edge of the interval, in the lane's private scan order: a
    /// `tx_seq`, a `(tx_seq, event_index)`, or a checkpoint.
    pub end_scan_position: P,
    /// Why the interval is exhausted once the scan drains it.
    pub exhaustion: RangeExhaustion,
    /// The checkpoint a scan that drains the window reports as fully covered: the window's last
    /// checkpoint in scan direction, or the one before it in scan direction where a request
    /// position cuts it, which lies outside a one-checkpoint window
    /// ([`ResolvedScan::natural_end_claim`] drops such a claim). `None` when the window holds no
    /// checkpoint, when the checkpoint before it would precede genesis, or when an ascending
    /// scan's `after` cursor resumes past the ledger tip (see [`ResolvedScan::resolve`]).
    /// Cursors do not move it: a non-empty window's natural end claims its edge even when the
    /// scan reads nothing, because the request's starting cursor vouches for every position
    /// before it.
    pub window_claim: Option<u64>,
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
}

/// The error for a request that needs data below this server's serving floor, whose lowest
/// available checkpoint is `floor_checkpoint`: that data is pruned and permanently gone.
/// It is `OutOfRange` with a `BELOW_EARLIEST_AVAILABLE` [`ErrorInfo`] whose
/// `lowestAvailableCheckpoint` is `floor_checkpoint`; retrying it on this server fails the same
/// way.
pub fn below_serving_floor(floor_checkpoint: u64) -> RpcError {
    let error_info = ledger_error_info(
        ErrorReason::BelowEarliestAvailable,
        [("lowestAvailableCheckpoint", floor_checkpoint.to_string())],
    );
    RpcError::new(
        tonic::Code::OutOfRange,
        format!(
            "requested data below earliest available; lowest available checkpoint is {floor_checkpoint}"
        ),
    )
    .with_details(ErrorDetails::new().with_error_info(error_info))
}

/// The error for a request that needs versions of an object (a package, for one) below
/// `oldest_retained_version`, the oldest one this server keeps: the object's version at the end of
/// `floor_checkpoint`, the lowest checkpoint whose object state it keeps. Older versions are
/// pruned and permanently gone. It is [`below_serving_floor`] for `floor_checkpoint`, with the
/// version as `lowestAvailableVersion` too, so a client can list from it instead.
pub fn below_oldest_retained_version(
    floor_checkpoint: u64,
    oldest_retained_version: u64,
) -> RpcError {
    let error_info = ledger_error_info(
        ErrorReason::BelowEarliestAvailable,
        [
            ("lowestAvailableCheckpoint", floor_checkpoint.to_string()),
            (
                "lowestAvailableVersion",
                oldest_retained_version.to_string(),
            ),
        ],
    );
    RpcError::new(
        tonic::Code::OutOfRange,
        format!(
            "requested object versions below earliest available; lowest available checkpoint is \
             {floor_checkpoint}, and lowest available version is {oldest_retained_version}"
        ),
    )
    .with_details(ErrorDetails::new().with_error_info(error_info))
}

/// The error for a request that reads above the data this server has indexed, all of which lies
/// below `checkpoint_hi_exclusive`. `bound` names what reads past it: "cursor" for an `after` or
/// `before` cursor, or the request field, such as "start_checkpoint", "end_checkpoint" or
/// "at_checkpoint"; `requested_checkpoint` is the checkpoint of the cursor's public position or
/// the field's value. It is `OutOfRange` with an `ABOVE_LATEST_AVAILABLE` [`ErrorInfo`] carrying
/// all three, by which clients recognize a request that becomes servable once this server
/// catches up.
pub(crate) fn above_latest_available(
    bound: &str,
    requested_checkpoint: u64,
    checkpoint_hi_exclusive: u64,
) -> RpcError {
    let message = if bound == "cursor" {
        format!(
            "requested data above latest available; cursor is at checkpoint \
             {requested_checkpoint}, and only checkpoints below {checkpoint_hi_exclusive} are \
             available"
        )
    } else {
        format!(
            "requested data above latest available; {bound} is {requested_checkpoint}, and only \
             checkpoints below {checkpoint_hi_exclusive} are available"
        )
    };
    let error_info = ledger_error_info(
        ErrorReason::AboveLatestAvailable,
        [
            ("bound", bound.to_owned()),
            ("requestedCheckpoint", requested_checkpoint.to_string()),
            (
                "firstUnavailableCheckpoint",
                checkpoint_hi_exclusive.to_string(),
            ),
        ],
    );
    RpcError::new(tonic::Code::OutOfRange, message)
        .with_details(ErrorDetails::new().with_error_info(error_info))
}

/// An [`ErrorInfo`] in the [`ErrorReason::DOMAIN`] domain with `reason` and `metadata`.
pub(crate) fn ledger_error_info<const N: usize>(
    reason: ErrorReason,
    metadata: [(&str, String); N],
) -> ErrorInfo {
    let metadata = metadata
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect();
    ErrorInfo::default()
        .with_reason(reason.as_str_name())
        .with_domain(ErrorReason::DOMAIN)
        .with_metadata(metadata)
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
}

impl<P> ScanWindow<P> {
    /// A window whose edges are checkpoint boundaries, `ledger` in ledger coordinates.
    pub fn whole_checkpoints(range: Range<P>, ledger: LedgerWindow) -> Self {
        Self {
            range,
            start_cuts_checkpoint: false,
            end_cuts_checkpoint: false,
            ledger,
        }
    }
}

impl LedgerWindow {
    /// The window of a checkpoint scan over `range`, the request's checkpoint window
    /// `start_checkpoint..end_checkpoint` clamped to this server's tip.
    pub fn checkpoints(
        range: &Range<u64>,
        start_checkpoint: Option<u64>,
        end_checkpoint: Option<u64>,
    ) -> Self {
        let start = LedgerPoint::checkpoint_start(range.start);
        let end = LedgerPoint::checkpoint_start(range.end);
        let requested = range.is_empty().then(|| {
            (
                LedgerPoint::checkpoint_start(start_checkpoint.unwrap_or(0)),
                LedgerPoint::checkpoint_start(end_checkpoint.unwrap_or(range.end)),
            )
        });
        Self {
            start,
            end,
            requested,
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
    /// Record the request's checkpoint bounds, then apply cursor bounds to tighten the `window`
    /// scanning interval. Cursors bound the scan by position alone; their public positions only
    /// label the cursors this scan hands back. If the backend data source has a serving floor,
    /// `apply_serving_floor` must be called to reconcile the scanning interval.
    ///
    /// The window's request positions and its cursors bound the scan by intersection. A position
    /// that cuts the window's first checkpoint in scan direction acts as a starting cursor does:
    /// it vouches for every position before it, so the scan still claims that checkpoint once it
    /// reaches its end. A position that cuts the window's last checkpoint in scan direction acts as
    /// a far cursor does: the scan never reaches that checkpoint's end, so its natural end does not
    /// claim it.
    ///
    /// In a window that reaches the ledger tip, empty or not, an ascending scan whose `after`
    /// cursor resumes past the tip ends empty where that cursor resumes, or at a `before` cursor
    /// at or below that position, and claims no checkpoint.
    pub fn resolve(
        cp_range: ResolvedCheckpointRange,
        window: ScanWindow<P>,
        options: &QueryOptions,
    ) -> Result<Self, RpcError> {
        let ScanWindow {
            range,
            start_cuts_checkpoint,
            end_cuts_checkpoint,
            ledger,
        } = window;
        if cp_range.reaches_ledger_tip {
            reject_descending_cursors_past_tip(options, range.end, cp_range.range.end)?;
        }

        // Positions that cross leave nothing between them: collapse the window onto its terminal
        // edge in scan direction.
        let range = match options.ordering {
            _ if range.start <= range.end => range,
            Ordering::Ascending => range.end..range.end,
            Ordering::Descending => range.start..range.start,
        };

        let entry_checkpoint = if cp_range.is_empty() {
            // No checkpoint entered, pin entry to terminal boundary
            cp_range.range.end
        } else if options.is_ascending() {
            cp_range.range.start
        } else {
            cp_range.range.end - 1
        };

        let window_claim = if cp_range.is_empty() {
            None
        } else {
            match options.ordering {
                Ordering::Ascending => cp_range
                    .range
                    .end
                    .checked_sub(1 + u64::from(end_cuts_checkpoint)),
                Ordering::Descending => {
                    Some(cp_range.range.start + u64::from(start_cuts_checkpoint))
                }
            }
        };

        let (end_scan_position, end_public_position) = match options.ordering {
            Ordering::Ascending => (range.end, ledger.end),
            Ordering::Descending => (range.start, ledger.start),
        };
        let resolved = Self {
            bounds: ScanBounds::from_range(range.start..range.end),
            entry_checkpoint,
            end_public_position,
            end_scan_position,
            exhaustion: cp_range.exhaustion,
            window_claim,
        }
        .apply_cursor_bounds(options);

        if cp_range.reaches_ledger_tip
            && let Some((point, position, exhaustion)) =
                ascending_after_past_tip_end(options, range.end)
        {
            // The client's coverage already reaches past this server's tip, so a claim from this
            // view could only lower it.
            return Ok(Self {
                bounds: ScanBounds::empty_at(position),
                end_public_position: point,
                end_scan_position: position,
                exhaustion,
                window_claim: None,
                ..resolved
            });
        }

        if !cp_range.is_empty()
            && resolved.bounds.admits_no_position()
            && let Some((point, position)) = starting_cursor_end(options, &range)
        {
            // The starting cursor leaves the window nothing to scan, and no far cursor stops the
            // scan first: the page ends with the window's reason, where `starting_cursor_end`
            // places it. A far cursor at or before the window's edge ends the page instead.
            return Ok(Self {
                bounds: ScanBounds::empty_at(position),
                end_public_position: point,
                end_scan_position: position,
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
    /// the window's last checkpoint in scan direction, just below its exclusive upper edge
    /// ascending, or its inclusive lower edge descending. A scan that drains its window has
    /// covered every checkpoint up to that edge, and so has a scan its starting cursor leaves
    /// empty, because the cursor vouches for every position before it. A request position that
    /// cuts that last checkpoint moves the claim to the checkpoint before it in scan direction.
    /// `None` when the window has no checkpoint, when an ascending scan's `after` cursor resumes
    /// past the ledger tip, or when the claim would precede `entry_checkpoint`.
    pub fn natural_end_claim(&self, options: &QueryOptions) -> Option<u64> {
        let claim = self.window_claim?;
        match options.ordering {
            Ordering::Ascending => Some(claim).filter(|claim| *claim >= self.entry_checkpoint),
            Ordering::Descending => Some(claim).filter(|claim| *claim <= self.entry_checkpoint),
        }
    }

    /// Where this scan ends once it drains its range, as its terminal frame renders it: its
    /// reason, the terminal cursor `position_at` builds from the end's scan position and public
    /// position (see [`Self::end_cursor_public_position`]), where the frame resumes in ledger
    /// coordinates (see [`Self::end_resume_position`]), and the natural end's claim (see
    /// [`Self::natural_end_claim`]). `window` is the window this scan was resolved from.
    pub fn range_end(
        &self,
        window: &ScanWindow<P>,
        options: &QueryOptions,
        position_at: impl FnOnce(P, LedgerPoint) -> Position,
    ) -> RangeEnd {
        RangeEnd {
            exhaustion: self.exhaustion,
            position: position_at(
                self.end_scan_position,
                self.end_cursor_public_position(options),
            ),
            cursor_position: self.end_resume_position(window, options),
            natural_end_claim: self.natural_end_claim(options),
        }
    }

    /// The public position of this scan's terminal cursor, at its end scan position: that of a
    /// request cursor that names that position (see `request_cursor_point`), else
    /// [`Self::end_public_position`].
    pub fn end_cursor_public_position(&self, options: &QueryOptions) -> LedgerPoint {
        request_cursor_point(options, self.end_scan_position).unwrap_or(self.end_public_position)
    }

    /// Where this scan's terminal frame resumes in ledger coordinates, known without a read.
    /// `window` is the window this scan was resolved from.
    ///
    /// The frame resumes at its end scan position, clamped so it never lies before the request's
    /// effective start ascending (the window's start, the position its `after` cursor resumes at)
    /// or past its effective end descending (the window's end, its `before` cursor). A window
    /// that holds no checkpoint names the request's own start (ascending) or end (descending):
    /// past this server's tip, an ascending terminal echoes where the request starts.
    ///
    /// A position is placed by the serving floor, by a window edge (the request's own fields), by
    /// the public position of a request cursor that names it (see `request_cursor_point`): where
    /// an `after` cursor resumes, including past this server's tip, or a `before` cursor's
    /// position; or, for the end scan position itself, by its cursor's public position.
    pub fn end_resume_position(
        &self,
        window: &ScanWindow<P>,
        options: &QueryOptions,
    ) -> LedgerPoint {
        if let RangeExhaustion::ServingFloor { floor_checkpoint } = self.exhaustion {
            // The floor is the first position of its checkpoint.
            return LedgerPoint::checkpoint_start(floor_checkpoint);
        }
        if let Some((requested_start, requested_end)) = window.ledger.requested {
            return match options.ordering {
                Ordering::Ascending => requested_start,
                Ordering::Descending => requested_end,
            };
        }
        let position = match options.ordering {
            Ordering::Ascending => {
                let after_resumes_at = options
                    .after
                    .as_ref()
                    // An Item at the last position keeps naming itself, a saturated resume point.
                    .map(|after| after_resume::<P>(after).0);
                let entry = after_resumes_at
                    .map_or(window.range.start, |resume| resume.max(window.range.start));
                self.end_scan_position.max(entry)
            }
            Ordering::Descending => {
                let entry = options.before.as_ref().map_or(window.range.end, |before| {
                    ScanCursor::<P>::coordinate(before).min(window.range.end)
                });
                self.end_scan_position.min(entry)
            }
        };
        // The clamp's entry is a window edge or a position a request cursor names, so a position
        // neither places is the end scan position.
        window_edge_point(window, options.ordering, position)
            .or_else(|| request_cursor_point(options, position))
            .unwrap_or_else(|| self.end_cursor_public_position(options))
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

        if let Some((point, position, kind)) = cursor_terminal {
            self.set_terminal_record(point, position, RangeExhaustion::CursorBound { kind });

            self.bounds = ScanBounds::empty_at(self.end_scan_position);
        }

        self
    }

    fn set_terminal_record(
        &mut self,
        point: LedgerPoint,
        position: P,
        exhaustion: RangeExhaustion,
    ) {
        self.end_public_position = point;
        self.end_scan_position = position;
        self.exhaustion = exhaustion;
    }

    /// Tightens the low bound, and returns the terminal record if the cursor empties the interval.
    fn apply_after_cursor(
        &mut self,
        options: &QueryOptions,
    ) -> Option<(LedgerPoint, P, CursorKind)> {
        let cursor = options.after.as_ref()?;
        let position: P = cursor.coordinate();

        // Symbolic resume in every lane: an Item admits strictly-after, a
        // Boundary admits from itself.
        let candidate = match cursor.kind {
            CursorKind::Item => Bound::Excluded(position),
            CursorKind::Boundary => Bound::Included(position),
        };

        if !lower_bound_gte(candidate, self.bounds.lo) {
            return None;
        }

        // The after cursor is the terminal bound for a descending scan. Its closing cursor names
        // the boundary just below the lowest position the scan admits, where `after` resumes, so
        // that sent back as `before` it resumes with the next position down.
        let (descending_terminal, descending_terminal_kind, descending_terminal_point) =
            after_resume::<P>(cursor);
        if !options.is_ascending() {
            self.set_terminal_record(
                descending_terminal_point,
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

        // Report the cursor that emptied the bound. An ascending `after` cursor reports its own
        // position and kind; `ResolvedScan::resolve` restates it as the boundary at the first
        // position it admits. An Item at the last position admits none, so it keeps Item kind.
        if options.is_ascending() {
            Some((cursor.position.public_position(), position, cursor.kind))
        } else {
            Some((
                descending_terminal_point,
                descending_terminal,
                descending_terminal_kind,
            ))
        }
    }

    /// Tightens the high bound, and returns the terminal record if the cursor empties the interval.
    fn apply_before_cursor(
        &mut self,
        options: &QueryOptions,
    ) -> Option<(LedgerPoint, P, CursorKind)> {
        let cursor = options.before.as_ref()?;
        let point = cursor.position.public_position();
        let position: P = cursor.coordinate();

        if !hi_admits_upper_bound(self.bounds.hi, position) {
            return None;
        }

        if options.is_ascending() {
            self.set_terminal_record(
                point,
                position,
                RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
                },
            );
        }

        self.bounds.hi = Bound::Excluded(position);

        if !self.bounds.is_empty() {
            return None;
        }

        Some((point, position, CursorKind::Boundary))
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
                LedgerPoint::checkpoint_start(floor_checkpoint),
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
    /// A descending window that reaches past the tip returns `OutOfRange` with the
    /// `ABOVE_LATEST_AVAILABLE` reason: an `end_checkpoint` above the tip, or, with
    /// `end_checkpoint` unset, a `start_checkpoint` at or above it. A descending scan starts at
    /// its window's high end, so answering from this view would skip the checkpoints between the
    /// tip and that end; like a descending scan's cursor past the tip, the request becomes
    /// servable once this server catches up. An ascending window past the tip ends at the tip,
    /// where the client resumes once this server has caught up, so only an ascending scan ends
    /// with `LedgerTip`.
    ///
    /// The request's cursors do not narrow the window here: [`ResolvedScan::resolve`] bounds the
    /// scan by the cursors' positions.
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
            return Err(above_latest_available(
                field,
                requested,
                checkpoint_hi_exclusive,
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

/// Where a page ends when its starting cursor sits at or past the window's edge in scan
/// direction, `window.end` ascending or `window.start` descending, and no far cursor stops the
/// scan at or before that edge, as `(public position, boundary position)`. An `after` cursor
/// starts an ascending scan at the first position it admits; a `before` cursor starts a
/// descending scan below its position. A far cursor stops the scan as `apply_cursor_bounds`
/// applies it: a `before` at or below the window's end, an `after` at or above the window's
/// start.
///
/// - At the edge, the page ends at the window's edge, as the page that handed the cursor back
///   did.
/// - Past the edge, the page ends at the boundary where the starting cursor resumes: the boundary
///   after an `after` Item cursor's item, or a `before` cursor's position. Ending at the window's
///   edge instead would hand back a cursor behind the client's, which replays the positions in
///   between when resumed without the window.
///
/// Either way the starting cursor names where the page ends, so the end carries the public
/// position where it resumes. `None` otherwise, including for an `after` Item cursor at the last
/// position, which names no boundary after it.
fn starting_cursor_end<P: ScanPosition>(
    options: &QueryOptions,
    window: &Range<P>,
) -> Option<(LedgerPoint, P)>
where
    CursorToken: ScanCursor<P>,
{
    let position = |cursor: &CursorToken| -> P { cursor.coordinate() };
    match options.ordering {
        Ordering::Ascending => {
            if options
                .before
                .as_ref()
                .is_some_and(|before| position(before) <= window.end)
            {
                return None;
            }
            let after = options.after.as_ref()?;
            let (first_admitted, kind, point) = after_resume::<P>(after);
            // An Item at the last position admits nothing, so it ends no scan here.
            (kind == CursorKind::Boundary && first_admitted >= window.end)
                .then_some((point, first_admitted))
        }
        Ordering::Descending => {
            if options
                .after
                .as_ref()
                .is_some_and(|after| position(after) >= window.start)
            {
                return None;
            }
            let before = options.before.as_ref()?;
            let upper = position(before);
            (upper <= window.start).then(|| (before.position.public_position(), upper))
        }
    }
}

/// Rejects a descending scan whose cursor names a position at or past `tip`, the first position
/// this server has not indexed. Such a cursor comes from a newer view of the ledger than this
/// server's, from another replica or derived from a newer tip. Answering from this view would
/// start the scan below its `before` cursor, skipping items the client has not seen, or end a
/// scan bounded by its `after` cursor empty, though items lie above that cursor. `OutOfRange` with
/// the `ABOVE_LATEST_AVAILABLE` reason (see [`above_latest_available`]) marks the request as
/// reading past the data this server has, the mirror of a cursor below the pruned floor; unlike
/// that one, it becomes servable once this server catches up, so clients that recognize the
/// reason may retry it. An ascending scan's cursors may lie past the tip: its `before` cursor
/// leaves the tip as the scan's high end, and [`ascending_after_past_tip_end`] places the end of a
/// page whose `after` cursor resumes past it.
fn reject_descending_cursors_past_tip<P: Ord>(
    options: &QueryOptions,
    tip: P,
    checkpoint_hi_exclusive: u64,
) -> Result<(), RpcError>
where
    CursorToken: ScanCursor<P>,
{
    if options.is_ascending() {
        return Ok(());
    }
    for cursor in [options.before.as_ref(), options.after.as_ref()]
        .into_iter()
        .flatten()
    {
        let position = cursor.coordinate();
        // A boundary exactly at the tip is where a caught-up scan resumes; an item there, or any
        // position beyond it, has not been indexed here.
        let past_tip = match cursor.kind {
            CursorKind::Item => position >= tip,
            CursorKind::Boundary => position > tip,
        };
        if past_tip {
            let requested_checkpoint = cursor.position.public_position().checkpoint;
            return Err(above_latest_available(
                "cursor",
                requested_checkpoint,
                checkpoint_hi_exclusive,
            ));
        }
    }
    Ok(())
}

/// Where an ascending page ends when its `after` cursor resumes past `tip`, the first position
/// this server has not indexed, as `(public position, position, reason)`. Such a cursor comes
/// from a newer view of the ledger than this server's, from another replica or derived from a
/// newer tip. This server holds nothing past its tip, so the page is empty and ends with
/// `LedgerTip` where the cursor resumes: at the boundary after an Item cursor's item, or at a
/// Boundary cursor's position. The client resumes from there once this server has caught up;
/// ending at this server's tip instead would hand back a cursor behind the client's, which
/// replays the positions in between.
///
/// A `before` cursor at or below that position leaves nothing between the cursors, so the page
/// ends at `before` with `CursorBound`, as it does on a server that has caught up. An Item cursor
/// at the last position admits nothing, so its page ends at `before` too, or with no `before`, at
/// the Item with `CursorBound`.
///
/// `None` for a descending scan, or for an `after` cursor that resumes at or below `tip`.
fn ascending_after_past_tip_end<P: ScanPosition>(
    options: &QueryOptions,
    tip: P,
) -> Option<(LedgerPoint, P, RangeExhaustion)>
where
    CursorToken: ScanCursor<P>,
{
    let after = options.after.as_ref().filter(|_| options.is_ascending())?;
    let (end, kind, point) = after_resume::<P>(after);
    if end <= tip {
        return None;
    }
    if let Some(before) = options.before.as_ref()
        && before.coordinate() <= end
    {
        return Some((
            before.position.public_position(),
            before.coordinate(),
            RangeExhaustion::CursorBound {
                kind: CursorKind::Boundary,
            },
        ));
    }
    let exhaustion = match kind {
        CursorKind::Boundary => RangeExhaustion::LedgerTip,
        CursorKind::Item => RangeExhaustion::CursorBound { kind },
    };
    Some((point, end, exhaustion))
}

/// Where an `after` cursor resumes a scan, as `(position, kind)` of the cursor that names that
/// place, and that place's public position: a Boundary cursor's own position, or the boundary
/// after an Item cursor's item (its key with the finest index incremented). An Item at the last
/// position names no boundary after it, so it names itself, still an Item.
fn after_resume<P: ScanPosition>(after: &CursorToken) -> (P, CursorKind, LedgerPoint)
where
    CursorToken: ScanCursor<P>,
{
    let position: P = after.coordinate();
    match (after.kind, position.successor()) {
        (CursorKind::Item, Some(next)) => (
            next,
            CursorKind::Boundary,
            after.position.item_resume(ScanDirection::Ascending),
        ),
        _ => (position, after.kind, after.position.public_position()),
    }
}

/// The public position of scan position `position` when a request cursor names it, the point a
/// terminal there resumes at: where the `after` cursor resumes (see [`after_resume`]), or the
/// `before` cursor's own position, `after` first where both name it. `None` when no request
/// cursor names it.
fn request_cursor_point<P: ScanPosition>(options: &QueryOptions, position: P) -> Option<LedgerPoint>
where
    CursorToken: ScanCursor<P>,
{
    let after = options.after.as_ref().and_then(|after| {
        let (resume, _, point) = after_resume::<P>(after);
        (resume == position).then_some(point)
    });
    after.or_else(|| {
        options
            .before
            .as_ref()
            .filter(|before| ScanCursor::<P>::coordinate(*before) == position)
            .map(|before| before.position.public_position())
    })
}

/// The point of `position` when it is one of `window`'s edges. Where the edges are one scan
/// position (the window spans only empty checkpoints, or a start index past its checkpoint's
/// end), it names the edge in scan direction, the one its checkpoint claim reaches.
fn window_edge_point<P: Eq + Copy>(
    window: &ScanWindow<P>,
    ordering: Ordering,
    position: P,
) -> Option<LedgerPoint> {
    let ledger = &window.ledger;
    let (terminal_edge, entry_edge) = match ordering {
        Ordering::Ascending => (
            (window.range.end, ledger.end),
            (window.range.start, ledger.start),
        ),
        Ordering::Descending => (
            (window.range.start, ledger.start),
            (window.range.end, ledger.end),
        ),
    };
    if position == terminal_edge.0 {
        Some(terminal_edge.1)
    } else if position == entry_edge.0 {
        Some(entry_edge.1)
    } else {
        None
    }
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

/// Asserts `error` is `OutOfRange` with `message` and an [`ErrorInfo`] in the
/// [`ErrorReason::DOMAIN`] domain whose reason is `reason` and whose metadata is exactly
/// `metadata`.
#[cfg(test)]
fn assert_out_of_range_error_info(
    error: RpcError,
    message: &str,
    reason: ErrorReason,
    metadata: &[(&str, &str)],
) {
    let status = error.into_status_proto();
    assert_eq!(status.code, tonic::Code::OutOfRange as i32, "{status:?}");
    assert_eq!(status.message, message);
    let error_infos: Vec<ErrorInfo> = status
        .details
        .iter()
        .filter_map(|detail| detail.to_msg::<ErrorInfo>().ok())
        .collect();
    let [error_info] = error_infos.as_slice() else {
        panic!("expected exactly one ErrorInfo in {status:?}");
    };
    assert_eq!(error_info.reason, reason.as_str_name(), "{status:?}");
    assert_eq!(error_info.domain, ErrorReason::DOMAIN, "{status:?}");
    let expected_metadata: std::collections::BTreeMap<String, String> = metadata
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    assert_eq!(error_info.metadata, expected_metadata, "{status:?}");
}

/// Asserts `error` is [`below_serving_floor`] for `floor_checkpoint`: its message and its
/// `BELOW_EARLIEST_AVAILABLE` [`ErrorInfo`].
#[cfg(test)]
pub(crate) fn assert_below_serving_floor_error(error: RpcError, floor_checkpoint: u64) {
    assert_out_of_range_error_info(
        error,
        &format!(
            "requested data below earliest available; lowest available checkpoint is \
             {floor_checkpoint}"
        ),
        ErrorReason::BelowEarliestAvailable,
        &[("lowestAvailableCheckpoint", &floor_checkpoint.to_string())],
    );
}

/// Asserts `error` is [`below_oldest_retained_version`] for `floor_checkpoint` and
/// `oldest_retained_version`.
#[cfg(test)]
pub(crate) fn assert_below_oldest_retained_version_error(
    error: RpcError,
    floor_checkpoint: u64,
    oldest_retained_version: u64,
) {
    assert_out_of_range_error_info(
        error,
        &format!(
            "requested object versions below earliest available; lowest available checkpoint is \
             {floor_checkpoint}, and lowest available version is {oldest_retained_version}"
        ),
        ErrorReason::BelowEarliestAvailable,
        &[
            ("lowestAvailableCheckpoint", &floor_checkpoint.to_string()),
            (
                "lowestAvailableVersion",
                &oldest_retained_version.to_string(),
            ),
        ],
    );
}

/// Asserts `error` is [`above_latest_available`] for `bound` at `requested_checkpoint` on a
/// server that has indexed the checkpoints below `checkpoint_hi_exclusive`: its message and its
/// `ABOVE_LATEST_AVAILABLE` [`ErrorInfo`].
#[cfg(test)]
fn assert_above_latest_available_error(
    error: RpcError,
    bound: &str,
    requested_checkpoint: u64,
    checkpoint_hi_exclusive: u64,
) {
    let requested = match bound {
        "cursor" => format!("cursor is at checkpoint {requested_checkpoint}"),
        field => format!("{field} is {requested_checkpoint}"),
    };
    assert_out_of_range_error_info(
        error,
        &format!(
            "requested data above latest available; {requested}, and only checkpoints below \
             {checkpoint_hi_exclusive} are available"
        ),
        ErrorReason::AboveLatestAvailable,
        &[
            ("bound", bound),
            ("requestedCheckpoint", &requested_checkpoint.to_string()),
            (
                "firstUnavailableCheckpoint",
                &checkpoint_hi_exclusive.to_string(),
            ),
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The version floor names the checkpoint floor and the oldest retained version, each in its
    /// own metadata entry and in the message, without swapping them.
    #[test]
    fn below_oldest_retained_version_names_checkpoint_and_version() {
        assert_below_oldest_retained_version_error(below_oldest_retained_version(9, 3), 9, 3);
    }

    /// The event-coordinate window spanning whole transactions `[start, end)`.
    fn tx_window(range: Range<u64>) -> Range<IntraTxCoordinate> {
        IntraTxCoordinate::start_of_tx(range.start)..IntraTxCoordinate::start_of_tx(range.end)
    }

    fn query_options_from_proto(
        request: Option<&ProtoQueryOptions>,
    ) -> Result<QueryOptions, RpcError> {
        QueryOptions::transactions_from_proto(request, 100, 1_000)
    }

    fn resolved_range(range: Range<u64>) -> ResolvedScan<u64> {
        ResolvedScan {
            bounds: ScanBounds::from_range(range),
            end_public_position: model_point(20, 0),
            end_scan_position: 20,
            exhaustion: RangeExhaustion::CheckpointBound,
            entry_checkpoint: 0,
            window_claim: Some(19),
        }
    }

    /// The empty resolution `resolved_range` reaches when a cursor empties it.
    fn empty_resolved_range(
        end_public_position: LedgerPoint,
        end_scan_position: u64,
        exhaustion: RangeExhaustion,
    ) -> ResolvedScan<u64> {
        ResolvedScan {
            bounds: ScanBounds::empty_at(end_scan_position),
            end_public_position,
            end_scan_position,
            exhaustion,
            entry_checkpoint: 0,
            window_claim: Some(19),
        }
    }

    fn tx_position(tx_seq: u64, point: LedgerPoint) -> Position {
        Position::Transactions {
            tx_seq,
            public_position: point,
        }
    }

    fn ev_position(tx_seq: u64, event_index: u32, point: LedgerPoint) -> Position {
        Position::Events {
            tx_seq,
            event_index,
            public_position: point,
        }
    }

    fn tx_item(tx_seq: u64) -> CursorToken {
        CursorToken::item(tx_position(tx_seq, model_point(tx_seq, 0)))
    }

    fn tx_boundary(tx_seq: u64) -> CursorToken {
        CursorToken::boundary(tx_position(tx_seq, model_point(tx_seq, 0)))
    }

    /// The boundary an ascending scan frontier at the start of `tx_seq` mints, placed after the
    /// transaction before it (see [`after_transaction_point`]).
    fn tx_frontier(tx_seq: u64) -> CursorToken {
        CursorToken::boundary(tx_position(tx_seq, after_transaction_point(tx_seq)))
    }

    fn cp_item(checkpoint: u64) -> CursorToken {
        CursorToken::item(Position::Checkpoints { checkpoint })
    }

    fn ev_item(tx_seq: u64, index: u32) -> CursorToken {
        CursorToken::item(ev_position(tx_seq, index, model_point(tx_seq, index)))
    }

    fn ev_boundary(tx_seq: u64, index: u32) -> CursorToken {
        CursorToken::boundary(ev_position(tx_seq, index, model_point(tx_seq, index)))
    }

    fn resolved_intra_tx() -> ResolvedScan<IntraTxCoordinate> {
        ResolvedScan {
            bounds: IntraTxScanBounds::from_range(
                IntraTxCoordinate::start_of_tx(0)..IntraTxCoordinate::start_of_tx(10),
            ),
            entry_checkpoint: 2,
            end_public_position: model_point(10, 0),
            end_scan_position: IntraTxCoordinate::start_of_tx(10),
            exhaustion: RangeExhaustion::CheckpointBound,
            window_claim: Some(8),
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
        let error = result.expect_err("expected OutOfRange below the serving floor");
        assert_below_serving_floor_error(error, floor_checkpoint);
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
            end_public_position: model_point(100, 0),
            end_scan_position: 100,
            exhaustion: RangeExhaustion::CheckpointBound,
            entry_checkpoint: 0,
            window_claim: Some(19),
        };
        resolved.apply_serving_floor(50, 10, &asc).unwrap();
        assert_eq!(resolved.natural_end_claim(&asc), Some(19));
        assert_eq!(resolved.range(), 50..100);
        assert_eq!(resolved.entry_checkpoint, 10);
        assert_eq!(resolved.end_public_position, model_point(100, 0));
        assert_eq!(resolved.end_scan_position, 100);
        assert_eq!(resolved.exhaustion, RangeExhaustion::CheckpointBound);

        // Descending, floor inside: entry (the high edge) untouched; the
        // scan ends at the floor, which it reaches as its serving floor.
        let mut resolved = ResolvedScan {
            bounds: ScanBounds::from_range(0..100),
            end_public_position: LedgerPoint::GENESIS,
            end_scan_position: 0,
            exhaustion: RangeExhaustion::CheckpointBound,
            entry_checkpoint: 20,
            window_claim: Some(0),
        };
        resolved.apply_serving_floor(50, 10, &desc).unwrap();
        assert_eq!(resolved.range(), 50..100);
        assert_eq!(resolved.entry_checkpoint, 20);
        assert_eq!(
            resolved.end_public_position,
            LedgerPoint::checkpoint_start(10)
        );
        assert_eq!(resolved.end_scan_position, 50);
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
                end_public_position: model_point(40, 0),
                end_scan_position: 40,
                exhaustion: RangeExhaustion::CheckpointBound,
                entry_checkpoint: 0,
                window_claim: Some(7),
            };
            assert_below_serving_floor(resolved.apply_serving_floor(floor_tx, 10, &asc), 10);

            let mut resolved = ResolvedScan {
                bounds: ScanBounds::from_range(0..40),
                end_public_position: LedgerPoint::GENESIS,
                end_scan_position: 0,
                exhaustion: RangeExhaustion::CheckpointBound,
                entry_checkpoint: 8,
                window_claim: Some(0),
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
            end_public_position: model_point(100, 0),
            end_scan_position: IntraTxCoordinate::start_of_tx(100),
            exhaustion: RangeExhaustion::CheckpointBound,
            entry_checkpoint: 0,
            window_claim: Some(19),
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
        assert_eq!(resolved.end_public_position, model_point(100, 0));
        assert_eq!(
            resolved.end_scan_position,
            IntraTxCoordinate::start_of_tx(100)
        );

        // Descending, floor inside: the scan ends at the floor, which it reaches as its serving
        // floor.
        let mut resolved = ResolvedScan {
            bounds: IntraTxScanBounds::from_range(
                IntraTxCoordinate::start_of_tx(0)..IntraTxCoordinate::start_of_tx(100),
            ),
            end_public_position: LedgerPoint::GENESIS,
            end_scan_position: IntraTxCoordinate::start_of_tx(0),
            exhaustion: RangeExhaustion::CheckpointBound,
            entry_checkpoint: 20,
            window_claim: Some(0),
        };
        resolved
            .apply_serving_floor(IntraTxCoordinate::start_of_tx(50), 10, &desc)
            .unwrap();
        assert_eq!(
            resolved.bounds.lo,
            Bound::Included(IntraTxCoordinate::start_of_tx(50))
        );
        assert_eq!(resolved.entry_checkpoint, 20);
        assert_eq!(
            resolved.end_public_position,
            LedgerPoint::checkpoint_start(10)
        );
        assert_eq!(
            resolved.end_scan_position,
            IntraTxCoordinate::start_of_tx(50)
        );
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
                end_public_position: model_point(40, 0),
                end_scan_position: IntraTxCoordinate::start_of_tx(40),
                exhaustion: RangeExhaustion::CheckpointBound,
                entry_checkpoint: 0,
                window_claim: Some(7),
            };
            assert_below_serving_floor(
                resolved.apply_serving_floor(IntraTxCoordinate::start_of_tx(floor_tx), 10, &asc),
                10,
            );

            let mut resolved = ResolvedScan {
                bounds: IntraTxScanBounds::from_range(
                    IntraTxCoordinate::start_of_tx(0)..IntraTxCoordinate::start_of_tx(40),
                ),
                end_public_position: LedgerPoint::GENESIS,
                end_scan_position: IntraTxCoordinate::start_of_tx(0),
                exhaustion: RangeExhaustion::CheckpointBound,
                entry_checkpoint: 8,
                window_claim: Some(0),
            };
            assert_below_serving_floor(
                resolved.apply_serving_floor(IntraTxCoordinate::start_of_tx(floor_tx), 10, &desc),
                10,
            );
        }
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
                reached_floor.end_public_position,
                reached_floor.end_scan_position,
            ),
            (
                RangeExhaustion::ServingFloor { floor_checkpoint },
                LedgerPoint::checkpoint_start(floor_checkpoint),
                floor,
            )
        );

        let at_floor = CursorToken::boundary(tx_position(
            reached_floor.end_scan_position,
            reached_floor.end_cursor_public_position(&walk),
        ));
        let below_floor = tx_boundary(33);
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
        let after = tx_item(20).encode();
        let before = tx_item(30).encode();
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
        request.before = Some(tx_item(30).encode());
        let options = query_options_from_proto(Some(&request)).unwrap();
        assert!(!options.has_after_cursor());

        // `after` raises the low end → explicit.
        let mut request = ProtoQueryOptions::default();
        request.after = Some(tx_item(20).encode());
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
        let after = tx_item(9).encode();
        let before = tx_item(30).encode();
        let mut request = ProtoQueryOptions::default();
        request.after = Some(after);
        request.before = Some(before);
        assert!(query_options_from_proto(Some(&request)).is_ok());
    }

    #[test]
    fn accepts_cursors_for_different_checkpoint_range_and_ordering() {
        let token = tx_item(9).encode();
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
            after: Some(tx_item(11)),
            before: None,
        };
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options).range(),
            12..20
        );

        let options = QueryOptions {
            after: Some(tx_item(u64::MAX)),
            ..options
        };
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            empty_resolved_range(
                model_point(u64::MAX, 0),
                u64::MAX,
                RangeExhaustion::CursorBound {
                    kind: CursorKind::Item,
                },
            )
        );

        let options = QueryOptions {
            ordering: Ordering::Descending,
            after: Some(tx_item(11)),
            before: Some(tx_item(19)),
            ..options
        };
        let bounded = resolved_range(10..20).apply_cursor_bounds(&options);
        assert_eq!(bounded.range(), 12..19);
        assert_eq!(
            bounded.exhaustion,
            RangeExhaustion::CursorBound {
                kind: CursorKind::Boundary,
            }
        );
        // The terminal is the boundary just above the `after` item, which the scan excluded.
        assert_eq!(bounded.end_scan_position, 12);
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
            after: Some(tx_item(11)),
            before: Some(tx_item(12)),
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
                end_public_position: after_transaction_point(12),
                end_scan_position: 12,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
                },
                window_claim: Some(19),
            }
        );
        assert!(crossed.range().is_empty());
    }

    #[test]
    fn applies_boundary_cursor_bounds_without_item_offset() {
        let options = QueryOptions {
            limit_items: 2,
            ordering: Ordering::Ascending,
            after: Some(tx_boundary(20)),
            before: None,
        };
        assert_eq!(
            resolved_range(10..30).apply_cursor_bounds(&options).range(),
            20..30
        );

        let options = QueryOptions {
            ordering: Ordering::Descending,
            after: None,
            before: Some(tx_boundary(20)),
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

    /// (start_checkpoint = 30, tip = 20, ascending): resolves empty at the tip — entry checkpoint
    /// 20, terminal at the start of checkpoint 20, LedgerTip.
    #[test]
    fn ascending_window_past_tip_empties_with_ledger_tip() {
        let options = QueryOptions::events_from_proto(None, 100, 100).unwrap();
        let cp_range = ResolvedCheckpointRange::from_request(Some(30), None, 20, &options).unwrap();
        assert!(cp_range.is_empty());
        let ledger = LedgerWindow::checkpoints(&cp_range.range, Some(30), None);
        assert_eq!(
            ResolvedScan::<IntraTxCoordinate>::resolve(
                cp_range,
                ScanWindow::whole_checkpoints(tx_window(100..100), ledger),
                &options
            )
            .unwrap(),
            ResolvedScan {
                bounds: IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(100)),
                entry_checkpoint: 20,
                end_public_position: LedgerPoint::checkpoint_start(20),
                end_scan_position: IntraTxCoordinate::start_of_tx(100),
                exhaustion: RangeExhaustion::LedgerTip,
                window_claim: None,
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
        for (start, end, bound, requested) in [
            (Some(30), None, "start_checkpoint", 30),
            (Some(20), None, "start_checkpoint", 20),
            (Some(10), Some(21), "end_checkpoint", 21),
            (None, Some(u64::MAX), "end_checkpoint", u64::MAX),
            (Some(25), Some(30), "end_checkpoint", 30),
        ] {
            let error = ResolvedCheckpointRange::from_request(start, end, 20, &options)
                .expect_err("descending window past the tip");
            assert_above_latest_available_error(error, bound, requested, 20);
        }

        let resolved = ResolvedCheckpointRange::from_request(Some(10), Some(20), 20, &options);
        assert_eq!(resolved.unwrap().range, 10..20);
        assert_eq!(
            ResolvedCheckpointRange::from_request(Some(20), Some(20), 20, &options).unwrap(),
            ResolvedCheckpointRange::empty_at(20, RangeExhaustion::CheckpointBound, false)
        );
    }

    /// A descending cursor past the tip reports the checkpoint of its public position as
    /// `requestedCheckpoint`; a checkpoint cursor reports its checkpoint.
    #[test]
    fn descending_cursor_past_tip_reports_its_public_checkpoint() {
        let descending = |before| options_with(Ordering::Descending, None, Some(before));
        // Position `(TIP + 1, 1)`.
        let error = resolve_tx_request(
            (None, None),
            &descending(tx_boundary(first_tx(TIP + 1) + 1)),
        )
        .expect_err("descending transaction cursor past the tip");
        assert_above_latest_available_error(error, "cursor", TIP + 1, TIP);

        let error = resolve_checkpoint_request((None, None), &descending(cp_item(TIP)))
            .expect_err("descending checkpoint cursor past the tip");
        assert_above_latest_available_error(error, "cursor", TIP, TIP);
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
            let ledger = LedgerWindow::checkpoints(&cp_range.range, Some(10), Some(10));
            assert_eq!(
                ResolvedScan::<IntraTxCoordinate>::resolve(
                    cp_range,
                    ScanWindow::whole_checkpoints(tx_window(100..100), ledger),
                    &options
                )
                .unwrap(),
                ResolvedScan {
                    bounds: IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(100)),
                    entry_checkpoint: 10,
                    end_public_position: LedgerPoint::checkpoint_start(10),
                    end_scan_position: IntraTxCoordinate::start_of_tx(100),
                    exhaustion: RangeExhaustion::CheckpointBound,
                    window_claim: None,
                }
            );
        }
    }

    /// If the bounds are already empty, cursors do not apply.
    #[test]
    fn cursor_bounds_pass_empty_resolution_through_unchanged() {
        let cursor = ev_item(50, 2);
        for ascending in [true, false] {
            let mut request = ProtoQueryOptions::default();
            if !ascending {
                request.ordering = Some(ProtoOrdering::Descending as i32);
            }
            request.after = Some(cursor.encode());
            let options = QueryOptions::events_from_proto(Some(&request), 100, 100).unwrap();
            let cp_range =
                ResolvedCheckpointRange::from_request(Some(10), Some(10), 20, &options).unwrap();
            let resolved = ResolvedScan::<IntraTxCoordinate>::resolve(
                cp_range,
                ScanWindow::whole_checkpoints(
                    tx_window(100..100),
                    LedgerWindow::checkpoints(&(0..0), None, None),
                ),
                &options,
            )
            .unwrap();
            assert_eq!(resolved.clone().apply_cursor_bounds(&options), resolved);
        }
    }

    /// resolve sets entry_checkpoint to the scan's first checkpoint and the terminal
    /// (end_public_position, end_scan_position) to its last, per ordering.
    #[test]
    fn resolve_orients_entry_and_terminal_by_ordering() {
        let options = directional_options(true);
        let cp_range =
            ResolvedCheckpointRange::from_request(Some(3), Some(10), 20, &options).unwrap();
        assert_eq!(cp_range.range, 3..10);
        let window = ScanWindow::whole_checkpoints(
            tx_window(100..200),
            LedgerWindow::checkpoints(&cp_range.range, Some(3), Some(10)),
        );
        let resolved =
            ResolvedScan::<IntraTxCoordinate>::resolve(cp_range.clone(), window.clone(), &options)
                .unwrap();
        assert_eq!(
            resolved.bounds,
            IntraTxScanBounds::from_range(
                IntraTxCoordinate::start_of_tx(100)..IntraTxCoordinate::start_of_tx(200)
            )
        );
        assert_eq!(resolved.entry_checkpoint, 3);
        assert_eq!(
            resolved.end_public_position,
            LedgerPoint::checkpoint_start(10)
        );
        assert_eq!(
            resolved.end_scan_position,
            IntraTxCoordinate::start_of_tx(200)
        );

        let options = directional_options(false);
        let resolved =
            ResolvedScan::<IntraTxCoordinate>::resolve(cp_range, window, &options).unwrap();
        assert_eq!(resolved.entry_checkpoint, 9);
        assert_eq!(
            resolved.end_public_position,
            LedgerPoint::checkpoint_start(3)
        );
        assert_eq!(
            resolved.end_scan_position,
            IntraTxCoordinate::start_of_tx(100)
        );
    }

    /// (ascending, `after` past the window's end): the page ends with the window's reason at the
    /// boundary where the cursor resumes, at the public position there, and claims the window's
    /// last checkpoint. An Item cursor's boundary sits after its event, so resuming from it does
    /// not re-serve the event.
    #[test]
    fn event_after_cursor_past_window_ends_where_it_resumes() {
        // Checkpoints 1..4, transactions 5..20.
        let window = (Some(1), Some(4));
        for (cursor, end_scan_position) in [
            (
                ev_item(25, 0),
                IntraTxCoordinate {
                    tx_seq: 25,
                    index: 1,
                },
            ),
            (ev_boundary(25, 0), IntraTxCoordinate::start_of_tx(25)),
        ] {
            let options = after_options(Ordering::Ascending, cursor.clone());
            let resolved = resolve_event_request(window, &options).unwrap();
            assert!(resolved.is_empty(), "{cursor:?}");
            assert_eq!(
                resolved.exhaustion,
                RangeExhaustion::CheckpointBound,
                "{cursor:?}"
            );
            assert_eq!(
                (resolved.end_public_position, resolved.end_scan_position),
                (
                    model_point(end_scan_position.tx_seq, end_scan_position.index),
                    end_scan_position
                ),
                "{cursor:?}"
            );
            assert_eq!(resolved.natural_end_claim(&options), Some(3), "{cursor:?}");
        }
    }

    /// `before` cursor that empties the interval sets end_scan_position, and exhaustion to
    /// CursorBound.
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
            end_public_position: model_point(100, 0),
            end_scan_position: IntraTxCoordinate::start_of_tx(100),
            exhaustion: RangeExhaustion::CheckpointBound,
            window_claim: Some(3),
        };

        // This cursor empties the interval; the entry checkpoint stays at the window's.
        for token in [ev_item(90, 0), ev_boundary(90, 0)] {
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
                    end_public_position: model_point(90, 0),
                    // takes on the cursor position that emptied the range
                    end_scan_position: new_coordinate,
                    exhaustion: RangeExhaustion::CursorBound {
                        kind: CursorKind::Boundary,
                    },
                    window_claim: Some(3),
                }
            );
        }
    }

    /// (ascending, after inside the window): Item admits strictly after its
    /// coordinate, Boundary from it; entry checkpoint and terminal untouched.
    #[test]
    fn after_cursor_tightens_ascending_lower_bound() {
        let options = after_options(Ordering::Ascending, ev_item(5, 1));
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

        let options = after_options(Ordering::Ascending, ev_boundary(5, 1));
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
        let options = after_options(Ordering::Ascending, ev_boundary(2, 0));
        assert_eq!(seed.clone().apply_cursor_bounds(&options), seed);
        let options = after_options(Ordering::Ascending, ev_item(2, 0));
        assert_eq!(seed.clone().apply_cursor_bounds(&options), seed);
    }

    /// Tightens the lower bound, and the terminal becomes the boundary just below the lowest
    /// admitted position, because the after cursor is where a descending scan stops: the slot
    /// after an Item cursor, which the scan excluded, and a Boundary cursor itself.
    #[test]
    fn descending_after_cursor_sets_terminal_and_tightens() {
        let options = after_options(Ordering::Descending, ev_item(5, 1));
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
                end_public_position: model_point(5, 2),
                end_scan_position: IntraTxCoordinate {
                    tx_seq: 5,
                    index: 2,
                },
                exhaustion: RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
                },
                ..resolved_intra_tx()
            }
        );

        let options = after_options(Ordering::Descending, ev_boundary(5, 1));
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
                end_public_position: model_point(5, 1),
                end_scan_position: IntraTxCoordinate {
                    tx_seq: 5,
                    index: 1,
                },
                exhaustion: RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
                },
                ..resolved_intra_tx()
            }
        );
    }

    /// When the after cursor empties the window, the record empties at the boundary just below
    /// what the cursor admits, at the public position there: the slot after an Item cursor, a
    /// Boundary cursor itself, both as Boundary.
    #[test]
    fn after_cursor_empties_descending_interval() {
        let seed = ResolvedScan {
            bounds: IntraTxScanBounds::from_range(
                IntraTxCoordinate::start_of_tx(3)..IntraTxCoordinate::start_of_tx(9),
            ),
            entry_checkpoint: 9,
            end_public_position: model_point(3, 0),
            end_scan_position: IntraTxCoordinate::start_of_tx(3),
            exhaustion: RangeExhaustion::CheckpointBound,
            window_claim: Some(3),
        };

        let emptied_at = |coordinate: IntraTxCoordinate| ResolvedScan {
            bounds: IntraTxScanBounds::empty_at(coordinate),
            entry_checkpoint: 9,
            end_public_position: model_point(coordinate.tx_seq, coordinate.index),
            end_scan_position: coordinate,
            exhaustion: RangeExhaustion::CursorBound {
                kind: CursorKind::Boundary,
            },
            window_claim: Some(3),
        };

        let options = after_options(Ordering::Descending, ev_item(100, 10));
        assert_eq!(
            seed.clone().apply_cursor_bounds(&options),
            emptied_at(IntraTxCoordinate {
                tx_seq: 100,
                index: 11,
            })
        );
        let options = after_options(Ordering::Descending, ev_boundary(100, 10));
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
        let options = before_options(Ordering::Descending, ev_item(5, 1));
        assert_eq!(seed.clone().apply_cursor_bounds(&options), expected);
        let options = before_options(Ordering::Descending, ev_boundary(5, 1));
        assert_eq!(seed.apply_cursor_bounds(&options), expected);
    }

    #[test]
    fn before_cursor_above_window_never_tightens() {
        for cursor in [ev_boundary(12, 0), ev_item(12, 0)] {
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
            end_public_position: model_point(5, 1),
            end_scan_position: IntraTxCoordinate {
                tx_seq: 5,
                index: 1,
            },
            exhaustion: RangeExhaustion::CursorBound {
                kind: CursorKind::Boundary,
            },
            window_claim: Some(8),
        };
        let options = before_options(Ordering::Ascending, ev_boundary(5, 1));
        assert_eq!(resolved_intra_tx().apply_cursor_bounds(&options), expected);
        let options = before_options(Ordering::Ascending, ev_item(5, 1));
        assert_eq!(resolved_intra_tx().apply_cursor_bounds(&options), expected);
    }

    /// When a before cursor empties an ascending interval, the bounds are emptied at the cursor,
    /// and end_scan_position and end_public_position are set to the cursor's.
    #[test]
    fn before_cursor_empties_ascending_interval() {
        let seed = ResolvedScan {
            bounds: IntraTxScanBounds::from_range(
                IntraTxCoordinate::start_of_tx(3)..IntraTxCoordinate::start_of_tx(10),
            ),
            entry_checkpoint: 2,
            ..resolved_intra_tx()
        };
        let expected = ResolvedScan {
            bounds: IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(3)),
            end_public_position: model_point(3, 0),
            end_scan_position: IntraTxCoordinate::start_of_tx(3),
            exhaustion: RangeExhaustion::CursorBound {
                kind: CursorKind::Boundary,
            },
            ..seed.clone()
        };
        let options = before_options(Ordering::Ascending, ev_boundary(3, 0));
        assert_eq!(seed.clone().apply_cursor_bounds(&options), expected);
        let options = before_options(Ordering::Ascending, ev_item(3, 0));
        assert_eq!(seed.apply_cursor_bounds(&options), expected);
    }

    /// (ascending, after and before both present): a `before` past the window's end does not stop
    /// the scan, so an `after` past the window ends the page with the window's reason where it
    /// resumes; if both empty the interval, the before wins at the min position.
    #[test]
    fn ascending_with_both_cursors_picks_the_terminal_owner() {
        // Checkpoints 1..4, transactions 5..20. The before cursor lies past the window's end, and
        // the after cursor past it empties the window.
        let options = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: Some(ev_item(25, 0)),
            before: Some(ev_boundary(30, 0)),
        };
        let resolved = resolve_event_request((Some(1), Some(4)), &options).unwrap();
        assert!(resolved.is_empty());
        assert_eq!(resolved.exhaustion, RangeExhaustion::CheckpointBound);
        assert_eq!(
            (resolved.end_public_position, resolved.end_scan_position),
            (
                model_point(25, 1),
                IntraTxCoordinate {
                    tx_seq: 25,
                    index: 1
                }
            )
        );

        // When both cursors would empty, `before` cursor attribution remains.
        let options = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Ascending,
            after: Some(ev_boundary(12, 0)),
            before: Some(ev_boundary(3, 0)),
        };
        assert_eq!(
            resolved_intra_tx().apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(3)),
                entry_checkpoint: 2,
                end_public_position: model_point(3, 0),
                end_scan_position: IntraTxCoordinate::start_of_tx(3),
                exhaustion: RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
                },
                window_claim: Some(8),
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
            after: Some(ev_boundary(12, 0)),
            before: Some(ev_boundary(15, 0)),
        };
        assert_eq!(
            resolved_intra_tx().apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(12)),
                entry_checkpoint: 2,
                end_public_position: model_point(12, 0),
                end_scan_position: IntraTxCoordinate::start_of_tx(12),
                exhaustion: RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
                },
                window_claim: Some(8),
            }
        );

        // After empties, before also files: before wins at the min position.
        let options = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Descending,
            after: Some(ev_boundary(12, 0)),
            before: Some(ev_item(5, 2)),
        };
        assert_eq!(
            resolved_intra_tx().apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: IntraTxScanBounds::empty_at(IntraTxCoordinate {
                    tx_seq: 5,
                    index: 2,
                }),
                entry_checkpoint: 2,
                end_public_position: model_point(5, 2),
                end_scan_position: IntraTxCoordinate {
                    tx_seq: 5,
                    index: 2,
                },
                exhaustion: RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
                },
                window_claim: Some(8),
            }
        );

        // After tightens without emptying, before empties: the before-claim
        // overrides the after-cursor's eagerly written terminal.
        let options = QueryOptions {
            limit_items: 100,
            ordering: Ordering::Descending,
            after: Some(ev_item(5, 0)),
            before: Some(ev_item(5, 0)),
        };
        assert_eq!(
            resolved_intra_tx().apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(5)),
                entry_checkpoint: 2,
                end_public_position: model_point(5, 0),
                end_scan_position: IntraTxCoordinate::start_of_tx(5),
                exhaustion: RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
                },
                window_claim: Some(8),
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
        let options = after_options(Ordering::Descending, ev_boundary(2, 0));
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
        let options = before_options(Ordering::Ascending, ev_boundary(10, 0));
        assert_eq!(
            resolved_intra_tx().apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: IntraTxScanBounds::from_range(
                    IntraTxCoordinate::start_of_tx(0)..IntraTxCoordinate::start_of_tx(10)
                ),
                entry_checkpoint: 2,
                end_public_position: model_point(10, 0),
                end_scan_position: IntraTxCoordinate::start_of_tx(10),
                exhaustion: RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
                },
                window_claim: Some(8),
            }
        );

        let options = after_options(Ordering::Descending, ev_boundary(0, 0));
        assert_eq!(
            resolved_intra_tx().apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: IntraTxScanBounds::from_range(
                    IntraTxCoordinate::start_of_tx(0)..IntraTxCoordinate::start_of_tx(10)
                ),
                entry_checkpoint: 2,
                end_public_position: LedgerPoint::GENESIS,
                end_scan_position: IntraTxCoordinate::start_of_tx(0),
                exhaustion: RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
                },
                window_claim: Some(8),
            }
        );
    }

    // An after-Item cursor makes the exclusive lower bound one past the cursor, while an
    // after-Boundary cursor makes the lower bound equal to the cursor.
    #[test]
    fn tx_after_cursor_tightens_ascending_lower_bound() {
        // Item at n admits strictly-after symbolically; the successor lives
        // at the store edge, so both forms fetch from n + 1.
        let options = after_options(Ordering::Ascending, tx_item(12));
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

        let options = after_options(Ordering::Ascending, tx_boundary(12));
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
        let options = after_options(Ordering::Descending, tx_item(12));
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: ScanBounds {
                    lo: Bound::Excluded(12),
                    hi: Bound::Excluded(20),
                },
                entry_checkpoint: 0,
                end_public_position: after_transaction_point(13),
                end_scan_position: 13,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
                },
                window_claim: Some(19),
            }
        );
        let options = after_options(Ordering::Descending, tx_boundary(13));
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: ScanBounds::from_range(13..20),
                entry_checkpoint: 0,
                end_public_position: after_transaction_point(13),
                end_scan_position: 13,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
                },
                window_claim: Some(19),
            }
        );
    }

    /// When the after cursor empties the window, the record empties at the boundary just below
    /// what the cursor admits, at the public position there, as Boundary for either cursor kind.
    #[test]
    fn tx_after_cursor_empties_descending_interval() {
        // Item at 24 and Boundary at 25 scan the same exclusive edge, and both empty the window
        // at the boundary at 25, after transaction 24.
        let options = after_options(Ordering::Descending, tx_frontier(25));
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: ScanBounds::from_range(25..25),
                entry_checkpoint: 0,
                end_public_position: after_transaction_point(25),
                end_scan_position: 25,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
                },
                window_claim: Some(19),
            }
        );
        let options = after_options(Ordering::Descending, tx_item(24));
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: ScanBounds::from_range(25..25),
                entry_checkpoint: 0,
                end_public_position: after_transaction_point(25),
                end_scan_position: 25,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
                },
                window_claim: Some(19),
            }
        );
    }

    /// after-Item at n and after-Boundary at n + 1 admit the same interval, and a page they leave
    /// empty past the window ends on the same terminal: the boundary at n + 1, after item n.
    #[test]
    fn tx_after_item_and_successor_boundary_admit_the_same_interval() {
        let item = after_options(Ordering::Ascending, tx_item(24));
        let successor = after_options(Ordering::Ascending, tx_frontier(25));

        // Checkpoints 1..6, transactions 5..30: both admit 25..30.
        let wide = (Some(1), Some(6));
        assert_eq!(resolve_tx_request(wide, &item).unwrap().range(), 25..30);
        assert_eq!(
            resolve_tx_request(wide, &successor).unwrap().range(),
            25..30
        );

        // Checkpoints 1..4, transactions 5..20: both leave the window empty and end at the
        // boundary at 25 with the window's reason.
        let narrow = (Some(1), Some(4));
        let resolved = resolve_tx_request(narrow, &item).unwrap();
        assert_eq!(resolved, resolve_tx_request(narrow, &successor).unwrap());
        assert!(resolved.is_empty());
        assert_eq!(
            (
                resolved.exhaustion,
                resolved.end_public_position,
                resolved.end_scan_position
            ),
            (
                RangeExhaustion::CheckpointBound,
                after_transaction_point(25),
                25
            )
        );
    }

    /// (ascending, after-Boundary at u64::MAX): nothing can lie at MAX; empties at
    /// (cursor's public position, MAX), Boundary.
    #[test]
    fn tx_after_boundary_at_max_empties_interval() {
        let options = after_options(Ordering::Ascending, tx_boundary(u64::MAX));
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            empty_resolved_range(
                model_point(u64::MAX, 0),
                u64::MAX,
                RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
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
            after: Some(tx_item(u64::MAX)),
            before: Some(tx_boundary(15)),
        };
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: ScanBounds::from_range(15..15),
                entry_checkpoint: 0,
                end_public_position: model_point(15, 0),
                end_scan_position: 15,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
                },
                window_claim: Some(19),
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
            after: Some(tx_item(u64::MAX)),
            before: Some(tx_boundary(15)),
        };
        assert_eq!(
            seed.apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: ScanBounds::from_range(15..15),
                entry_checkpoint: 7,
                end_public_position: model_point(15, 0),
                end_scan_position: 15,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
                },
                window_claim: Some(19),
            }
        );
    }

    /// For some ResolvedScan<u64>, if its bounds are Excluded(A) and Excluded(A + 1), its bounds
    /// report non-empty, but its range is empty.
    /// `test_list_transactions_after_last_item_ends_at_window_bound` checks that the result set is
    /// empty.
    #[test]
    fn tx_after_item_at_window_edge_defers_to_drain() {
        let options = after_options(Ordering::Ascending, tx_item(19));
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

    /// Exclusive after on descending bumps the bounds, and sets the end_public_position,
    /// end_scan_position (the boundary just above the item), and exhaustion from the after
    /// cursor.
    #[test]
    fn tx_after_item_at_window_edge_descending_keeps_stamped_terminal() {
        let options = after_options(Ordering::Descending, tx_item(19));
        let initial_range = resolved_range(10..20);
        let resolved = initial_range.clone().apply_cursor_bounds(&options);
        assert_eq!(
            resolved,
            ResolvedScan {
                bounds: ScanBounds {
                    lo: Bound::Excluded(19),
                    hi: Bound::Excluded(20),
                },
                end_public_position: after_transaction_point(20),
                end_scan_position: 20,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: CursorKind::Boundary,
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
        let options = after_options(Ordering::Descending, tx_item(9));
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            resolved_range(10..20)
        );
    }

    #[test]
    fn item_cursor_can_be_used_as_after_or_before() {
        let token = tx_item(11).encode();

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
        resolve_checkpoint_request((None, end_checkpoint), options)
    }

    fn resolve_transactions(options: &QueryOptions) -> Result<ResolvedScan<u64>, RpcError> {
        resolve_tx_request((None, None), options)
    }

    fn resolve_events(options: &QueryOptions) -> Result<ResolvedScan<IntraTxCoordinate>, RpcError> {
        resolve_event_request((None, None), options)
    }

    fn is_out_of_range<P>(resolved: Result<ResolvedScan<P>, RpcError>) -> bool {
        resolved.is_err_and(|e| e.into_status_proto().code == tonic::Code::OutOfRange as i32)
    }

    /// Where a resolved scan's page ends: `(the end's public position, terminal position, end
    /// reason, natural end claim)`.
    fn page_end<P: ScanPosition>(
        resolved: &ResolvedScan<P>,
        options: &QueryOptions,
    ) -> (LedgerPoint, P, RangeExhaustion, Option<u64>)
    where
        CursorToken: ScanCursor<P>,
    {
        (
            resolved.end_public_position,
            resolved.end_scan_position,
            resolved.exhaustion,
            resolved.natural_end_claim(options),
        )
    }

    /// An ascending `after` cursor at the tip is caught up: its page ends at the tip and claims
    /// the checkpoint below it. A window the request ends at the tip does not reach it, so a cursor
    /// past the tip ends that window's page where it resumes, with `CheckpointBound` and the
    /// window's claim.
    #[test]
    fn ascending_after_cursor_at_the_tip_edge() {
        let ascending = |cursor| after_options(Ordering::Ascending, cursor);

        let tip_boundary = CursorToken::boundary(Position::Checkpoints { checkpoint: TIP });
        for cursor in [cp_item(TIP - 1), tip_boundary] {
            let options = ascending(cursor);
            let resolved = resolve_checkpoints(None, &options).unwrap();
            assert!(resolved.is_empty());
            assert_eq!(
                page_end(&resolved, &options),
                (
                    LedgerPoint::checkpoint_start(TIP),
                    TIP,
                    RangeExhaustion::LedgerTip,
                    Some(TIP - 1)
                )
            );
        }

        let options = ascending(cp_item(TIP + 5));
        let resolved = resolve_checkpoints(Some(TIP), &options).unwrap();
        assert_eq!(
            page_end(&resolved, &options),
            (
                LedgerPoint::checkpoint_start(TIP + 6),
                TIP + 6,
                RangeExhaustion::CheckpointBound,
                Some(TIP - 1)
            )
        );
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
                kind: CursorKind::Boundary
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

    // Cursors read in either direction, on the ledger of the cursor-past-tip tests (checkpoint
    // `c` holds transactions `5c..5c + 5`, tip at checkpoint 20), where every transaction emits
    // events 0 and 1.

    /// What issued a cursor, which decides the public position it carries.
    #[derive(Clone, Copy, Debug)]
    enum Issuer {
        /// An item: the model point of the position.
        Item,
        /// A range end, a subscription boundary, a restated cursor bound or a descending scan
        /// frontier: the model point of the position.
        ExactBoundary,
        /// An ascending scan frontier: after the position before it (see
        /// [`after_transaction_point`]).
        AscendingFrontier,
    }

    const ISSUERS: [Issuer; 3] = [
        Issuer::Item,
        Issuer::ExactBoundary,
        Issuer::AscendingFrontier,
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

    /// The model point just before the event slot `(tx, index)`, or before transaction `tx` at
    /// index 0.
    fn model_point(tx: u64, index: u32) -> LedgerPoint {
        LedgerPoint::event(containing_checkpoint(tx), tx % 5, index)
    }

    /// The point an ascending frontier at the start of `tx` carries: placed after the transaction
    /// before it (see [`RowPlacement`]), so at a checkpoint's start it names the end of the
    /// checkpoint before.
    ///
    /// [`RowPlacement`]: crate::ledger_history::row_placement::RowPlacement
    fn after_transaction_point(tx: u64) -> LedgerPoint {
        tx.checked_sub(1).map_or(LedgerPoint::GENESIS, |previous| {
            LedgerPoint::transaction(containing_checkpoint(previous), previous % 5 + 1)
        })
    }

    /// The transaction cursor `issuer` mints at `tx`, where it can mint one, carrying the public
    /// position that issuer places it at.
    fn issued_tx_cursor(issuer: Issuer, tx: u64) -> Option<CursorToken> {
        let indexed = tx < first_tx(TIP);
        match issuer {
            Issuer::Item => indexed.then(|| tx_item(tx)),
            Issuer::ExactBoundary => Some(tx_boundary(tx)),
            Issuer::AscendingFrontier => Some(tx_frontier(tx)),
        }
    }

    /// The event cursor `issuer` mints at `(tx, index)`, where it can mint one, carrying the
    /// public position that issuer places it at.
    fn issued_event_cursor(issuer: Issuer, tx: u64, index: u32) -> Option<CursorToken> {
        let indexed = tx < first_tx(TIP);
        match issuer {
            Issuer::Item => (indexed && index < 2).then(|| ev_item(tx, index)),
            Issuer::ExactBoundary => (indexed || index == 0).then(|| ev_boundary(tx, index)),
            Issuer::AscendingFrontier => (indexed || index == 0).then(|| {
                let point = if index == 0 {
                    after_transaction_point(tx)
                } else {
                    model_point(tx, index)
                };
                CursorToken::boundary(ev_position(tx, index, point))
            }),
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

    /// Transaction cursors a server further along than the model mints past its tip: in the first
    /// unindexed checkpoint and the one after it.
    fn past_tip_tx_cursors() -> Vec<CursorToken> {
        let tip_tx = first_tx(TIP);
        [
            tx_item(tip_tx),
            tx_boundary(tip_tx + 1),
            tx_item(tip_tx + 4),
            tx_boundary(tip_tx + 5),
            tx_item(tip_tx + 7),
        ]
        .to_vec()
    }

    /// Event cursors a server further along than the model mints past its tip.
    fn past_tip_event_cursors() -> Vec<CursorToken> {
        let tip_tx = first_tx(TIP);
        [
            ev_item(tip_tx, 0),
            ev_boundary(tip_tx, 1),
            ev_item(tip_tx, 1),
            ev_boundary(tip_tx + 1, 0),
            ev_item(tip_tx + 5, 2),
        ]
        .to_vec()
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
        requests_in(
            WINDOWS.into_iter().chain(PAST_TIP_WINDOWS),
            cursors,
            pair_cursors,
        )
    }

    /// [`requests`] in `windows`.
    fn requests_in(
        windows: impl IntoIterator<Item = Window>,
        cursors: &[CursorToken],
        pair_cursors: &[CursorToken],
    ) -> Vec<(Window, QueryOptions)> {
        let mut requests = Vec::new();
        for window in windows {
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

    /// A request's scan and the window it was resolved from, which places its terminal in
    /// ledger coordinates. The model's checkpoint windows have checkpoint edges, so their ledger
    /// coordinates are a checkpoint scan's whatever the lane.
    fn resolve_in_window<P: ScanPosition>(
        window: Window,
        options: &QueryOptions,
        lane_range: impl FnOnce(Range<u64>) -> Range<P>,
    ) -> Result<(ResolvedScan<P>, ScanWindow<P>), RpcError>
    where
        CursorToken: ScanCursor<P>,
    {
        let cp_range = ResolvedCheckpointRange::from_request(window.0, window.1, TIP, options)?;
        let ledger = LedgerWindow::checkpoints(&cp_range.range, window.0, window.1);
        let scan_window = ScanWindow::whole_checkpoints(lane_range(cp_range.range.clone()), ledger);
        let resolved = ResolvedScan::resolve(cp_range, scan_window.clone(), options)?;
        Ok((resolved, scan_window))
    }

    fn resolve_tx_in_window(
        window: Window,
        options: &QueryOptions,
    ) -> Result<(ResolvedScan<u64>, ScanWindow<u64>), RpcError> {
        resolve_in_window(window, options, |range| {
            first_tx(range.start)..first_tx(range.end)
        })
    }

    fn resolve_event_in_window(
        window: Window,
        options: &QueryOptions,
    ) -> Result<
        (
            ResolvedScan<IntraTxCoordinate>,
            ScanWindow<IntraTxCoordinate>,
        ),
        RpcError,
    > {
        resolve_in_window(window, options, |range| {
            tx_window(first_tx(range.start)..first_tx(range.end))
        })
    }

    fn resolve_checkpoint_in_window(
        window: Window,
        options: &QueryOptions,
    ) -> Result<(ResolvedScan<u64>, ScanWindow<u64>), RpcError> {
        resolve_in_window(window, options, std::convert::identity)
    }

    fn resolve_tx_request(
        window: Window,
        options: &QueryOptions,
    ) -> Result<ResolvedScan<u64>, RpcError> {
        resolve_tx_in_window(window, options).map(|(resolved, _)| resolved)
    }

    fn resolve_event_request(
        window: Window,
        options: &QueryOptions,
    ) -> Result<ResolvedScan<IntraTxCoordinate>, RpcError> {
        resolve_event_in_window(window, options).map(|(resolved, _)| resolved)
    }

    fn resolve_checkpoint_request(
        window: Window,
        options: &QueryOptions,
    ) -> Result<ResolvedScan<u64>, RpcError> {
        resolve_checkpoint_in_window(window, options).map(|(resolved, _)| resolved)
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
                CursorKind::Item => key > cursor_key,
                CursorKind::Boundary => key >= cursor_key,
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
    /// - it errs exactly when it is descending and either its window reaches past the tip, or its
    ///   window reaches the tip and a cursor names a position past the tip, and then with
    ///   `OutOfRange`;
    /// - in a window that reaches the tip, empty or not, an ascending request whose `after` cursor
    ///   resumes past the tip ends an empty page where that cursor resumes, at the public position
    ///   there, with `LedgerTip` and no checkpoint claim, or at a `before` cursor below that
    ///   position, at `before`'s public position, with `CursorBound` (at `after`'s, where `before`
    ///   is at that position);
    /// - otherwise, in a non-empty window, its entry checkpoint is the window's first checkpoint
    ///   in scan direction, whatever its cursors' public positions;
    /// - otherwise, in a non-empty window, it ends with the window's reason and claims the
    ///   window's last checkpoint in scan direction exactly when no far cursor stops the scan at
    ///   or before the window's edge (ascending, a `before` at or below the window's end;
    ///   descending, an `after` at or above the window's start), and otherwise with
    ///   `CursorBound`. It ends at the window's edge, or past it, where a starting cursor beyond
    ///   the edge resumes: the first position an `after` cursor admits, or a `before` cursor's
    ///   position.
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
        let tip_key = (first_tx(TIP), 0);
        // A boundary exactly at the tip is where a caught-up scan resumes; an item there, or any
        // position beyond it, has not been indexed.
        let past_tip = |cursor: &CursorToken| {
            let cursor_key = scan_order(&cursor.position);
            match cursor.kind {
                CursorKind::Item => cursor_key >= tip_key,
                CursorKind::Boundary => cursor_key > tip_key,
            }
        };
        let window_reaches_tip = window.1.is_none_or(|end| end > TIP);
        let descending_window_past_tip = options.ordering == Ordering::Descending
            && match window.1 {
                Some(end) => end > TIP,
                None => window.0.unwrap_or(0) >= TIP,
            };
        let descending_cursor_past_tip = options.ordering == Ordering::Descending
            && window_reaches_tip
            && [options.before.as_ref(), options.after.as_ref()]
                .into_iter()
                .flatten()
                .any(past_tip);
        let out_of_range = descending_window_past_tip || descending_cursor_past_tip;
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

        let cursor_key = |cursor: &CursorToken| key(cursor.coordinate());
        let first_admitted_key = |cursor: &CursorToken| {
            let position: P = cursor.coordinate();
            match cursor.kind {
                CursorKind::Item => position.successor().map(&key),
                CursorKind::Boundary => Some(key(position)),
            }
        };

        if options.is_ascending()
            && window_reaches_tip
            && let Some(after) = options.after.as_ref().filter(|after| past_tip(after))
        {
            let resume = first_admitted_key(after).expect("model cursors have a successor");
            let resume_point = match after.kind {
                CursorKind::Item => after.position.item_resume(ScanDirection::Ascending),
                CursorKind::Boundary => after.position.public_position(),
            };
            // The terminal cursor carries `after`'s point where both cursors name its position.
            let (reason, end, end_cursor_public_position) = match options.before.as_ref() {
                Some(before) if cursor_key(before) <= resume => (
                    RangeExhaustion::CursorBound {
                        kind: CursorKind::Boundary,
                    },
                    cursor_key(before),
                    if cursor_key(before) == resume {
                        resume_point
                    } else {
                        before.position.public_position()
                    },
                ),
                _ => (RangeExhaustion::LedgerTip, resume, resume_point),
            };
            assert!(resolved.is_empty(), "empty page: {context}");
            assert_eq!(resolved.exhaustion, reason, "end reason: {context}");
            assert_eq!(
                key(resolved.end_scan_position),
                end,
                "end scan position: {context}"
            );
            assert_eq!(
                resolved.end_cursor_public_position(options),
                end_cursor_public_position,
                "end cursor public position: {context}"
            );
            assert_eq!(
                resolved.natural_end_claim(options),
                None,
                "checkpoint claim: {context}"
            );
            return;
        }

        let window_start = window.0.unwrap_or(0);
        let window_end = window.1.unwrap_or(TIP).min(TIP);
        if window_start >= window_end {
            return;
        }
        let window_lo = (first_tx(window_start), 0);
        let window_hi = (first_tx(window_end), 0);
        // Where the page ends with the window's reason, or `None` when a far cursor stops the
        // scan at or before the window's edge.
        let (entry, claim, reason, window_reason_end) = match options.ordering {
            Ordering::Ascending => (
                window_start,
                window_end - 1,
                if window_reaches_tip {
                    RangeExhaustion::LedgerTip
                } else {
                    RangeExhaustion::CheckpointBound
                },
                if options
                    .before
                    .as_ref()
                    .is_some_and(|before| cursor_key(before) <= window_hi)
                {
                    None
                } else {
                    match options.after.as_ref() {
                        None => Some(window_hi),
                        Some(after) => first_admitted_key(after).map(|first| first.max(window_hi)),
                    }
                },
            ),
            Ordering::Descending => (
                window_end - 1,
                window_start,
                RangeExhaustion::CheckpointBound,
                if options
                    .after
                    .as_ref()
                    .is_some_and(|after| cursor_key(after) >= window_lo)
                {
                    None
                } else {
                    Some(
                        options
                            .before
                            .as_ref()
                            .map_or(window_lo, |before| cursor_key(before).min(window_lo)),
                    )
                },
            ),
        };
        assert_eq!(
            resolved.entry_checkpoint, entry,
            "entry checkpoint: {context}"
        );
        match window_reason_end {
            Some(end) => {
                assert_eq!(resolved.exhaustion, reason, "end reason: {context}");
                assert_eq!(
                    key(resolved.end_scan_position),
                    end,
                    "end scan position: {context}"
                );
                assert_eq!(
                    resolved.natural_end_claim(options),
                    Some(claim),
                    "checkpoint claim: {context}"
                );
            }
            None => assert!(
                matches!(resolved.exhaustion, RangeExhaustion::CursorBound { .. }),
                "end reason: {context}: {:?}",
                resolved.exhaustion
            ),
        }
    }

    #[test]
    fn transaction_cursors_admit_their_positions_in_either_direction() {
        let cursors = [every_tx_cursor(0..=first_tx(TIP)), past_tip_tx_cursors()].concat();
        let pair_cursors = [every_tx_cursor(edge_txs()), past_tip_tx_cursors()].concat();
        let positions = ledger_txs();
        for (window, options) in requests(&cursors, &pair_cursors) {
            check_request(window, &options, resolve_tx_request, &positions, |tx| {
                (tx, 0)
            });
        }
    }

    #[test]
    fn event_cursors_admit_their_positions_in_either_direction() {
        let cursors = [
            every_event_cursor(0..=first_tx(TIP)),
            past_tip_event_cursors(),
        ]
        .concat();
        let pair_cursors = [
            every_event_cursor(event_edge_txs()),
            past_tip_event_cursors(),
        ]
        .concat();
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
        options: &QueryOptions,
        position_at: impl Fn(P, LedgerPoint) -> Position,
    ) -> CursorToken
    where
        CursorToken: ScanCursor<P>,
    {
        let kind = match resolved.exhaustion {
            RangeExhaustion::CursorBound { kind } => kind,
            RangeExhaustion::LedgerTip
            | RangeExhaustion::CheckpointBound
            | RangeExhaustion::ServingFloor { .. } => CursorKind::Boundary,
        };
        CursorToken {
            kind,
            position: position_at(
                resolved.end_scan_position,
                resolved.end_cursor_public_position(options),
            ),
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
        position_at: impl Fn(P, LedgerPoint) -> Position,
    ) where
        CursorToken: ScanCursor<P>,
    {
        let context = format!("window {window:?}, {options:?}");
        let resolved = resolve(window, options).unwrap();
        let closing = terminal_cursor(&resolved, options, position_at);
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
                tx_position,
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
                |position, point| ev_position(position.tx_seq, position.index, point),
            );
        }
    }

    /// A page its starting cursor leaves empty in a non-empty window, and that ends with the
    /// window's reason, closes on a cursor that resumes without the window exactly where the
    /// starting cursor does: it neither re-serves an `after` Item cursor's item nor falls back to
    /// the window's edge. (In a zero-width window the page closes at the window's edge.)
    fn check_empty_page_closing_cursor<P: ScanPosition + std::fmt::Debug>(
        window: Window,
        options: &QueryOptions,
        resolve: impl Fn(Window, &QueryOptions) -> Result<ResolvedScan<P>, RpcError>,
        positions: &[P],
        position_at: impl Fn(P, LedgerPoint) -> Position,
    ) where
        CursorToken: ScanCursor<P>,
    {
        let Ok(resolved) = resolve(window, options) else {
            return;
        };
        if !resolved.is_empty()
            || matches!(resolved.exhaustion, RangeExhaustion::CursorBound { .. })
        {
            return;
        }
        let context = format!("window {window:?}, {options:?}");
        let closing = terminal_cursor(&resolved, options, position_at);
        let resume = if options.is_ascending() {
            options_with(options.ordering, Some(closing.clone()), None)
        } else {
            options_with(options.ordering, None, Some(closing.clone()))
        };
        let admitted = |options: &QueryOptions| {
            resolve((None, None), options).map(|resolved| {
                positions
                    .iter()
                    .copied()
                    .filter(|position| resolved.bounds.contains(*position))
                    .collect::<Vec<P>>()
            })
        };
        match (admitted(options), admitted(&resume)) {
            (Ok(starting), Ok(resumed)) => {
                assert_eq!(resumed, starting, "{closing:?} resumes: {context}")
            }
            (Err(_), Err(_)) => {}
            (starting, resumed) => panic!(
                "{closing:?} resumes as {:?}, the starting cursor as {:?}: {context}",
                resumed.map_err(status),
                starting.map_err(status),
            ),
        }
    }

    /// Requests that start from each of `cursors`: `after` ascending, `before` descending.
    fn starting_cursor_requests(cursors: &[CursorToken]) -> Vec<(Window, QueryOptions)> {
        let mut requests = Vec::new();
        for window in WINDOWS {
            for cursor in cursors {
                requests.push((
                    window,
                    options_with(Ordering::Ascending, Some(cursor.clone()), None),
                ));
                requests.push((
                    window,
                    options_with(Ordering::Descending, None, Some(cursor.clone())),
                ));
            }
        }
        requests
    }

    #[test]
    fn transaction_empty_pages_close_where_their_starting_cursor_resumes() {
        let positions = ledger_txs();
        for (window, options) in starting_cursor_requests(&every_tx_cursor(0..=first_tx(TIP))) {
            check_empty_page_closing_cursor(
                window,
                &options,
                resolve_tx_request,
                &positions,
                tx_position,
            );
        }
    }

    #[test]
    fn event_empty_pages_close_where_their_starting_cursor_resumes() {
        let positions = ledger_event_slots();
        for (window, options) in starting_cursor_requests(&every_event_cursor(0..=first_tx(TIP))) {
            check_empty_page_closing_cursor(
                window,
                &options,
                resolve_event_request,
                &positions,
                |position, point| ev_position(position.tx_seq, position.index, point),
            );
        }
    }

    #[test]
    fn checkpoint_empty_pages_close_where_their_starting_cursor_resumes() {
        let positions: Vec<u64> = (0..TIP).collect();
        for (window, options) in starting_cursor_requests(&every_checkpoint_cursor()) {
            check_empty_page_closing_cursor(
                window,
                &options,
                resolve_checkpoint_request,
                &positions,
                |checkpoint, _| Position::Checkpoints { checkpoint },
            );
        }
    }

    /// Regression: a descending scan bounded by `after: Item(X)` excludes X, so its closing
    /// cursor is the boundary at the position after X, at the public position after the item.
    /// Sent back as `before`, it resumes with X.
    #[test]
    fn descending_after_item_closes_at_the_next_position() {
        // Inside a checkpoint: transaction 12 of checkpoint 2 closes at the boundary at 13.
        let resolved =
            resolve_transactions(&after_options(Ordering::Descending, tx_item(12))).unwrap();
        assert_eq!(resolved.range(), 13..100);
        assert_eq!(
            (resolved.end_public_position, resolved.end_scan_position),
            (after_transaction_point(13), 13)
        );
        let resumed =
            resolve_transactions(&before_options(Ordering::Descending, tx_frontier(13))).unwrap();
        assert_eq!(resumed.range(), 0..13);

        // Transaction 14 is the last of checkpoint 2: the next position starts checkpoint 3,
        // and the closing cursor's public position is the end of checkpoint 2. The boundary's
        // position bounds the resumed scan, so it still admits transaction 14.
        let resolved =
            resolve_transactions(&after_options(Ordering::Descending, tx_item(14))).unwrap();
        assert_eq!(resolved.range(), 15..100);
        assert_eq!(
            (resolved.end_public_position, resolved.end_scan_position),
            (after_transaction_point(15), 15)
        );
        let resumed =
            resolve_transactions(&before_options(Ordering::Descending, tx_frontier(15))).unwrap();
        assert_eq!(resumed.range(), 0..15);
        assert!(resumed.bounds.contains(14));

        let resolved =
            resolve_events(&after_options(Ordering::Descending, ev_item(14, 1))).unwrap();
        assert_eq!(
            resolved.end_scan_position,
            IntraTxCoordinate {
                tx_seq: 14,
                index: 2
            }
        );
        let resolved =
            resolve_events(&after_options(Ordering::Descending, ev_item(14, u32::MAX))).unwrap();
        assert_eq!(
            resolved.end_scan_position,
            IntraTxCoordinate::start_of_tx(15)
        );

        // No position follows the last one, so the closing cursor restates the item.
        let resolved = resolved_range(10..20)
            .apply_cursor_bounds(&after_options(Ordering::Descending, tx_item(u64::MAX)));
        assert_eq!(resolved.end_scan_position, u64::MAX);
        assert_eq!(
            resolved.exhaustion,
            RangeExhaustion::CursorBound {
                kind: CursorKind::Item
            }
        );
    }

    /// An Item and a Boundary cursor at every checkpoint up to two past the tip.
    fn every_checkpoint_cursor() -> Vec<CursorToken> {
        (0..=TIP + 2)
            .flat_map(|checkpoint| {
                [
                    cp_item(checkpoint),
                    CursorToken::boundary(Position::Checkpoints { checkpoint }),
                ]
            })
            .collect()
    }

    #[test]
    fn checkpoint_cursors_admit_their_positions_in_either_direction() {
        let cursors = every_checkpoint_cursor();
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

    /// Every request the model checks, plus each ordering with no cursor, in the model's windows
    /// and in a window that holds no checkpoint.
    fn terminal_requests(
        cursors: &[CursorToken],
        pair_cursors: &[CursorToken],
    ) -> Vec<(Window, QueryOptions)> {
        let windows = WINDOWS
            .into_iter()
            .chain(PAST_TIP_WINDOWS)
            .chain([(Some(6), Some(6))]);
        let mut requests = requests_in(windows.clone(), cursors, pair_cursors);
        for window in windows {
            for ordering in ORDERINGS {
                requests.push((window, options_with(ordering, None, None)));
            }
        }
        requests
    }

    /// One request on the model ledger: its terminal frame's `cursor_position` splits the model's
    /// positions exactly where the page leaves them. Ascending, a position lies before it exactly
    /// when the request admits it (every admitted position was delivered by the time the page
    /// ends) or the request's start excludes it (the point never lies before the start: the
    /// window's start, or where its `after` cursor resumes). Descending, a position lies at or
    /// after it exactly when the request admits it or its end excludes it (the window's end, or
    /// its `before` cursor). A window that holds no checkpoint ignores the cursors, and is
    /// bounded by its fields alone.
    ///
    /// Every terminal is placed without a read, by a window edge or the public position of a
    /// request cursor, and its cursor carries the public position of its own scan position.
    fn check_terminal_ledger_position<P: ScanPosition + std::fmt::Debug>(
        window: Window,
        options: &QueryOptions,
        resolve: impl Fn(Window, &QueryOptions) -> Result<(ResolvedScan<P>, ScanWindow<P>), RpcError>,
        positions: &[P],
        key: impl Fn(P) -> (u64, u32),
        point: impl Fn(P) -> LedgerPoint,
    ) where
        CursorToken: ScanCursor<P>,
    {
        let context = format!("window {window:?}, {options:?}");
        // `check_request` covers the requests that fail.
        let Ok((resolved, scan_window)) = resolve(window, options) else {
            return;
        };
        let cursor_position = resolved.end_resume_position(&scan_window, options);
        let cursor_point = resolved.end_cursor_public_position(options);

        // The terminal cursor's public position names its own scan position.
        let end_key = key(resolved.end_scan_position);
        for &position in positions {
            assert_eq!(
                point(position) < cursor_point,
                key(position) < end_key,
                "{position:?} against the cursor's {cursor_point:?}: {context}"
            );
        }
        let cursor_key = |cursor: &CursorToken| key(cursor.coordinate());
        let window_start = window.0.unwrap_or(0);
        let window_end = window.1.unwrap_or(TIP).min(TIP);
        let holds_no_checkpoint = window_start >= window_end;
        let start_excludes = |position: P| {
            key(position) < (first_tx(window_start), 0)
                || (!holds_no_checkpoint
                    && options
                        .after
                        .as_ref()
                        .is_some_and(|after| match after.kind {
                            CursorKind::Item => key(position) <= cursor_key(after),
                            CursorKind::Boundary => key(position) < cursor_key(after),
                        }))
        };
        let end_excludes = |position: P| {
            key(position) >= (first_tx(window_end), 0)
                || (!holds_no_checkpoint
                    && options
                        .before
                        .as_ref()
                        .is_some_and(|before| key(position) >= cursor_key(before)))
        };
        for &position in positions {
            let admitted = admits(window, options, key(position));
            let before_point = point(position) < cursor_position;
            if options.is_ascending() {
                assert_eq!(
                    before_point,
                    admitted || start_excludes(position),
                    "{position:?} against {cursor_position:?}: {context}"
                );
            } else {
                assert_eq!(
                    !before_point,
                    admitted || end_excludes(position),
                    "{position:?} against {cursor_position:?}: {context}"
                );
            }
        }
    }

    #[test]
    fn transaction_terminal_ledger_position_splits_the_ledger_where_the_page_ends() {
        let cursors = [every_tx_cursor(0..=first_tx(TIP)), past_tip_tx_cursors()].concat();
        let pair_cursors = [every_tx_cursor(edge_txs()), past_tip_tx_cursors()].concat();
        let positions = ledger_txs();
        for (window, options) in terminal_requests(&cursors, &pair_cursors) {
            check_terminal_ledger_position(
                window,
                &options,
                resolve_tx_in_window,
                &positions,
                |tx| (tx, 0),
                |tx| model_point(tx, 0),
            );
        }
    }

    #[test]
    fn event_terminal_ledger_position_splits_the_ledger_where_the_page_ends() {
        let cursors = [
            every_event_cursor(0..=first_tx(TIP)),
            past_tip_event_cursors(),
        ]
        .concat();
        let pair_cursors = [
            every_event_cursor(event_edge_txs()),
            past_tip_event_cursors(),
        ]
        .concat();
        let positions = ledger_event_slots();
        for (window, options) in terminal_requests(&cursors, &pair_cursors) {
            check_terminal_ledger_position(
                window,
                &options,
                resolve_event_in_window,
                &positions,
                |position| (position.tx_seq, position.index),
                |position| model_point(position.tx_seq, position.index),
            );
        }
    }

    /// A checkpoint scan's terminal is its own point, including where an `after` cursor past the
    /// tip places it: a checkpoint cursor's checkpoint is exact.
    #[test]
    fn checkpoint_terminal_ledger_position_splits_the_ledger_where_the_page_ends() {
        let cursors = every_checkpoint_cursor();
        let positions: Vec<u64> = (0..TIP).collect();
        for (window, options) in terminal_requests(&cursors, &cursors) {
            check_terminal_ledger_position(
                window,
                &options,
                resolve_checkpoint_in_window,
                &positions,
                |checkpoint| (first_tx(checkpoint), 0),
                LedgerPoint::checkpoint_start,
            );
        }
    }

    /// Past the tip, an ascending terminal echoes the request's start fields, whatever its
    /// cursors. With start fields inside the tip, an `after` cursor past it places the terminal
    /// where it resumes by the public position it carries, and its echo keeps the cursor's bytes.
    #[test]
    fn ascending_terminal_past_the_tip_echoes_the_requested_start() {
        let past_tip_point = LedgerPoint::checkpoint_start(TIP + 1);
        let past_tip_after = tx_boundary(first_tx(TIP + 1));
        for (after, cursor_point) in [
            (None, LedgerPoint::checkpoint_start(TIP)),
            (Some(past_tip_after.clone()), past_tip_point),
        ] {
            let options = options_with(Ordering::Ascending, after, None);
            let (resolved, window) = resolve_tx_in_window((Some(TIP + 3), None), &options).unwrap();
            assert_eq!(
                resolved.end_resume_position(&window, &options),
                LedgerPoint::checkpoint_start(TIP + 3)
            );
            assert_eq!(resolved.end_cursor_public_position(&options), cursor_point);
        }

        let options = options_with(Ordering::Ascending, Some(past_tip_after.clone()), None);
        let (resolved, window) = resolve_tx_in_window((Some(3), None), &options).unwrap();
        assert_eq!(
            resolved.end_resume_position(&window, &options),
            past_tip_point
        );
        assert_eq!(
            resolved.end_cursor_public_position(&options),
            past_tip_point
        );
        let watermark = resolved
            .range_end(&window, &options, tx_position)
            .terminal()
            .into_watermark(None);
        assert_eq!(watermark.cursor, Some(past_tip_after.encode()));
        assert_eq!(
            watermark.cursor_position,
            Some(past_tip_point.to_ledger_position(&past_tip_after.position))
        );

        // An Item cursor resumes right after its item.
        let past_tip_item = tx_item(first_tx(TIP));
        let options = options_with(Ordering::Ascending, Some(past_tip_item), None);
        let (resolved, window) = resolve_tx_in_window((Some(3), None), &options).unwrap();
        assert_eq!(
            resolved.end_resume_position(&window, &options),
            LedgerPoint::transaction(TIP, 1)
        );
        assert_eq!(
            resolved.end_cursor_public_position(&options),
            LedgerPoint::transaction(TIP, 1)
        );
    }

    /// A descending scan that reaches the serving floor resumes at the floor checkpoint's start.
    #[test]
    fn serving_floor_terminal_is_the_floor_checkpoint_start() {
        let options = options_with(Ordering::Descending, None, None);
        let (mut resolved, window) = resolve_tx_in_window((None, None), &options).unwrap();
        resolved
            .apply_serving_floor(first_tx(4), 4, &options)
            .unwrap();
        assert_eq!(
            resolved.end_resume_position(&window, &options),
            LedgerPoint::checkpoint_start(4)
        );
    }

    /// An ascending scan whose high end is the tip, with nothing left to scan, ends at the tip
    /// with `LedgerTip`, however its `after` cursor names the tip. A `before` cursor at the tip
    /// bounds the scan instead, and a descending scan ends at its `after` cursor.
    #[test]
    fn caught_up_ascending_scan_ends_with_ledger_tip() {
        let tip_tx = first_tx(TIP);
        let cursor_bound = RangeExhaustion::CursorBound {
            kind: CursorKind::Boundary,
        };
        // The `LedgerTip` terminal, an ascending frontier at the tip, and the last item, each
        // with the public position where it resumes.
        for (cursor, end_public_position) in [
            (tx_boundary(tip_tx), LedgerPoint::checkpoint_start(TIP)),
            (tx_frontier(tip_tx), after_transaction_point(tip_tx)),
            (tx_item(tip_tx - 1), after_transaction_point(tip_tx)),
        ] {
            for window in [(None, None), (Some(3), None), (Some(3), Some(TIP + 5))] {
                let options = after_options(Ordering::Ascending, cursor.clone());
                let resolved = resolve_tx_request(window, &options).unwrap();
                let context = format!("{cursor:?} {window:?}");
                assert!(resolved.is_empty(), "{context}");
                assert_eq!(resolved.exhaustion, RangeExhaustion::LedgerTip, "{context}");
                assert_eq!(
                    (resolved.end_public_position, resolved.end_scan_position),
                    (end_public_position, tip_tx),
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
                    Some(tx_boundary(position)),
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

        let events =
            resolve_events(&after_options(Ordering::Ascending, ev_boundary(tip_tx, 0))).unwrap();
        assert!(events.is_empty());
        assert_eq!(events.exhaustion, RangeExhaustion::LedgerTip);
        assert_eq!(
            events.end_scan_position,
            IntraTxCoordinate::start_of_tx(tip_tx)
        );

        let descending =
            resolve_transactions(&after_options(Ordering::Descending, tx_boundary(tip_tx)))
                .unwrap();
        assert!(descending.is_empty());
        assert_eq!(descending.exhaustion, cursor_bound);
        assert_eq!(
            (descending.end_public_position, descending.end_scan_position),
            (LedgerPoint::checkpoint_start(TIP), tip_tx)
        );
    }

    /// A descending frontier resumed descending claims coverage from the window's end: the
    /// cursor vouches for every position above it, so the scan's first item claims the
    /// checkpoint above the item's, though the scan never read that checkpoint.
    #[test]
    fn descending_resume_claims_what_its_cursor_vouches_for() {
        // Descending frontier at transaction 92, in checkpoint 18.
        let options = before_options(Ordering::Descending, tx_boundary(92));
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

    /// Cursor positions, and the public positions cursors carry, at the numeric edges resolve
    /// without overflow.
    #[test]
    fn cursors_at_numeric_edges_resolve_without_overflow() {
        // A boundary at genesis admits every position.
        let resolved =
            resolve_transactions(&after_options(Ordering::Ascending, tx_boundary(0))).unwrap();
        assert_eq!(resolved.range(), 0..100);
        // A `before` at `u64::MAX` is past every window, so it narrows nothing.
        let resolved =
            resolve_transactions(&before_options(Ordering::Ascending, tx_boundary(u64::MAX)))
                .unwrap();
        assert_eq!(resolved.range(), 0..100);
        // An `after` at `u64::MAX` is past the tip, and past a window that ends below the tip.
        let options = after_options(Ordering::Descending, tx_boundary(u64::MAX));
        assert!(is_out_of_range(resolve_transactions(&options)));
        let resolved = resolve_tx_request((None, Some(10)), &options).unwrap();
        assert!(resolved.is_empty());
        // An ascending `after` Item at the last position admits nothing, so its page ends at it,
        // in a window with checkpoints or without.
        let options = after_options(Ordering::Ascending, tx_item(u64::MAX));
        for window in [(None, None), (Some(TIP + 1), None)] {
            let resolved = resolve_tx_request(window, &options).unwrap();
            assert!(resolved.is_empty(), "{window:?}");
            assert_eq!(
                (resolved.end_scan_position, resolved.exhaustion),
                (
                    u64::MAX,
                    RangeExhaustion::CursorBound {
                        kind: CursorKind::Item
                    }
                ),
                "{window:?}"
            );
        }

        // An Item whose public position sits at every numeric edge resumes right after it, where
        // the point saturates rather than overflowing.
        let edge_point = LedgerPoint::transaction(u64::MAX, u64::MAX);
        let options = after_options(
            Ordering::Ascending,
            CursorToken::item(tx_position(u64::MAX - 1, edge_point)),
        );
        let (resolved, window) = resolve_tx_in_window((None, None), &options).unwrap();
        assert_eq!(resolved.end_cursor_public_position(&options), edge_point);
        assert_eq!(resolved.end_resume_position(&window, &options), edge_point);

        let edge_point = LedgerPoint::event(u64::MAX, u64::MAX, u32::MAX);
        let options = after_options(
            Ordering::Ascending,
            CursorToken::item(ev_position(u64::MAX - 1, 0, edge_point)),
        );
        let (resolved, window) = resolve_event_in_window((None, None), &options).unwrap();
        assert_eq!(resolved.end_cursor_public_position(&options), edge_point);
        assert_eq!(resolved.end_resume_position(&window, &options), edge_point);
    }
}
