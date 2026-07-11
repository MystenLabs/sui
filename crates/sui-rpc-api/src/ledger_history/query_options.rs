// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::{Bound, Range};

use bytes::Bytes;
use sui_inverted_index::ScanDirection;
use sui_rpc::proto::sui::rpc::v2alpha::Ordering as ProtoOrdering;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::QueryOptions as ProtoQueryOptions;
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

/// Event-order coordinate. Boundary cursors may point at slots with no event.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EventPosition {
    pub tx_seq: u64,
    pub event_index: u32,
}

impl EventPosition {
    /// Fencepost at the first event slot of `tx_seq`; valid as a boundary even
    /// if the transaction has no events.
    pub fn start_of_tx(tx_seq: u64) -> Self {
        Self {
            tx_seq,
            event_index: 0,
        }
    }
}

impl From<EventPosition> for (u64, u32) {
    fn from(position: EventPosition) -> Self {
        (position.tx_seq, position.event_index)
    }
}

impl From<(u64, u32)> for EventPosition {
    fn from((tx_seq, event_index): (u64, u32)) -> Self {
        Self {
            tx_seq,
            event_index,
        }
    }
}

/// Validated, normalized form of `QueryOptions` (the proto wire type).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryOptions {
    pub limit_items: usize,
    pub ordering: Ordering,
    after: Option<CursorToken>,
    before: Option<CursorToken>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedCheckpointRange {
    pub range: Range<u64>,
    pub end_reason: QueryEndReason,
}

/// Scan window + terminal bookkeeping in the endpoint's scan space (checkpoint
/// seq for `list_checkpoints`, tx seq for `list_transactions`): the resolved
/// checkpoint window projected through the endpoint's coordinate translation,
/// then clamped by the cursors' positions.
///
/// `end_position` is the terminal-watermark position — where the scan ends and
/// where the natural-completion resume cursor points. When a cursor bounds the
/// end of the window the stamp records the cursor's own position and
/// `end_reason` becomes `CursorBound`, which is never emitted
/// (`reached_range_end` excludes it), so cursor-derived stamps are
/// bookkeeping-only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanRange {
    pub range: Range<u64>,
    pub end_position: Position,
    pub end_reason: QueryEndReason,
}

/// Semantic scan bounds over explicit event coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventScanBounds {
    pub lo: Bound<EventPosition>,
    pub hi: Bound<EventPosition>,
}

