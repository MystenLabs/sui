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
/// interval to scan. Unlike [`ResolvedScan<u64>`]/[`ResolvedScan<IntraTxCoordinate>`], the
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
    /// Checkpoint containing the interval's first position in scan direction.
    pub entry_checkpoint: u64,
    /// Checkpoint containing the `end_position`.
    pub end_checkpoint: u64,
    /// Scan-direction terminal edge of the interval.
    pub end_position: P,
    /// Why the interval is exhausted once the scan drains it.
    pub exhaustion: RangeExhaustion,
}

/// How a lane reads wire cursors: the token's coordinate in the lane's
/// position space.
pub trait ScanCursor<P> {
    fn coordinate(&self) -> P;
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

impl<P: Copy + Ord> ResolvedScan<P>
where
    CursorToken: ScanCursor<P>,
{
    /// Record the request's checkpoint bounds, then apply cursor bounds to tighten the `range`
    /// scanning interval. If the backend data source has a serving floor, `apply_serving_floor`
    /// must be called to reconcile the scanning interval.
    pub fn resolve(
        cp_range: ResolvedCheckpointRange,
        range: Range<P>,
        options: &QueryOptions,
    ) -> Self {
        let entry_checkpoint = if cp_range.is_empty() {
            // No checkpoint entered, pin entry to terminal boundary
            cp_range.range.end
        } else if options.is_ascending() {
            cp_range.range.start
        } else {
            cp_range.range.end.saturating_sub(1)
        };

        Self {
            bounds: ScanBounds::from_range(range.start..range.end),
            entry_checkpoint,
            end_checkpoint: cp_range.terminal_checkpoint(options.ordering),
            end_position: match options.ordering {
                Ordering::Ascending => range.end,
                Ordering::Descending => range.start,
            },
            exhaustion: cp_range.exhaustion,
        }
        .apply_cursor_bounds(options)
    }

    pub fn is_empty(&self) -> bool {
        self.bounds.is_empty()
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

        if options.is_ascending() {
            self.entry_checkpoint = self.entry_checkpoint.max(checkpoint);
        }

        // Symbolic resume in every lane: an Item admits strictly-after, a
        // Boundary admits from itself; successor arithmetic exists only at
        // the store edge.
        let candidate = match cursor.kind {
            sui_rpc_cursor::CursorKind::Item => Bound::Excluded(position),
            sui_rpc_cursor::CursorKind::Boundary => Bound::Included(position),
        };

        if !lower_bound_gte(candidate, self.bounds.lo) {
            return None;
        }

        // The after cursor is the terminal bound for a descending scan.
        if !options.is_ascending() {
            self.set_terminal_record(
                checkpoint,
                position,
                RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
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
        let kind = if options.is_ascending() {
            cursor.kind
        } else {
            sui_rpc_cursor::CursorKind::Boundary
        };
        Some((checkpoint, position, kind))
    }

    /// Tightens the high bound, and returns the terminal record if the cursor empties the interval.
    fn apply_before_cursor(
        &mut self,
        options: &QueryOptions,
    ) -> Option<(u64, P, sui_rpc_cursor::CursorKind)> {
        let cursor = options.before.as_ref()?;
        let checkpoint = cursor.position.checkpoint();
        let position: P = cursor.coordinate();

        if !options.is_ascending() {
            self.entry_checkpoint = self.entry_checkpoint.min(checkpoint);
        }

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

    pub fn apply_serving_floor(&mut self, floor: P, floor_checkpoint: u64, options: &QueryOptions) {
        let floored_lo = Bound::Included(floor);
        let floored = ScanBounds {
            lo: floored_lo,
            hi: self.bounds.hi,
        };
        if floored.is_empty() {
            // Canonical empty form everywhere in this module: the interval
            // collapses onto its reported terminal bound.
            self.bounds = ScanBounds::empty_at(self.end_position);
            return;
        }
        self.bounds.lo = floored_lo;
        if options.is_ascending() {
            self.entry_checkpoint = self.entry_checkpoint.max(floor_checkpoint);
        } else {
            self.end_checkpoint = floor_checkpoint;
            self.end_position = floor;
        }
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
    /// Validate the requested checkpoint window, clamp it to the indexed tip,
    /// tighten it by the request's cursors, and attribute the exhaustion
    /// reason the scan will report once it drains it — request to resolved
    /// window in one call.
    pub fn from_request(
        start_checkpoint: Option<u64>,
        end_checkpoint: Option<u64>,
        checkpoint_hi_exclusive: u64,
        options: &QueryOptions,
    ) -> Result<Self, RpcError> {
        validate_checkpoint_bounds(start_checkpoint, end_checkpoint)?;
        let start = start_checkpoint.unwrap_or(0);

        let requested_end = end_checkpoint.unwrap_or(checkpoint_hi_exclusive);
        let mut high_exhaustion =
            if end_checkpoint.is_none() || requested_end > checkpoint_hi_exclusive {
                RangeExhaustion::LedgerTip
            } else {
                RangeExhaustion::CheckpointBound
            };
        let mut start = start;
        let mut end = requested_end.min(checkpoint_hi_exclusive);
        let mut low_exhaustion = RangeExhaustion::CheckpointBound;
        let mut cursor_bound = false;

        if let Some(cursor) = &options.after
            && cursor.position.checkpoint() >= start
        {
            start = cursor.position.checkpoint();
            cursor_bound = true;
            if matches!(options.ordering, Ordering::Descending) {
                low_exhaustion = RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                };
            }
        }

        if let Some(cursor) = &options.before
            && let Some(upper) = match cursor.kind {
                sui_rpc_cursor::CursorKind::Item => cursor.position.checkpoint().checked_add(1),
                sui_rpc_cursor::CursorKind::Boundary => Some(cursor.position.checkpoint()),
            }
            && upper <= end
        {
            end = upper;
            cursor_bound = true;
            if matches!(options.ordering, Ordering::Ascending) {
                high_exhaustion = RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                };
            }
        }

        if start >= checkpoint_hi_exclusive {
            return Ok(Self::empty_at(
                checkpoint_hi_exclusive,
                RangeExhaustion::LedgerTip,
            ));
        }

        if start >= end {
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
            return Ok(Self::empty_at(checkpoint, exhaustion));
        }

        let exhaustion = match options.ordering {
            Ordering::Ascending => high_exhaustion,
            Ordering::Descending => low_exhaustion,
        };
        Ok(Self {
            range: start..end,
            exhaustion,
        })
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
        }
    }

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
            entry_checkpoint: end_checkpoint,
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
            end_checkpoint: 20,
            end_position: 100,
            exhaustion: RangeExhaustion::CheckpointBound,
            entry_checkpoint: 0,
        };
        resolved.apply_serving_floor(50, 10, &asc);
        assert_eq!(resolved.range(), 50..100);
        assert_eq!(resolved.entry_checkpoint, 10);
        assert_eq!(resolved.end_checkpoint, 20);
        assert_eq!(resolved.end_position, 100);

        // Descending, floor inside: entry (the high edge) untouched; the
        // terminal pins to the floor.
        let mut resolved = ResolvedScan {
            bounds: ScanBounds::from_range(0..100),
            end_checkpoint: 0,
            end_position: 0,
            exhaustion: RangeExhaustion::CheckpointBound,
            entry_checkpoint: 20,
        };
        resolved.apply_serving_floor(50, 10, &desc);
        assert_eq!(resolved.range(), 50..100);
        assert_eq!(resolved.entry_checkpoint, 20);
        assert_eq!(resolved.end_checkpoint, 10);
        assert_eq!(resolved.end_position, 50);

        // Floor at/past the high end (covers the == boundary), both
        // directions: empty intersection, canonicalized at the reported
        // terminal bound; no metadata moves. Descending, the terminal must
        // NOT move to the floor (checkpoint 10 lies outside the requested
        // interval).
        for floor_tx in [40, 50] {
            let mut resolved = ResolvedScan {
                bounds: ScanBounds::from_range(0..40),
                end_checkpoint: 8,
                end_position: 40,
                exhaustion: RangeExhaustion::CheckpointBound,
                entry_checkpoint: 0,
            };
            resolved.apply_serving_floor(floor_tx, 10, &asc);
            assert!(resolved.is_empty());
            assert_eq!(resolved.range(), 40..40);
            assert_eq!(resolved.entry_checkpoint, 0);
            assert_eq!(resolved.end_checkpoint, 8);
            assert_eq!(resolved.end_position, 40);

            let mut resolved = ResolvedScan {
                bounds: ScanBounds::from_range(0..40),
                end_checkpoint: 0,
                end_position: 0,
                exhaustion: RangeExhaustion::CheckpointBound,
                entry_checkpoint: 8,
            };
            resolved.apply_serving_floor(floor_tx, 10, &desc);
            assert!(resolved.is_empty());
            assert_eq!(resolved.range(), 0..0);
            assert_eq!(resolved.entry_checkpoint, 8);
            assert_eq!(resolved.end_checkpoint, 0);
            assert_eq!(resolved.end_position, 0);
        }
    }

