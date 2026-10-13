// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Objects table: stores object data indexed by (ObjectID, version).

use anyhow::{Context, Result};
use bytes::Bytes;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;

use crate::tables::DEFAULT_COLUMN;

pub const NAME: &str = "objects";

pub fn encode_key(object_key: &ObjectKey) -> Vec<u8> {
    let mut raw_key = Vec::with_capacity(40);
    raw_key.extend_from_slice(object_key.0.as_ref());
    raw_key.extend_from_slice(&object_key.1.value().to_be_bytes());
    raw_key
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
