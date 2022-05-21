// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0



use blake2::digest::Update;
use crypto::traits::VerifyingKey;
use serde::{Deserialize, Serialize};

use crate::{Batch, BatchDigest};

/// The message exchanged between workers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(bound(deserialize = "PublicKey: VerifyingKey"))]
pub enum WorkerMessage<PublicKey: VerifyingKey> {
    /// Used by workers to send a new batch or to reply to a batch request.
    Batch(Batch),
    /// Used by workers to request batches.
    BatchRequest(Vec<BatchDigest>, /* origin */ PublicKey),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientBatchRequest(pub Vec<BatchDigest>);

/// Indicates a serialized `WorkerMessage::Batch` message.
pub type SerializedBatchMessage = Vec<u8>;

pub fn serialized_batch_digest<K: AsRef<[u8]>>(sbm: K) -> BatchDigest {
    BatchDigest::new(crypto::blake2b_256(|hasher| hasher.update(&sbm)))
}
