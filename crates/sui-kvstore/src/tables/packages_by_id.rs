// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Packages-by-ID table: maps package_id to original_id.
//!
//! Row key: `package_id (32B)`
//! Column: default (original_id 32B)

use anyhow::{Context, Result};
use bytes::Bytes;

use crate::tables::DEFAULT_COLUMN;

pub const NAME: &str = "packages_by_id";

pub fn encode_key(package_id: &[u8]) -> Vec<u8> {
    package_id.to_vec()
}

pub fn encode(original_id: &[u8]) -> [(&'static str, Bytes); 1] {
    [(DEFAULT_COLUMN, Bytes::from(original_id.to_vec()))]
}

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<Vec<u8>> {
    let (_, value) = row.first().context("empty row")?;
    Ok(value.to_vec())
}
