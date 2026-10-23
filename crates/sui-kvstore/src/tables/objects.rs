// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Objects table: stores object data indexed by (ObjectID, version).

use anyhow::{Context, Result};
use bytes::Bytes;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::object::Object;
use sui_types::storage::ObjectKey;

use crate::tables::DEFAULT_COLUMN;

pub const NAME: &str = "objects";

pub fn encode_key(object_key: &ObjectKey) -> Vec<u8> {
    let mut raw_key = object_key.0.to_vec();
    raw_key.extend(object_key.1.value().to_be_bytes());
    raw_key
}

/// The `ObjectKey` of a row key built by [`encode_key`]: the object id followed by the version,
/// big-endian.
pub fn decode_key(row_key: &[u8]) -> Result<ObjectKey> {
    let (id, version) = row_key
        .split_at_checked(ObjectID::LENGTH)
        .context("objects row key is shorter than an object id")?;
    let id = ObjectID::from_bytes(id).context("objects row key has an invalid object id")?;
    let version: [u8; 8] = version
        .try_into()
        .context("objects row key version is not 8 bytes")?;
    Ok(ObjectKey(
        id,
        SequenceNumber::from_u64(u64::from_be_bytes(version)),
    ))
}

pub fn encode_key_upper_bound(object_key: &ObjectKey) -> Vec<u8> {
    encode_key(object_key)
}

pub fn encode(object: &Object) -> Result<[(&'static str, Bytes); 1]> {
    Ok([(DEFAULT_COLUMN, Bytes::from(bcs::to_bytes(object)?))])
}

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<Object> {
    let (_, value) = row.first().context("empty row")?;
    Ok(bcs::from_bytes(value)?)
}
