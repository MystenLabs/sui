// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Checkpoints table: stores full checkpoint data indexed by sequence number.

use anyhow::{Context, Result};
use bytes::Bytes;
use sui_types::crypto::AuthorityStrongQuorumSignInfo;
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointSequenceNumber, CheckpointSummary,
};

use crate::Checkpoint;

pub mod col {
    pub const SUMMARY: &str = "s";
    pub const SIGNATURES: &str = "sg";
    pub const CONTENTS: &str = "c";
}

pub const NAME: &str = "checkpoints";

pub fn encode_key(sequence_number: CheckpointSequenceNumber) -> Vec<u8> {
    sequence_number.to_be_bytes().to_vec()
}

pub fn encode(
    summary: &CheckpointSummary,
    signatures: &AuthorityStrongQuorumSignInfo,
    contents: &CheckpointContents,
) -> Result<[(&'static str, Bytes); 3]> {
    Ok([
        (col::SUMMARY, Bytes::from(bcs::to_bytes(summary)?)),
        (col::SIGNATURES, Bytes::from(bcs::to_bytes(signatures)?)),
        (col::CONTENTS, Bytes::from(bcs::to_bytes(contents)?)),
    ])
}

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<Checkpoint> {
    let mut summary = None;
    let mut contents = None;
    let mut signatures = None;

    for (column, value) in row {
        match column.as_ref() {
            b"s" => summary = Some(bcs::from_bytes(value)?),
            b"c" => contents = Some(bcs::from_bytes(value)?),
            b"sg" => signatures = Some(bcs::from_bytes(value)?),
            _ => {}
        }
    }

    Ok(Checkpoint {
        summary: summary.context("summary field is missing")?,
        contents: contents.context("contents field is missing")?,
        signatures: signatures.context("signatures field is missing")?,
    })
}
