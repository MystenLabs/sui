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
pub mod progress;
pub mod schema;
pub mod tables;
pub mod writers;

// Re-exports for public API
pub use config::{FileFormat, IndexerConfig, OutputStoreConfig, PipelineConfig};
pub use handlers::{
    AnalyticsBatch, AnalyticsHandler, AnalyticsMetadata, BackfillBoundaries, BackfillHandler,
};
pub use indexer::build_analytics_indexer;
pub use pipeline::Pipeline;
pub use progress::{MaxCheckpointReader, spawn_snowflake_monitors};
pub use schema::{ColumnValue, RowSchema};
