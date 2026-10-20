// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Object-versions-by-checkpoint table: an object's version and status at the end of every
//! checkpoint that changed it.
//!
//! Row key: `object_id (32B) || cp_sequence_number (8B big-endian)`
//! Columns: `v` (object version, 8B big-endian), `s` (one-byte [`ObjectStatus`])
//!
//! An object's state as of checkpoint `C` is the row with the greatest checkpoint `<= C`, found
//! by a reverse scan from `(object_id, C)` down to `(object_id, 0)` limited to one row. The cost
//! of that lookup does not depend on how many versions the object has.
//!
//! When a checkpoint changes an object more than once, only the final version is recorded;
//! intermediate versions stay addressable by version through the `objects` table.

use anyhow::{Context, Result, bail};
use bytes::Bytes;
use sui_types::base_types::{ObjectID, SequenceNumber};

pub const NAME: &str = "object_versions_by_checkpoint";

pub mod col {
    pub const VERSION: &str = "v";
    pub const STATUS: &str = "s";
}

/// An object's status at the end of a checkpoint that changed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectStatus {
    /// The object exists; its contents are in the `objects` table at the recorded version.
    Live,
    /// The object was deleted. The recorded version is the deleting transaction's lamport
    /// version and has no `objects` row.
    Deleted,
    /// The object was wrapped into another object. The recorded version is the wrapping
    /// transaction's lamport version and has no `objects` row.
    Wrapped,
}

impl ObjectStatus {
    fn to_byte(self) -> u8 {
        match self {
            ObjectStatus::Live => 0,
            ObjectStatus::Deleted => 1,
            ObjectStatus::Wrapped => 2,
        }
    }

    fn from_byte(byte: u8) -> Result<Self> {
        Ok(match byte {
            0 => ObjectStatus::Live,
            1 => ObjectStatus::Deleted,
            2 => ObjectStatus::Wrapped,
            _ => bail!("invalid object status byte {byte}"),
        })
    }
}

pub fn encode_key(object_id: &ObjectID, cp_sequence_number: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(ObjectID::LENGTH + 8);
    key.extend_from_slice(object_id.as_ref());
    key.extend_from_slice(&cp_sequence_number.to_be_bytes());
    key
}

pub fn encode(version: SequenceNumber, status: ObjectStatus) -> [(&'static str, Bytes); 2] {
    [
        (
            col::VERSION,
            Bytes::copy_from_slice(&version.value().to_be_bytes()),
        ),
        (col::STATUS, Bytes::copy_from_slice(&[status.to_byte()])),
    ]
}

/// Parse the object's version and status from a row's cells.
pub fn decode(row: &[(Bytes, Bytes)]) -> Result<(SequenceNumber, ObjectStatus)> {
    let mut version = None;
    let mut status = None;
    for (column, value) in row {
        match column.as_ref() {
            b"v" => {
                let bytes = value.as_ref().try_into().context("invalid version")?;
                version = Some(SequenceNumber::from_u64(u64::from_be_bytes(bytes)));
            }
            b"s" => {
                let [byte] = value.as_ref() else {
                    bail!("expected 1-byte status, got {} bytes", value.len());
                };
                status = Some(ObjectStatus::from_byte(*byte)?);
            }
            _ => {}
        }
    }
    Ok((
        version.context("missing version column")?,
        status.context("missing status column")?,
    ))
}
