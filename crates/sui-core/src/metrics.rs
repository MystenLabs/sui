// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Histogram;
use tokio::time::Instant;

/// Increment an IntGauge metric, and decrement it when the scope ends.
/// metrics must be an Arc containing a struct containing the field $field.
#[macro_export]
macro_rules! scoped_counter {
    ($metrics: expr, $field: ident) => {{
        let metrics = $metrics.clone();
        metrics.$field.inc();
        ::scopeguard::guard(metrics, |metrics| {
            metrics.$field.dec();
        })
    }};
}

pub fn start_timer(metrics: Histogram) -> impl Drop {
    let start_ts = Instant::now();
    scopeguard::guard((metrics, start_ts), |(metrics, start_ts)| {
        metrics.observe(start_ts.elapsed().as_secs_f64());
    })
}