/// [`ScanRange`]'s event-space counterpart (`list_events`): the scan window is
/// a pair of `Bound<EventPosition>`s instead of a half-open `Range<u64>`, so
/// cursor clamping needs no ±1 arithmetic on the composite coordinate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventScanRange {
    pub bounds: EventScanBounds,
    pub end_position: Position,
    pub end_reason: QueryEndReason,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckpointRange {
    start: u64,
    end: u64,
    high_reason: QueryEndReason,
    indexed_tip: u64,
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

    pub fn apply_cursor_bounds(&self, resolved: ScanRange) -> ScanRange {
        if resolved.is_empty() {
            return resolved;
        }

        let mut start = resolved.range.start;
        let mut end = resolved.range.end;
        let mut end_position = resolved.end_position;
        let mut end_reason = resolved.end_reason;
        let mut cursor_terminal = None;

        // CursorBound bookkeeping stamps record the cursor's own position (raw
        // coordinate, not the ±1-adjusted scan bound). Terminal watermarks are
        // only emitted on natural range completion, so the stamped coordinate
        // is wire-invisible — same convention as the event lane.
        if let Some(cursor) = &self.after {
            let position = u64_position_coordinate(&cursor.position);
            let Some(after) = (match cursor.kind {
                sui_rpc_cursor::CursorKind::Item => position.checked_add(1),
                sui_rpc_cursor::CursorKind::Boundary => Some(position),
            }) else {
                return ScanRange::empty_at(cursor.position, QueryEndReason::CursorBound);
            };
            if after >= start {
                start = after;
                if matches!(self.ordering, Ordering::Descending) || after >= end {
                    cursor_terminal = Some(cursor.position);
                }
                if matches!(self.ordering, Ordering::Descending) {
                    end_position = cursor.position;
                    end_reason = QueryEndReason::CursorBound;
                }
            }
        }

        if let Some(cursor) = &self.before {
            let position = u64_position_coordinate(&cursor.position);
            if position <= end {
                end = position;
                if matches!(self.ordering, Ordering::Ascending) || position <= start {
                    cursor_terminal = Some(cursor.position);
                }
                if matches!(self.ordering, Ordering::Ascending) {
                    end_position = cursor.position;
                    end_reason = QueryEndReason::CursorBound;
                }
            }
        }

        if start >= end {
            if let Some(position) = cursor_terminal {
                end_position = position;
            }
            if self.after.is_some() || self.before.is_some() {
                end_reason = QueryEndReason::CursorBound;
            }
            ScanRange::empty_at(end_position, end_reason)
        } else {
            ScanRange {
                range: start..end,
                end_position,
                end_reason,
            }
        }
    }

    pub fn apply_event_cursor_bounds(&self, resolved: EventScanRange) -> EventScanRange {
        if resolved.is_empty() {
            return resolved;
        }

        let mut bounds = resolved.bounds;
        let mut end_position = resolved.end_position;
        let mut end_reason = resolved.end_reason;
        let mut cursor_terminal = None;

        if let Some(cursor) = &self.after {
            let position = event_position_coordinate(&cursor.position);
            let candidate = match cursor.kind {
                sui_rpc_cursor::CursorKind::Item => Bound::Excluded(position),
                sui_rpc_cursor::CursorKind::Boundary => Bound::Included(position),
            };
            if lower_bound_gte(candidate, bounds.lo) {
                let candidate_bounds = EventScanBounds {
                    lo: candidate,
                    hi: bounds.hi,
                };
                bounds.lo = candidate;
                if matches!(self.ordering, Ordering::Descending) || candidate_bounds.is_empty() {
                    cursor_terminal = Some(cursor.position);
                }
                if matches!(self.ordering, Ordering::Descending) {
                    end_position = cursor.position;
                    end_reason = QueryEndReason::CursorBound;
                }
            }
        }

        if let Some(cursor) = &self.before {
            let position = event_position_coordinate(&cursor.position);
            if hi_admits_upper_bound(bounds.hi, position) {
                let candidate = Bound::Excluded(position);
                let candidate_bounds = EventScanBounds {
                    lo: bounds.lo,
                    hi: candidate,
                };
                bounds.hi = candidate;
                if matches!(self.ordering, Ordering::Ascending) || candidate_bounds.is_empty() {
                    cursor_terminal = Some(cursor.position);
                }
                if matches!(self.ordering, Ordering::Ascending) {
                    end_position = cursor.position;
                    end_reason = QueryEndReason::CursorBound;
                }
            }
        }

        // CursorBound bookkeeping records the cursor's raw position rather than
        // a packed successor. Terminal watermarks are only emitted for natural
        // range completion, so this is wire-invisible; coordinate-adjacent event
        // cursor pairs are left for the scan adapter to collapse to an empty
        // packed range.
        if bounds.is_empty() {
            if let Some(position) = cursor_terminal {
                end_position = position;
            }
            if self.after.is_some() || self.before.is_some() {
                end_reason = QueryEndReason::CursorBound;
            }
            EventScanRange::empty_at(end_position, end_reason)
        } else {
            EventScanRange {
                bounds,
                end_position,
                end_reason,
            }
        }
    }
}

/// The scan-space coordinate of a scalar-lane position (`list_checkpoints` /
/// `list_transactions`).
fn u64_position_coordinate(position: &Position) -> u64 {
    match *position {
        Position::Checkpoints { checkpoint } => checkpoint,
        Position::Transactions { tx_seq, .. } => tx_seq,
        Position::Events { .. } => panic!("event queries must use apply_event_cursor_bounds"),
    }
}

/// The scan-space coordinate of an event-lane position.
fn event_position_coordinate(position: &Position) -> EventPosition {
    match *position {
        Position::Events {
            tx_seq,
            event_index,
            ..
        } => EventPosition {
            tx_seq,
            event_index,
        },
        _ => unreachable!("validated at decode"),
    }
}

