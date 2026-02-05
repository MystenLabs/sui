// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO(migration): Remove this module once GraphQL reads per-pipeline watermarks.

use std::collections::HashMap;

use crate::ALL_PIPELINE_NAMES;

/// Tracks per-pipeline watermarks and computes a min across all pipelines.
/// Used to dual-write the legacy `[0]` row in `watermark_alt` so old GraphQL
/// binaries that read only that row continue to work.
pub(crate) struct LegacyWatermarkTracker {
    watermarks: HashMap<String, u64>,
    last_written: Option<u64>,
}

impl LegacyWatermarkTracker {
    pub fn new() -> Self {
        Self {
            watermarks: HashMap::with_capacity(ALL_PIPELINE_NAMES.len()),
            last_written: None,
        }
    }

    /// Record a pipeline's latest checkpoint_hi_inclusive.
    /// Returns `Some((min, prev_last_written))` when all pipelines have reported
    /// AND the min has advanced past `last_written`. Eagerly sets `last_written`
    /// so concurrent callers don't redundantly attempt the same write. Pass
    /// `prev_last_written` to `rollback` if the write fails.
    pub fn update(&mut self, pipeline: &str, checkpoint_hi: u64) -> Option<(u64, Option<u64>)> {
        self.watermarks.insert(pipeline.to_owned(), checkpoint_hi);

        if self.watermarks.len() < ALL_PIPELINE_NAMES.len() {
            return None;
        }

        let min = *self.watermarks.values().min()?;

        if self.last_written.is_some_and(|prev| min <= prev) {
            return None;
        }

        let prev = self.last_written;
        self.last_written = Some(min);
        Some((min, prev))
    }

    /// Roll back after a failed write, but only if no other caller has since
    /// advanced `last_written` past the failed value.
    pub fn rollback(&mut self, failed_min: u64, prev: Option<u64>) {
        if self.last_written == Some(failed_min) {
            self.last_written = prev;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_all_reported() {
        let mut tracker = LegacyWatermarkTracker::new();
        // Only 2 of 5 pipelines reported — should not fire.
        assert_eq!(tracker.update("a", 10), None);
        assert_eq!(tracker.update("b", 20), None);
    }

    #[test]
    fn all_reported_returns_min() {
        let mut tracker = LegacyWatermarkTracker::new();
        for (i, name) in ALL_PIPELINE_NAMES.iter().enumerate() {
            let result = tracker.update(name, (i as u64 + 1) * 10);
            if i < ALL_PIPELINE_NAMES.len() - 1 {
                assert_eq!(result, None);
            } else {
                // Last pipeline reports — min is 10 (first pipeline).
                assert_eq!(result, Some((10, None)));
            }
        }
    }

    #[test]
    fn unchanged_min_returns_none() {
        let mut tracker = LegacyWatermarkTracker::new();
        for (i, name) in ALL_PIPELINE_NAMES.iter().enumerate() {
            tracker.update(name, (i as u64 + 1) * 10);
        }
        // last_written is already 10, non-min pipeline advances — no change.
        assert_eq!(tracker.update(ALL_PIPELINE_NAMES[1], 100), None);
    }

    #[test]
    fn advanced_min() {
        let mut tracker = LegacyWatermarkTracker::new();
        for (i, name) in ALL_PIPELINE_NAMES.iter().enumerate() {
            tracker.update(name, (i as u64 + 1) * 10);
        }
        // Advance the min pipeline (index 0) from 10 to 25. prev was 10.
        assert_eq!(
            tracker.update(ALL_PIPELINE_NAMES[0], 25),
            Some((20, Some(10)))
        );
    }

    #[test]
    fn rollback_allows_retry() {
        let mut tracker = LegacyWatermarkTracker::new();
        for (i, name) in ALL_PIPELINE_NAMES.iter().enumerate() {
            tracker.update(name, (i as u64 + 1) * 10);
        }
        // last_written is now Some(10). Simulate failed write: roll back to prev (None).
        tracker.rollback(10, None);
        // Next update should return Some(10) again.
        assert_eq!(tracker.update(ALL_PIPELINE_NAMES[1], 100), Some((10, None)));
    }

    #[test]
    fn rollback_noop_if_already_advanced() {
        let mut tracker = LegacyWatermarkTracker::new();
        for (i, name) in ALL_PIPELINE_NAMES.iter().enumerate() {
            tracker.update(name, (i as u64 + 1) * 10);
        }
        // last_written = 10. Advance min to 20.
        assert_eq!(
            tracker.update(ALL_PIPELINE_NAMES[0], 25),
            Some((20, Some(10)))
        );
        // last_written = 20. Stale rollback for 10 should be ignored.
        tracker.rollback(10, None);
        assert_eq!(tracker.last_written, Some(20));
    }
}
