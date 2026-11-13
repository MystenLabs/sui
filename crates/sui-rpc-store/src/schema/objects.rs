// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `(ObjectID, version)` → `StoredObject`.
//!
//! Holds every version of every object that has ever existed plus
//! tombstones for versions at which an object was deleted or
//! wrapped. A prefix scan on the 32-byte object id walks all
//! versions of one object in ascending order; the value at each
//! position is either a BCS-encoded live [`Object`] or a tombstone
//! marker carrying the [`TombstoneKind`].
//!
//! Tombstones let version-bounded reads distinguish three states
//! at `(id, version)`: a live row (object existed at that version),
//! a tombstone row (object was removed at that version), and a
//! missing row (object did not exist at that version).

use std::ops::RangeInclusive;

use bytes::Buf;
use bytes::BufMut;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::Protobuf;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::digests::ObjectDigest;
use sui_types::object::Object;
use sui_types::storage::ObjectOrTombstone;

use crate::proto::StoredObject;
use crate::proto::StoredObjectTombstone;
use crate::proto::StoredObjectTombstoneKind;
use crate::proto::stored_object;

pub const NAME: &str = "objects";

/// `(ObjectID, version)`. Encoded as 32 raw id bytes followed by an
/// 8-byte big-endian version, so versions of the same object cluster
/// in sorted order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    pub id: ObjectID,
    pub version: SequenceNumber,
}

pub type Value = Protobuf<StoredObject>;

impl Encode for Key {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.id.as_ref());
        buf.put_slice(&self.version.value().to_be_bytes());
        Ok(())
    }
}

impl Decode for Key {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() != ObjectID::LENGTH + 8 {
            return Err(DecodeError::msg(format!(
                "expected {} bytes for {NAME} Key, got {}",
                ObjectID::LENGTH + 8,
                buf.remaining(),
            )));
        }
        let mut id_bytes = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut id_bytes);
        let version = SequenceNumber::from_u64(buf.get_u64());
        Ok(Key {
            id: ObjectID::new(id_bytes),
            version,
        })
    }
}

pub fn options(resolver: &sui_consistent_store::CfOptionsResolver) -> rocksdb::Options {
    resolver.options(NAME)
}

/// Prefix encoder for "every version of `id`". Encodes as the 32 raw
/// id bytes -- the leading bytes of every `Key` whose `id` matches, so
/// a prefix scan walks one object's versions in isolation.
pub struct ObjectIdPrefix(pub ObjectID);

impl Encode for ObjectIdPrefix {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.0.as_ref());
        Ok(())
    }
}

/// Why a tombstone row was written: the object was either
/// `Deleted` (including the `unwrapped_then_deleted` shape) or
/// `Wrapped` (nested inside another object and removed from the
/// live set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TombstoneKind {
    Deleted,
    Wrapped,
}

/// Typed view of a row in the `objects` CF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// The row carries a live version of the object.
    Live(Object),
    /// The row marks the version at which the object was removed
    /// from the live set.
    Tombstone(TombstoneKind),
}

impl Status {
    /// Convenience for callers that only care about the live case.
    pub fn into_live(self) -> Option<Object> {
        match self {
            Status::Live(object) => Some(object),
            Status::Tombstone(_) => None,
        }
    }
}

/// Build a live `StoredObject` row from a canonical [`Object`].
///
/// BCS-encode failures here would indicate either OOM or a bug in
/// the type's `Serialize` impl; we panic rather than thread a
/// `Result` through every call site.
pub fn store(object: &Object) -> Value {
    let bcs = bcs::to_bytes(object).expect("bcs encode Object");
    Protobuf(StoredObject {
        kind: Some(stored_object::Kind::Bcs(bcs.into())),
    })
}

/// Build a tombstone `StoredObject` row marking the version at
/// which an object was deleted or wrapped.
pub fn tombstone(kind: TombstoneKind) -> Value {
    let proto_kind = match kind {
        TombstoneKind::Deleted => StoredObjectTombstoneKind::Deleted,
        TombstoneKind::Wrapped => StoredObjectTombstoneKind::Wrapped,
    };
    Protobuf(StoredObject {
        kind: Some(stored_object::Kind::Tombstone(StoredObjectTombstone {
            kind: proto_kind as i32,
        })),
    })
}

