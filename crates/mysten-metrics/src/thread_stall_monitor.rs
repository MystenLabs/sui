// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Once;

use tracing::{error, info};

use crate::{get_metrics, spawn_logged_monitored_task};

static THREAD_STALL_MONITOR: Once = Once::new();

const MONITOR_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

/// Monitors temporary stalls in tokio scheduling every MONITOR_INTERVAL.
/// Logs an error and increments a metric if more than 2 * MONITOR_INTERVAL has elapsed,
/// which means the stall lasted longer than MONITOR_INTERVAL.
pub fn start_thread_stall_monitor() {
    let mut called = true;
    THREAD_STALL_MONITOR.call_once(|| {
        called = false;
    });
    if called {
        return;
    }
    if tokio::runtime::Handle::try_current().is_err() {
        info!("Not running in a tokio runtime, not starting thread stall monitor.");
        return;
    }

    spawn_logged_monitored_task!(
        async move {
            let Some(metrics) = get_metrics() else {
                info!("Metrics uninitialized, not starting thread stall monitor.");
                return;
            };
            let mut last_sleep_time = tokio::time::Instant::now();
            loop {
                tokio::time::sleep(MONITOR_INTERVAL).await;
                let current_time = tokio::time::Instant::now();
                let stalled_duration = current_time - last_sleep_time - MONITOR_INTERVAL;
                last_sleep_time = current_time;
                if stalled_duration > MONITOR_INTERVAL {
                    metrics
                        .thread_stall_duration_sec
                        .observe(stalled_duration.as_secs_f64());
                    // TODO: disable this in simulation tests with artificial thread stalls?
                    error!(
                        "Thread stalled for {}s. Possible causes include CPU overload or too much blocking calls.",
                        stalled_duration.as_secs_f64()
                    );
                }
            }
        },
        "ThreadStallMonitor"
    );
}
