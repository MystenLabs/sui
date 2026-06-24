// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `(StructTag, ObjectID)` → latest live `version`.
//!
//! Type-only filtering: list every live object of a given Move
//! type regardless of owner. The `StructTag` component is
//! BCS-encoded, so [`TypeFilter`]
//! values double as valid prefix encoders for this CF.

use bytes::Buf;
use bytes::BufMut;
use move_core_types::language_storage::StructTag;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::Iter;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;
use sui_types::base_types::ObjectID;
use sui_types::object::Object;

use crate::schema::keys::U64Varint;
use crate::schema::type_filter::TypeFilter;

pub const NAME: &str = "object_by_type";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key {
    pub type_: StructTag,
    pub object_id: ObjectID,
}

pub type Value = U64Varint;

impl Encode for Key {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        let type_bytes = bcs::to_bytes(&self.type_)
            .map_err(|e| EncodeError::with_source("bcs encode StructTag", e))?;
        buf.put_slice(&type_bytes);
        buf.put_slice(self.object_id.as_ref());
        Ok(())
    }
}

impl Decode for Key {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() < ObjectID::LENGTH {
            return Err(DecodeError::msg(format!(
                "{NAME} Key too short: {} bytes",
                buf.remaining(),
            )));
        }
        let prefix = buf.copy_to_bytes(buf.remaining() - ObjectID::LENGTH);
        let type_: StructTag = bcs::from_bytes(&prefix)
            .map_err(|e| DecodeError::with_source("bcs decode StructTag", e))?;
        let mut id = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut id);
        Ok(Key {
            type_,
            object_id: ObjectID::new(id),
        })
    }
}

pub fn options(resolver: &sui_consistent_store::CfOptionsResolver) -> rocksdb::Options {
    resolver.options(NAME)
}

/// Build the `(Key, Value)` pair indexing a Move object by its
/// Move type.
///
/// Returns `None` for objects that aren't Move objects (packages,
/// for example) — those have no `StructTag` and aren't part of
/// this index.
pub fn store(object: &Object) -> Option<(Key, U64Varint)> {
    let type_: StructTag = object.type_()?.clone().into();
    Some((
        Key {
            type_,
            object_id: object.id(),
        },
        U64Varint(object.version().value()),
    ))
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Iterate over every live object whose Move type matches
    /// `type_filter`, regardless of owner. See
    /// [`TypeFilter`] for the
    /// matching contract.
    ///
    /// `TypeFilter` encodes to the same leading bytes that
    /// [`Key`] uses for its `type_` component, so the filter
    /// value passes through directly as the prefix.
    pub fn iter_objects_of_type<'a>(
        &'a self,
        type_filter: &'a TypeFilter,
    ) -> Result<Iter<'a, Key, U64Varint>, Error> {
        self.object_by_type.iter_prefix(type_filter)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use move_core_types::identifier::Identifier;
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
    fn store_derives_key_from_object() {
        let id = ObjectID::random();
        let object = dummy_object(id);
        let (key, value) = store(&object).expect("Move object");
        assert_eq!(key.object_id, id);
        assert_eq!(value.0, object.version().value());
    }

    #[test]
    fn iter_returns_empty_for_unknown_type() {
        let (_dir, _db, schema) = fresh_db();
        let bogus = TypeFilter::Package(SuiAddress::from_bytes([9u8; 32]).unwrap());
        let count = schema.iter_objects_of_type(&bogus).unwrap().count();
        assert_eq!(count, 0);
    }

    #[test]
    fn iter_finds_objects_matching_type_filter() {
        let (_dir, db, schema) = fresh_db();

        let mut target_ids = BTreeSet::new();
        let mut batch = db.batch();
        let mut shared_type = None;
        for _ in 0..3 {
            let id = ObjectID::random();
            target_ids.insert(id);
            let (k, v) = store(&dummy_object(id)).unwrap();
            shared_type.get_or_insert(k.type_.clone());
            batch.put(&schema.object_by_type, &k, &v).unwrap();
        }
        batch.commit().unwrap();

        let shared_type = shared_type.unwrap();
        let matching = TypeFilter::Type(shared_type.clone());
        let found: BTreeSet<ObjectID> = schema
            .iter_objects_of_type(&matching)
            .unwrap()
            .map(|res| res.unwrap().0.object_id)
            .collect();
        assert_eq!(found, target_ids);

        let mismatched = TypeFilter::Type(StructTag {
            name: Identifier::new("Other").unwrap(),
            ..shared_type
        });
        let mismatched_count = schema.iter_objects_of_type(&mismatched).unwrap().count();
        assert_eq!(mismatched_count, 0);
    }
}
