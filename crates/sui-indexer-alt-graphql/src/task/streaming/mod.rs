// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Streaming subscription pipeline.
//!
//! # 1. Initialization: filling the store while waiting for readiness
//!
//! gRPC starts streaming at checkpoint C. Packages from checkpoints < C live
//! only in the DB, so subscriptions must wait until kv_packages has indexed
//! them before being served.
//!
//! Meanwhile, checkpoints C, C+1, C+2, ... populate the store:
//!
//! ```text
//!   gRPC stream:   C,   C+1,   C+2,   ...
//!                  ↓     ↓      ↓
//!   StreamingPkgStore  stores packages from each streamed checkpoint
//!
//!   kv_packages_hi:  [.......... must reach ≥ C-1 ..........]
//!                                      │
//!                                      ▼
//!          Subscriptions unblock; start receiving from current tip (C+N)
//! ```
//!
//! # 2. Eviction: draining the store as kv_packages catches up
//!
//! After startup, the stream keeps advancing while kv_packages indexes in the
//! background. Packages in the store at checkpoints ≤ kv_packages_hi are safely
//! in the DB and can be removed. Periodic eviction keeps the store bounded.
//!
//! ```text
//!   gRPC stream:       ..., C+10, C+11, C+12, C+13, C+14, ...
//!   kv_packages_hi:    .........  C+11 ............
//!
//!                               │
//!                               ▼
//!   StreamingPkgStore keeps packages at cp > C+11 (C+12, C+13, C+14, ...)
//!   Packages at cp ≤ C+11 are evicted and served by:
//!     PackageCache (shared, LRU + system-package invalidation) → DB
//! ```

// Only the (staging-gated) subscription resolvers backfill via `scan_checkpoints`; `test` keeps the
// module's own unit tests compiling without the feature.
#[cfg(any(feature = "staging", test))]
mod checkpoint_resume;
mod checkpoint_stream_task;
mod gap_recovery;
mod package_eviction_task;
mod processed_checkpoint;
mod streamed_package_store;
mod subscription_readiness;
#[cfg(test)]
mod test_utils;

use std::sync::Arc;

use sui_indexer_alt_reader::package_resolver::PackageCache;

#[cfg(feature = "staging")]
pub(crate) use checkpoint_stream_task::CheckpointBroadcaster;
pub(crate) use checkpoint_stream_task::CheckpointStreamTask;
#[cfg(feature = "staging")]
pub(crate) use checkpoint_stream_task::SubscriptionBroadcast;
#[cfg(feature = "staging")]
pub(crate) use checkpoint_stream_task::broadcast_error;
#[cfg(feature = "staging")]
pub(crate) use checkpoint_stream_task::hydrate_executed_transaction;
#[cfg(feature = "staging")]
pub(crate) use checkpoint_stream_task::reconnect_error;
pub(crate) use package_eviction_task::PackageEvictionTask;
pub(crate) use processed_checkpoint::ProcessedCheckpoint;
pub(crate) use processed_checkpoint::ProcessedTransaction;
pub(crate) use streamed_package_store::StreamedPackageStore;
pub(crate) use subscription_readiness::SubscriptionReadiness;

/// The full layered package store used by streaming subscriptions:
/// streamed index → shared PackageCache → DB.
pub(crate) type StreamingPackageStore = StreamedPackageStore<Arc<PackageCache>>;
