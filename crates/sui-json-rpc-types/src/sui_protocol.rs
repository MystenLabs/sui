// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::DisplayFromStr;
use serde_with::serde_as;
use sui_protocol_config::{ProtocolConfig, ProtocolConfigValue, ProtocolVersion};
use sui_types::sui_serde::Readable;
use sui_types::sui_serde::{AsProtocolVersion, BigInt};

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", rename = "ProtocolConfigValue")]
pub enum SuiProtocolConfigValue {
    U16(
        #[schemars(with = "BigInt<u16>")]
        #[serde_as(as = "BigInt<u16>")]
        u16,
    ),
    U32(
        #[schemars(with = "BigInt<u32>")]
        #[serde_as(as = "BigInt<u32>")]
        u32,
    ),
    U64(
        #[schemars(with = "BigInt<u64>")]
        #[serde_as(as = "BigInt<u64>")]
        u64,
    ),
    F64(
        #[schemars(with = "String")]
        #[serde_as(as = "DisplayFromStr")]
        f64,
    ),
    Bool(
        #[schemars(with = "String")]
        #[serde_as(as = "DisplayFromStr")]
        bool,
    ),
}

// Guards the `usize as u64` cast below.
const _: () = assert!(std::mem::size_of::<usize>() <= std::mem::size_of::<u64>());

impl From<ProtocolConfigValue> for SuiProtocolConfigValue {
    fn from(value: ProtocolConfigValue) -> Self {
        match value {
            ProtocolConfigValue::u16(y) => SuiProtocolConfigValue::U16(y),
            ProtocolConfigValue::u32(y) => SuiProtocolConfigValue::U32(y),
            ProtocolConfigValue::u64(x) => SuiProtocolConfigValue::U64(x),
            // usize widens to u64 losslessly; reuse U64 rather than add a platform-dependent variant.
            ProtocolConfigValue::usize(x) => SuiProtocolConfigValue::U64(x as u64),
            ProtocolConfigValue::bool(z) => SuiProtocolConfigValue::Bool(z),
        }
    }
}

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", rename = "ProtocolConfig")]
pub struct ProtocolConfigResponse {
    #[schemars(with = "AsProtocolVersion")]
    #[serde_as(as = "Readable<AsProtocolVersion, _>")]
    pub min_supported_protocol_version: ProtocolVersion,
    #[schemars(with = "AsProtocolVersion")]
    #[serde_as(as = "Readable<AsProtocolVersion, _>")]
    pub max_supported_protocol_version: ProtocolVersion,
    #[schemars(with = "AsProtocolVersion")]
    #[serde_as(as = "Readable<AsProtocolVersion, _>")]
    pub protocol_version: ProtocolVersion,
    pub feature_flags: BTreeMap<String, bool>,
    pub attributes: BTreeMap<String, Option<SuiProtocolConfigValue>>,
    /// Lossless view of every protocol-config attribute and feature flag, rendered to
    /// JSON. Unlike `attributes`, this includes non-scalar fields (e.g. lists) and is the
    /// preferred surface for clients that need to read complex values.
    #[serde(default)]
    #[schemars(with = "BTreeMap<String, serde_json::Value>")]
    pub configs: BTreeMap<String, serde_json::Value>,
}

impl From<ProtocolConfig> for ProtocolConfigResponse {
    fn from(config: ProtocolConfig) -> Self {
        // Render emits explicit `Null`s for fields unset at this protocol version; filter them
        // out so the public `configs` map only carries values that are actually configured.
        let mut configs = config
            .render::<serde_json::Value>(&mut mysten_common::rpc_format::Unmetered)
            .expect("render to serde_json::Value should succeed")
            .into_iter()
            .filter(|(_, v)| !v.is_null())
            .collect::<BTreeMap<String, serde_json::Value>>();

        // Merge feature flags into `configs` so it stands alone as a complete view.
        for (k, v) in config.feature_map() {
            let old = configs.insert(k, serde_json::Value::Bool(v));
            debug_assert!(
                old.is_none(),
                "feature flags and attributes can't have keys which are the same"
            );
        }

        ProtocolConfigResponse {
            protocol_version: config.version,
            attributes: config
                .attr_map()
                .into_iter()
                .map(|(k, v)| (k, v.map(SuiProtocolConfigValue::from)))
                .collect(),
            min_supported_protocol_version: ProtocolVersion::MIN,
            max_supported_protocol_version: ProtocolVersion::MAX,
            feature_flags: config.feature_map(),
            configs,
        }
    }
}
