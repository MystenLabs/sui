// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `ObjectID` → latest live `version`.
//!
//! Resolves the latest live version of an object. Callers then read
//! the corresponding row from [`super::objects`](super::objects) to
//! fetch the full object — the convenience method
//! [`RpcStoreSchema::get_object`](super::RpcStoreSchema::get_object)
//! composes both lookups in one call.
//!
//! Both `Key` and `Value` are thin newtypes (a 32-byte `ObjectID`
//! and a varint-encoded `u64`), so no `store` helper is provided —
//! indexer pipelines stage writes directly via
//! `batch.put(&schema.live_objects, &Key(id), &U64Varint(version.value()))`.

use bytes::Buf;
use bytes::BufMut;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::object::Object;

use crate::schema::keys::U64Varint;

pub const NAME: &str = "live_objects";

/// Wrapper around `ObjectID` whose encoding is the raw 32 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key(pub ObjectID);

pub type Value = U64Varint;

impl Encode for Key {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.0.as_ref());
        Ok(())
    }
}

impl Decode for Key {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() != ObjectID::LENGTH {
            return Err(DecodeError::msg(format!(
                "expected {} bytes for {NAME} Key, got {}",
                ObjectID::LENGTH,
                buf.remaining(),
            )));
        }
        let mut bytes = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut bytes);
        Ok(Key(ObjectID::new(bytes)))
    }
}

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Look up the latest live version of an object id, without
    /// fetching the object itself.
    pub fn get_live_object_version(&self, id: ObjectID) -> Result<Option<SequenceNumber>, Error> {
        Ok(self
            .live_objects
            .get(&Key(id))?
            .map(|v| SequenceNumber::from_u64(v.0)))
    }

    /// Look up the latest live version of an object by id.
    ///
    /// Composes the [`live_objects`](super::live_objects) lookup
    /// (id → version) with the [`objects`](super::objects) lookup
    /// ((id, version) → `Object`) so callers don't have to chain
    /// them manually.
    ///
    /// Returns `Ok(None)` if either side is missing. The two CFs
    /// are written together by the same indexer pipeline, so an
    /// inconsistency between them indicates a bug rather than an
    /// expected condition; we surface the bug as a missing object
    /// rather than panicking.
    pub fn get_object(&self, id: ObjectID) -> Result<Option<Object>, Error> {
        let Some(version) = self.get_live_object_version(id)? else {
            return Ok(None);
        };
        self.get_object_by_key(id, version)
    }
}

#[cfg(test)]
mod tests {
    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_types::base_types::SuiAddress;

    use super::*;
    use crate::RpcStoreSchema;
    use crate::schema::objects;

    fn fresh_db() -> (tempfile::TempDir, sui_consistent_store::Db, RpcStoreSchema) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    fn dummy_object(id: ObjectID) -> Object {
        Object::with_id_owner_for_testing(id, SuiAddress::ZERO)
    }

    #[test]
    fn get_live_object_version_returns_none_for_unknown_id() {
        let (_dir, _db, schema) = fresh_db();
        assert!(
            schema
                .get_live_object_version(ObjectID::random())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn live_object_version_round_trips() {
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();

        let mut batch = db.batch();
        batch
            .put(&schema.live_objects, &Key(id), &U64Varint(7))
            .unwrap();
        batch.commit().unwrap();

        assert_eq!(
            schema.get_live_object_version(id).unwrap(),
            Some(SequenceNumber::from_u64(7)),
        );
    }

    #[test]
    fn get_object_returns_none_when_live_pointer_missing() {
        // `objects` has a row at (id, v), but no live pointer
        // points at it — `get_object` should miss.
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();
        let object = dummy_object(id);
        let version = object.version();

        let mut batch = db.batch();
        batch
            .put(
                &schema.objects,
                &objects::Key { id, version },
                &objects::store(&object),
            )
            .unwrap();
        batch.commit().unwrap();

        assert!(schema.get_object(id).unwrap().is_none());
    }

    #[test]
    fn get_object_returns_none_when_object_row_missing() {
        // Inconsistent state: live pointer references a version
        // that has no corresponding `objects` row. Surface as a
        // miss rather than panicking.
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();

        let mut batch = db.batch();
        batch
            .put(&schema.live_objects, &Key(id), &U64Varint(7))
            .unwrap();
        batch.commit().unwrap();

        assert!(schema.get_object(id).unwrap().is_none());
    }

    #[test]
    fn get_object_composes_live_and_objects() {
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();
        let object = dummy_object(id);
        let version = object.version();

        let mut batch = db.batch();
        batch
            .put(
                &schema.objects,
                &objects::Key { id, version },
                &objects::store(&object),
            )
            .unwrap();
        batch
            .put(&schema.live_objects, &Key(id), &U64Varint(version.value()))
            .unwrap();
        batch.commit().unwrap();

        let read = schema.get_object(id).unwrap().expect("object present");
        assert_eq!(read, object);
    }
}
