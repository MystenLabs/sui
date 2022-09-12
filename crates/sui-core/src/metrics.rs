// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{Histogram, IntCounter};
use tokio::time::Instant;

pub fn start_timer(metrics: Histogram) -> impl Drop {
    let start_ts = Instant::now();
    scopeguard::guard((metrics, start_ts), |(metrics, start_ts)| {
        metrics.observe(start_ts.elapsed().as_secs_f64());
    })
}

pub struct TaskUtilizationGuard<'a> {
    metric: &'a IntCounter,
    start: Instant,
}

pub trait TaskUtilizationExt {
    /// Measures amount of time spent until guard is dropped and increments the counter by duration in mcs
    /// Primary usage for this counter is to measure 'utilization' of the single task
    /// E.g. having rate(metric) / 1_000_000 can tell what portion of time this task is busy
    /// For the tasks that are run in single thread this indicates how close is this task to a complete saturation
    fn utilization_timer(&self) -> TaskUtilizationGuard;
}

impl TaskUtilizationExt for IntCounter {
    fn utilization_timer(&self) -> TaskUtilizationGuard {
        TaskUtilizationGuard {
            start: Instant::now(),
            metric: self,
        }
    }
}

impl<'a> Drop for TaskUtilizationGuard<'a> {
    fn drop(&mut self) {
        self.metric.inc_by(self.start.elapsed().as_micros() as u64);
    }
}
