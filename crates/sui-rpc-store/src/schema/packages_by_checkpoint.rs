// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `(checkpoint, original_package_id, version)` → `storage_id`.
//!
//! Lists package versions across every package in the order they were
//! published: the key is the 8-byte big-endian publish checkpoint, the
//! 32 raw bytes of the package's original id, then the 8-byte
//! big-endian version, so key order is checkpoint order. This is the
//! same key layout as the Bigtable `packages_by_checkpoint` table
//! (`sui-kvstore`), and backs
//! [`RpcIndexes::packages_by_checkpoint_iter`](sui_types::storage::RpcIndexes::packages_by_checkpoint_iter).
//!
//! The value is the 32-byte storage id holding that version. Items are
//! identified by `(original_id, version)`, never by storage id alone:
//! system packages are upgraded in place, so every version of a system
//! package shares one storage id.
//!
//! The CF is a projection of [`super::package_versions`]: one row per
//! `package_versions` row whose publish checkpoint is known. The
//! `PackageVersions` pipeline writes both CFs in the same commit batch,
//! so they cannot drift. Restore-floor rows (publish checkpoint unset)
//! have no row here. The backfill, which re-indexes checkpoints `L`
//! through `T`, re-publishes each version published in that window with
//! its real checkpoint and writes its row here alongside, so once the
//! backfill reaches `T`, the floor rows left are versions published
//! before the available window. Like `package_versions`, the CF is never
//! pruned.
//!
//! Every database open (`RpcStoreSchema::open`) reconciles this CF
//! against `package_versions`, writing any row that is missing. That
//! builds the CF for a database created before it existed, and finishes
//! a build a crash interrupted; see
//! `RpcStoreSchema::build_packages_by_checkpoint`.

use bytes::Buf;
use bytes::BufMut;
use sui_consistent_store::Db;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_consistent_store::error::Error;
use sui_types::base_types::ObjectID;
use sui_types::storage::PackagePublishKey;

use super::package_versions::decode_storage_id;

pub const NAME: &str = "packages_by_checkpoint";

/// `(checkpoint, original_id, version)`, encoded big-endian so rows
/// sort by publish checkpoint, then by package, then by version. The
/// derived `Ord` matches the encoded byte order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    pub checkpoint: u64,
    pub original_id: ObjectID,
    pub version: u64,
}

/// The on-chain storage id holding the keyed package version, stored
/// as its 32 raw bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StorageId(pub ObjectID);

pub type Value = StorageId;

const KEY_LENGTH: usize = 8 + ObjectID::LENGTH + 8;

impl Encode for Key {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(&self.checkpoint.to_be_bytes());
        buf.put_slice(self.original_id.as_ref());
        buf.put_slice(&self.version.to_be_bytes());
        Ok(())
    }
}

impl Decode for Key {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() != KEY_LENGTH {
            return Err(DecodeError::msg(format!(
                "expected {KEY_LENGTH} bytes for {NAME} Key, got {}",
                buf.remaining(),
            )));
        }
        let checkpoint = buf.get_u64();
        let mut id = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut id);
        let version = buf.get_u64();
        Ok(Key {
            checkpoint,
            original_id: ObjectID::new(id),
            version,
        })
    }
}

impl From<PackagePublishKey> for Key {
    fn from(key: PackagePublishKey) -> Self {
        Key {
            checkpoint: key.checkpoint,
            original_id: key.original_id,
            version: key.version,
        }
    }
}

impl From<Key> for PackagePublishKey {
    fn from(key: Key) -> Self {
        PackagePublishKey {
            checkpoint: key.checkpoint,
            original_id: key.original_id,
            version: key.version,
        }
    }
}

impl Encode for StorageId {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.0.as_ref());
        Ok(())
    }
}

impl Decode for StorageId {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        let bytes = buf.copy_to_bytes(buf.remaining());
        Ok(StorageId(decode_storage_id(&bytes)?))
    }
}

pub fn options(resolver: &sui_consistent_store::CfOptionsResolver) -> rocksdb::Options {
    resolver.options(NAME)
}

/// Build the `(Key, Value)` pair recording that version `version` of
/// the package originally published at `original_id` was published in
/// `checkpoint` and lives at the storage id `storage_id`.
pub fn store(
    checkpoint: u64,
    original_id: ObjectID,
    version: u64,
    storage_id: ObjectID,
) -> (Key, Value) {
    (
        Key {
            checkpoint,
            original_id,
            version,
        },
        StorageId(storage_id),
    )
}

/// `package_versions` rows checked and written per batch by the build:
/// bounds the build's memory and write-batch size regardless of how many
/// package versions the chain holds.
const BUILD_CHUNK_ROWS: usize = 10_000;

