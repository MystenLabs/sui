// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `(original_package_id, version)` → `PackageVersionInfo`.
//!
//! Lists every published version of a Move package: the storage id
//! under which each version lives, and the checkpoint at which the
//! version was published. The publish checkpoint lets a
//! checkpoint-bounded read resolve the latest version of a package
//! as of a given checkpoint — see
//! [`get_package_at_checkpoint`](super::RpcStoreSchema::get_package_at_checkpoint).
//!
//! Rows written by the live-set restore at the anchor checkpoint
//! leave the publish checkpoint unset (a restore floor): those
//! versions predate the available window, so a checkpoint-bounded
//! read treats them as having always existed.

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

pub fn options(resolver: &sui_consistent_store::CfOptionsResolver) -> rocksdb::Options {
    resolver.options(NAME)
}

/// Build the `(Key, Value)` pair recording that version `version`
/// of the package originally published at `original_id` lives at
/// the on-chain storage id `storage_id`, published in `checkpoint`.
/// Written by tip indexing and the post-restore backfill.
pub fn store(
    original_id: ObjectID,
    version: u64,
    storage_id: ObjectID,
    checkpoint: u64,
) -> (Key, Value) {
    (
        Key {
            original_id,
            version,
        },
        Protobuf(PackageVersionInfo {
            storage_id: storage_id.to_vec().into(),
            checkpoint: Some(checkpoint),
        }),
    )
}

/// Like [`store`], but for rows written by the live-set restore at
/// the anchor checkpoint: the publish checkpoint is left unset (a
/// restore floor), marking a version that was published before the
/// available window. A checkpoint-bounded read treats such a version
/// as having always existed.
pub fn store_restored(original_id: ObjectID, version: u64, storage_id: ObjectID) -> (Key, Value) {
    (
        Key {
            original_id,
            version,
        },
        Protobuf(PackageVersionInfo {
            storage_id: storage_id.to_vec().into(),
            checkpoint: None,
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

/// Decode a `storage_id` field (the 32 raw `ObjectID` bytes) from a
/// stored `PackageVersionInfo`.
fn decode_storage_id(bytes: &[u8]) -> Result<ObjectID, DecodeError> {
    let array: [u8; ObjectID::LENGTH] = bytes.try_into().map_err(|_| {
        DecodeError::msg(format!(
            "expected {} bytes for storage_id, got {}",
            ObjectID::LENGTH,
            bytes.len(),
        ))
    })?;
    Ok(ObjectID::new(array))
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
        Ok(Some(decode_storage_id(&stored.into_inner().storage_id)?))
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

    /// Resolve the latest version of the package originally
    /// published at `original_id` that existed as of `checkpoint`,
    /// returning its `(version, storage_id)`.
    ///
    /// Walks every recorded version of the package — cheap, since
    /// even the most-upgraded mainnet packages have on the order of
    /// a hundred versions — and keeps the highest version whose
    /// publish checkpoint is at or before `checkpoint`. Restore-floor
    /// rows (no recorded publish checkpoint) count as having always
    /// existed, since they predate the available window.
    ///
    /// Returns `Ok(None)` when no version of the package existed as
    /// of `checkpoint` (it was first published later, or the package
    /// is unknown to this store).
    pub fn get_package_at_checkpoint(
        &self,
        original_id: ObjectID,
        checkpoint: u64,
    ) -> Result<Option<(u64, ObjectID)>, Error> {
        let mut latest: Option<(u64, ObjectID)> = None;
        for row in self.iter_package_versions(original_id)? {
            let (key, value) = row?;
            let info = value.into_inner();
            let existed = match info.checkpoint {
                // Restore floor: published before the available
                // window, so it existed as of any queried checkpoint.
                None => true,
                Some(published) => published <= checkpoint,
            };
            if !existed {
                continue;
            }
            // `iter_package_versions` yields ascending versions, so
            // any qualifying row supersedes the prior candidate.
            latest = Some((key.version, decode_storage_id(&info.storage_id)?));
        }
        Ok(latest)
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
        assert!(
            schema
                .get_package_storage_id(original, 1)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn store_then_get_round_trips() {
        let (_dir, db, schema) = fresh_db();
        let original = ObjectID::random();
        let storage = ObjectID::random();

        let (k, v) = store(original, 3, storage, 100);
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
            let (k, v) = store(original, version, ObjectID::random(), version);
            batch.put(&schema.package_versions, &k, &v).unwrap();
        }
        // An unrelated package — must not show up in the iter.
        let (k, v) = store(other, 1, ObjectID::random(), 1);
        batch.put(&schema.package_versions, &k, &v).unwrap();
        batch.commit().unwrap();

        let versions: Vec<u64> = schema
            .iter_package_versions(original)
            .unwrap()
            .map(|res| res.unwrap().0.version)
            .collect();
        assert_eq!(versions, vec![1, 2, 5]);
    }

    #[test]
    fn get_package_at_checkpoint_resolves_latest_in_window() {
        let (_dir, db, schema) = fresh_db();
        let original = ObjectID::random();
        let (s1, s2, s3) = (ObjectID::random(), ObjectID::random(), ObjectID::random());

        let mut batch = db.batch();
        // v1 is a restore floor (published before the available
        // window); v2 and v3 were published at checkpoints 10 and 20.
        let (k, v) = store_restored(original, 1, s1);
        batch.put(&schema.package_versions, &k, &v).unwrap();
        let (k, v) = store(original, 2, s2, 10);
        batch.put(&schema.package_versions, &k, &v).unwrap();
        let (k, v) = store(original, 3, s3, 20);
        batch.put(&schema.package_versions, &k, &v).unwrap();
        batch.commit().unwrap();

        // Before any real publish, only the restore-floor v1 exists.
        assert_eq!(
            schema.get_package_at_checkpoint(original, 5).unwrap(),
            Some((1, s1)),
        );
        // v2's publish checkpoint, then the gap before v3.
        assert_eq!(
            schema.get_package_at_checkpoint(original, 10).unwrap(),
            Some((2, s2)),
        );
        assert_eq!(
            schema.get_package_at_checkpoint(original, 15).unwrap(),
            Some((2, s2)),
        );
        // v3's publish checkpoint and beyond.
        assert_eq!(
            schema.get_package_at_checkpoint(original, 20).unwrap(),
            Some((3, s3)),
        );
        assert_eq!(
            schema.get_package_at_checkpoint(original, 9_999).unwrap(),
            Some((3, s3)),
        );
    }

    #[test]
    fn get_package_at_checkpoint_returns_none_before_first_publish() {
        let (_dir, db, schema) = fresh_db();
        let original = ObjectID::random();

        let mut batch = db.batch();
        let (k, v) = store(original, 1, ObjectID::random(), 50);
        batch.put(&schema.package_versions, &k, &v).unwrap();
        batch.commit().unwrap();

        // The package's first version was published at checkpoint 50,
        // so it does not exist as of an earlier checkpoint.
        assert_eq!(
            schema.get_package_at_checkpoint(original, 49).unwrap(),
            None,
        );
        assert!(
            schema
                .get_package_at_checkpoint(original, 50)
                .unwrap()
                .is_some()
        );
        // An unknown package resolves to nothing.
        assert_eq!(
            schema
                .get_package_at_checkpoint(ObjectID::random(), 100)
                .unwrap(),
            None,
        );
    }
}
