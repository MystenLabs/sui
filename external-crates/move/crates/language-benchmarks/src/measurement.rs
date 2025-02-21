// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use criterion::Criterion;
use std::time::Duration;

pub fn wall_time_measurement() -> Criterion {
    Criterion::default()
        .without_plots()
        .noise_threshold(0.20)
        .confidence_level(0.9)
        .warm_up_time(Duration::from_secs(10)) // Warm-up time before measurements start
        .measurement_time(Duration::from_secs(30)) // Measurement time of 30 seconds
}
