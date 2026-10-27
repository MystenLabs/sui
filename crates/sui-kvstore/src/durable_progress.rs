// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use tokio::sync::watch;

/// Per pipeline, the highest committer watermark through which this process's data rows are
/// durable.
///
/// An entry is recorded as soon as the rows a committer watermark covers are durable, before
/// the watermark row itself is written. The watermark row only records where a restarted
/// process resumes; if that write fails, loses its compare-and-set to another instance, or never
/// happens because the process crashes, the data rows are still durable, and a restart rewrites
/// them idempotently from the last stored watermark.
///
/// Clones share the same state. Subscribers are notified only when a pipeline's entry
/// advances.
#[derive(Clone)]
pub struct DurableProgress {
    sender: Arc<watch::Sender<HashMap<String, CommitterWatermark>>>,
}

impl DurableProgress {
    pub(crate) fn new() -> Self {
        let (sender, _) = watch::channel(HashMap::new());
        Self {
            sender: Arc::new(sender),
        }
    }

    /// Keeps the watermark with the highest `checkpoint_hi_inclusive` per pipeline; lower or
    /// equal checkpoints are ignored and do not notify subscribers.
    pub(crate) fn record(&self, pipeline: &str, watermark: CommitterWatermark) {
        self.sender.send_if_modified(|watermarks| {
            if let Some(existing) = watermarks.get_mut(pipeline) {
                if watermark.checkpoint_hi_inclusive <= existing.checkpoint_hi_inclusive {
                    return false;
                }
                *existing = watermark;
            } else {
                watermarks.insert(pipeline.to_string(), watermark);
            }
            true
        });
    }

    pub fn subscribe(&self) -> watch::Receiver<HashMap<String, CommitterWatermark>> {
        self.sender.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watermark(checkpoint: u64) -> CommitterWatermark {
        CommitterWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: checkpoint,
            tx_hi: checkpoint * 10,
            timestamp_ms_hi_inclusive: checkpoint * 1_000,
        }
    }

    #[test]
    fn higher_checkpoint_replaces_entry_and_notifies() {
        let progress = DurableProgress::new();
        let mut rx = progress.subscribe();

        progress.record("p", watermark(5));
        assert!(rx.has_changed().unwrap());
        assert_eq!(
            rx.borrow_and_update()["p"].checkpoint_hi_inclusive,
            5,
            "first record inserts the entry",
        );

        progress.record("p", watermark(7));
        assert!(rx.has_changed().unwrap());
        let latest = rx.borrow_and_update()["p"];
        assert_eq!(latest.checkpoint_hi_inclusive, 7);
        assert_eq!(latest.timestamp_ms_hi_inclusive, 7_000);
    }

    #[test]
    fn lower_or_equal_checkpoint_is_ignored_without_notifying() {
        let progress = DurableProgress::new();
        let mut rx = progress.subscribe();

        progress.record("p", watermark(5));
        let _ = rx.borrow_and_update();

        let mut equal = watermark(5);
        equal.timestamp_ms_hi_inclusive = 99_999;
        progress.record("p", equal);
        assert!(!rx.has_changed().unwrap());

        progress.record("p", watermark(3));
        assert!(!rx.has_changed().unwrap());

        let current = rx.borrow()["p"];
        assert_eq!(current.checkpoint_hi_inclusive, 5);
        assert_eq!(current.timestamp_ms_hi_inclusive, 5_000);
    }

    #[test]
    fn pipelines_are_independent() {
        let progress = DurableProgress::new();
        let rx = progress.subscribe();

        progress.record("a", watermark(10));
        progress.record("b", watermark(2));
        // A lower checkpoint for `b` than `a`'s entry still advances `b`.
        progress.record("b", watermark(3));
        // A lower checkpoint for `a` does not regress it.
        progress.record("a", watermark(4));

        let watermarks = rx.borrow();
        assert_eq!(watermarks.len(), 2);
        assert_eq!(watermarks["a"].checkpoint_hi_inclusive, 10);
        assert_eq!(watermarks["b"].checkpoint_hi_inclusive, 3);
    }

    #[test]
    fn record_without_subscribers_is_retained() {
        let progress = DurableProgress::new();
        progress.record("p", watermark(1));
        assert_eq!(
            progress.subscribe().borrow()["p"].checkpoint_hi_inclusive,
            1
        );
    }
}
