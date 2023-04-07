// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

pub mod traits;
pub use traits::Map;
pub mod metrics;
pub mod rocks;
pub use rocks::TypedStoreError;
pub mod sally;
pub mod test_db;
pub use metrics::DBMetrics;

pub type StoreError = rocks::TypedStoreError;