impl ResolvedCheckpointRange {
    /// A checkpoint window that resolved to nothing, ending at `checkpoint`
    /// for `reason`; the endpoint projections carry both into the scan-space
    /// [`ScanRange::empty_at`] / [`EventScanRange::empty_at`].
    pub fn empty_at(checkpoint: u64, reason: QueryEndReason) -> Self {
        Self {
            range: checkpoint..checkpoint,
            end_reason: reason,
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

    /// Project into `list_checkpoints` scan space, where the scan coordinate
    /// IS the checkpoint: the terminal position is the window's terminal edge.
    pub fn with_checkpoint_range(self, range: Range<u64>, ordering: Ordering) -> ScanRange {
        let checkpoint = match ordering {
            Ordering::Ascending => range.end,
            Ordering::Descending => range.start,
        };
        ScanRange {
            range,
            end_position: Position::Checkpoints { checkpoint },
            end_reason: self.end_reason,
        }
    }

    /// Project into `list_transactions` scan space: `range` is the tx window
    /// the checkpoint window translated to; the terminal position pairs its
    /// edge with the checkpoint window's terminal edge.
    pub fn with_tx_range(self, range: Range<u64>, ordering: Ordering) -> ScanRange {
        let checkpoint = self.terminal_checkpoint(ordering);
        let tx_seq = match ordering {
            Ordering::Ascending => range.end,
            Ordering::Descending => range.start,
        };
        ScanRange {
            range,
            end_position: Position::Transactions { checkpoint, tx_seq },
            end_reason: self.end_reason,
        }
    }
}

impl ScanRange {
    /// A scan that resolved to nothing but still owes the client an answer:
    /// the window is structurally empty — handlers short-circuit on
    /// `is_empty()` without spawning a scan — while `end_position` /
    /// `end_reason` still feed the `QueryEnd` and, on natural completion
    /// (e.g. a caught-up tail polling at the ledger tip), the emitted
    /// terminal watermark. The coordinate the collapsed window sits at is
    /// symbolic; nothing iterates it.
    pub fn empty_at(end_position: Position, end_reason: QueryEndReason) -> Self {
        let coordinate = u64_position_coordinate(&end_position);
        Self {
            range: coordinate..coordinate,
            end_position,
            end_reason,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.range.is_empty()
    }
}

impl EventScanBounds {
    pub fn tx_span(start_tx: u64, end_tx: u64) -> Self {
        Self {
            lo: Bound::Included(EventPosition::start_of_tx(start_tx)),
            hi: Bound::Excluded(EventPosition::start_of_tx(end_tx)),
        }
    }

    pub fn empty_at(position: EventPosition) -> Self {
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

    pub fn contains(&self, position: EventPosition) -> bool {
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

impl EventScanRange {
    /// Event-space counterpart of [`ScanRange::empty_at`]: structurally
    /// empty bounds, terminal bookkeeping intact.
    pub fn empty_at(end_position: Position, end_reason: QueryEndReason) -> Self {
        Self {
            bounds: EventScanBounds::empty_at(event_position_coordinate(&end_position)),
            end_position,
            end_reason,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.bounds.is_empty()
    }
}

fn lower_bound_gte(candidate: Bound<EventPosition>, current: Bound<EventPosition>) -> bool {
    let Some(candidate) = lower_bound_key(candidate) else {
        return false;
    };
    match lower_bound_key(current) {
        Some(current) => candidate >= current,
        None => true,
    }
}

fn lower_bound_key(bound: Bound<EventPosition>) -> Option<(EventPosition, u8)> {
    match bound {
        Bound::Included(position) => Some((position, 0)),
        Bound::Excluded(position) => Some((position, 1)),
        Bound::Unbounded => None,
    }
}

fn hi_admits_upper_bound(current: Bound<EventPosition>, candidate: EventPosition) -> bool {
    match current {
        Bound::Included(position) | Bound::Excluded(position) => candidate <= position,
        Bound::Unbounded => true,
    }
}

impl CheckpointRange {
    pub fn from_request(
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
        let high_reason = if end_checkpoint.is_none() || requested_end > checkpoint_hi_exclusive {
            QueryEndReason::LedgerTip
        } else {
            QueryEndReason::CheckpointBound
        };
        let end = requested_end.min(checkpoint_hi_exclusive);

        Ok(Self {
            start,
            end,
            high_reason,
            indexed_tip: checkpoint_hi_exclusive,
        })
    }

    pub fn resolve(self, options: &QueryOptions) -> ResolvedCheckpointRange {
        let mut start = self.start;
        let mut end = self.end;
        let mut low_reason = QueryEndReason::CheckpointBound;
        let mut high_reason = self.high_reason;
        let mut cursor_bound = false;

        if let Some(cursor) = &options.after
            && cursor.position.checkpoint() >= start
        {
            start = cursor.position.checkpoint();
            cursor_bound = true;
            if matches!(options.ordering, Ordering::Descending) {
                low_reason = QueryEndReason::CursorBound;
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
                high_reason = QueryEndReason::CursorBound;
            }
        }

        if start >= self.indexed_tip {
            return ResolvedCheckpointRange::empty_at(self.indexed_tip, QueryEndReason::LedgerTip);
        }

        if start >= end {
            let reason = if cursor_bound {
                QueryEndReason::CursorBound
            } else {
                match options.ordering {
                    Ordering::Ascending => high_reason,
                    Ordering::Descending => low_reason,
                }
            };
            let checkpoint = match options.ordering {
                Ordering::Ascending => end,
                Ordering::Descending => start,
            };
            return ResolvedCheckpointRange::empty_at(checkpoint, reason);
        }

        let end_reason = match options.ordering {
            Ordering::Ascending => high_reason,
            Ordering::Descending => low_reason,
        };
        ResolvedCheckpointRange {
            range: start..end,
            end_reason,
        }
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

    fn resolved_range(range: Range<u64>) -> ScanRange {
        ScanRange {
            range,
            end_position: Position::Transactions {
                checkpoint: 20,
                tx_seq: 20,
            },
            end_reason: QueryEndReason::CheckpointBound,
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

    #[test]
    fn tx_range_covers_partial_endpoint_transactions() {
        let bounds = EventScanBounds {
            lo: Bound::Included(EventPosition {
                tx_seq: 10,
                event_index: 2,
            }),
            hi: Bound::Excluded(EventPosition::start_of_tx(13)),
        };

        assert_eq!(bounds.tx_range(), Some(10..13));
    }

    #[test]
    fn tx_range_keeps_tx_of_nonzero_exclusive_hi() {
        let bounds = EventScanBounds {
            lo: Bound::Unbounded,
            hi: Bound::Excluded(EventPosition {
                tx_seq: 13,
                event_index: 1,
            }),
        };

        assert_eq!(bounds.tx_range(), Some(0..14));
    }

    #[test]
    fn tx_range_empty_bounds_yield_none() {
        let bounds = EventScanBounds::tx_span(10, 10);
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
            options.apply_cursor_bounds(resolved_range(0..100)).range,
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
            options.apply_cursor_bounds(resolved_range(10..20)).range,
            12..20
        );

        let options = QueryOptions {
            after: Some(tx_item(1, u64::MAX)),
            ..options
        };
        assert_eq!(
            options.apply_cursor_bounds(resolved_range(10..20)),
            ScanRange::empty_at(
                Position::Transactions {
                    checkpoint: 1,
                    tx_seq: u64::MAX,
                },
                QueryEndReason::CursorBound
            )
        );

        let options = QueryOptions {
            ordering: Ordering::Descending,
            after: Some(tx_item(1, 11)),
            before: Some(tx_item(1, 19)),
            ..options
        };
        let bounded = options.apply_cursor_bounds(resolved_range(10..20));
        assert_eq!(bounded.range, 12..19);
        assert_eq!(bounded.end_reason, QueryEndReason::CursorBound);
        // The stamp records the cursor's own position (raw coordinate, not the
        // +1-adjusted scan bound) — wire-invisible under CursorBound.
        assert_eq!(
            bounded.end_position,
            Position::Transactions {
                checkpoint: 1,
                tx_seq: 11,
            }
        );

        let options = QueryOptions {
            before: Some(tx_item(1, 12)),
            ..options
        };
        assert_eq!(
            options.apply_cursor_bounds(resolved_range(10..20)),
            ScanRange::empty_at(
                Position::Transactions {
                    checkpoint: 1,
                    tx_seq: 12,
                },
                QueryEndReason::CursorBound
            )
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
            options.apply_cursor_bounds(resolved_range(10..30)).range,
            20..30
        );

        let options = QueryOptions {
            ordering: Ordering::Descending,
            after: None,
            before: Some(tx_boundary(2, 20)),
            ..options
        };
        assert_eq!(
            options.apply_cursor_bounds(resolved_range(10..30)).range,
            10..20
        );
    }

    #[test]
    fn resolves_checkpoint_range_with_terminal_reason() {
        assert_eq!(
            CheckpointRange::from_request(None, None, 20)
                .unwrap()
                .resolve(&query_options_from_proto(None).unwrap())
                .end_reason,
            QueryEndReason::LedgerTip
        );
        assert!(CheckpointRange::from_request(Some(10), Some(9), 20).is_err());

        let range = CheckpointRange::from_request(Some(10), None, 20).unwrap();
        let resolved = range.resolve(&query_options_from_proto(None).unwrap());
        assert_eq!(resolved.range, 10..20);
        assert_eq!(resolved.end_reason, QueryEndReason::LedgerTip);

        let range = CheckpointRange::from_request(Some(30), None, 20).unwrap();
        assert_eq!(
            range.resolve(&query_options_from_proto(None).unwrap()),
            ResolvedCheckpointRange::empty_at(20, QueryEndReason::LedgerTip)
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
        assert_eq!(resolved.end_reason, QueryEndReason::CheckpointBound);
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
            options.apply_cursor_bounds(resolved_range(10..20)).range,
            12..20
        );

        request.after = None;
        request.before = Some(token);
        let options = query_options_from_proto(Some(&request)).unwrap();
        assert_eq!(
            options.apply_cursor_bounds(resolved_range(10..20)).range,
            10..11
        );
    }
}
