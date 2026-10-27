// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Start and end positions inside the checkpoints at the edges of a `ListTransactions` /
//! `ListEvents` request's checkpoint window.
//!
//! A position is a gap between items, named by checkpoint-relative offsets: `(checkpoint,
//! transaction_index[, event_index])` is the gap just before that transaction (or that event of
//! it). The start position `(start_checkpoint, start_transaction_index[, start_event_index])` is
//! the first position a scan admits. The end position is exclusive and lies in the window's last
//! checkpoint, `(end_checkpoint - 1, end_transaction_index[, end_event_index])`, so a server that
//! ignores it returns a superset. Offsets past the end of their checkpoint (or transaction) clamp
//! to its end instead of failing: the gap after a checkpoint's last item is `(C, n)` for its
//! transaction count `n`, which clients cannot know.

use std::ops::Range;

use sui_rpc::proto::sui::rpc::v2::ListEventsRequest;
use sui_rpc::proto::sui::rpc::v2::ListTransactionsRequest;

use crate::ErrorReason;
use crate::RpcError;
use crate::ledger_history::query_options::IntraTxCoordinate;
use crate::ledger_history::query_options::ScanWindow;
use crate::proto::google::rpc::bad_request::FieldViolation;

/// A request's start and end positions within its checkpoint window's edge checkpoints. An unset
/// position leaves its edge at the checkpoint boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CheckpointPositions {
    /// Where the scan starts in `start_checkpoint` (inclusive).
    start: Option<PositionInCheckpoint>,
    /// Where the scan ends in `end_checkpoint - 1` (exclusive).
    end: Option<PositionInCheckpoint>,
    /// The request's `end_checkpoint`, set whenever `end` is.
    end_checkpoint: Option<u64>,
}

/// A gap between items inside one checkpoint: just before the event at `event_index` of the
/// transaction at `transaction_index`. Event index 0 is the gap before the whole transaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PositionInCheckpoint {
    transaction_index: u64,
    event_index: u32,
}

impl CheckpointPositions {
    pub fn from_transactions_request(request: &ListTransactionsRequest) -> Result<Self, RpcError> {
        type Request = ListTransactionsRequest;
        require_field_when_set(
            request.start_checkpoint,
            Request::START_CHECKPOINT_FIELD.name,
            request.start_transaction_index,
            Request::START_TRANSACTION_INDEX_FIELD.name,
        )?;
        require_field_when_set(
            request.end_checkpoint,
            Request::END_CHECKPOINT_FIELD.name,
            request.end_transaction_index,
            Request::END_TRANSACTION_INDEX_FIELD.name,
        )?;
        Ok(Self {
            start: PositionInCheckpoint::from_fields(request.start_transaction_index, None),
            end: PositionInCheckpoint::from_fields(request.end_transaction_index, None),
            end_checkpoint: request.end_checkpoint,
        })
    }

    pub fn from_events_request(request: &ListEventsRequest) -> Result<Self, RpcError> {
        type Request = ListEventsRequest;
        require_field_when_set(
            request.start_checkpoint,
            Request::START_CHECKPOINT_FIELD.name,
            request.start_transaction_index,
            Request::START_TRANSACTION_INDEX_FIELD.name,
        )?;
        require_field_when_set(
            request.start_transaction_index,
            Request::START_TRANSACTION_INDEX_FIELD.name,
            request.start_event_index,
            Request::START_EVENT_INDEX_FIELD.name,
        )?;
        require_field_when_set(
            request.end_checkpoint,
            Request::END_CHECKPOINT_FIELD.name,
            request.end_transaction_index,
            Request::END_TRANSACTION_INDEX_FIELD.name,
        )?;
        require_field_when_set(
            request.end_transaction_index,
            Request::END_TRANSACTION_INDEX_FIELD.name,
            request.end_event_index,
            Request::END_EVENT_INDEX_FIELD.name,
        )?;
        Ok(Self {
            start: PositionInCheckpoint::from_fields(
                request.start_transaction_index,
                request.start_event_index,
            ),
            end: PositionInCheckpoint::from_fields(
                request.end_transaction_index,
                request.end_event_index,
            ),
            end_checkpoint: request.end_checkpoint,
        })
    }

