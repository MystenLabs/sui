// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! [`ObjectStore`] adapter — point lookups by id or `(id, version)`.
//!
//! Delegates to the inherent helpers
//! [`RpcStoreSchema::get_object`] (latest live version via
//! `live_objects` → `objects`) and
//! [`RpcStoreSchema::get_object_by_key`] (direct version lookup).
//! The trait returns `Option<Object>`, so storage errors are
//! logged and surfaced as `None`; callers that need to distinguish
//! "missing" from "errored" should go through the inherent helpers
//! directly.

use sui_consistent_store::reader::Reader;
use sui_types::base_types::ObjectID;
use sui_types::base_types::VersionNumber;
use sui_types::object::Object;
use sui_types::storage::ObjectStore;
use tracing::error;

use crate::reader::RpcStoreReader;

impl<R: Reader + Send + Sync> ObjectStore for RpcStoreReader<R> {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        match self.schema().get_object(*object_id) {
            Ok(object) => object,
            Err(e) => {
                error!(?object_id, "get_object: {e:#}");
                None
            }
        }
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: VersionNumber) -> Option<Object> {
        match self.schema().get_object_by_key(*object_id, version) {
            Ok(object) => object,
            Err(e) => {
                error!(?object_id, ?version, "get_object_by_key: {e:#}");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SuiAddress;
    use sui_types::object::Object;
    use sui_types::storage::ObjectKey;
    use sui_types::storage::ObjectStore;

    use crate::RpcStoreSchema;
    use crate::reader::RpcStoreReader;
    use crate::schema::live_objects;
    use crate::schema::objects;
    use crate::schema::primitives::U64Varint;

    fn open() -> (tempfile::TempDir, Db, RpcStoreSchema) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    fn dummy(id: ObjectID) -> Object {
        Object::with_id_owner_for_testing(id, SuiAddress::ZERO)
    }

    fn seed(db: &Db, schema: &RpcStoreSchema, object: &Object) {
        let mut batch = db.batch();
        batch
            .put(
                &schema.objects,
                &objects::Key {
                    id: object.id(),
                    version: object.version(),
                },
                &objects::store(object),
            )
            .unwrap();
        batch
            .put(
                &schema.live_objects,
                &live_objects::Key(object.id()),
                &U64Varint(object.version().value()),
            )
            .unwrap();
        batch.commit().unwrap();
    }

    #[test]
    fn get_object_returns_latest_via_live_pointer() {
        let (_dir, db, schema) = open();
        let object = dummy(ObjectID::from_single_byte(1));
        seed(&db, &schema, &object);

        let reader = RpcStoreReader::new(db.clone(), Arc::new(schema));
        let read = reader
            .get_object(&object.id())
            .expect("object present via live pointer");
        assert_eq!(read, object);
    }

    #[test]
    fn get_object_returns_none_for_missing_id() {
        let (_dir, db, schema) = open();
        let reader = RpcStoreReader::new(db.clone(), Arc::new(schema));
        assert!(reader.get_object(&ObjectID::from_single_byte(7)).is_none());
    }

    #[test]
    fn get_object_by_key_returns_exact_version() {
        let (_dir, db, schema) = open();
        let object = dummy(ObjectID::from_single_byte(2));
        seed(&db, &schema, &object);

        let reader = RpcStoreReader::new(db.clone(), Arc::new(schema));
        let read = reader
            .get_object_by_key(&object.id(), object.version())
            .expect("object present at exact version");
        assert_eq!(read, object);
    }

    #[test]
    fn get_object_by_key_returns_none_for_unknown_version() {
        let (_dir, db, schema) = open();
        let object = dummy(ObjectID::from_single_byte(3));
        seed(&db, &schema, &object);

        let reader = RpcStoreReader::new(db.clone(), Arc::new(schema));
        let missing_version = sui_types::base_types::SequenceNumber::from_u64(99);
        assert!(
            reader
                .get_object_by_key(&object.id(), missing_version)
                .is_none()
        );
    }

    #[test]
    fn multi_get_objects_uses_default_delegate() {
        let (_dir, db, schema) = open();
        let o1 = dummy(ObjectID::from_single_byte(4));
        let o2 = dummy(ObjectID::from_single_byte(5));
        seed(&db, &schema, &o1);
        // Note: o2 is intentionally not seeded.

        let reader = RpcStoreReader::new(db.clone(), Arc::new(schema));
        let results = reader.multi_get_objects(&[o1.id(), o2.id()]);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].as_ref(), Some(&o1));
        assert!(results[1].is_none());
    }

    #[test]
    fn multi_get_objects_by_key_uses_default_delegate() {
        let (_dir, db, schema) = open();
        let o1 = dummy(ObjectID::from_single_byte(6));
        let o2 = dummy(ObjectID::from_single_byte(7));
        seed(&db, &schema, &o1);
        seed(&db, &schema, &o2);

        let reader = RpcStoreReader::new(db.clone(), Arc::new(schema));
        let keys = vec![
            ObjectKey(o1.id(), o1.version()),
            ObjectKey(o2.id(), o2.version()),
        ];
        let results = reader.multi_get_objects_by_key(&keys);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].as_ref(), Some(&o1));
        assert_eq!(results[1].as_ref(), Some(&o2));
    }
}
