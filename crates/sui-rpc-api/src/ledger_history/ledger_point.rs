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
//!
//! [`Position::public_position`]: crate::ledger_history::cursor::Position::public_position

use sui_inverted_index::ScanDirection;

use crate::ledger_history::cursor::LedgerPoint;
use crate::ledger_history::query_options::IntraTxCoordinate;

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
