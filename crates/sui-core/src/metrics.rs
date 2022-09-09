// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[macro_export]
macro_rules! start_timer {
    ($metrics: expr, $start_ts: expr) => {{
        let metrics = $metrics;
        let start_ts = $start_ts;
        scopeguard::guard(metrics, |metrics| {
            metrics.observe(start_ts.elapsed().as_secs_f64());
        })
    }};
}
pub use start_timer;
