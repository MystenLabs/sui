// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Packages table: stores package metadata indexed by (original_id, version).
//!
//! Row key: `original_id (32B) || package_version (8B big-endian)`
//! Columns: `cp` (checkpoint sequence number), `pi` (package_id), `sp` (is_system_package)
//!
//! The actual serialized object is stored in the `objects` table and fetched
//! separately via `(package_id, version)`.

use anyhow::{Context, Result};
use bytes::Bytes;

use crate::PackageData;

pub const NAME: &str = "packages";

pub mod col {
    pub const CP: &str = "cp";
    pub const PACKAGE_ID: &str = "pi";
    pub const IS_SYSTEM_PACKAGE: &str = "sp";
}

pub fn encode_key(original_id: &[u8], package_version: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(40);
    key.extend_from_slice(original_id);
    key.extend_from_slice(&package_version.to_be_bytes());
    key
}

pub fn encode_key_upper_bound(original_id: &[u8]) -> Vec<u8> {
    encode_key(original_id, u64::MAX)
}

pub fn encode(
    cp_sequence_number: u64,
    package_id: &[u8],
    is_system_package: bool,
) -> [(&'static str, Bytes); 3] {
    [
        (
            col::CP,
            Bytes::from(cp_sequence_number.to_be_bytes().to_vec()),
        ),
        (col::PACKAGE_ID, Bytes::from(package_id.to_vec())),
        (
            col::IS_SYSTEM_PACKAGE,
            Bytes::from(vec![u8::from(is_system_package)]),
        ),
    ]
}

pub fn decode(key: &[u8], row: &[(Bytes, Bytes)]) -> Result<PackageData> {
    anyhow::ensure!(key.len() == 40, "expected 40-byte key, got {}", key.len());

    let original_id = key[..32].to_vec();
    let package_version = u64::from_be_bytes(key[32..40].try_into()?);

    let mut data = PackageData {
        package_id: Vec::new(),
        package_version,
        original_id,
        is_system_package: false,
        cp_sequence_number: 0,
    };

    for (col, value) in row {
        match col.as_ref() {
            b"cp" => {
                data.cp_sequence_number = u64::from_be_bytes(
                    value
                        .as_ref()
                        .try_into()
                        .context("invalid cp_sequence_number")?,
                );
            }
            b"pi" => {
                data.package_id = value.to_vec();
            }
            b"sp" => {
                data.is_system_package = value.as_ref().first().copied().unwrap_or(0) != 0;
            }
            _ => {}
        }
    }

    Ok(data)
}
