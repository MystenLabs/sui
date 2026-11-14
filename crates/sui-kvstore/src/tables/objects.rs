// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Objects table: every version of every object, plus tombstones at the versions where objects
//! were deleted or wrapped.
//!
//! Row key: `object_id (32B) || version (8B big-endian)`
//!
//! A row holds exactly one of:
//! - `""`: the BCS-encoded [`Object`] at that version (a live row), or
//! - a tombstone, marking the version at which the object was removed from the live set:
//!   - `s`: a one-byte [`ObjectStatus`], `Deleted` or `Wrapped`, and
//!   - `t`: the raw 32-byte digest of the transaction that removed the object. Tombstones written
//!     before this column existed lack it.
//!
//! The newest row at or below version `V` is the object's state at `V`: a live row is the
//! object's contents, a tombstone means the object did not exist at `V`. Tombstones are written
//! by a separate pipeline from live rows, so they are only present where that pipeline has run.

use anyhow::{Context, Result, bail, ensure};
use bytes::Bytes;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::digests::TransactionDigest;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;

use crate::object_status::ObjectStatus;

pub const NAME: &str = "objects";

pub mod col {
    /// BCS of a live object version.
    pub const OBJECT: &str = crate::tables::DEFAULT_COLUMN;
    /// One-byte [`ObjectStatus`](crate::ObjectStatus) of a tombstone.
    pub const STATUS: &str = "s";
    /// Raw 32-byte digest of the transaction that removed the object, in a tombstone.
    pub const TRANSACTION: &str = "t";
}

/// A decoded row of the objects table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectRow {
    /// The object's contents at the row's version.
    Live(Object),
    /// The object was removed at the row's version.
    Tombstone(ObjectTombstone),
}

/// A row of the objects table, read with or without live objects' contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectVersionRow {
    /// A live version, with the object's contents if the read included them.
    Live(Option<Object>),
    /// The object was removed at the row's version.
    Tombstone(ObjectTombstone),
}

/// How an object was removed at a tombstone row's version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectTombstone {
    /// `Deleted` or `Wrapped`.
    pub status: ObjectStatus,
    /// The transaction that removed the object. `None` for tombstones written without it.
    pub transaction: Option<TransactionDigest>,
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

