// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Public wire-compatible types used by the minimal control HTTP API.

use serde::{Deserialize, Serialize};

/// Status response for the local forking runtime.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForkingStatus {
    /// Latest locally produced checkpoint sequence number.
    pub checkpoint: u64,
    /// Current local epoch.
    pub epoch: u64,
    /// Current simulated on-chain time (seconds since Unix epoch).
    pub clock_timestamp_ms: u64,
}

/// Generic envelope returned by control API endpoints.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}