    /// A superset of the checkpoints whose first transaction's `tx_seq`
    /// [`Self::transaction_window`] and [`Self::event_window`] read for the checkpoint `window`,
    /// so a backend can fetch them all in one read.
    pub fn boundary_checkpoints(&self, window: &Range<u64>) -> Vec<u64> {
        let mut checkpoints = vec![window.start];
        if !window.is_empty() {
            checkpoints.push(window.end);
            if self.start.is_some() {
                checkpoints.push(window.start + 1);
            }
            if self.end.is_some() {
                checkpoints.push(window.end - 1);
            }
        }
        checkpoints
    }

    /// The `tx_seq` scan window for the checkpoint `window` (the request's checkpoint bounds
    /// resolved against this server's tip), narrowed to these positions. `boundary(c)` returns
    /// the `tx_seq` of checkpoint `c`'s first transaction, which is one past the last transaction
    /// of checkpoint `c - 1`.
    pub fn transaction_window(
        &self,
        window: &Range<u64>,
        boundary: impl FnMut(u64) -> Result<u64, RpcError>,
    ) -> Result<ScanWindow<u64>, RpcError> {
        self.narrow(window, boundary, |tx_seq, _event_index| tx_seq)
    }

    /// The event-coordinate scan window for the checkpoint `window`, narrowed to these positions.
    /// See [`Self::transaction_window`] for `boundary`. An event index past its transaction's
    /// events needs no clamp: as a start it admits the next transaction's events, and as an end
    /// it admits all of its own transaction's.
    pub fn event_window(
        &self,
        window: &Range<u64>,
        boundary: impl FnMut(u64) -> Result<u64, RpcError>,
    ) -> Result<ScanWindow<IntraTxCoordinate>, RpcError> {
        self.narrow(window, boundary, |tx_seq, index| IntraTxCoordinate {
            tx_seq,
            index,
        })
    }

    fn narrow<P: Copy + Eq>(
        &self,
        window: &Range<u64>,
        mut boundary: impl FnMut(u64) -> Result<u64, RpcError>,
        position: impl Fn(u64, u32) -> P,
    ) -> Result<ScanWindow<P>, RpcError> {
        let window_start = boundary(window.start)?;
        let start_of_tx = |tx_seq| position(tx_seq, 0);
        if window.is_empty() {
            let edge = start_of_tx(window_start);
            return Ok(ScanWindow::whole_checkpoints(edge..edge));
        }
        let window_end = boundary(window.end)?;

        // A non-empty window starts at the requested `start_checkpoint`. A transaction index past
        // that checkpoint's end starts at the next checkpoint's first transaction, and drops the
        // event index, which names an event of a transaction past the checkpoint.
        let start = match self.start {
            None => start_of_tx(window_start),
            Some(start) => {
                let next_checkpoint = boundary(window.start + 1)?;
                let tx_seq = window_start.saturating_add(start.transaction_index);
                if tx_seq < next_checkpoint {
                    position(tx_seq, start.event_index)
                } else {
                    start_of_tx(next_checkpoint)
                }
            }
        };

        // The end position applies only while the window still ends at the requested
        // `end_checkpoint`: an `end_checkpoint` past this server's tip leaves the position's
        // checkpoint unindexed, so the window ends at the tip instead. A transaction index past
        // the last checkpoint's end ends after that whole checkpoint.
        let end = match self.end.filter(|_| self.end_checkpoint == Some(window.end)) {
            None => start_of_tx(window_end),
            Some(end) => {
                let tx_seq = boundary(window.end - 1)?.saturating_add(end.transaction_index);
                if tx_seq < window_end {
                    position(tx_seq, end.event_index)
                } else {
                    start_of_tx(window_end)
                }
            }
        };

        Ok(ScanWindow {
            start_cuts_checkpoint: start != start_of_tx(window_start),
            end_cuts_checkpoint: end != start_of_tx(window_end),
            range: start..end,
        })
    }
}

impl PositionInCheckpoint {
    /// The position a request's index fields name, or `None` when its transaction index is
    /// unset. An unset event index is the gap before the whole transaction.
    fn from_fields(transaction_index: Option<u64>, event_index: Option<u32>) -> Option<Self> {
        transaction_index.map(|transaction_index| Self {
            transaction_index,
            event_index: event_index.unwrap_or(0),
        })
    }
}