/// Cells of a tombstone row. `status` must be `Deleted` or `Wrapped`; `transaction` is the
/// transaction that removed the object.
pub fn encode_tombstone(
    status: ObjectStatus,
    transaction: &TransactionDigest,
) -> Result<[(&'static str, Bytes); 2]> {
    ensure!(
        status != ObjectStatus::Live,
        "a tombstone's status must be Deleted or Wrapped"
    );
    Ok([
        (col::STATUS, Bytes::copy_from_slice(&[status.to_byte()])),
        (
            col::TRANSACTION,
            Bytes::copy_from_slice(transaction.inner()),
        ),
    ])
}

/// Parse a row as a live object or a tombstone, selecting the cells by column.
pub fn decode(row: &[(Bytes, Bytes)]) -> Result<ObjectRow> {
    if let Some(tombstone) = decode_tombstone(row)? {
        return Ok(ObjectRow::Tombstone(tombstone));
    }
    let object = cell(row, col::OBJECT).context("live row has no object")?;
    Ok(ObjectRow::Live(
        bcs::from_bytes(object).context("invalid object BCS")?,
    ))
}

/// Parse a row read with live objects' contents if `with_contents`, or with them stripped
/// otherwise, as a live version, with its object only if read with contents, or a tombstone.
pub fn decode_version_row(row: &[(Bytes, Bytes)], with_contents: bool) -> Result<ObjectVersionRow> {
    Ok(if with_contents {
        match decode(row)? {
            ObjectRow::Live(object) => ObjectVersionRow::Live(Some(object)),
            ObjectRow::Tombstone(tombstone) => ObjectVersionRow::Tombstone(tombstone),
        }
    } else {
        decode_tombstone(row)?.map_or(ObjectVersionRow::Live(None), ObjectVersionRow::Tombstone)
    })
}

/// Parse a row as a tombstone, or `None` for a live row. Live rows are told apart by column
/// alone, so this also works on rows read with live objects' contents stripped.
pub fn decode_tombstone(row: &[(Bytes, Bytes)]) -> Result<Option<ObjectTombstone>> {
    if is_live(row)? {
        return Ok(None);
    }
    let status = cell(row, col::STATUS).context("tombstone has no status")?;
    let [byte] = status.as_ref() else {
        bail!("expected 1-byte status, got {} bytes", status.len());
    };
    let status = match ObjectStatus::from_byte(*byte)? {
        ObjectStatus::Live => bail!("tombstone has status Live"),
        status => status,
    };
    let transaction = cell(row, col::TRANSACTION)
        .map(|digest| {
            <[u8; 32]>::try_from(digest.as_ref())
                .map(TransactionDigest::new)
                .with_context(|| {
                    format!(
                        "expected 32-byte transaction digest, got {} bytes",
                        digest.len()
                    )
                })
        })
        .transpose()?;
    Ok(Some(ObjectTombstone {
        status,
        transaction,
    }))
}

/// The value of `row`'s cell in `column`, if it has one.
fn cell<'row>(row: &'row [(Bytes, Bytes)], column: &str) -> Option<&'row Bytes> {
    row.iter()
        .find(|(name, _)| name == column.as_bytes())
        .map(|(_, value)| value)
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

    fn cells<const N: usize>(encoded: [(&'static str, Bytes); N]) -> Vec<(Bytes, Bytes)> {
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
        let transaction = TransactionDigest::random();
        for status in [ObjectStatus::Deleted, ObjectStatus::Wrapped] {
            assert_eq!(
                decode(&cells(encode_tombstone(status, &transaction).unwrap())).unwrap(),
                ObjectRow::Tombstone(ObjectTombstone {
                    status,
                    transaction: Some(transaction)
                }),
            );
        }
    }

    /// A tombstone's transaction is its raw digest in column `t`, decoded when present, also
    /// from rows read with live objects' contents stripped, which decode as live.
    #[test]
    fn decode_reads_tombstone_transactions() {
        let transaction = TransactionDigest::random();
        let mut row = cells(encode_tombstone(ObjectStatus::Deleted, &transaction).unwrap());
        let tombstone = ObjectTombstone {
            status: ObjectStatus::Deleted,
            transaction: Some(transaction),
        };
        assert_eq!(decode(&row).unwrap(), ObjectRow::Tombstone(tombstone));
        assert_eq!(decode_tombstone(&row).unwrap(), Some(tombstone));

        let stripped_live = vec![(Bytes::from_static(col::OBJECT.as_bytes()), Bytes::new())];
        assert_eq!(decode_tombstone(&stripped_live).unwrap(), None);

        let short = &transaction.inner()[..31];
        row.last_mut().unwrap().1 = Bytes::copy_from_slice(short);
        assert!(decode(&row).is_err(), "31-byte transaction decoded");
    }

    /// A tombstone stores the removing transaction as its raw digest in column `t`. A row without
    /// that cell, written before the column existed, decodes with no transaction.
    #[test]
    fn tombstone_transaction_cell() {
        let transaction = TransactionDigest::random();
        for (byte, status) in [(1u8, ObjectStatus::Deleted), (2, ObjectStatus::Wrapped)] {
            let with_transaction = cells(encode_tombstone(status, &transaction).unwrap());
            assert_eq!(
                with_transaction,
                vec![
                    (Bytes::from_static(b"s"), Bytes::copy_from_slice(&[byte])),
                    (
                        Bytes::from_static(b"t"),
                        Bytes::copy_from_slice(transaction.inner())
                    ),
                ],
            );
            let status_only = vec![with_transaction[0].clone()];
            for (row, transaction) in [(with_transaction, Some(transaction)), (status_only, None)] {
                assert_eq!(
                    decode(&row).unwrap(),
                    ObjectRow::Tombstone(ObjectTombstone {
                        status,
                        transaction
                    })
                );
            }
        }
    }

    /// The cell is chosen by column, not position, and unknown columns are ignored.
    #[test]
    fn decode_selects_cell_by_column() {
        let transaction = TransactionDigest::random();
        let mut row = vec![(Bytes::from_static(b"other"), Bytes::from_static(b"junk"))];
        row.extend(cells(
            encode_tombstone(ObjectStatus::Wrapped, &transaction).unwrap(),
        ));
        assert_eq!(
            decode(&row).unwrap(),
            ObjectRow::Tombstone(ObjectTombstone {
                status: ObjectStatus::Wrapped,
                transaction: Some(transaction)
            })
        );
    }

    #[test]
    fn decode_rejects_malformed_rows() {
        let object = Object::with_id_owner_for_testing(ObjectID::random(), SuiAddress::ZERO);
        let transaction = TransactionDigest::random();
        let mut both = cells(encode(&object).unwrap());
        both.extend(cells(
            encode_tombstone(ObjectStatus::Deleted, &transaction).unwrap(),
        ));
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
        assert!(encode_tombstone(ObjectStatus::Live, &transaction).is_err());
    }
}
