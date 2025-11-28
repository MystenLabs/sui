// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Handlers for the analytics indexer.
//!
//! This module contains:
//! - `handler`: The default analytics handler for writing to object stores
//! - `backfill`: Backfill mode handler with boundary alignment
//! - `tables`: Table-specific processors for each analytics pipeline

use std::ops::Range;
use std::path::PathBuf;

use sui_types::base_types::EpochId;

use crate::config::FileFormat;
use crate::metrics::Metrics;

pub mod backfill;
pub mod handler;
pub mod tables;

pub use backfill::{BackfillBoundaries, BackfillHandler, EpochBoundaries};
pub use handler::{AnalyticsBatch, AnalyticsHandler, AnalyticsMetadata};

/// Construct the object store path for an analytics file.
/// Path format: {dir_prefix}/epoch_{epoch}/{start}_{end}.{ext}
pub fn construct_file_path(
    dir_prefix: &str,
    epoch_num: EpochId,
    checkpoint_range: Range<u64>,
    file_format: FileFormat,
) -> PathBuf {
    let extension = match file_format {
        FileFormat::Csv => "csv",
        FileFormat::Parquet => "parquet",
    };
    PathBuf::from(dir_prefix)
        .join(format!("epoch_{}", epoch_num))
        .join(format!(
            "{}_{}.{}",
            checkpoint_range.start, checkpoint_range.end, extension
        ))
}

/// Record file size metrics for a pipeline.
pub fn record_file_metrics(metrics: &Metrics, pipeline_name: &str, size: usize) {
    metrics
        .file_size_bytes
        .with_label_values(&[pipeline_name])
        .observe(size as f64);
}