/// `InvalidArgument` when `when_set` is set but `required` is not, for a field that only means
/// something together with another: an index with the checkpoint or transaction it indexes into.
fn require_field_when_set<R, W>(
    required: Option<R>,
    required_name: &'static str,
    when_set: Option<W>,
    when_set_name: &'static str,
) -> Result<(), RpcError> {
    if when_set.is_some() && required.is_none() {
        return Err(FieldViolation::new(required_name)
            .with_description(format!(
                "{required_name} is required when {when_set_name} is set"
            ))
            .with_reason(ErrorReason::FieldMissing)
            .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use sui_rpc::proto::sui::rpc::v2::Ordering as ProtoOrdering;
    use sui_rpc::proto::sui::rpc::v2::QueryOptions as ProtoQueryOptions;
    use sui_rpc_cursor::CursorToken;
    use sui_rpc_cursor::Position;

    use crate::ledger_history::query_options::Ordering;
    use crate::ledger_history::query_options::QueryOptions;
    use crate::ledger_history::query_options::RangeExhaustion;
    use crate::ledger_history::query_options::ResolvedCheckpointRange;
    use crate::ledger_history::query_options::ResolvedScan;

    use super::*;

    /// The model ledger's first transaction of each checkpoint, and the tip. Checkpoint 0 holds
    /// transactions 0..3, checkpoint 1 none, checkpoint 2 3..7, checkpoint 3 7..9, and checkpoint
    /// 4 9..14.
    const BOUNDARIES: [u64; 6] = [0, 3, 3, 7, 9, 14];
    const TIP: u64 = 5;

    const ORDERINGS: [Ordering; 2] = [Ordering::Ascending, Ordering::Descending];

    fn boundary(checkpoint: u64) -> Result<u64, RpcError> {
        Ok(BOUNDARIES[checkpoint as usize])
    }

    /// A request over checkpoints `[start, end)` with the given start and end positions, each a
    /// `(transaction_index, event_index)`.
    fn events_request(
        start: Option<u64>,
        end: Option<u64>,
        start_position: Option<(u64, Option<u32>)>,
        end_position: Option<(u64, Option<u32>)>,
    ) -> ListEventsRequest {
        let mut request = ListEventsRequest::default();
        request.start_checkpoint = start;
        request.end_checkpoint = end;
        request.start_transaction_index = start_position.map(|(tx, _)| tx);
        request.start_event_index = start_position.and_then(|(_, event)| event);
        request.end_transaction_index = end_position.map(|(tx, _)| tx);
        request.end_event_index = end_position.and_then(|(_, event)| event);
        request
    }

    fn transactions_request(
        start: Option<u64>,
        end: Option<u64>,
        start_transaction_index: Option<u64>,
        end_transaction_index: Option<u64>,
    ) -> ListTransactionsRequest {
        let mut request = ListTransactionsRequest::default();
        request.start_checkpoint = start;
        request.end_checkpoint = end;
        request.start_transaction_index = start_transaction_index;
        request.end_transaction_index = end_transaction_index;
        request
    }

    fn options(
        ordering: Ordering,
        after: Option<Position>,
        before: Option<Position>,
    ) -> QueryOptions {
        let cursor = |position| CursorToken::boundary(position).encode();
        let mut proto = ProtoQueryOptions::default();
        if ordering == Ordering::Descending {
            proto.ordering = Some(ProtoOrdering::Descending as i32);
        }
        proto.after = after.map(cursor);
        proto.before = before.map(cursor);
        QueryOptions::transactions_from_proto(Some(&proto), 100, 100).unwrap()
    }

    fn tx_position(tx_seq: u64) -> Position {
        Position::Transactions { tx_seq }
    }

    fn resolve_transactions(
        request: &ListTransactionsRequest,
        options: &QueryOptions,
    ) -> ResolvedScan<u64> {
        let positions = CheckpointPositions::from_transactions_request(request).unwrap();
        let cp_range = ResolvedCheckpointRange::from_request(
            request.start_checkpoint,
            request.end_checkpoint,
            TIP,
            options,
        )
        .unwrap();
        let window = positions
            .transaction_window(&cp_range.range, boundary)
            .unwrap();
        ResolvedScan::resolve(cp_range, window, options).unwrap()
    }

    fn event_window(request: &ListEventsRequest) -> ScanWindow<IntraTxCoordinate> {
        let positions = CheckpointPositions::from_events_request(request).unwrap();
        let window = request.start_checkpoint.unwrap_or(0)..request.end_checkpoint.unwrap_or(TIP);
        positions.event_window(&window, boundary).unwrap()
    }

    fn event(tx_seq: u64, index: u32) -> IntraTxCoordinate {
        IntraTxCoordinate { tx_seq, index }
    }

    fn is_invalid_argument(error: RpcError) -> bool {
        error.into_status_proto().code == tonic::Code::InvalidArgument as i32
    }

    /// What a resolved scan ends with: `(range, terminal position, end reason, natural end
    /// claim)`.
    fn terminal(
        resolved: &ResolvedScan<u64>,
        options: &QueryOptions,
    ) -> (Range<u64>, u64, RangeExhaustion, Option<u64>) {
        (
            resolved.range(),
            resolved.end_position,
            resolved.exhaustion,
            resolved.natural_end_claim(options),
        )
    }

    const CHECKPOINT_BOUND: RangeExhaustion = RangeExhaustion::CheckpointBound;
    const CURSOR_BOUND: RangeExhaustion = RangeExhaustion::CursorBound {
        kind: sui_rpc_cursor::CursorKind::Boundary,
    };

    #[test]
    fn unset_and_zero_start_indexes_scan_whole_checkpoints() {
        for ordering in ORDERINGS {
            let options = options(ordering, None, None);
            let unset = transactions_request(Some(2), Some(4), None, None);
            let zero = transactions_request(Some(2), Some(4), Some(0), None);
            let expected = match ordering {
                Ordering::Ascending => (3..9, 9, CHECKPOINT_BOUND, Some(3)),
                Ordering::Descending => (3..9, 3, CHECKPOINT_BOUND, Some(2)),
            };
            assert_eq!(
                terminal(&resolve_transactions(&unset, &options), &options),
                expected
            );
            assert_eq!(
                terminal(&resolve_transactions(&zero, &options), &options),
                expected
            );
        }
    }

    /// A start position inside the first checkpoint narrows the scan. Ascending, it vouches for
    /// the positions before it, so the scan still claims that checkpoint; descending, the scan
    /// never covers that checkpoint, so its end claims the one above.
    #[test]
    fn start_position_inside_checkpoint() {
        let request = transactions_request(Some(2), Some(4), Some(1), None);
        let ascending = options(Ordering::Ascending, None, None);
        assert_eq!(
            terminal(&resolve_transactions(&request, &ascending), &ascending),
            (4..9, 9, CHECKPOINT_BOUND, Some(3))
        );
        let descending = options(Ordering::Descending, None, None);
        assert_eq!(
            terminal(&resolve_transactions(&request, &descending), &descending),
            (4..9, 4, CHECKPOINT_BOUND, Some(3))
        );
    }

    /// The end position lies in `end_checkpoint - 1`, and an ascending scan that stops inside it
    /// does not claim it. Index 0 excludes that whole checkpoint.
    #[test]
    fn end_position_inside_last_checkpoint() {
        let ascending = options(Ordering::Ascending, None, None);
        let descending = options(Ordering::Descending, None, None);
        let request = transactions_request(Some(2), Some(4), None, Some(1));
        assert_eq!(
            terminal(&resolve_transactions(&request, &ascending), &ascending),
            (3..8, 8, CHECKPOINT_BOUND, Some(2))
        );
        assert_eq!(
            terminal(&resolve_transactions(&request, &descending), &descending),
            (3..8, 3, CHECKPOINT_BOUND, Some(2))
        );

        let request = transactions_request(Some(2), Some(4), None, Some(0));
        assert_eq!(
            terminal(&resolve_transactions(&request, &ascending), &ascending),
            (3..7, 7, CHECKPOINT_BOUND, Some(2))
        );
    }

    /// Indexes at or past the end of their checkpoint clamp to its end: a start moves to the next
    /// checkpoint, and an end covers the whole checkpoint.
    #[test]
    fn indexes_past_checkpoint_end_clamp() {
        for (start_index, end_index) in [(4, 2), (u64::MAX, u64::MAX)] {
            let request =
                transactions_request(Some(2), Some(4), Some(start_index), Some(end_index));
            let ascending = options(Ordering::Ascending, None, None);
            assert_eq!(
                terminal(&resolve_transactions(&request, &ascending), &ascending),
                (7..9, 9, CHECKPOINT_BOUND, Some(3))
            );
            let descending = options(Ordering::Descending, None, None);
            assert_eq!(
                terminal(&resolve_transactions(&request, &descending), &descending),
                (7..9, 7, CHECKPOINT_BOUND, Some(3))
            );
        }
    }

    #[test]
    fn event_positions() {
        // Mid-checkpoint start and end, with event indexes.
        let window = event_window(&events_request(
            Some(2),
            Some(4),
            Some((1, Some(2))),
            Some((0, Some(3))),
        ));
        assert_eq!(window.range, event(4, 2)..event(7, 3));

        // An event index past its transaction's events needs no clamp: the end admits all of
        // that transaction's events.
        let window = event_window(&events_request(Some(2), Some(4), None, Some((1, Some(9)))));
        assert_eq!(window.range, event(3, 0)..event(8, 9));

        // A transaction index past the checkpoint drops the event index: the start is the next
        // checkpoint's first event, and the end covers the whole checkpoint.
        let window = event_window(&events_request(
            Some(2),
            Some(4),
            Some((4, Some(5))),
            Some((2, Some(1))),
        ));
        assert_eq!(window.range, event(7, 0)..event(9, 0));
        assert!(window.start_cuts_checkpoint);
        assert!(!window.end_cuts_checkpoint);

        // In an empty checkpoint, every start position is its end, which is also its start.
        let window = event_window(&events_request(Some(1), Some(3), Some((0, Some(2))), None));
        assert_eq!(window.range, event(3, 0)..event(7, 0));
        assert!(!window.start_cuts_checkpoint);
    }

    /// A start position past the end position leaves nothing to scan, and claims nothing.
    #[test]
    fn crossed_positions_scan_nothing() {
        let request = transactions_request(Some(3), Some(4), Some(1), Some(0));
        let ascending = options(Ordering::Ascending, None, None);
        assert_eq!(
            terminal(&resolve_transactions(&request, &ascending), &ascending),
            (7..7, 7, CHECKPOINT_BOUND, None)
        );
        let descending = options(Ordering::Descending, None, None);
        assert_eq!(
            terminal(&resolve_transactions(&request, &descending), &descending),
            (8..8, 8, CHECKPOINT_BOUND, None)
        );
    }

    /// An `end_checkpoint` past the tip leaves its last checkpoint unindexed: the scan ends at
    /// the tip, ignoring the end position.
    #[test]
    fn end_position_past_the_tip_does_not_apply() {
        let request = transactions_request(Some(3), Some(TIP + 1), None, Some(0));
        let ascending = options(Ordering::Ascending, None, None);
        assert_eq!(
            terminal(&resolve_transactions(&request, &ascending), &ascending),
            (7..14, 14, RangeExhaustion::LedgerTip, Some(4))
        );
    }

    /// Positions and cursors bound the scan by intersection: the later start and the earlier end
    /// win, and the end's reason and cursor follow whichever ends it.
    #[test]
    fn positions_intersect_cursors() {
        // Scans transactions 4..8.
        let request = transactions_request(Some(2), Some(4), Some(1), Some(1));
        let cases = [
            (
                options(Ordering::Ascending, Some(tx_position(6)), None),
                (6..8, 8, CHECKPOINT_BOUND),
            ),
            (
                options(Ordering::Ascending, Some(tx_position(2)), None),
                (4..8, 8, CHECKPOINT_BOUND),
            ),
            (
                options(Ordering::Ascending, None, Some(tx_position(5))),
                (4..5, 5, CURSOR_BOUND),
            ),
            (
                options(Ordering::Ascending, None, Some(tx_position(9))),
                (4..8, 8, CHECKPOINT_BOUND),
            ),
            (
                options(Ordering::Descending, None, Some(tx_position(6))),
                (4..6, 4, CHECKPOINT_BOUND),
            ),
            (
                options(Ordering::Descending, Some(tx_position(5)), None),
                (5..8, 5, CURSOR_BOUND),
            ),
        ];
        for (options, expected) in cases {
            let resolved = resolve_transactions(&request, &options);
            assert_eq!(
                (resolved.range(), resolved.end_position, resolved.exhaustion),
                expected,
                "{options:?}"
            );
        }
    }

    #[test]
    fn index_without_its_bound_is_invalid_argument() {
        let transactions = [
            transactions_request(None, Some(4), Some(1), None),
            transactions_request(Some(2), None, None, Some(1)),
        ];
        for request in transactions {
            let error = CheckpointPositions::from_transactions_request(&request).unwrap_err();
            assert!(is_invalid_argument(error), "{request:?}");
        }

        let mut start_event_alone = events_request(Some(2), Some(4), None, None);
        start_event_alone.start_event_index = Some(1);
        let mut end_event_alone = events_request(Some(2), Some(4), None, None);
        end_event_alone.end_event_index = Some(1);
        let events = [
            events_request(None, Some(4), Some((1, None)), None),
            events_request(Some(2), None, None, Some((1, Some(0)))),
            start_event_alone,
            end_event_alone,
        ];
        for request in events {
            let error = CheckpointPositions::from_events_request(&request).unwrap_err();
            assert!(is_invalid_argument(error), "{request:?}");
        }
    }
}
