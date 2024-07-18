// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{register_int_counter_with_registry, IntCounter, Registry};

#[derive(Clone)]
pub struct DeepbookIndexerMetrics {
    pub data_ingestion_checkpoint: IntCounter,
}

impl DeepbookIndexerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            data_ingestion_checkpoint: register_int_counter_with_registry!(
                "deepbook_txn_count",
                "Number of deepbook transactions.",
                registry,
            )
            .unwrap(),
        }
    }
}
