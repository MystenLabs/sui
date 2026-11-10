// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Object-versions-by-checkpoint table: an object's version and status at the end of every
//! checkpoint that changed it.
//!
//! Row key: `object_id (32B) || (u64::MAX - cp_sequence_number) (8B big-endian)`
//! Columns: `v` (object version, 8B big-endian), `s` (one-byte [`ObjectStatus`])
//!
//! An object's rows sort newest checkpoint first, so its state as of checkpoint `C` is the first
//! row of a forward scan from `(object_id, C)` to the end of the object's rows, limited to one
//! row. That scan reads only the row it returns, whatever the object's history.
//!
//! When a checkpoint changes an object more than once, only the final version is recorded;
//! intermediate versions stay addressable by version through the `objects` table.

use anyhow::{Context, Result, bail, ensure};
use bytes::Bytes;
use sui_types::base_types::{ObjectID, SequenceNumber};

use crate::object_status::ObjectStatus;

pub const NAME: &str = "object_versions_by_checkpoint";

pub mod col {
    pub const VERSION: &str = "v";
    pub const STATUS: &str = "s";
}

/// Length of a row key: an object id followed by an 8-byte inverted checkpoint.
const KEY_LENGTH: usize = ObjectID::LENGTH + 8;

pub fn encode_key(object_id: &ObjectID, cp_sequence_number: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(KEY_LENGTH);
    key.extend_from_slice(object_id.as_ref());
    key.extend_from_slice(&(u64::MAX - cp_sequence_number).to_be_bytes());
    key
}

/// The object id and checkpoint of a row key built by [`encode_key`].
pub fn decode_key(row_key: &[u8]) -> Result<(ObjectID, u64)> {
    ensure!(
        row_key.len() == KEY_LENGTH,
        "object_versions_by_checkpoint row key has {} bytes, expected {KEY_LENGTH}",
        row_key.len(),
    );
    let (id, inverted) = row_key.split_at(ObjectID::LENGTH);
    let inverted = u64::from_be_bytes(inverted.try_into().expect("8-byte checkpoint suffix"));
    Ok((
        ObjectID::from_bytes(id).context("invalid object id in index row key")?,
        u64::MAX - inverted,
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    const CHECKPOINTS: [u64; 7] = [0, 1, 255, 256, 1 << 32, u64::MAX - 1, u64::MAX];

    #[test]
    fn keys_round_trip() {
        let id = ObjectID::random();
        for checkpoint in CHECKPOINTS {
            let key = encode_key(&id, checkpoint);
            assert_eq!(key.len(), ObjectID::LENGTH + 8);
            assert_eq!(decode_key(&key).unwrap(), (id, checkpoint));
        }
        assert!(decode_key(&encode_key(&id, 0)[1..]).is_err());
    }

    /// An object's rows sort newest checkpoint first: an earlier checkpoint has a later key.
    #[test]
    fn keys_sort_checkpoints_newest_first() {
        let id = ObjectID::random();
        for (i, a) in CHECKPOINTS.iter().enumerate() {
            for b in &CHECKPOINTS[i + 1..] {
                assert!(encode_key(&id, *a) > encode_key(&id, *b), "{a} < {b}");
            }
        }
        assert_eq!(encode_key(&id, 0)[ObjectID::LENGTH..], [0xff; 8]);
        assert_eq!(encode_key(&id, u64::MAX)[ObjectID::LENGTH..], [0x00; 8]);
    }
}
