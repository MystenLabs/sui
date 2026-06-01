// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `(original_package_id, version)` → `PackageVersionInfo`.
//!
//! Lists every published version of a Move package and the storage
//! id under which each version lives.

use bytes::Buf;
use bytes::BufMut;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::Iter;
use sui_consistent_store::Protobuf;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;
use sui_types::base_types::ObjectID;

use crate::proto::PackageVersionInfo;

pub const NAME: &str = "package_versions";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    pub original_id: ObjectID,
    pub version: u64,
}

pub type Value = Protobuf<PackageVersionInfo>;

impl Encode for Key {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.original_id.as_ref());
        buf.put_slice(&self.version.to_be_bytes());
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
        let mut id = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut id);
        let version = buf.get_u64();
        Ok(Key {
            original_id: ObjectID::new(id),
            version,
        })
    }
}

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}

/// Build the `(Key, Value)` pair recording that version `version`
/// of the package originally published at `original_id` lives at
/// the on-chain storage id `storage_id`.
pub fn store(original_id: ObjectID, version: u64, storage_id: ObjectID) -> (Key, Value) {
    (
        Key {
            original_id,
            version,
        },
        Protobuf(PackageVersionInfo {
            storage_id: storage_id.to_vec().into(),
        }),
    )
}

/// Prefix encoder for "all versions of the package originally
/// published at `original_id`". Encodes as the 32 raw id bytes —
/// exactly the leading bytes of every `Key` whose `original_id`
/// matches.
pub struct OriginalIdPrefix(pub ObjectID);

impl Encode for OriginalIdPrefix {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.0.as_ref());
        Ok(())
    }
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Look up the on-chain storage id that holds a specific
    /// version of a package.
    pub fn get_package_storage_id(
        &self,
        original_id: ObjectID,
        version: u64,
    ) -> Result<Option<ObjectID>, Error> {
        let Some(stored) = self.package_versions.get(&Key {
            original_id,
            version,
        })?
        else {
            return Ok(None);
        };
        let bytes = stored.into_inner().storage_id;
        let array: [u8; ObjectID::LENGTH] = bytes.as_ref().try_into().map_err(|_| {
            DecodeError::msg(format!(
                "expected {} bytes for storage_id, got {}",
                ObjectID::LENGTH,
                bytes.len(),
            ))
        })?;
        Ok(Some(ObjectID::new(array)))
    }

    /// Iterate every version of the package originally published
    /// at `original_id`, in ascending version order.
    pub fn iter_package_versions(
        &self,
        original_id: ObjectID,
    ) -> Result<Iter<'_, Key, Value>, Error> {
        self.package_versions
            .iter_prefix(&OriginalIdPrefix(original_id))
    }
}

#[cfg(test)]
mod tests {
    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;

    use super::*;
    use crate::RpcStoreSchema;

    fn fresh_db() -> (tempfile::TempDir, sui_consistent_store::Db, RpcStoreSchema) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    #[test]
    fn get_returns_none_for_unknown_version() {
        let (_dir, _db, schema) = fresh_db();
        let original = ObjectID::random();
        assert!(schema.get_package_storage_id(original, 1).unwrap().is_none());
    }

    #[test]
    fn store_then_get_round_trips() {
        let (_dir, db, schema) = fresh_db();
        let original = ObjectID::random();
        let storage = ObjectID::random();

        let (k, v) = store(original, 3, storage);
        let mut batch = db.batch();
        batch.put(&schema.package_versions, &k, &v).unwrap();
        batch.commit().unwrap();

        assert_eq!(
            schema.get_package_storage_id(original, 3).unwrap(),
            Some(storage),
        );
    }

    #[test]
    fn iter_walks_versions_for_one_package() {
        let (_dir, db, schema) = fresh_db();
        let original = ObjectID::random();
        let other = ObjectID::random();

        let mut batch = db.batch();
        // Three versions of the target package.
        for version in [1u64, 2, 5] {
            let (k, v) = store(original, version, ObjectID::random());
            batch.put(&schema.package_versions, &k, &v).unwrap();
        }
        // An unrelated package — must not show up in the iter.
        let (k, v) = store(other, 1, ObjectID::random());
        batch.put(&schema.package_versions, &k, &v).unwrap();
        batch.commit().unwrap();

        let versions: Vec<u64> = schema
            .iter_package_versions(original)
            .unwrap()
            .map(|res| res.unwrap().0.version)
            .collect();
        assert_eq!(versions, vec![1, 2, 5]);
    }
}
