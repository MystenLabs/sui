// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Once;
use std::time::Duration;
use std::time::Instant;

use tracing::{info, warn};

use crate::{get_metrics, spawn_logged_monitored_task};

static THREAD_STALL_MONITOR: Once = Once::new();

const MONITOR_INTERVAL: Duration = Duration::from_millis(50);
const ALERT_THRESHOLD: Duration = Duration::from_millis(500);

// These funcs are extern in order to be easily findable by debuggers
// To catch a thread stall in the act, do the following:
//
// Create a file `gdbcmd` with
//
//      set logging file gdb.txt
//      set logging on
//      set pagination off
//      set breakpoint pending on
//
//      b thread_monitor_report_stall
//      commands
//      thread apply all bt
//      continue
//      end
//
// Then run gdb with:
//     gdb -x gdbmcmd -p <pid of sui-node>
//
// You will need to type `c` to continue the process after it loads.
//
// The debugger will now print out all thread stacks every time a thread stall is detected.
#[inline(never)]
extern "C" fn thread_monitor_report_stall(duration_ms: u64) {
    warn!("Thread stalled for {}ms", duration_ms);
}

#[inline(never)]
extern "C" fn thread_monitor_report_stall_cleared(duration_ms: u64) {
    warn!("Thread stall cleared after {}ms", duration_ms);
}

/// Monitors temporary stalls in tokio scheduling every MONITOR_INTERVAL.
/// Calls `thread_monitor_report_stall` if more than ALERT_THRESHOLD has elapsed.
/// When the stall clears, we observer the duration in a histogram.
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

    let last_update: Arc<Mutex<Instant>> = Arc::new(Mutex::new(Instant::now()));

    {
        let last_update = last_update.clone();
        std::thread::spawn(move || {
            info!("Starting thread stall monitor watchdog thread");
            let mut stall_duration = None;

            loop {
                std::thread::sleep(MONITOR_INTERVAL);
                let now = Instant::now();
                let last_update = *last_update.lock().unwrap();
                let time_since_last_update = now - last_update;
                if time_since_last_update > ALERT_THRESHOLD {
                    if stall_duration.is_none() {
                        thread_monitor_report_stall(time_since_last_update.as_millis() as u64);
                    }
                    stall_duration = Some(time_since_last_update);
                } else if let Some(dur) = stall_duration {
                    stall_duration = None;
                    thread_monitor_report_stall_cleared(dur.as_millis() as u64);
                    if let Some(metrics) = get_metrics() {
                        metrics.thread_stall_duration_sec.observe(dur.as_secs_f64());
                    }
                }
            }
        });
    }

    spawn_logged_monitored_task!(
        async move {
            info!("Starting thread stall monitor update task");
            loop {
                tokio::time::sleep(MONITOR_INTERVAL).await;
                *last_update.lock().unwrap() = Instant::now();
            }
        },
        "ThreadStallMonitor"
    );
}
