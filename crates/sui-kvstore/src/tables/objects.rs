// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Objects table: every version of every object, plus tombstones at the versions where objects
//! were deleted or wrapped.
//!
//! Row key: `object_id (32B) || version (8B big-endian)`
//!
//! A row holds exactly one of:
//! - `""`: the BCS-encoded [`Object`] at that version (a live row), or
//! - `s`: a one-byte [`ObjectStatus`], `Deleted` or `Wrapped` (a tombstone), marking the version
//!   at which the object was removed from the live set.
//!
//! The newest row at or below version `V` is the object's state at `V`: a live row is the
//! object's contents, a tombstone means the object did not exist at `V`. Tombstones are written
//! by a separate pipeline from live rows, so they are only present where that pipeline has run.

use anyhow::{Context, Result, bail, ensure};
use bytes::Bytes;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::object::Object;
use sui_types::storage::ObjectKey;

use crate::object_status::ObjectStatus;

pub const NAME: &str = "objects";

pub mod col {
    /// BCS of a live object version.
    pub const OBJECT: &str = crate::tables::DEFAULT_COLUMN;
    /// One-byte [`ObjectStatus`](crate::ObjectStatus) of a tombstone.
    pub const STATUS: &str = "s";
}

/// A decoded row of the objects table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectRow {
    /// The object's contents at the row's version.
    Live(Object),
    /// The object was removed at the row's version: either `Deleted` or `Wrapped`.
    Tombstone(ObjectStatus),
}

impl ObjectRow {
    /// The object, or `None` for a tombstone.
    pub fn into_live(self) -> Option<Object> {
        match self {
            ObjectRow::Live(object) => Some(object),
            ObjectRow::Tombstone(_) => None,
        }
    }
}

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
    Ok([(col::OBJECT, Bytes::from(bcs::to_bytes(object)?))])
}

/// Cells of a tombstone row. `status` must be `Deleted` or `Wrapped`.
pub fn encode_tombstone(status: ObjectStatus) -> Result<[(&'static str, Bytes); 1]> {
    ensure!(
        status != ObjectStatus::Live,
        "a tombstone's status must be Deleted or Wrapped"
    );
    Ok([(col::STATUS, Bytes::copy_from_slice(&[status.to_byte()]))])
}

/// Parse a row as a live object or a tombstone, selecting the cell by column.
pub fn decode(row: &[(Bytes, Bytes)]) -> Result<ObjectRow> {
    let mut object = None;
    let mut status = None;
    for (column, value) in row {
        if column == col::OBJECT.as_bytes() {
            object = Some(value);
        } else if column == col::STATUS.as_bytes() {
            status = Some(value);
        }
    }
    match (object, status) {
        (Some(value), None) => Ok(ObjectRow::Live(
            bcs::from_bytes(value).context("invalid object BCS")?,
        )),
        (None, Some(value)) => {
            let [byte] = value.as_ref() else {
                bail!("expected 1-byte status, got {} bytes", value.len());
            };
            match ObjectStatus::from_byte(*byte)? {
                ObjectStatus::Live => bail!("tombstone has status Live"),
                status => Ok(ObjectRow::Tombstone(status)),
            }
        }
        (Some(_), Some(_)) => bail!("row has both an object and a tombstone status"),
        (None, None) => bail!("row has neither an object nor a tombstone status"),
    }
}

/// Whether a row is live rather than a tombstone, told apart by column alone, so it also works on
/// rows read with their values stripped.
pub fn is_live(row: &[(Bytes, Bytes)]) -> Result<bool> {
    let has_column = |column: &str| row.iter().any(|(name, _)| name == column.as_bytes());
    match (has_column(col::OBJECT), has_column(col::STATUS)) {
        (true, false) => Ok(true),
        (false, true) => Ok(false),
        (true, true) => bail!("row has both an object and a tombstone status"),
        (false, false) => bail!("row has neither an object nor a tombstone status"),
    }
}

#[cfg(test)]
mod tests {
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SuiAddress;

    use super::*;

    fn cells(encoded: [(&'static str, Bytes); 1]) -> Vec<(Bytes, Bytes)> {
        encoded
            .into_iter()
            .map(|(column, value)| (Bytes::from_static(column.as_bytes()), value))
            .collect()
    }

    #[test]
    fn decode_round_trips_live_rows_and_tombstones() {
        let object = Object::with_id_owner_for_testing(ObjectID::random(), SuiAddress::ZERO);
        assert_eq!(
            decode(&cells(encode(&object).unwrap())).unwrap(),
            ObjectRow::Live(object),
        );
        for status in [ObjectStatus::Deleted, ObjectStatus::Wrapped] {
            assert_eq!(
                decode(&cells(encode_tombstone(status).unwrap())).unwrap(),
                ObjectRow::Tombstone(status),
            );
        }
    }

    /// The cell is chosen by column, not position, and unknown columns are ignored.
    #[test]
    fn decode_selects_cell_by_column() {
        let mut row = vec![(Bytes::from_static(b"other"), Bytes::from_static(b"junk"))];
        row.extend(cells(encode_tombstone(ObjectStatus::Wrapped).unwrap()));
        assert_eq!(
            decode(&row).unwrap(),
            ObjectRow::Tombstone(ObjectStatus::Wrapped)
        );
    }

    #[test]
    fn decode_rejects_malformed_rows() {
        let object = Object::with_id_owner_for_testing(ObjectID::random(), SuiAddress::ZERO);
        let mut both = cells(encode(&object).unwrap());
        both.extend(cells(encode_tombstone(ObjectStatus::Deleted).unwrap()));
        let status = |bytes: &'static [u8]| vec![(Bytes::from_static(b"s"), Bytes::from(bytes))];

        for (name, row) in [
            ("empty", vec![]),
            ("both", both),
            ("live status", status(&[0])),
            ("unknown status", status(&[3])),
            ("long status", status(&[1, 1])),
        ] {
            assert!(decode(&row).is_err(), "{name} row decoded");
        }
        assert!(encode_tombstone(ObjectStatus::Live).is_err());
    }
}
