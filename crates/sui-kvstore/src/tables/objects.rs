// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Objects tables: every version of every object, in the layout of an [`ObjectSchema`].
//!
//! Row keys:
//! - [`ObjectSchema::V1`], table `objects`: `object_id (32B) || version (8B big-endian)`. An
//!   object's versions sort oldest first.
//! - [`ObjectSchema::V2`], table `objects_v2`: `object_id (32B) || (u64::MAX - version) (8B
//!   big-endian)`. An object's versions sort newest first, so "the newest version at or below
//!   `V`" is the first row of a forward scan from `(object_id, V)`.
//!
//! A row holds exactly one of:
//! - `""`: the BCS-encoded [`Object`] at that version (a live row), or
//! - `s`: a one-byte [`ObjectStatus`], `Deleted` or `Wrapped` (a tombstone), marking the version
//!   at which the object was removed from the live set.
//!
//! In either table, the newest row at or below version `V` is the object's state at `V`: a live
//! row is the object's contents, a tombstone means the object did not exist at `V`. In `objects`,
//! tombstones are written by a separate pipeline from live rows (`kvstore_object_tombstones`), so
//! they are only present where that pipeline has run. In `objects_v2`, the `kvstore_objects_v2`
//! pipeline writes both kinds of row.

use anyhow::{Context, Result, bail, ensure};
use bytes::Bytes;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::object::Object;
use sui_types::storage::ObjectKey;

use crate::object_status::ObjectStatus;
use crate::tables::ObjectSchema;

/// The [`ObjectSchema::V1`] table.
pub const NAME: &str = "objects";
/// The [`ObjectSchema::V2`] table.
pub const NAME_V2: &str = "objects_v2";

/// Length of a row key: an object id followed by an 8-byte version.
const KEY_LENGTH: usize = ObjectID::LENGTH + 8;

/// The table that holds object versions in `schema`.
pub const fn table_name(schema: ObjectSchema) -> &'static str {
    match schema {
        ObjectSchema::V1 => NAME,
        ObjectSchema::V2 => NAME_V2,
    }
}

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

/// The 8-byte key suffix that encodes `version` in `schema`.
const fn version_suffix(schema: ObjectSchema, version: u64) -> [u8; 8] {
    match schema {
        ObjectSchema::V1 => version.to_be_bytes(),
        ObjectSchema::V2 => (u64::MAX - version).to_be_bytes(),
    }
}

/// The row key of `object_key` in `schema`'s table.
pub fn encode_key(schema: ObjectSchema, object_key: &ObjectKey) -> Vec<u8> {
    let mut key = Vec::with_capacity(KEY_LENGTH);
    key.extend_from_slice(object_key.0.as_ref());
    key.extend_from_slice(&version_suffix(schema, object_key.1.value()));
    key
}

/// The `ObjectKey` of a row key built by [`encode_key`] for `schema`.
pub fn decode_key(schema: ObjectSchema, row_key: &[u8]) -> Result<ObjectKey> {
    ensure!(
        row_key.len() == KEY_LENGTH,
        "objects row key has {} bytes, expected {KEY_LENGTH}",
        row_key.len(),
    );
    let (id, suffix) = row_key.split_at(ObjectID::LENGTH);
    let suffix = u64::from_be_bytes(suffix.try_into().expect("8-byte version suffix"));
    // Inverting the suffix is its own inverse, so it both encodes and decodes a V2 version.
    let version = u64::from_be_bytes(version_suffix(schema, suffix));
    Ok(ObjectKey(
        ObjectID::from_bytes(id).context("invalid object id in objects row key")?,
        SequenceNumber::from_u64(version),
    ))
}