impl super::RpcStoreSchema {
    /// Write every row this CF is missing relative to
    /// [`super::package_versions`], returning the number written.
    ///
    /// Runs on every database open, before any pipeline can write. It
    /// scans `package_versions`, projects each row with a known publish
    /// checkpoint to its key here (restore floors are skipped, as on the
    /// indexing path), and writes only the keys not already present.
    /// Work is committed in chunks of [`BUILD_CHUNK_ROWS`]; because every
    /// write is a put of a row derived from `package_versions`, a crash
    /// between chunks leaves a subset of the right rows, and the next
    /// open fills in the rest. On a database whose CF is already
    /// complete it reads both CFs and writes nothing.
    ///
    /// Only missing rows are added; rows here with no matching
    /// `package_versions` row are left alone. The pipeline writes both
    /// CFs together and neither is pruned, so such rows do not arise.
    pub(crate) fn build_packages_by_checkpoint(&self, db: &Db) -> Result<usize, Error> {
        self.build_packages_by_checkpoint_in_chunks(db, BUILD_CHUNK_ROWS)
    }

    fn build_packages_by_checkpoint_in_chunks(
        &self,
        db: &Db,
        chunk_rows: usize,
    ) -> Result<usize, Error> {
        let mut written = 0;
        let mut chunk = Vec::with_capacity(chunk_rows);
        for row in self.package_versions.iter(..)? {
            let (key, value) = row?;
            let info = value.into_inner();
            let Some(checkpoint) = info.checkpoint else {
                continue;
            };
            let storage_id = decode_storage_id(&info.storage_id)?;
            chunk.push(store(checkpoint, key.original_id, key.version, storage_id));
            if chunk.len() == chunk_rows {
                written += self.write_missing_rows(db, &chunk)?;
                chunk.clear();
            }
        }
        written += self.write_missing_rows(db, &chunk)?;
        Ok(written)
    }