/// Decode a stored row into the typed [`Status`].
fn decode(stored: StoredObject) -> Result<Status, Error> {
    match stored.kind {
        Some(stored_object::Kind::Bcs(bcs)) => {
            let object: Object = bcs::from_bytes(&bcs)
                .map_err(|e| DecodeError::with_source("bcs decode Object", e))?;
            Ok(Status::Live(object))
        }
        Some(stored_object::Kind::Tombstone(t)) => {
            let kind = match StoredObjectTombstoneKind::try_from(t.kind) {
                Ok(StoredObjectTombstoneKind::Deleted) => TombstoneKind::Deleted,
                Ok(StoredObjectTombstoneKind::Wrapped) => TombstoneKind::Wrapped,
                Ok(StoredObjectTombstoneKind::Unspecified) | Err(_) => {
                    return Err(DecodeError::msg(format!(
                        "unrecognised tombstone kind: {}",
                        t.kind,
                    ))
                    .into());
                }
            };
            Ok(Status::Tombstone(kind))
        }
        None => Err(DecodeError::msg("StoredObject row missing kind").into()),
    }
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Look up a specific version of an object, returning the live
    /// object if and only if the row at `(id, version)` is a live
    /// version. Tombstone rows and missing rows both return `None`;
    /// callers that need to distinguish the two should use
    /// [`get_object_status_by_key`](Self::get_object_status_by_key).
    ///
    /// For the "latest live version" of an object id, use
    /// [`get_object`](Self::get_object), which reverse-scans this CF
    /// for the greatest version.
    pub fn get_object_by_key(
        &self,
        id: ObjectID,
        version: SequenceNumber,
    ) -> Result<Option<Object>, Error> {
        Ok(self
            .get_object_status_by_key(id, version)?
            .and_then(Status::into_live))
    }

    /// Look up a specific version of an object, returning the
    /// typed [`Status`] so callers can distinguish live versions
    /// from tombstones. `Ok(None)` means no row was written at
    /// `(id, version)`.
    pub fn get_object_status_by_key(
        &self,
        id: ObjectID,
        version: SequenceNumber,
    ) -> Result<Option<Status>, Error> {
        let Some(stored) = self.objects.get(&Key { id, version })? else {
            return Ok(None);
        };
        Ok(Some(decode(stored.into_inner())?))
    }

    /// Look up the latest live version of an object by id.
    ///
    /// A single reverse prefix scan takes the object's greatest
    /// `(id, version)` row and decodes it, returning the live object or
    /// `None` if that row is a tombstone. This mirrors the validator
    /// perpetual store's `get_object` (reverse scan, then collapse
    /// `Deleted` / `Wrapped` to `None`). Returns `Ok(None)` when the
    /// object has no recorded version.
    pub fn get_object(&self, id: ObjectID) -> Result<Option<Object>, Error> {
        let Some(row) = self.objects.iter_rev_prefix(&ObjectIdPrefix(id))?.next() else {
            return Ok(None);
        };
        let (_key, value) = row?;
        Ok(decode(value.into_inner())?.into_live())
    }

    /// Look up the newest version of an object at or below `version`.
    ///
    /// A single bounded reverse scan takes the greatest `(id, v)` row with
    /// `v <= version`, returning the live object or `None` if that row is
    /// a tombstone, like the validator perpetual store's
    /// `find_object_lt_or_eq_version`. Returns `Ok(None)` when the object
    /// has no recorded version at or below `version`.
    pub fn get_object_lt_or_eq_version(
        &self,
        id: ObjectID,
        version: SequenceNumber,
    ) -> Result<Option<Object>, Error> {
        let lowest = Key {
            id,
            version: SequenceNumber::MIN,
        };
        let Some(row) = self.objects.iter_rev(lowest..=Key { id, version })?.next() else {
            return Ok(None);
        };
        let (_key, value) = row?;
        Ok(decode(value.into_inner())?.into_live())
    }

    /// Iterate the versions of `id` within `versions`, oldest first, or
    /// newest first if `descending`, each a live object or the tombstone at
    /// which the object was deleted or wrapped.
    pub fn object_versions_iter(
        &self,
        id: ObjectID,
        versions: RangeInclusive<SequenceNumber>,
        descending: bool,
    ) -> Result<impl Iterator<Item = Result<ObjectOrTombstone, Error>> + '_, Error> {
        let range = Key {
            id,
            version: *versions.start(),
        }..=Key {
            id,
            version: *versions.end(),
        };
        let rows: Box<dyn Iterator<Item = Result<(Key, Value), Error>> + '_> = if descending {
            Box::new(self.objects.iter_rev(range)?)
        } else {
            Box::new(self.objects.iter(range)?)
        };
        Ok(rows.map(|row| {
            let (key, value) = row?;
            Ok(match decode(value.into_inner())? {
                Status::Live(object) => ObjectOrTombstone::Object(object),
                Status::Tombstone(TombstoneKind::Deleted) => ObjectOrTombstone::Tombstone((
                    key.id,
                    key.version,
                    ObjectDigest::OBJECT_DIGEST_DELETED,
                )),
                Status::Tombstone(TombstoneKind::Wrapped) => ObjectOrTombstone::Tombstone((
                    key.id,
                    key.version,
                    ObjectDigest::OBJECT_DIGEST_WRAPPED,
                )),
            })
        }))
    }
}

