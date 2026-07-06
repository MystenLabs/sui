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
    /// Protobuf-encoded `prost_types::Struct` carrying every protocol-config attribute
    /// (scalar and non-scalar) and feature flag rendered via
    /// `ProtocolConfig::render::<prost_types::Value>`. Unset fields are preserved as
    /// explicit `NullValue` entries so the keyset is stable across protocol versions.
    pub const CONFIGS: &str = "c";
}

pub fn encode_key(protocol_version: u64) -> Vec<u8> {
    protocol_version.to_be_bytes().to_vec()
}

pub fn encode(configs: &BTreeMap<String, Value>) -> [(&'static str, Bytes); 1] {
    let configs_struct = Struct {
        fields: configs.clone(),
    };
    [(col::CONFIGS, Bytes::from(configs_struct.encode_to_vec()))]
}

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<ProtocolConfigData> {
    let mut data = ProtocolConfigData::default();

    for (col, value) in row {
        if col.as_ref() == b"c" {
            let configs_struct = Struct::decode(value.as_ref())?;
            data.configs = configs_struct.fields;
        }
    }

    Ok(data)
}
