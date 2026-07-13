// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_rpc::proto::sui::rpc::v2::QueryEndReason;

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

    use super::*;

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
}
