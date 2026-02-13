// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Protocol configs table: stores per-protocol-version config key-value pairs and feature flags.

use std::collections::BTreeMap;

use anyhow::Result;
use bytes::Bytes;

use crate::ProtocolConfigData;

pub const NAME: &str = "protocol_configs";

pub mod col {
    /// BCS-serialized config attributes map.
    pub const CONFIGS: &str = "cf";
    /// BCS-serialized feature flags map.
    pub const FLAGS: &str = "ff";
}

pub fn encode_key(protocol_version: u64) -> Vec<u8> {
    protocol_version.to_be_bytes().to_vec()
}

pub fn encode(
    configs: &BTreeMap<String, Option<String>>,
    flags: &BTreeMap<String, bool>,
) -> Result<[(&'static str, Bytes); 2]> {
    Ok([
        (col::CONFIGS, Bytes::from(bcs::to_bytes(configs)?)),
        (col::FLAGS, Bytes::from(bcs::to_bytes(flags)?)),
    ])
}

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<ProtocolConfigData> {
    let mut data = ProtocolConfigData::default();

    for (col, value) in row {
        match col.as_ref() {
            b"cf" => data.configs = bcs::from_bytes(value)?,
            b"ff" => data.flags = bcs::from_bytes(value)?,
            _ => {}
        }
    }

    Ok(data)
}