    /// Write the rows of `chunk` whose keys are absent from this CF, in
    /// one batch, returning how many were written.
    fn write_missing_rows(&self, db: &Db, chunk: &[(Key, Value)]) -> Result<usize, Error> {
        // One answer per key, in key order.
        let present = self
            .packages_by_checkpoint
            .multi_contains_keys(chunk.iter().map(|(key, _)| key))?;
        let mut batch = db.batch();
        for (index, (key, value)) in chunk.iter().enumerate() {
            if !present[index] {
                batch.put(&self.packages_by_checkpoint, key, value)?;
            }
        }
        let written = batch.len();
        if written > 0 {
            batch.commit()?;
        }
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use sui_consistent_store::DbOptions;
    use sui_types::SUI_FRAMEWORK_PACKAGE_ID;

    use super::*;
    use crate::RpcStoreSchema;
    use crate::schema::package_versions;

    const A: ObjectID = ObjectID::from_single_byte(0x0A);
    const A_V2: ObjectID = ObjectID::from_single_byte(0xA2);
    const B: ObjectID = ObjectID::from_single_byte(0x0B);
    const C: ObjectID = ObjectID::from_single_byte(0x0C);
    const SYSTEM: ObjectID = SUI_FRAMEWORK_PACKAGE_ID;

    fn fresh_db() -> (tempfile::TempDir, Db, RpcStoreSchema) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    fn key(checkpoint: u64, original_id: ObjectID, version: u64) -> Key {
        Key {
            checkpoint,
            original_id,
            version,
        }
    }

    /// Publishes in index order: two packages in checkpoint 5, two
    /// in-place upgrades of a system package (same storage id) in
    /// checkpoints 7 and 9, a user-package upgrade sharing checkpoint 9,
    /// and a last package in checkpoint 12.
    fn publishes() -> Vec<(Key, ObjectID)> {
        vec![
            (key(5, A, 1), A),
            (key(5, B, 1), B),
            (key(7, SYSTEM, 2), SYSTEM),
            (key(9, SYSTEM, 3), SYSTEM),
            (key(9, A, 2), A_V2),
            (key(12, C, 1), C),
        ]
    }

    fn seed_index(db: &Db, schema: &RpcStoreSchema, rows: &[(Key, ObjectID)]) {
        let mut batch = db.batch();
        for (key, storage_id) in rows {
            batch
                .put(&schema.packages_by_checkpoint, key, &StorageId(*storage_id))
                .unwrap();
        }
        batch.commit().unwrap();
    }

    fn seed_package_versions(
        db: &Db,
        schema: &RpcStoreSchema,
        rows: &[(ObjectID, u64, ObjectID, Option<u64>)],
    ) {
        let mut batch = db.batch();
        for &(original_id, version, storage_id, checkpoint) in rows {
            let (key, value) = match checkpoint {
                Some(checkpoint) => {
                    package_versions::store(original_id, version, storage_id, checkpoint)
                }
                None => package_versions::store_restored(original_id, version, storage_id),
            };
            batch.put(&schema.package_versions, &key, &value).unwrap();
        }
        batch.commit().unwrap();
    }

    fn index_rows(schema: &RpcStoreSchema) -> Vec<(Key, ObjectID)> {
        schema
            .packages_by_checkpoint
            .iter(..)
            .unwrap()
            .map(|row| {
                let (key, StorageId(storage_id)) = row.unwrap();
                (key, storage_id)
            })
            .collect()
    }

    #[test]
    fn build_indexes_published_versions_and_skips_restore_floors() {
        let (_dir, db, schema) = fresh_db();
        // A mid-backfill `package_versions`: `B` and the system
        // package's first version are still restore floors; the rest
        // have their real publish checkpoints.
        seed_package_versions(
            &db,
            &schema,
            &[
                (SYSTEM, 1, SYSTEM, None),
                (SYSTEM, 2, SYSTEM, Some(7)),
                (SYSTEM, 3, SYSTEM, Some(9)),
                (A, 1, A, Some(5)),
                (A, 2, A_V2, Some(9)),
                (B, 1, B, None),
                (C, 1, C, Some(12)),
            ],
        );

        assert_eq!(schema.build_packages_by_checkpoint(&db).unwrap(), 5);
        let mut expected = publishes();
        expected.retain(|(key, _)| key.original_id != B);
        assert_eq!(index_rows(&schema), expected);

        // Rerunning on a built index changes nothing.
        assert_eq!(schema.build_packages_by_checkpoint(&db).unwrap(), 0);
        assert_eq!(index_rows(&schema), expected);
    }

    #[test]
    fn build_over_restore_floors_only_writes_nothing() {
        let (_dir, db, schema) = fresh_db();
        seed_package_versions(&db, &schema, &[(A, 1, A, None), (B, 1, B, None)]);

        // Nothing has a publish checkpoint yet, so nothing is written.
        assert_eq!(schema.build_packages_by_checkpoint(&db).unwrap(), 0);
        assert_eq!(schema.build_packages_by_checkpoint(&db).unwrap(), 0);
        assert!(index_rows(&schema).is_empty());

        // Once a version has a real checkpoint, the next build picks it up.
        seed_package_versions(&db, &schema, &[(A, 1, A, Some(5))]);
        assert_eq!(schema.build_packages_by_checkpoint(&db).unwrap(), 1);
        assert_eq!(index_rows(&schema), vec![(key(5, A, 1), A)]);
    }

    #[test]
    fn build_completes_a_partially_built_index() {
        let (_dir, db, schema) = fresh_db();
        // Every `package_versions` row of the fixture has a real
        // checkpoint, and the index holds some of them, as a crash
        // between build chunks would leave it.
        let rows: Vec<_> = publishes()
            .into_iter()
            .map(|(key, storage_id)| {
                (
                    key.original_id,
                    key.version,
                    storage_id,
                    Some(key.checkpoint),
                )
            })
            .collect();
        seed_package_versions(&db, &schema, &rows);
        seed_index(&db, &schema, &[publishes()[1], publishes()[4]]);

        // Six eligible rows in chunks of four exercise both a full-chunk
        // flush and the final partial chunk; only the four missing rows
        // are written.
        assert_eq!(
            schema
                .build_packages_by_checkpoint_in_chunks(&db, 4)
                .unwrap(),
            4,
        );
        assert_eq!(index_rows(&schema), publishes());
        assert_eq!(
            schema
                .build_packages_by_checkpoint_in_chunks(&db, 4)
                .unwrap(),
            0,
        );
    }

    #[test]
    fn opening_a_database_fills_in_missing_index_rows() {
        let dir = tempfile::tempdir().unwrap();
        let open = || Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        {
            // `package_versions` populated without the index (as by a
            // binary that predates it).
            let (db, schema) = open();
            seed_package_versions(
                &db,
                &schema,
                &[(A, 1, A, Some(5)), (A, 2, A_V2, Some(9)), (B, 1, B, None)],
            );
            assert!(index_rows(&schema).is_empty());
        }
        {
            let (db, schema) = open();
            assert_eq!(
                index_rows(&schema),
                vec![(key(5, A, 1), A), (key(9, A, 2), A_V2)],
            );
            // Drop one index row, as an interrupted build would leave it
            // missing.
            let mut batch = db.batch();
            batch
                .delete(&schema.packages_by_checkpoint, &key(9, A, 2))
                .unwrap();
            batch.commit().unwrap();
        }

        let (_db, schema) = open();
        assert_eq!(
            index_rows(&schema),
            vec![(key(5, A, 1), A), (key(9, A, 2), A_V2)],
        );
    }
}
