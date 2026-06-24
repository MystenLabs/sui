// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `(ObjectID, checkpoint)` -> `version`.
//!
//! Resolves an object's version *as of* a checkpoint: the version it
//! had at the end of the most recent checkpoint, at or before the one
//! queried, in which it changed. One row is written per
//! `(object, checkpoint-it-changed-in)`, carrying the object's final
//! version in that checkpoint -- a live version, or the tombstone
//! version at which it was deleted or wrapped.
//!
//! The key is the 32-byte object id followed by an 8-byte big-endian
//! checkpoint, so a reverse prefix scan from `(id, C)` lands on the
//! greatest checkpoint `<= C` -- the object's state as of `C`. This is
//! the checkpoint-pinned counterpart to the version-keyed
//! [`super::objects`] CF (which answers "object at version V" but not
//! "object as of checkpoint C"), and the analog of the indexer's
//! Postgres `obj_versions` table that the GraphQL service relies on
//! for `checkpoint_viewed_at` historical reads.
//!
//! Rows carry a `from_restore` provenance flag, set only by the
//! live-set restore at the anchor checkpoint and left unset by tip
//! indexing and backfill. It lets a read whose checkpoint falls before
//! any recorded change fall back to the restore floor -- telling an
//! object that existed before the available window (and so is live at
//! the queried checkpoint) apart from one created later. See
//! [`get_object_version_at_checkpoint`](super::RpcStoreSchema::get_object_version_at_checkpoint).
//!
//! Pruning is effects-driven, in lockstep with [`super::objects`]: a
//! transaction that supersedes an object retracts that object's
//! checkpoint-pinned entries below the superseding checkpoint, so the
//! retained set always matches the versions [`super::objects`] keeps
//! (the floor at the retention boundary, and everything newer).

use std::ops::Bound;

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
use sui_types::base_types::SequenceNumber;
use sui_types::object::Object;

use crate::proto::ObjectVersionInfo;

pub const NAME: &str = "object_version_by_checkpoint";

/// `(ObjectID, checkpoint)`. Encoded as 32 raw id bytes followed by an
/// 8-byte big-endian checkpoint sequence number, so the rows for one
/// object cluster together in ascending checkpoint order and a reverse
/// scan resolves the floor checkpoint for a point-in-time read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    pub id: ObjectID,
    pub checkpoint: u64,
}

/// The object's final version in the keyed checkpoint, plus a
/// `from_restore` provenance flag (set only on rows written by the
/// live-set restore at the anchor checkpoint).
pub type Value = Protobuf<ObjectVersionInfo>;

impl Encode for Key {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.id.as_ref());
        buf.put_slice(&self.checkpoint.to_be_bytes());
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
        let checkpoint = buf.get_u64();
        Ok(Key {
            id: ObjectID::new(id),
            checkpoint,
        })
    }
}

pub fn options(resolver: &sui_consistent_store::CfOptionsResolver) -> rocksdb::Options {
    resolver.options(NAME)
}

/// Build the `(Key, Value)` pair recording that, as of `checkpoint`,
/// the object `id`'s final version was `version`. Written by tip
/// indexing and backfill; `from_restore` is left unset so these (the
/// common case) pay no extra bytes.
pub fn store(id: ObjectID, checkpoint: u64, version: SequenceNumber) -> (Key, Value) {
    (
        Key { id, checkpoint },
        Protobuf(ObjectVersionInfo {
            version: version.value(),
            from_restore: None,
        }),
    )
}

/// Like [`store`], but marks the row as written by the live-set restore
/// at the anchor checkpoint (`from_restore = Some(true)`). The
/// checkpoint-pinned read uses this flag to tell a static object's
/// restore floor (the object existed before the available window) apart
/// from an object created in the anchor checkpoint.
pub fn store_restored(id: ObjectID, checkpoint: u64, version: SequenceNumber) -> (Key, Value) {
    (
        Key { id, checkpoint },
        Protobuf(ObjectVersionInfo {
            version: version.value(),
            from_restore: Some(true),
        }),
    )
}

/// Prefix encoder for "every checkpoint at which `id` changed".
/// Encodes as the 32 raw id bytes -- the leading bytes of every `Key`
/// whose `id` matches.
pub struct ObjectIdPrefix(pub ObjectID);

