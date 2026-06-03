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
use sui_types::object::Object;

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

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
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
    /// For the "latest live version" of an object id, callers
    /// should go through `live_objects` first to resolve the
    /// version, then pass it here.
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
}

#[cfg(test)]
mod tests {
    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_types::base_types::SuiAddress;

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
}
