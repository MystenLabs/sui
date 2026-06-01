// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `(ObjectID, version)` → `StoredObject`.
//!
//! Holds every version of every object that has ever existed. A
//! prefix scan on the 32-byte object id walks all versions of one
//! object in ascending order.

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

/// Build a `StoredObject` row from a canonical [`Object`].
///
/// BCS-encode failures here would indicate either OOM or a bug in
/// the type's `Serialize` impl; we panic rather than thread a
/// `Result` through every call site.
pub fn store(object: &Object) -> Value {
    let bcs = bcs::to_bytes(object).expect("bcs encode Object");
    Protobuf(StoredObject { bcs: bcs.into() })
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Look up a specific version of an object.
    ///
    /// For the "latest live version" of an object id, callers
    /// should go through `live_objects` first to resolve the
    /// version, then pass it here.
    pub fn get_object_by_key(
        &self,
        id: ObjectID,
        version: SequenceNumber,
    ) -> Result<Option<Object>, Error> {
        let Some(stored) = self.objects.get(&Key { id, version })? else {
            return Ok(None);
        };
        let object: Object = bcs::from_bytes(&stored.into_inner().bcs)
            .map_err(|e| DecodeError::with_source("bcs decode Object", e))?;
        Ok(Some(object))
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

        assert_eq!(
            schema.get_object_by_key(id, v1).unwrap().unwrap(),
            o1,
        );
        assert_eq!(
            schema.get_object_by_key(id, v2).unwrap().unwrap(),
            o2,
        );
    }
}
