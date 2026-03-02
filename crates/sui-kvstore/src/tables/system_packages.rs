// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! System packages table: index of system packages (those published by address 0x0).
//!
//! Row key: `original_id (32B)`
//! Column: default (cp_sequence_number u64 — first-seen checkpoint)

use anyhow::{Context, Result};
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

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<u64> {
    let (_, value) = row.first().context("empty row")?;
    let bytes: [u8; 8] = value.as_ref().try_into()?;
    Ok(u64::from_be_bytes(bytes))
}