impl Encode for ObjectIdPrefix {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.0.as_ref());
        Ok(())
    }
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Resolve the version an object had as of `checkpoint`.
    ///
    /// The caller is responsible for confirming `checkpoint` is within
    /// this index's available range; this method assumes it is.
    ///
    /// Two steps:
    ///
    /// 1. **Floor scan** — the greatest entry at or before `checkpoint`
    ///    is the version live then: the row written at the most recent
    ///    checkpoint, at or before the queried one, in which the object
    ///    changed (or a restore/backfill floor row).
    /// 2. **Restore fallback** — if no entry is at or before
    ///    `checkpoint`, the object's earliest entry is the only
    ///    candidate, and it lies after `checkpoint`. If that row is a
    ///    restore floor (`from_restore` set), the object predates the
    ///    available window and was live then at that version. Otherwise
    ///    it is a later change or creation, so the object did not exist
    ///    as of `checkpoint`.
    ///
    /// Returns `Ok(None)` when the object did not exist as of
    /// `checkpoint` (or its history there has been pruned). The returned
    /// version may point at a tombstone row in [`super::objects`]; use
    /// [`get_object_at_checkpoint`](Self::get_object_at_checkpoint) to
    /// collapse that to "no live object".
    pub fn get_object_version_at_checkpoint(
        &self,
        id: ObjectID,
        checkpoint: u64,
    ) -> Result<Option<SequenceNumber>, Error> {
        // Floor scan: reverse-scan this object's own prefix from
        // `(id, checkpoint)` downward and take the first row. The
        // `(id, 0)` lower bound keeps the scan from spilling into the
        // previous object id when this one has no entry in range.
        let lo = Key { id, checkpoint: 0 };
        let hi = Key { id, checkpoint };
        if let Some(row) = self
            .object_version_by_checkpoint
            .iter_rev((Bound::Included(lo), Bound::Included(hi)))?
            .next()
        {
            let (_key, value) = row?;
            return Ok(Some(SequenceNumber::from_u64(value.into_inner().version)));
        }

        // Restore fallback: a restore-floor row means the object
        // predates the window and was live at this version; any other
        // earliest row is a later change/creation, so the object did not
        // exist as of `checkpoint`.
        if let Some(row) = self
            .object_version_by_checkpoint
            .iter_prefix(&ObjectIdPrefix(id))?
            .next()
        {
            let (_key, value) = row?;
            let info = value.into_inner();
            if info.from_restore.unwrap_or(false) {
                return Ok(Some(SequenceNumber::from_u64(info.version)));
            }
        }

        Ok(None)
    }

    /// Resolve the live object as of `checkpoint`, composing
    /// [`get_object_version_at_checkpoint`](Self::get_object_version_at_checkpoint)
    /// with the version-keyed [`super::objects`] lookup.
    ///
    /// Returns `Ok(None)` when the object did not exist as of
    /// `checkpoint`, or was deleted or wrapped at or before it (the
    /// resolved version points at a tombstone row).
    pub fn get_object_at_checkpoint(
        &self,
        id: ObjectID,
        checkpoint: u64,
    ) -> Result<Option<Object>, Error> {
        let Some(version) = self.get_object_version_at_checkpoint(id, checkpoint)? else {
            return Ok(None);
        };
        self.get_object_by_key(id, version)
    }

    /// Iterate every `(checkpoint, version)` recorded for `id`, in
    /// ascending checkpoint order.
    pub fn iter_object_versions_by_checkpoint(
        &self,
        id: ObjectID,
    ) -> Result<Iter<'_, Key, Value>, Error> {
        self.object_version_by_checkpoint
            .iter_prefix(&ObjectIdPrefix(id))
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

    fn seq(v: u64) -> SequenceNumber {
        SequenceNumber::from_u64(v)
    }

    fn put(schema: &RpcStoreSchema, db: &Db, id: ObjectID, checkpoint: u64, version: u64) {
        let (k, v) = store(id, checkpoint, seq(version));
        let mut batch = db.batch();
        batch
            .put(&schema.object_version_by_checkpoint, &k, &v)
            .unwrap();
        batch.commit().unwrap();
    }

    #[test]
    fn returns_none_before_first_entry() {
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();
        // Object first appears at checkpoint 10.
        put(&schema, &db, id, 10, 5);

        // A read pinned before the object existed sees nothing.
        assert!(
            schema
                .get_object_version_at_checkpoint(id, 9)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn resolves_greatest_checkpoint_at_or_before() {
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();
        // The object changed at checkpoints 10, 20, and 30.
        put(&schema, &db, id, 10, 5);
        put(&schema, &db, id, 20, 6);
        put(&schema, &db, id, 30, 7);

        // Exactly on a change checkpoint resolves that version.
        assert_eq!(
            schema.get_object_version_at_checkpoint(id, 20).unwrap(),
            Some(seq(6)),
        );
        // Between changes resolves the prior change (the floor).
        assert_eq!(
            schema.get_object_version_at_checkpoint(id, 25).unwrap(),
            Some(seq(6)),
        );
        // After the last change resolves the latest known version.
        assert_eq!(
            schema.get_object_version_at_checkpoint(id, 1_000).unwrap(),
            Some(seq(7)),
        );
        // The first change checkpoint resolves its version.
        assert_eq!(
            schema.get_object_version_at_checkpoint(id, 10).unwrap(),
            Some(seq(5)),
        );
    }

    #[test]
    fn isolates_objects_from_each_other() {
        let (_dir, db, schema) = fresh_db();
        let a = ObjectID::from_single_byte(1);
        let b = ObjectID::from_single_byte(2);
        put(&schema, &db, a, 10, 5);
        put(&schema, &db, b, 5, 9);

        // `a` has no entry at or before checkpoint 9 even though `b`
        // does -- the reverse scan must not spill across the id bound.
        assert!(
            schema
                .get_object_version_at_checkpoint(a, 9)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(b, 9).unwrap(),
            Some(seq(9)),
        );
    }

    #[test]
    fn restore_floor_resolves_below_anchor_but_a_creation_does_not() {
        let (_dir, db, schema) = fresh_db();
        let static_obj = ObjectID::from_single_byte(1);
        let created_obj = ObjectID::from_single_byte(2);

        let mut batch = db.batch();
        // `static_obj`: a live-set restore floor at anchor checkpoint
        // 100 -- the object existed before the available window and
        // never changed in it.
        let (k1, v1) = store_restored(static_obj, 100, seq(5));
        batch
            .put(&schema.object_version_by_checkpoint, &k1, &v1)
            .unwrap();
        // `created_obj`: an ordinary (tip/backfill) row at 100 -- the
        // object was created in checkpoint 100. Same key checkpoint, no
        // `from_restore` flag.
        let (k2, v2) = store(created_obj, 100, seq(7));
        batch
            .put(&schema.object_version_by_checkpoint, &k2, &v2)
            .unwrap();
        batch.commit().unwrap();

        // Below the anchor the provenance flag is what separates them:
        // the restore floor is live, the creation did not yet exist.
        assert_eq!(
            schema
                .get_object_version_at_checkpoint(static_obj, 50)
                .unwrap(),
            Some(seq(5)),
            "restore floor is live below the anchor",
        );
        assert!(
            schema
                .get_object_version_at_checkpoint(created_obj, 50)
                .unwrap()
                .is_none(),
            "an object created at the anchor did not exist below it",
        );

        // At and above the anchor the plain floor scan resolves both.
        assert_eq!(
            schema
                .get_object_version_at_checkpoint(static_obj, 100)
                .unwrap(),
            Some(seq(5)),
        );
        assert_eq!(
            schema
                .get_object_version_at_checkpoint(created_obj, 100)
                .unwrap(),
            Some(seq(7)),
        );
        assert_eq!(
            schema
                .get_object_version_at_checkpoint(static_obj, 999)
                .unwrap(),
            Some(seq(5)),
        );
    }

    #[test]
    fn get_object_at_checkpoint_composes_with_objects() {
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();
        let object = Object::with_id_owner_for_testing(id, SuiAddress::ZERO);
        let version = object.version();

        let mut batch = db.batch();
        batch
            .put(
                &schema.objects,
                &objects::Key { id, version },
                &objects::store(&object),
            )
            .unwrap();
        let (k, v) = store(id, 42, version);
        batch
            .put(&schema.object_version_by_checkpoint, &k, &v)
            .unwrap();
        batch.commit().unwrap();

        let read = schema
            .get_object_at_checkpoint(id, 42)
            .unwrap()
            .expect("object present");
        assert_eq!(read, object);
    }

    #[test]
    fn get_object_at_checkpoint_returns_none_for_tombstone() {
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();
        let tombstone_version = seq(7);

        let mut batch = db.batch();
        // The object was removed at checkpoint 50: a tombstone in
        // `objects` at the tombstone version, and an index row pointing
        // at it.
        batch
            .put(
                &schema.objects,
                &objects::Key {
                    id,
                    version: tombstone_version,
                },
                &objects::tombstone(objects::TombstoneKind::Deleted),
            )
            .unwrap();
        let (k, v) = store(id, 50, tombstone_version);
        batch
            .put(&schema.object_version_by_checkpoint, &k, &v)
            .unwrap();
        batch.commit().unwrap();

        // The version resolves, but the object collapses to None.
        assert_eq!(
            schema.get_object_version_at_checkpoint(id, 50).unwrap(),
            Some(tombstone_version),
        );
        assert!(schema.get_object_at_checkpoint(id, 50).unwrap().is_none());
    }

    #[test]
    fn iter_walks_checkpoints_for_one_object() {
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();
        let other = ObjectID::random();
        put(&schema, &db, id, 30, 7);
        put(&schema, &db, id, 10, 5);
        put(&schema, &db, id, 20, 6);
        put(&schema, &db, other, 15, 1);

        let checkpoints: Vec<u64> = schema
            .iter_object_versions_by_checkpoint(id)
            .unwrap()
            .map(|res| res.unwrap().0.checkpoint)
            .collect();
        assert_eq!(checkpoints, vec![10, 20, 30]);
    }
}
