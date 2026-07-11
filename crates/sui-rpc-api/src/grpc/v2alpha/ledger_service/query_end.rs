// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub(super) use crate::ledger_history::watermark::terminal_watermark;
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
    use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
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
    fn terminal_watermark_selects_natural_scan_and_cursor_candidates_without_deduplication() {
        let options = options();
        let position = Position::Transactions {
            checkpoint: 9,
            tx_seq: 4,
        };
        let natural = terminal_watermark(
            &options,
            position,
            None,
            QueryEndReason::CheckpointBound,
            None,
        )
        .expect("natural completion has a boundary watermark");
        assert_eq!(natural.checkpoint, Some(8));
        assert_eq!(
            natural.cursor,
            Some(CursorToken::boundary(position).encode())
        );

        let mut frontier = Watermark::default();
        frontier.checkpoint = Some(7);
        frontier.cursor = Some("scan-frontier".into());
        assert_eq!(
            terminal_watermark(
                &options,
                position,
                Some(frontier.clone()),
                QueryEndReason::ScanLimit,
                None,
            ),
            Some(frontier.clone())
        );
        assert_eq!(
            terminal_watermark(
                &options,
                position,
                Some(frontier.clone()),
                QueryEndReason::ScanLimit,
                Some(999),
            ),
            Some(frontier)
        );

        let cursor_bound = terminal_watermark(
            &options,
            position,
            None,
            QueryEndReason::CursorBound,
            Some(6),
        )
        .expect("cursor completion has a boundary watermark");
        assert_eq!(cursor_bound.checkpoint, Some(6));
        assert_eq!(
            cursor_bound.cursor,
            Some(CursorToken::boundary(position).encode())
        );
    }
}
