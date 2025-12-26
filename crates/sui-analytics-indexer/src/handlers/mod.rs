// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Handlers for the analytics indexer.
//!
//! This module contains:
//! - `handler`: The default analytics handler for writing to object stores
//! - `tables`: Table-specific processors for each analytics pipeline

use crate::metrics::Metrics;

pub mod handler;
pub mod tables;

pub use handler::{AnalyticsHandler, Batch, CheckpointRows, Row};

/// Record file size metrics for a pipeline.
pub fn record_file_metrics(metrics: &Metrics, pipeline_name: &str, size: usize) {
    metrics
        .file_size_bytes
        .with_label_values(&[pipeline_name])
        .observe(size as f64);
}
