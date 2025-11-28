// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Backfill mode support for re-processing analytics data.
//!
//! This module provides:
//! - `BackfillBoundaries`: Pre-loaded map of existing file boundaries
//! - `BackfillHandler`: Handler that aligns batches with existing files

mod boundaries;
mod handler;

pub use boundaries::{BackfillBoundaries, TargetFile};
pub use handler::{BackfillBatch, BackfillHandler};
