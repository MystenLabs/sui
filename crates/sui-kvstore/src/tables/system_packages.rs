// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! System packages table: index of system packages (those output by transactions sent by `0x0`).
//!
//! Row key: `original_id (32B)`
//! Column: default (cp_sequence_number u64 — the checkpoint of the write that set the cell)
//!
//! The pipeline rewrites a package's row every time a system transaction outputs a version of it,
//! so the cell holds the checkpoint of the latest such write, not the package's first version.
//! Readers use only the row keys.

use bytes::Bytes;

use crate::tables::DEFAULT_COLUMN;

pub const NAME: &str = "system_packages";

pub fn encode_key(original_id: &[u8]) -> Vec<u8> {
    original_id.to_vec()
}

pub fn encode(cp_sequence_number: u64) -> [(&'static str, Bytes); 1] {
    [(
        DEFAULT_COLUMN,
        Bytes::from(cp_sequence_number.to_be_bytes().to_vec()),
    )]
}
