// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Types and interface for the node-local "wait for executed effects" endpoint.
//!
//! This lets a client learn the moment a full node has *locally executed* a transaction — before
//! the transaction is included in a certified checkpoint or indexed. It exists to measure how
//! quickly a node makes execution results available, independent of the checkpoint/indexing path.

use serde::{Deserialize, Serialize};

use crate::base_types::TransactionDigest;
use crate::digests::TransactionEffectsDigest;
use crate::effects::TransactionEffects;
use crate::error::SuiError;

/// Request to wait until this node has locally executed a transaction.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WaitForLocalEffectsRequest {
    /// Digest of the transaction to wait for.
    pub transaction_digest: TransactionDigest,
    /// Maximum time to wait for local execution. `None` uses the server default.
    pub timeout_ms: Option<u64>,
    /// When true, the response includes the full [`TransactionEffects`], not just their digest.
    pub include_details: bool,
}

/// Response for [`WaitForLocalEffectsRequest`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WaitForLocalEffectsResponse {
    /// The node has locally executed the transaction.
    Executed {
        effects_digest: TransactionEffectsDigest,
        /// Present only when `include_details` was set on the request. Boxed because
        /// [`TransactionEffects`] is large relative to the other response variant.
        effects: Option<Box<TransactionEffects>>,
    },
    /// The node did not locally execute the transaction before the timeout elapsed.
    TimedOut,
}

/// Result of a completed local execution (the transaction was executed before any timeout).
pub struct LocalEffects {
    pub effects_digest: TransactionEffectsDigest,
    /// Populated only when the caller asked for details.
    pub effects: Option<Box<TransactionEffects>>,
}

/// Interface the RPC layer uses to wait on node-local execution without depending on `sui-core`.
///
/// Mirrors the injection pattern used by [`crate::transaction_executor::TransactionExecutor`]: the
/// concrete implementation lives in `sui-core` and is injected into the RPC service.
#[async_trait::async_trait]
pub trait LocalEffectsWaiter: Send + Sync {
    /// Resolve once this node has locally executed `digest`. Does not impose a timeout itself — the
    /// caller bounds the wait.
    async fn wait_for_local_effects(
        &self,
        digest: TransactionDigest,
        include_details: bool,
    ) -> Result<LocalEffects, SuiError>;
}
