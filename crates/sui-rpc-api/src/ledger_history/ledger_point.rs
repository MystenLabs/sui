// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Resume points in public ledger coordinates, the `Watermark.cursor_position` of every list and
//! subscription frame.
//!
//! A [`LedgerPoint`] is the boundary immediately before the item with key `(checkpoint,
//! transaction_index, event_index)`; it orders lexicographically, an unset index counting as 0
//! (see `LedgerPosition` in `query_options.proto`). Cursors name the same boundaries by
//! `tx_seq`, which stays private, and carry the public position of their coordinate
//! ([`Position::public_position`]), so a server places any cursor without a read. A scan
//! frontier is placed by the index row (checkpoint and position in it) of one transaction the
//! scan already reads: [`RowPlacement`] says which one, and how its row places the boundary.
//! Checkpoint boundaries, the request's own fields, and items need no read.

use sui_inverted_index::ScanDirection;
use sui_rpc::proto::sui::rpc::v2::LedgerPosition;

use crate::ledger_history::cursor::LedgerPoint;
use crate::ledger_history::cursor::Position;
use crate::ledger_history::query_options::IntraTxCoordinate;

/// The resume point of a frame carrying the item at cursor `position`, whose public position is
/// the item's key: ascending, the boundary right after the item (the key with its lane's finest
/// index incremented, never carried into the next transaction or checkpoint); descending, the
/// key.
pub fn item_resume(position: &Position, direction: ScanDirection) -> LedgerPoint {
    let key = position.public_position();
    if !direction.is_ascending() {
        return key;
    }
    // The finest index of a real item never reaches its numeric maximum; saturating keeps the
    // impossible edge, which a client's cursor can still name, from wrapping.
    match position {
        Position::Checkpoints { .. } => {
            LedgerPoint::checkpoint_start(key.checkpoint.saturating_add(1))
        }
        Position::Transactions { .. } => {
            LedgerPoint::transaction(key.checkpoint, key.transaction_index.saturating_add(1))
        }
        Position::Events { .. } => LedgerPoint::event(
            key.checkpoint,
            key.transaction_index,
            key.event_index.saturating_add(1),
        ),
    }
}

/// The wire form of `point` for a watermark whose cursor is `position`: only the coordinates of
/// that lane's keys are set.
pub fn ledger_position(point: LedgerPoint, position: &Position) -> LedgerPosition {
    let proto = LedgerPosition::default().with_checkpoint(point.checkpoint);
    match position {
        Position::Checkpoints { .. } => proto,
        Position::Transactions { .. } => proto.with_transaction_index(point.transaction_index),
        Position::Events { .. } => proto
            .with_transaction_index(point.transaction_index)
            .with_event_index(point.event_index),
    }
}

/// Which transaction's index row places a transaction- or event-lane boundary, and how. A scan
/// boundary is placed by the transaction on its covered side: ascending, the last transaction
/// before it (or the transaction it lies inside); descending, the transaction right after it. That
/// transaction lies in the scanned, indexed part of the ledger whenever the scan reached the
/// boundary, including a boundary at the indexed tip.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RowPlacement {
    /// The transaction whose row places the boundary.
    pub tx_seq: u64,
    /// Whether the boundary lies right after that transaction rather than before one of its
    /// events.
    after_transaction: bool,
    event_index: u32,
}

impl RowPlacement {
    /// The placement of the transaction-lane boundary right before `tx_seq`, or `None` for the
    /// ascending boundary at genesis, which is [`LedgerPoint::GENESIS`]. It is the event-lane
    /// boundary before the transaction's first event.
    pub fn transaction_boundary(tx_seq: u64, direction: ScanDirection) -> Option<Self> {
        Self::event_boundary(IntraTxCoordinate::start_of_tx(tx_seq), direction)
    }

    /// The placement of the event-lane boundary right before `position`, or `None` for the
    /// ascending boundary at genesis, which is [`LedgerPoint::GENESIS`].
    pub fn event_boundary(position: IntraTxCoordinate, direction: ScanDirection) -> Option<Self> {
        if direction.is_ascending() && position.index == 0 {
            position.tx_seq.checked_sub(1).map(|tx_seq| Self {
                tx_seq,
                after_transaction: true,
                event_index: 0,
            })
        } else {
            Some(Self {
                tx_seq: position.tx_seq,
                after_transaction: false,
                event_index: position.index,
            })
        }
    }

