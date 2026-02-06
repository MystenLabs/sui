// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::HistogramOpts;
use prometheus::HistogramVec;
use prometheus::IntCounterVec;
use prometheus::IntGaugeVec;
use prometheus::Registry;
use prometheus::register_histogram_vec_with_registry;
use prometheus::register_int_counter_vec_with_registry;
use prometheus::register_int_gauge_vec_with_registry;

#[derive(Clone)]
pub struct Metrics {
    pub max_checkpoint_on_store: IntGaugeVec,
    pub total_too_large_to_deserialize: IntCounterVec,
    pub file_size_bytes: HistogramVec,
    /// Latest uploaded checkpoint (inclusive) per pipeline.
    pub latest_uploaded_checkpoint: IntGaugeVec,
    /// Latest uploaded epoch per pipeline.
    pub latest_uploaded_epoch: IntGaugeVec,
}

impl Metrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            max_checkpoint_on_store: register_int_gauge_vec_with_registry!(
                "max_checkpoint_on_store",
                "Max checkpoint on the db table.",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            total_too_large_to_deserialize: register_int_counter_vec_with_registry!(
                "total_too_large_to_deserialize",
                "Total number of rows skipped due to size.",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            file_size_bytes: register_histogram_vec_with_registry!(
                HistogramOpts::new("file_size_bytes", "Size of generated files in bytes.",)
                    .buckets(vec![
                        1_000.0,         // 1 KB
                        10_000.0,        // 10 KB
                        100_000.0,       // 100 KB
                        1_000_000.0,     // 1 MB
                        10_000_000.0,    // 10 MB
                        50_000_000.0,    // 50 MB
                        100_000_000.0,   // 100 MB
                        250_000_000.0,   // 250 MB
                        500_000_000.0,   // 500 MB
                        1_000_000_000.0, // 1 GB
                        2_000_000_000.0, // 2 GB
                    ]),
                &["pipeline"],
                registry,
            )
            .unwrap(),
            latest_uploaded_checkpoint: register_int_gauge_vec_with_registry!(
                "latest_uploaded_checkpoint",
                "Latest checkpoint uploaded to object store (inclusive).",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            latest_uploaded_epoch: register_int_gauge_vec_with_registry!(
                "latest_uploaded_epoch",
                "Latest epoch uploaded to object store.",
                &["pipeline"],
                registry,
            )
            .unwrap(),
        }
    }
}
