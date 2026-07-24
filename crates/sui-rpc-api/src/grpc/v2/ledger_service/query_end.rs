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
    use std::collections::BTreeMap;
    use std::time::Instant;

    use prometheus::Registry;

    use crate::metrics::{ListApiMetrics, ListRequestMetrics};

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

    fn assert_query_end_metric(
        produced: usize,
        limit_items: usize,
        scan_end_reason: QueryEndReason,
        expected_reason: QueryEndReason,
        expected_label: &str,
    ) {
        let registry = Registry::new();
        let metrics = ListApiMetrics::new(&registry);
        let handles = metrics.stream_metrics("list_transactions", "digest");
        let mut request_metrics = ListRequestMetrics::new(Some(handles), Instant::now());

        let effective_reason = effective_terminal_reason(produced, limit_items, scan_end_reason);
        assert_eq!(effective_reason, expected_reason);
        request_metrics.finish_success(effective_reason, None);

        let families = registry.gather();
        let query_ends = families
            .iter()
            .find(|family| family.name() == "list_query_ends_total")
            .expect("list_query_ends_total metric family");
        assert_eq!(query_ends.get_metric().len(), 1);
        let metric = &query_ends.get_metric()[0];
        assert_eq!(metric.get_counter().value(), 1.0);
        let labels = metric
            .get_label()
            .iter()
            .map(|label| (label.name(), label.value()))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(labels.get("method"), Some(&"list_transactions"));
        assert_eq!(labels.get("reason"), Some(&expected_label));
    }

    #[test]
    fn query_end_metrics_use_effective_reason_and_item_limit_precedence() {
        assert_query_end_metric(
            3,
            3,
            QueryEndReason::ScanLimit,
            QueryEndReason::ItemLimit,
            "item_limit",
        );
        for (reason, label) in [
            (QueryEndReason::ScanLimit, "scan_limit"),
            (QueryEndReason::LedgerTip, "ledger_tip"),
            (QueryEndReason::CheckpointBound, "checkpoint_bound"),
            (QueryEndReason::CursorBound, "cursor_bound"),
        ] {
            assert_query_end_metric(2, 3, reason, reason, label);
        }
    }
}
