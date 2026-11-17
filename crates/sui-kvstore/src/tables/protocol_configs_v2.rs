// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Protocol configs v2 table: the protocol-config attributes and feature flags of each protocol
//! version the chain ran, in separate columns.
//!
//! Row key: `protocol_version` (8B big-endian)
//! Columns: `a` (attributes), `f` (feature flags)
//!
//! A protocol version the chain skipped has no row.

use std::collections::BTreeMap;

use anyhow::Context as _;
use anyhow::Result;
use anyhow::bail;
use bytes::Bytes;
use prost::Message as _;
use prost_types::Struct;
use prost_types::Value;
use prost_types::value::Kind;

use crate::ProtocolConfigV2Data;

pub const NAME: &str = "protocol_configs_v2";

pub mod col {
    /// Protobuf-encoded `prost_types::Struct` of every protocol-config attribute (scalar and
    /// non-scalar) rendered via `ProtocolConfig::render::<prost_types::Value>`. Attributes unset
    /// at the row's protocol version are explicit `NullValue` entries, so the keyset is stable
    /// across protocol versions. Holds no feature flags.
    pub const ATTRIBUTES: &str = "a";
    /// Protobuf-encoded `prost_types::Struct` mapping every feature flag to a `BoolValue`.
    pub const FEATURE_FLAGS: &str = "f";
}

pub fn encode_key(protocol_version: u64) -> Vec<u8> {
    protocol_version.to_be_bytes().to_vec()
}

pub fn encode(
    attributes: BTreeMap<String, Value>,
    feature_flags: BTreeMap<String, bool>,
) -> [(&'static str, Bytes); 2] {
    let attributes = Struct { fields: attributes };
    let feature_flags = Struct {
        fields: feature_flags
            .into_iter()
            .map(|(name, enabled)| (name, Value::from(enabled)))
            .collect(),
    };
    [
        (col::ATTRIBUTES, Bytes::from(attributes.encode_to_vec())),
        (
            col::FEATURE_FLAGS,
            Bytes::from(feature_flags.encode_to_vec()),
        ),
    ]
}

/// Parse a protocol version's attributes and feature flags from a row's cells.
pub fn decode(row: &[(Bytes, Bytes)]) -> Result<ProtocolConfigV2Data> {
    let mut attributes = None;
    let mut feature_flags = None;
    for (column, value) in row {
        match column.as_ref() {
            b"a" => {
                let decoded = Struct::decode(value.as_ref()).context("invalid attributes")?;
                attributes = Some(decoded.fields);
            }
            b"f" => {
                let decoded = Struct::decode(value.as_ref()).context("invalid feature flags")?;
                feature_flags = Some(decode_feature_flags(decoded)?);
            }
            _ => {}
        }
    }
    Ok(ProtocolConfigV2Data {
        attributes: attributes.context("missing attributes column")?,
        feature_flags: feature_flags.context("missing feature flags column")?,
    })
}

fn decode_feature_flags(flags: Struct) -> Result<BTreeMap<String, bool>> {
    flags
        .fields
        .into_iter()
        .map(|(name, value)| match value.kind {
            Some(Kind::BoolValue(enabled)) => Ok((name, enabled)),
            other => bail!("feature flag `{name}` is not a bool: {other:?}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn null() -> Value {
        Value {
            kind: Some(Kind::NullValue(0)),
        }
    }

    /// Attributes and feature flags come back from their own columns, including attributes that
    /// are booleans or unset.
    #[test]
    fn round_trips_attributes_and_feature_flags() {
        let attributes = BTreeMap::from([
            ("max_tx_size_bytes".to_owned(), Value::from(128 * 1024)),
            (
                "bridge_should_try_to_finalize_committee".to_owned(),
                Value::from(true),
            ),
            ("max_move_object_size".to_owned(), null()),
        ]);
        let feature_flags = BTreeMap::from([
            ("package_upgrades".to_owned(), true),
            ("zklogin_auth".to_owned(), false),
        ]);

        let row: Vec<(Bytes, Bytes)> = encode(attributes.clone(), feature_flags.clone())
            .into_iter()
            .map(|(column, value)| (Bytes::from_static(column.as_bytes()), value))
            .collect();
        let decoded = decode(&row).unwrap();

        assert_eq!(decoded.attributes, attributes);
        assert_eq!(decoded.feature_flags, feature_flags);
    }

    #[test]
    fn rejects_non_bool_feature_flag() {
        let flags = Struct {
            fields: BTreeMap::from([("package_upgrades".to_owned(), Value::from(1))]),
        };
        let row = [
            (
                Bytes::from_static(col::ATTRIBUTES.as_bytes()),
                Bytes::from(Struct::default().encode_to_vec()),
            ),
            (
                Bytes::from_static(col::FEATURE_FLAGS.as_bytes()),
                Bytes::from(flags.encode_to_vec()),
            ),
        ];
        assert!(decode(&row).is_err());
    }

    #[test]
    fn rejects_row_missing_a_column() {
        let [attributes, feature_flags] = encode(BTreeMap::new(), BTreeMap::new());
        for (column, value) in [attributes, feature_flags] {
            let row = [(Bytes::from_static(column.as_bytes()), value)];
            assert!(decode(&row).is_err(), "decoded a row with only `{column}`");
        }
    }
}
