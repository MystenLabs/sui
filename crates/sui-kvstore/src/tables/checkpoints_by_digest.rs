// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Checkpoints by digest table: maps checkpoint digest to sequence number.

use anyhow::{Context, Result};
use bytes::Bytes;
use sui_types::digests::CheckpointDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::tables::DEFAULT_COLUMN;

pub const NAME: &str = "checkpoints_by_digest";

pub fn encode_key(digest: &CheckpointDigest) -> Vec<u8> {
    digest.inner().to_vec()
}

pub fn encode(sequence_number: CheckpointSequenceNumber) -> [(&'static str, Bytes); 1] {
    [(
        DEFAULT_COLUMN,
        Bytes::from(sequence_number.to_be_bytes().to_vec()),
    )]
}

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<CheckpointSequenceNumber> {
    let (_, value) = row.first().context("empty row")?;
    let bytes: [u8; 8] = value.as_ref().try_into()?;
    Ok(u64::from_be_bytes(bytes))
}
