// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use blake2::digest::Update;
use crypto::traits::VerifyingKey;
use serde::{Deserialize, Serialize};
use thiserror::Error;

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

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DigestError {
    #[error("Invalid argument: invalid byte at {0}")]
    InvalidArgumentError(usize),
}

fn read_one_transaction(sbm: &[u8], offset: usize) -> Result<(&[u8], usize), DigestError> {
    let length = u64::from_le_bytes(
        sbm[offset..offset + 8]
            .try_into()
            .map_err(|_| DigestError::InvalidArgumentError(offset))?,
    );
    let length = usize::try_from(length).map_err(|_| DigestError::InvalidArgumentError(offset))?;
    let end = offset + 8 + length;
    Ok((&sbm[offset + 8..end], end))
}

pub fn hash_all_transactions(sbm: &SerializedBatchMessage) -> Result<BatchDigest, DigestError> {
    let mut offset = 4; // skip the enum variant selector
    let num_transactions = u64::from_le_bytes(
        sbm[offset..offset + 8]
            .try_into()
            .map_err(|_| DigestError::InvalidArgumentError(offset))?,
    );
    offset += 8;
    let mut transactions = Vec::new();
    for _i in 0..num_transactions {
        let (tx_ref, new_offset) = read_one_transaction(sbm, offset)?;
        transactions.push(tx_ref);
        offset = new_offset;
    }
    Ok(BatchDigest::new(crypto::blake2b_256(|hasher| {
        transactions.iter().for_each(|tx| hasher.update(tx))
    })))
}
