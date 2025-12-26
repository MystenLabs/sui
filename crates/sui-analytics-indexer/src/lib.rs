// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Analytics indexer for Sui blockchain data.
//!
//! This crate provides an analytics indexer that processes Sui blockchain data
//! and writes it to columnar formats (CSV, Parquet) for analytics workloads.

pub mod config;
pub mod handlers;
pub mod indexer;
pub mod metrics;
pub mod package_store;
pub mod pipeline;
pub mod progress_monitoring;
pub mod schema;
pub mod store;
pub mod tables;
mod writers;

// Re-exports for public API
pub use config::{BatchSizeConfig, FileFormat, IndexerConfig, OutputStoreConfig, PipelineConfig};
pub use handlers::{AnalyticsHandler, Batch, Row};
pub use indexer::build_analytics_indexer;
pub use pipeline::Pipeline;
pub use progress_monitoring::MaxCheckpointReader;
pub use schema::{ColumnValue, RowSchema};
pub use store::AnalyticsStore;
pub use store::{FileRangeEntry, FileRangeIndex};
