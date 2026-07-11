// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;

/// Effective terminal reason for a successful query stream.
///
/// Hitting the requested item limit takes precedence over the underlying chunk
/// scan result. The caller remains responsible for constructing and attaching
/// `QueryEnd`.
pub(super) fn effective_terminal_reason(
    produced: usize,
    limit_items: usize,
    scan_end_reason: QueryEndReason,
) -> QueryEndReason {
    if produced == limit_items {
        QueryEndReason::ItemLimit
    } else {
        scan_end_reason
    }
}

#[cfg(test)]
mod tests {
    use crate::ledger_history::query_options::QueryOptions;
    use crate::ledger_history::watermark::BoundaryTerminal;
    use crate::ledger_history::watermark::terminal_watermark;
    use sui_rpc_cursor::{CursorToken, Position};

    use super::*;

    fn options() -> QueryOptions {
        QueryOptions::transactions_from_proto(None, 10, 10).unwrap()
    }

    #[test]
    fn item_limit_takes_precedence_without_overriding_earlier_terminal_reasons() {
        assert_eq!(
            effective_terminal_reason(3, 3, QueryEndReason::ScanLimit),
            QueryEndReason::ItemLimit
        );
        assert_eq!(
            effective_terminal_reason(2, 3, QueryEndReason::LedgerTip),
            QueryEndReason::LedgerTip
        );
        assert_eq!(
            effective_terminal_reason(2, 3, QueryEndReason::ScanLimit),
            QueryEndReason::ScanLimit
        );
    }

    #[test]
    fn terminal_watermark_handles_only_range_and_cursor_boundaries() {
        let options = options();
        let position = Position::Transactions {
            checkpoint: 9,
            tx_seq: 4,
        };
        let natural = terminal_watermark(
            &options,
            BoundaryTerminal::RangeEnd {
                reason: QueryEndReason::CheckpointBound,
                position,
            },
            None,
        );
        assert_eq!(natural.checkpoint, Some(8));
        assert_eq!(
            natural.cursor,
            Some(CursorToken::boundary(position).encode())
        );

        let mut cursor_candidate = sui_rpc::proto::sui::rpc::v2alpha::Watermark::default();
        cursor_candidate.checkpoint = Some(7);
        cursor_candidate.cursor = Some(b"cursor-bound".to_vec().into());
        let cursor_bound = terminal_watermark(
            &options,
            BoundaryTerminal::CursorBound {
                position,
                watermark: cursor_candidate,
            },
            Some(6),
        );
        assert_eq!(cursor_bound.checkpoint, Some(6));
        assert_eq!(
            cursor_bound.cursor.as_deref(),
            Some(b"cursor-bound".as_slice())
        );
    }

    #[test]
    #[should_panic(expected = "invalid boundary terminal reason ScanLimit")]
    fn scan_limit_is_not_a_boundary_terminal() {
        BoundaryTerminal::new(
            QueryEndReason::ScanLimit,
            Position::Transactions {
                checkpoint: 9,
                tx_seq: 4,
            },
            None,
        );
    }
}
