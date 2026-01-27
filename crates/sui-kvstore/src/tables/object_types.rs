// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Object types table: stores object type indexed by object ID.

use anyhow::{Context, Result};
use bytes::Bytes;
use sui_types::base_types::{ObjectID, ObjectType};

use crate::tables::DEFAULT_COLUMN;

pub const NAME: &str = "object_types";

pub fn encode_key(object_id: &ObjectID) -> Vec<u8> {
    object_id.to_vec()
}

pub fn encode(object_type: &ObjectType) -> Result<[(&'static str, Bytes); 1]> {
    Ok([(DEFAULT_COLUMN, Bytes::from(bcs::to_bytes(object_type)?))])
}

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<ObjectType> {
    let (_, value) = row.first().context("empty row")?;
    Ok(bcs::from_bytes(value)?)
}