#[cfg(test)]
mod tests {
    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_types::base_types::SuiAddress;
    use sui_types::object::Owner;

    use super::*;
    use crate::RpcStoreSchema;

    fn fresh_db() -> (tempfile::TempDir, sui_consistent_store::Db, RpcStoreSchema) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    fn dummy_object(id: ObjectID) -> Object {
        Object::with_id_owner_for_testing(id, SuiAddress::ZERO)
    }

    #[test]
    fn get_returns_none_for_unknown_key() {
        let (_dir, _db, schema) = fresh_db();
        let id = ObjectID::random();
        assert!(
            schema
                .get_object_by_key(id, SequenceNumber::from_u64(1))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn store_then_get_round_trips() {
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();
        let object = dummy_object(id);
        let version = object.version();

        let mut batch = db.batch();
        batch
            .put(&schema.objects, &Key { id, version }, &store(&object))
            .unwrap();
        batch.commit().unwrap();

        let read = schema
            .get_object_by_key(id, version)
            .unwrap()
            .expect("object present");
        assert_eq!(read, object);
    }

    #[test]
    fn tombstone_round_trips_with_kind() {
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();
        let v_del = SequenceNumber::from_u64(5);
        let v_wrap = SequenceNumber::from_u64(9);

        let mut batch = db.batch();
        batch
            .put(
                &schema.objects,
                &Key { id, version: v_del },
                &tombstone(TombstoneKind::Deleted),
            )
            .unwrap();
        batch
            .put(
                &schema.objects,
                &Key {
                    id,
                    version: v_wrap,
                },
                &tombstone(TombstoneKind::Wrapped),
            )
            .unwrap();
        batch.commit().unwrap();

        assert_eq!(
            schema.get_object_status_by_key(id, v_del).unwrap(),
            Some(Status::Tombstone(TombstoneKind::Deleted)),
        );
        assert_eq!(
            schema.get_object_status_by_key(id, v_wrap).unwrap(),
            Some(Status::Tombstone(TombstoneKind::Wrapped)),
        );
        // The live-only accessor flattens both tombstones to None.
        assert!(schema.get_object_by_key(id, v_del).unwrap().is_none());
        assert!(schema.get_object_by_key(id, v_wrap).unwrap().is_none());
    }

    #[test]
    fn get_object_status_distinguishes_missing_from_tombstone() {
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();
        let version = SequenceNumber::from_u64(3);

        // No row at all: status is None.
        assert!(
            schema
                .get_object_status_by_key(id, version)
                .unwrap()
                .is_none()
        );

        // Write a tombstone at `version` and observe the
        // distinction from the missing-row case above.
        let mut batch = db.batch();
        batch
            .put(
                &schema.objects,
                &Key { id, version },
                &tombstone(TombstoneKind::Deleted),
            )
            .unwrap();
        batch.commit().unwrap();
        assert_eq!(
            schema.get_object_status_by_key(id, version).unwrap(),
            Some(Status::Tombstone(TombstoneKind::Deleted)),
        );
    }

    #[test]
    fn distinct_versions_of_same_id_are_isolated() {
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();
        let v1 = SequenceNumber::from_u64(1);
        let v2 = SequenceNumber::from_u64(2);
        let o1 = dummy_object(id);
        let o2 = dummy_object(id);

        let mut batch = db.batch();
        batch
            .put(&schema.objects, &Key { id, version: v1 }, &store(&o1))
            .unwrap();
        batch
            .put(&schema.objects, &Key { id, version: v2 }, &store(&o2))
            .unwrap();
        batch.commit().unwrap();

        assert_eq!(schema.get_object_by_key(id, v1).unwrap().unwrap(), o1,);
        assert_eq!(schema.get_object_by_key(id, v2).unwrap().unwrap(), o2,);
    }

    #[test]
    fn get_object_returns_the_latest_live_version() {
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();
        let object = dummy_object(id);
        let version = object.version();

        let mut batch = db.batch();
        batch
            .put(&schema.objects, &Key { id, version }, &store(&object))
            .unwrap();
        batch.commit().unwrap();

        assert_eq!(schema.get_object(id).unwrap(), Some(object));
    }

    #[test]
    fn get_object_returns_none_when_latest_is_tombstone() {
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();
        let live = dummy_object(id);
        let live_version = SequenceNumber::from_u64(1);
        let tombstone_version = SequenceNumber::from_u64(5);

        let mut batch = db.batch();
        // An older live version, then a newer tombstone marking the
        // object's removal.
        batch
            .put(
                &schema.objects,
                &Key {
                    id,
                    version: live_version,
                },
                &store(&live),
            )
            .unwrap();
        batch
            .put(
                &schema.objects,
                &Key {
                    id,
                    version: tombstone_version,
                },
                &tombstone(TombstoneKind::Deleted),
            )
            .unwrap();
        batch.commit().unwrap();

        // The greatest row is the tombstone, which collapses to "no
        // live object" even though an older live version is still
        // present.
        assert!(schema.get_object(id).unwrap().is_none());
    }

    #[test]
    fn get_object_isolates_objects_by_id() {
        let (_dir, db, schema) = fresh_db();
        let a = ObjectID::from_single_byte(1);
        let b = ObjectID::from_single_byte(2);
        let oa = dummy_object(a);
        let ob = dummy_object(b);

        let mut batch = db.batch();
        batch
            .put(
                &schema.objects,
                &Key {
                    id: a,
                    version: SequenceNumber::from_u64(9),
                },
                &store(&oa),
            )
            .unwrap();
        batch
            .put(
                &schema.objects,
                &Key {
                    id: b,
                    version: SequenceNumber::from_u64(4),
                },
                &store(&ob),
            )
            .unwrap();
        batch.commit().unwrap();

        // The reverse prefix scan must not spill across the id bound.
        assert_eq!(schema.get_object(a).unwrap(), Some(oa));
        assert_eq!(schema.get_object(b).unwrap(), Some(ob));
    }

    #[test]
    fn get_object_lt_or_eq_version_stops_at_tombstones() {
        let (_dir, db, schema) = fresh_db();
        let below = ObjectID::from_single_byte(1);
        let id = ObjectID::from_single_byte(2);
        let above = ObjectID::from_single_byte(3);
        let v = SequenceNumber::from_u64;
        let live = |id, version| {
            store(&Object::with_id_owner_version_for_testing(
                id,
                v(version),
                Owner::AddressOwner(SuiAddress::ZERO),
            ))
        };

        // `id` is live at 1, wrapped at 3, unwrapped at 4 and deleted at 6. The neighboring
        // objects' rows sit right next to `id`'s in key order, on either side.
        let mut batch = db.batch();
        for (row_id, version, value) in [
            (below, u64::MAX, live(below, u64::MAX)),
            (id, 1, live(id, 1)),
            (id, 3, tombstone(TombstoneKind::Wrapped)),
            (id, 4, live(id, 4)),
            (id, 6, tombstone(TombstoneKind::Deleted)),
            (above, 0, live(above, 0)),
        ] {
            let key = Key {
                id: row_id,
                version: v(version),
            };
            batch.put(&schema.objects, &key, &value).unwrap();
        }
        batch.commit().unwrap();

        for (version, expected) in [
            (0, None),
            (1, Some(1)),
            (2, Some(1)),
            (3, None),
            (4, Some(4)),
            (5, Some(4)),
            (6, None),
            (u64::MAX, None),
        ] {
            let found = schema
                .get_object_lt_or_eq_version(id, v(version))
                .unwrap()
                .map(|object| (object.id(), object.version().value()));
            assert_eq!(
                found,
                expected.map(|version| (id, version)),
                "at or below version {version}"
            );
        }
    }

    #[test]
    fn object_versions_iter_lists_tombstones_within_bounds() {
        let (_dir, db, schema) = fresh_db();
        let below = ObjectID::from_single_byte(1);
        let id = ObjectID::from_single_byte(2);
        let above = ObjectID::from_single_byte(3);
        let v = SequenceNumber::from_u64;
        let live = |id, version| {
            store(&Object::with_id_owner_version_for_testing(
                id,
                v(version),
                Owner::AddressOwner(SuiAddress::ZERO),
            ))
        };

        // `id` is live at 1 and 2, wrapped at 3, unwrapped at 4, live at 5 and deleted at 6. The
        // neighboring objects' rows sit right next to `id`'s in key order, on either side.
        let mut batch = db.batch();
        for (row_id, version, value) in [
            (below, u64::MAX, live(below, u64::MAX)),
            (id, 1, live(id, 1)),
            (id, 2, live(id, 2)),
            (id, 3, tombstone(TombstoneKind::Wrapped)),
            (id, 4, live(id, 4)),
            (id, 5, live(id, 5)),
            (id, 6, tombstone(TombstoneKind::Deleted)),
            (above, 0, live(above, 0)),
        ] {
            let key = Key {
                id: row_id,
                version: v(version),
            };
            batch.put(&schema.objects, &key, &value).unwrap();
        }
        batch.commit().unwrap();

        // Each listed version, with `L` for a live object, `W` for wrapped and `D` for deleted.
        for (low, high, descending, expected) in [
            (
                0,
                u64::MAX,
                false,
                vec![(1, 'L'), (2, 'L'), (3, 'W'), (4, 'L'), (5, 'L'), (6, 'D')],
            ),
            (
                0,
                u64::MAX,
                true,
                vec![(6, 'D'), (5, 'L'), (4, 'L'), (3, 'W'), (2, 'L'), (1, 'L')],
            ),
            // Both ends are inclusive, and tombstones are listed in place.
            (2, 4, false, vec![(2, 'L'), (3, 'W'), (4, 'L')]),
            (2, 4, true, vec![(4, 'L'), (3, 'W'), (2, 'L')]),
            (3, 3, false, vec![(3, 'W')]),
            (7, u64::MAX, true, vec![]),
        ] {
            let listed: Vec<_> = schema
                .object_versions_iter(id, v(low)..=v(high), descending)
                .unwrap()
                .map(|item| match item.unwrap() {
                    ObjectOrTombstone::Object(object) => {
                        assert_eq!(object.id(), id);
                        (object.version().value(), 'L')
                    }
                    ObjectOrTombstone::Tombstone((object_id, version, digest)) => {
                        assert_eq!(object_id, id);
                        let kind = if digest.is_deleted() {
                            'D'
                        } else {
                            assert!(digest.is_wrapped());
                            'W'
                        };
                        (version.value(), kind)
                    }
                })
                .collect();
            assert_eq!(
                listed, expected,
                "versions {low}..={high}, descending {descending}"
            );
        }
    }
}
