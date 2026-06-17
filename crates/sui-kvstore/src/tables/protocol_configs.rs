// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Protocol configs table: stores per-protocol-version config key-value pairs and feature flags.

use std::collections::BTreeMap;

use anyhow::Result;
use bytes::Bytes;
use prost::Message as _;
use prost_types::Struct;
use prost_types::Value;

use crate::ProtocolConfigData;

pub const NAME: &str = "protocol_configs";

pub mod col {
    /// BCS-serialized scalar-only attributes map. Legacy qualifier; new readers should prefer
    /// the lossless `CONFIGS` cell instead.
    pub const ATTRIBUTES: &str = "cf";
    /// BCS-serialized feature flags map.
    pub const FLAGS: &str = "ff";
    /// Protobuf-encoded `prost_types::Struct` carrying every protocol-config attribute
    /// (scalar and non-scalar) and feature flag rendered via
    /// `ProtocolConfig::render::<prost_types::Value>`. Unset fields are preserved as
    /// explicit `NullValue` entries so the keyset is stable across protocol versions.
    pub const CONFIGS: &str = "c";
}

pub fn encode_key(protocol_version: u64) -> Vec<u8> {
    protocol_version.to_be_bytes().to_vec()
}

pub fn encode(
    attributes: &BTreeMap<String, Option<String>>,
    flags: &BTreeMap<String, bool>,
    configs: &BTreeMap<String, Value>,
) -> Result<[(&'static str, Bytes); 3]> {
    let configs_struct = Struct {
        fields: configs.clone(),
    };
    Ok([
        (col::ATTRIBUTES, Bytes::from(bcs::to_bytes(attributes)?)),
        (col::FLAGS, Bytes::from(bcs::to_bytes(flags)?)),
        (col::CONFIGS, Bytes::from(configs_struct.encode_to_vec())),
    ])
}

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<ProtocolConfigData> {
    let mut data = ProtocolConfigData::default();

    for (col, value) in row {
        match col.as_ref() {
            b"cf" => data.attributes = bcs::from_bytes(value)?,
            b"ff" => data.flags = bcs::from_bytes(value)?,
            b"c" => {
                let configs_struct = Struct::decode(value.as_ref())?;
                data.configs = configs_struct.fields;
            }
            _ => {}
        }
    }

    Ok(data)
}
