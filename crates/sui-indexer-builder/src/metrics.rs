// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{IntCounterVec, IntGaugeVec};

pub trait IndexerMetricProvider: Send + Sync {
    fn get_tasks_latest_retrieved_checkpoints(&self) -> &IntGaugeVec;

    fn get_tasks_remaining_checkpoints_metric(&self) -> &IntGaugeVec;

    fn get_tasks_processed_checkpoints_metric(&self) -> &IntCounterVec;

    fn get_inflight_live_tasks_metrics(&self) -> &IntGaugeVec;

    fn boxed(self) -> Box<dyn IndexerMetricProvider>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}