    /// [`ResolvedScan::<IntraTxCoordinate>::apply_serving_floor`] mirrors the tx behavior in
    /// event coordinates.
    #[test]
    fn event_serving_floor_reconciles_or_canonicalizes_empty() {
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
        };
        resolved.apply_serving_floor(IntraTxCoordinate::start_of_tx(50), 10, &asc);
        assert_eq!(
            resolved.bounds.lo,
            Bound::Included(IntraTxCoordinate::start_of_tx(50))
        );
        assert_eq!(resolved.entry_checkpoint, 10);
        assert_eq!(resolved.end_checkpoint, 20);
        assert_eq!(resolved.end_position, IntraTxCoordinate::start_of_tx(100));

        // Descending, floor inside: terminal pins to the floor.
        let mut resolved = ResolvedScan {
            bounds: IntraTxScanBounds::from_range(
                IntraTxCoordinate::start_of_tx(0)..IntraTxCoordinate::start_of_tx(100),
            ),
            end_checkpoint: 0,
            end_position: IntraTxCoordinate::start_of_tx(0),
            exhaustion: RangeExhaustion::CheckpointBound,
            entry_checkpoint: 20,
        };
        resolved.apply_serving_floor(IntraTxCoordinate::start_of_tx(50), 10, &desc);
        assert_eq!(
            resolved.bounds.lo,
            Bound::Included(IntraTxCoordinate::start_of_tx(50))
        );
        assert_eq!(resolved.entry_checkpoint, 20);
        assert_eq!(resolved.end_checkpoint, 10);
        assert_eq!(resolved.end_position, IntraTxCoordinate::start_of_tx(50));

