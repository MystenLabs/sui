// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{Batch, BatchDigest};
use crypto::PublicKey;
use serde::{Deserialize, Serialize};

/// The message exchanged between workers.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkerMessage {
    /// Used by workers to send a new batch or to reply to a batch request.
    Batch(Batch),
    /// Used by workers to request batches.
    BatchRequest(Vec<BatchDigest>, /* origin */ PublicKey),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientBatchRequest(pub Vec<BatchDigest>);
