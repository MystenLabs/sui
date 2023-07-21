// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{Batch, BatchDigest};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(test)]
#[path = "tests/batch_serde.rs"]
mod batch_serde;

/// Used by workers to send a new batch.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerBatchMessage {
    pub batch: Batch,
}

/// Used by primary to bulk request batches from workers local store.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestBatchesRequest {
    pub batch_digests: Vec<BatchDigest>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestBatchesResponse {
    pub batches: Vec<Batch>,
    // If true, the primary should request the batches from the workers again.
    // This may not be something that can be trusted from a remote worker.
    pub is_size_limit_reached: bool,
}

// TODO: support propagating errors from the worker to the primary.
pub type TxResponse = tokio::sync::oneshot::Sender<BatchDigest>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DigestError {
    #[error("Invalid argument: invalid byte at {0}")]
    InvalidArgumentError(usize),
    #[error("Invalid length")]
    InvalidLengthError,
}