        // Floor that empties the bounds (covers the == boundary), both
        // directions: canonical empty at the reported terminal bound, no
        // metadata moves.
        for floor_tx in [40, 50] {
            let mut resolved = ResolvedScan {
                bounds: IntraTxScanBounds::from_range(
                    IntraTxCoordinate::start_of_tx(0)..IntraTxCoordinate::start_of_tx(40),
                ),
                end_checkpoint: 8,
                end_position: IntraTxCoordinate::start_of_tx(40),
                exhaustion: RangeExhaustion::CheckpointBound,
                entry_checkpoint: 0,
            };
            resolved.apply_serving_floor(IntraTxCoordinate::start_of_tx(floor_tx), 10, &asc);
            assert!(resolved.is_empty());
            assert_eq!(
                resolved.bounds,
                IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(40))
            );
            assert_eq!(resolved.entry_checkpoint, 0);
            assert_eq!(resolved.end_checkpoint, 8);
            assert_eq!(resolved.end_position, IntraTxCoordinate::start_of_tx(40));

            let mut resolved = ResolvedScan {
                bounds: IntraTxScanBounds::from_range(
                    IntraTxCoordinate::start_of_tx(0)..IntraTxCoordinate::start_of_tx(40),
                ),
                end_checkpoint: 0,
                end_position: IntraTxCoordinate::start_of_tx(0),
                exhaustion: RangeExhaustion::CheckpointBound,
                entry_checkpoint: 8,
            };
            resolved.apply_serving_floor(IntraTxCoordinate::start_of_tx(floor_tx), 10, &desc);
            assert!(resolved.is_empty());
            assert_eq!(
                resolved.bounds,
                IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(0))
            );
            assert_eq!(resolved.entry_checkpoint, 8);
            assert_eq!(resolved.end_checkpoint, 0);
            assert_eq!(resolved.end_position, IntraTxCoordinate::start_of_tx(0));
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
        assert_eq!(bounded.end_position, 11);
    }

    // (descending, after = Item 11, before = Item 12): the cursors leave the
    // empty gap (11, 12), so the fetch serves nothing and the CursorBound
    // frame comes from the descending stop side (the after cursor, at 11).
    #[test]
    fn descending_before_adjacent_to_after_empties_at_after_cursor() {
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
                end_position: 11,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
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

    /// (start_checkpoint = 30, tip = 20, both orderings): resolves empty at the tip —
    /// entry and terminal checkpoints 20, LedgerTip.
    #[test]
    fn resolve_window_past_tip_empties_with_ledger_tip() {
        for ascending in [true, false] {
            let mut request = ProtoQueryOptions::default();
            if !ascending {
                request.ordering = Some(ProtoOrdering::Descending as i32);
            }
            let options = QueryOptions::events_from_proto(Some(&request), 100, 100).unwrap();
            let cp_range =
                ResolvedCheckpointRange::from_request(Some(30), None, 20, &options).unwrap();
            assert!(cp_range.is_empty());
            assert_eq!(
                ResolvedScan::<IntraTxCoordinate>::resolve(
                    cp_range,
                    IntraTxCoordinate::tx_window(100..100),
                    &options
                ),
                ResolvedScan {
                    // Bounds are based on the requested checkpoint range and
                    // checkpoint_hi_exclusive or end_checkpoint
                    bounds: IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(100)),
                    // Entry and end checkpoint take on the terminal checkpoint
                    entry_checkpoint: 20,
                    end_checkpoint: 20,
                    end_position: IntraTxCoordinate::start_of_tx(100),
                    exhaustion: RangeExhaustion::LedgerTip,
                }
            );
        }
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
                ),
                ResolvedScan {
                    bounds: IntraTxScanBounds::empty_at(IntraTxCoordinate::start_of_tx(100)),
                    entry_checkpoint: 10,
                    end_checkpoint: 10,
                    end_position: IntraTxCoordinate::start_of_tx(100),
                    exhaustion: RangeExhaustion::CheckpointBound,
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
                ResolvedCheckpointRange::from_request(Some(30), None, 20, &options).unwrap();
            let resolved = ResolvedScan::<IntraTxCoordinate>::resolve(
                cp_range,
                IntraTxCoordinate::tx_window(100..100),
                &options,
            );
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
        );
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
        );
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
        };

        // This cursor will empty the interval as it will pull entry_checkpoint to 2
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
                    entry_checkpoint: 2,
                    end_checkpoint: 2,
                    // takes on the cursor position that emptied the range
                    end_position: new_coordinate,
                    exhaustion: RangeExhaustion::CursorBound {
                        kind: sui_rpc_cursor::CursorKind::Boundary,
                    },
                }
            );
        }
    }

    /// (ascending, after inside the window): Item admits strictly after its
    /// coordinate, Boundary from it; entry checkpoint rises; terminal untouched.
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
                entry_checkpoint: 3,
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
                entry_checkpoint: 3,
                ..resolved_intra_tx()
            }
        );
    }

    #[test]
    fn after_cursor_below_lower_bound_only_affects_entry_checkpoint() {
        let seed = ResolvedScan {
            bounds: IntraTxScanBounds {
                lo: Bound::Included(IntraTxCoordinate::start_of_tx(5)),
                hi: Bound::Excluded(IntraTxCoordinate::start_of_tx(10)),
            },
            ..resolved_intra_tx()
        };
        let expected = ResolvedScan {
            entry_checkpoint: 4,
            ..seed.clone()
        };
        let options = after_options(Ordering::Ascending, ev_boundary(4, 2, 0));
        assert_eq!(seed.clone().apply_cursor_bounds(&options), expected);
        let options = after_options(Ordering::Ascending, ev_item(4, 2, 0));
        assert_eq!(seed.apply_cursor_bounds(&options), expected);
    }

    /// Tightens the lower bound, and the terminal becomes the cursor's, because
    /// the after cursor is where a descending scan stops.
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
                    index: 1,
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

    /// When the after cursor empties the window, the record empties at the
    /// cursor's coordinate and checkpoint; either cursor kind (Item or Boundary)
    /// normalizes to Boundary.
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
        };

        // Either cursor kind in: the descending recording must normalize the
        // terminal kind to Boundary.
        let expected_coordinate = IntraTxCoordinate {
            tx_seq: 100,
            index: 10,
        };
        let expected = ResolvedScan {
            bounds: IntraTxScanBounds::empty_at(expected_coordinate),
            entry_checkpoint: 9,
            end_checkpoint: 100,
            end_position: expected_coordinate,
            exhaustion: RangeExhaustion::CursorBound {
                kind: sui_rpc_cursor::CursorKind::Boundary,
            },
        };

        let options = after_options(Ordering::Descending, ev_item(100, 100, 10));
        assert_eq!(seed.clone().apply_cursor_bounds(&options), expected);
        let options = after_options(Ordering::Descending, ev_boundary(100, 100, 10));
        assert_eq!(seed.apply_cursor_bounds(&options), expected);
    }

    /// (descending, before inside the window, either cursor kind (Item or
    /// Boundary)): hi becomes exclusive at the cursor; entry checkpoint folds
    /// down; terminal untouched.
    #[test]
    fn before_cursor_tightens_descending_upper_bound() {
        let seed = ResolvedScan {
            entry_checkpoint: 8,
            ..resolved_intra_tx()
        };
        // The before-arm is kind-agnostic: Item and Boundary cursors produce
        // identical records.
        let expected = ResolvedScan {
            bounds: IntraTxScanBounds {
                lo: Bound::Included(IntraTxCoordinate::start_of_tx(0)),
                hi: Bound::Excluded(IntraTxCoordinate {
                    tx_seq: 5,
                    index: 1,
                }),
            },
            entry_checkpoint: 6,
            ..seed.clone()
        };
        let options = before_options(Ordering::Descending, ev_item(6, 5, 1));
        assert_eq!(seed.clone().apply_cursor_bounds(&options), expected);
        let options = before_options(Ordering::Descending, ev_boundary(6, 5, 1));
        assert_eq!(seed.apply_cursor_bounds(&options), expected);
    }

    #[test]
    fn before_cursor_above_window_never_tightens() {
        let expected = ResolvedScan {
            entry_checkpoint: 1,
            ..resolved_intra_tx()
        };
        for cursor in [ev_boundary(1, 12, 0), ev_item(1, 12, 0)] {
            let options = before_options(Ordering::Descending, cursor.clone());
            assert_eq!(resolved_intra_tx().apply_cursor_bounds(&options), expected);

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
                entry_checkpoint: 12,
                end_checkpoint: 12,
                end_position: IntraTxCoordinate::start_of_tx(12),
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Item,
                },
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
                entry_checkpoint: 12,
                end_checkpoint: 3,
                end_position: IntraTxCoordinate::start_of_tx(3),
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
            }
        );
    }

    /// (descending, after and before both present): the after's frame stands when
    /// only it files; when both file, the min position wins; a before that empties
    /// overrides the after's earlier terminal.
    #[test]
    fn descending_with_both_cursors_picks_the_terminal_owner() {
        // After empties, before rejected: the after-claim survives, and the
        // rejected before-cursor still folds entry down.
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
                entry_checkpoint: 1,
                end_checkpoint: 9,
                end_position: IntraTxCoordinate::start_of_tx(12),
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
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
                entry_checkpoint: 1,
                end_checkpoint: 1,
                end_position: IntraTxCoordinate {
                    tx_seq: 5,
                    index: 2,
                },
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
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
                entry_checkpoint: 1,
                end_checkpoint: 1,
                end_position: IntraTxCoordinate::start_of_tx(5),
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
            }
        );
    }

    /// (descending, after below the window): complete noop — even entry_checkpoint
    /// is untouched, since a descending scan doesn't enter from the after side.
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
                entry_checkpoint: 3,
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
                entry_checkpoint: 3,
                ..resolved_range(10..20)
            }
        );
    }

    /// Tightens the lower bound, and the terminal becomes the cursor's, because
    /// the after cursor is where a descending scan stops.
    #[test]
    fn tx_descending_after_cursor_sets_terminal_and_tightens() {
        // Item at 12 and Boundary at 13 scan the same exclusive edge (13..20),
        // but each terminal echoes its own raw cursor coordinate.
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
                end_position: 12,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
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
            }
        );
    }

    /// When the after cursor empties the window, the record empties at the
    /// cursor's coordinate and checkpoint; either cursor kind (Item or Boundary)
    /// normalizes to Boundary.
    #[test]
    fn tx_after_cursor_empties_descending_interval() {
        // Item at 24 and Boundary at 25 scan the same exclusive edge, but the
        // terminal echoes each cursor's raw coordinate.
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
            }
        );
        let options = after_options(Ordering::Descending, tx_item(9, 24));
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: ScanBounds::from_range(24..24),
                entry_checkpoint: 0,
                end_checkpoint: 9,
                end_position: 24,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
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
                entry_checkpoint: 5,
                end_checkpoint: 5,
                end_position: 24,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Item,
                },
            }
        );
        let options = after_options(Ordering::Ascending, tx_boundary(5, 25));
        assert_eq!(
            resolved_range(10..20).apply_cursor_bounds(&options),
            ResolvedScan {
                bounds: ScanBounds::from_range(25..25),
                entry_checkpoint: 5,
                end_checkpoint: 5,
                end_position: 25,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
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
                entry_checkpoint: 5,
                end_checkpoint: 3,
                end_position: 15,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
            }
        );

        // Descending: the before-cursor's entry fold also runs now.
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
                entry_checkpoint: 3,
                end_checkpoint: 3,
                end_position: 15,
                exhaustion: RangeExhaustion::CursorBound {
                    kind: sui_rpc_cursor::CursorKind::Boundary,
                },
            }
        );
    }

    /// Resume-equivalence witness for the raw cursor echo: an after-Item at n
    /// admits exactly what the successor-form echo it replaced (Boundary at
    /// n + 1) admits, so feeding either token back scans the same interval.
    /// The terminals differ by design (each echoes its own raw coordinate);
    /// the descending pair is pinned by
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
                entry_checkpoint: 5,
                ..initial_range
            }
        );
        assert!(!resolved.is_empty());
        assert!(resolved.range().is_empty());
    }

    /// Exclusive after on descending bumps the bounds, and sets the end_checkpoint, end_position,
    /// and exhaustion to the after cursor.
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
                end_position: 19,
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
}