    /// The boundary, given the checkpoint of [`Self::tx_seq`] and its index in that checkpoint.
    /// A boundary right after a checkpoint's last transaction keeps that checkpoint, with a
    /// transaction index past its end.
    pub fn point(self, checkpoint: u64, transaction_index: u64) -> LedgerPoint {
        if self.after_transaction {
            LedgerPoint::transaction(checkpoint, transaction_index + 1)
        } else {
            LedgerPoint::event(checkpoint, transaction_index, self.event_index)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TX: Position = Position::Transactions {
        tx_seq: 0,
        public_position: LedgerPoint::GENESIS,
    };
    const EVENT: Position = Position::Events {
        tx_seq: 0,
        event_index: 0,
        public_position: LedgerPoint::GENESIS,
    };
    const CHECKPOINT: Position = Position::Checkpoints { checkpoint: 0 };

    #[test]
    fn item_resume_is_above_the_key_ascending_and_the_key_descending() {
        for (item, key, above) in [
            (
                Position::Checkpoints { checkpoint: 4 },
                LedgerPoint::checkpoint_start(4),
                LedgerPoint::checkpoint_start(5),
            ),
            (
                Position::Transactions {
                    tx_seq: 9,
                    public_position: LedgerPoint::transaction(4, 2),
                },
                LedgerPoint::transaction(4, 2),
                LedgerPoint::transaction(4, 3),
            ),
            (
                Position::Events {
                    tx_seq: 9,
                    event_index: 1,
                    public_position: LedgerPoint::event(4, 2, 1),
                },
                LedgerPoint::event(4, 2, 1),
                LedgerPoint::event(4, 2, 2),
            ),
        ] {
            assert_eq!(item_resume(&item, ScanDirection::Ascending), above);
            assert_eq!(item_resume(&item, ScanDirection::Descending), key);
        }
    }

    #[test]
    fn ledger_position_sets_only_the_lane_coordinates() {
        let point = LedgerPoint::event(4, 2, 1);
        let checkpoint = ledger_position(point, &CHECKPOINT);
        assert_eq!(
            (
                checkpoint.checkpoint,
                checkpoint.transaction_index,
                checkpoint.event_index
            ),
            (Some(4), None, None)
        );
        let transaction = ledger_position(point, &TX);
        assert_eq!(
            (
                transaction.checkpoint,
                transaction.transaction_index,
                transaction.event_index
            ),
            (Some(4), Some(2), None)
        );
        let event = ledger_position(point, &EVENT);
        assert_eq!(
            (event.checkpoint, event.transaction_index, event.event_index),
            (Some(4), Some(2), Some(1))
        );
    }

    #[test]
    fn row_placement_reads_the_covered_side() {
        // Ascending, a transaction boundary is placed after the transaction before it.
        let placement = RowPlacement::transaction_boundary(10, ScanDirection::Ascending).unwrap();
        assert_eq!(placement.tx_seq, 9);
        assert_eq!(placement.point(3, 4), LedgerPoint::transaction(3, 5));
        assert_eq!(
            RowPlacement::transaction_boundary(0, ScanDirection::Ascending),
            None
        );

        // Descending, before the transaction at the boundary.
        let placement = RowPlacement::transaction_boundary(10, ScanDirection::Descending).unwrap();
        assert_eq!(placement.tx_seq, 10);
        assert_eq!(placement.point(3, 4), LedgerPoint::transaction(3, 4));

        // An event boundary inside a transaction is placed by that transaction either way.
        let inside = IntraTxCoordinate {
            tx_seq: 10,
            index: 2,
        };
        for direction in [ScanDirection::Ascending, ScanDirection::Descending] {
            let placement = RowPlacement::event_boundary(inside, direction).unwrap();
            assert_eq!(placement.tx_seq, 10);
            assert_eq!(placement.point(3, 4), LedgerPoint::event(3, 4, 2));
        }

        // At a transaction's start, ascending reads the transaction before it.
        let start = IntraTxCoordinate::start_of_tx(10);
        let placement = RowPlacement::event_boundary(start, ScanDirection::Ascending).unwrap();
        assert_eq!(placement.tx_seq, 9);
        assert_eq!(placement.point(3, 4), LedgerPoint::event(3, 5, 0));
        let placement = RowPlacement::event_boundary(start, ScanDirection::Descending).unwrap();
        assert_eq!(placement.tx_seq, 10);
        assert_eq!(placement.point(3, 4), LedgerPoint::event(3, 4, 0));
        assert_eq!(
            RowPlacement::event_boundary(
                IntraTxCoordinate::start_of_tx(0),
                ScanDirection::Ascending
            ),
            None
        );
    }
}
