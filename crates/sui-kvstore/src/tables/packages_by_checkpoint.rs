// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Packages-by-checkpoint table: checkpoint-ordered index of packages.
//!
//! Row key: `cp_sequence_number (8B) || original_id (32B) || package_version (8B)`
//! Column: default (empty marker — metadata fetched from `packages` table)

use anyhow::Result;
use bytes::Bytes;

use crate::tables::DEFAULT_COLUMN;

pub const NAME: &str = "packages_by_checkpoint";

pub fn encode_key(cp_sequence_number: u64, original_id: &[u8], package_version: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(48);
    key.extend_from_slice(&cp_sequence_number.to_be_bytes());
    key.extend_from_slice(original_id);
    key.extend_from_slice(&package_version.to_be_bytes());
    key
}

pub fn encode() -> [(&'static str, Bytes); 1] {
    [(DEFAULT_COLUMN, Bytes::new())]
}

/// Extract (cp_sequence_number, original_id, package_version) from a 48-byte key.
pub fn decode_key(key: &[u8]) -> Result<(u64, Vec<u8>, u64)> {
    anyhow::ensure!(key.len() == 48, "expected 48-byte key, got {}", key.len());
    let cp = u64::from_be_bytes(key[..8].try_into()?);
    let original_id = key[8..40].to_vec();
    let version = u64::from_be_bytes(key[40..48].try_into()?);
    Ok((cp, original_id, version))
}