/// The lowest and highest row keys any row of `object_id` can have, in either schema. Scans that
/// must cover an object's newest or oldest row use these, rather than the key of a boundary
/// version, so no version falls outside them.
pub fn object_key_bounds(object_id: &ObjectID) -> (Vec<u8>, Vec<u8>) {
    let with_suffix = |byte| {
        let mut key = Vec::with_capacity(KEY_LENGTH);
        key.extend_from_slice(object_id.as_ref());
        key.extend_from_slice(&[byte; 8]);
        key
    };
    (with_suffix(0x00), with_suffix(0xff))
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

    /// Versions at the edges of the key space and in between.
    fn sample_versions() -> Vec<SequenceNumber> {
        [
            0,
            1,
            2,
            255,
            256,
            1 << 32,
            SequenceNumber::MAX.value() - 1,
            SequenceNumber::MAX.value(),
            SequenceNumber::MAX.value() + 1,
            u64::MAX - 1,
            u64::MAX,
        ]
        .into_iter()
        .map(SequenceNumber::from_u64)
        .collect()
    }

    #[test]
    fn keys_round_trip_in_both_schemas() {
        let id = ObjectID::random();
        for schema in [ObjectSchema::V1, ObjectSchema::V2] {
            for version in sample_versions() {
                let key = ObjectKey(id, version);
                let encoded = encode_key(schema, &key);
                assert_eq!(encoded.len(), ObjectID::LENGTH + 8);
                assert_eq!(&encoded[..ObjectID::LENGTH], id.as_ref());
                assert_eq!(decode_key(schema, &encoded).unwrap(), key, "{schema:?}");
            }
        }
    }

    /// V1 keys sort an object's versions oldest first, V2 keys newest first.
    #[test]
    fn v2_keys_sort_versions_newest_first() {
        let id = ObjectID::random();
        let versions = sample_versions();
        for (i, a) in versions.iter().enumerate() {
            for b in &versions[i + 1..] {
                assert!(a < b);
                let key = |schema, version| encode_key(schema, &ObjectKey(id, version));
                assert!(key(ObjectSchema::V1, *a) < key(ObjectSchema::V1, *b));
                assert!(
                    key(ObjectSchema::V2, *a) > key(ObjectSchema::V2, *b),
                    "{a} < {b}, so its V2 key must sort after",
                );
            }
        }
        assert_eq!(
            encode_key(
                ObjectSchema::V2,
                &ObjectKey(id, SequenceNumber::from_u64(0))
            )[ObjectID::LENGTH..],
            [0xff; 8],
        );
        assert_eq!(
            encode_key(
                ObjectSchema::V2,
                &ObjectKey(id, SequenceNumber::from_u64(u64::MAX))
            )[ObjectID::LENGTH..],
            [0x00; 8],
        );
    }

    /// Every key of an object lies within its bounds, and the keys of neighbouring ids do not.
    #[test]
    fn object_key_bounds_cover_exactly_one_object() {
        let id = ObjectID::from_single_byte(0x42);
        let below = ObjectID::from_single_byte(0x41);
        let above = ObjectID::from_single_byte(0x43);
        let (lo, hi) = object_key_bounds(&id);
        for schema in [ObjectSchema::V1, ObjectSchema::V2] {
            for version in sample_versions() {
                let inside = encode_key(schema, &ObjectKey(id, version));
                assert!(lo <= inside && inside <= hi, "{schema:?} {version}");
                for other in [below, above] {
                    let outside = encode_key(schema, &ObjectKey(other, version));
                    assert!(outside < lo || outside > hi, "{schema:?} {other} {version}");
                }
            }
        }
    }

    #[test]
    fn decode_key_rejects_wrong_lengths() {
        let key = encode_key(
            ObjectSchema::V2,
            &ObjectKey(ObjectID::random(), SequenceNumber::from_u64(7)),
        );
        for schema in [ObjectSchema::V1, ObjectSchema::V2] {
            assert!(decode_key(schema, &key[..key.len() - 1]).is_err());
            assert!(decode_key(schema, &[key.as_slice(), &[0]].concat()).is_err());
        }
    }
}
