// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{register_int_gauge_vec_with_registry, IntGaugeVec, Registry};

#[derive(Clone)]
pub struct DataIngestionMetrics {
    pub last_uploaded_checkpoint: IntGaugeVec,
}

impl DataIngestionMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            last_uploaded_checkpoint: register_int_gauge_vec_with_registry!(
                "last_uploaded_checkpoint",
                "Number of uploaded checkpoints.",
                &["task"],
                registry,
            )
            .unwrap(),
        }
    }
}
