// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Public wire-compatible types used by the control HTTP API.

use serde::{Deserialize, Serialize};

/// Request payload for advancing simulated on-chain time.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdvanceClockRequest {
    /// Number of seconds to advance the local clock.
    pub seconds: u64,
}

/// Response payload for an executed transaction.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecuteTxResponse {
    /// Base64 encoded transaction effects.
    pub effects: String,
    /// Optional error returned by execution.
    pub error: Option<String>,
}

/// Status response for the local forking runtime.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForkingStatus {
    /// Latest locally produced checkpoint sequence number.
    pub checkpoint: u64,
    /// Current local epoch.
    pub epoch: u64,
}

/// Generic envelope returned by control API endpoints.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}
