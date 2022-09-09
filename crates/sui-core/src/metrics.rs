// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Histogram;
use tokio::time::Instant;

pub fn start_timer(metrics: Histogram) -> impl Drop {
    let start_ts = Instant::now();
    scopeguard::guard((metrics, start_ts), |(metrics, start_ts)| {
        metrics.observe(start_ts.elapsed().as_secs_f64());
    })
}
