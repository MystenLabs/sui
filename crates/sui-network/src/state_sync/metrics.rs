// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::histogram::Histogram;
use prometheus::{register_int_gauge_with_registry, IntGauge, Registry};
use std::sync::Arc;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tap::Pipe;

#[derive(Clone)]
pub(super) struct Metrics(Option<Arc<Inner>>);

impl std::fmt::Debug for Metrics {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        fmt.debug_struct("Metrics").finish()
    }
}

impl Metrics {
    pub fn enabled(registry: &Registry) -> Self {
        Metrics(Some(Inner::new(registry)))
    }

    pub fn disabled() -> Self {
        Metrics(None)
    }

    pub fn set_highest_known_checkpoint(&self, sequence_number: CheckpointSequenceNumber) {
        if let Some(inner) = &self.0 {
            inner.highest_known_checkpoint.set(sequence_number as i64);
        }
    }

    pub fn set_highest_verified_checkpoint(&self, sequence_number: CheckpointSequenceNumber) {
        if let Some(inner) = &self.0 {
            inner
                .highest_verified_checkpoint
                .set(sequence_number as i64);
        }
    }

    pub fn set_highest_synced_checkpoint(&self, sequence_number: CheckpointSequenceNumber) {
        if let Some(inner) = &self.0 {
            inner.highest_synced_checkpoint.set(sequence_number as i64);
        }
    }

    pub fn checkpoint_summary_age_metric(&self) -> Option<&Histogram> {
        if let Some(inner) = &self.0 {
            return Some(&inner.checkpoint_summary_age_ms);
        }
        None
    }
}

struct Inner {
    highest_known_checkpoint: IntGauge,
    highest_verified_checkpoint: IntGauge,
    highest_synced_checkpoint: IntGauge,
    checkpoint_summary_age_ms: Histogram,
}

impl Inner {
    pub fn new(registry: &Registry) -> Arc<Self> {
        Self {
            highest_known_checkpoint: register_int_gauge_with_registry!(
                "highest_known_checkpoint",
                "Highest known checkpoint",
                registry
            )
            .unwrap(),

            highest_verified_checkpoint: register_int_gauge_with_registry!(
                "highest_verified_checkpoint",
                "Highest verified checkpoint",
                registry
            )
            .unwrap(),

            highest_synced_checkpoint: register_int_gauge_with_registry!(
                "highest_synced_checkpoint",
                "Highest synced checkpoint",
                registry
            )
            .unwrap(),

            checkpoint_summary_age_ms: Histogram::new_in_registry(
                "checkpoint_summary_age_ms",
                "Age of checkpoints summaries when they arrive and are verified.",
                registry,
            ),
        }
        .pipe(Arc::new)
    }
}
