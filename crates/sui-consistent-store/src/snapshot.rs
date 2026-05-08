// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Consistent reads against a captured snapshot.
//!
//! [`Snapshot`] is a cheap-to-clone token returned by
//! [`Db::at_snapshot`](crate::Db::at_snapshot) (or
//! [`Db::latest_snapshot`](crate::Db::latest_snapshot)) that doubles
//! as a [`Reader`] for snapshot-bound reads. It is
//! the token used to construct snapshot-bound projections via
//! [`DbMap::at`](crate::DbMap::at) (single CF) or
//! [`SchemaAtSnapshot::at`](crate::SchemaAtSnapshot::at) (whole
//! schema). The token itself does not expose read methods — reads
//! always go through a [`DbMap<_, _, Snapshot>`](crate::DbMap)
//! produced by re-binding.
//!
//! # Ownership
//!
//! `Snapshot` co-owns a [`Db`] handle and an `Arc` to a private
//! internal snapshot entry. As long
//! as a `Snapshot` (or any clone of it) exists, the underlying
//! snapshot is kept alive even if
//! [`Db::drop_snapshot`](crate::Db::drop_snapshot) has been called
//! for that checkpoint or the snapshot has been evicted from the
//! buffer by capacity pressure.
//!
//! Snapshot-bound [`DbMap`](crate::DbMap)s constructed from a
//! `Snapshot` own their own clone of it; the originating value need
//! not outlive the projection or any iterators built from it.
//!
//! # Examples
//!
//! ```
//! use bytes::Buf;
//! use bytes::BufMut;
//!
//! use sui_consistent_store::Db;
//! use sui_consistent_store::DbMap;
//! use sui_consistent_store::DbOptions;
//! use sui_consistent_store::Decode;
//! use sui_consistent_store::Encode;
//! use sui_consistent_store::Reader;
//! use sui_consistent_store::Schema;
//! use sui_consistent_store::Watermark;
//! use sui_consistent_store::error::DecodeError;
//! use sui_consistent_store::error::EncodeError;
//! use sui_consistent_store::error::OpenError;
//!
//! #[derive(Debug, PartialEq, Eq)]
//! struct U64Be(u64);
//!
//! impl Encode for U64Be {
//!     fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
//!         buf.put_slice(&self.0.to_be_bytes());
//!         Ok(())
//!     }
//! }
//!
//! impl Decode for U64Be {
//!     fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
//!         if buf.remaining() != 8 {
//!             return Err(DecodeError::msg("expected 8 bytes"));
//!         }
//!         Ok(Self(buf.get_u64()))
//!     }
//! }
//!
//! struct MySchema<R: Reader = Db> {
//!     items: DbMap<U64Be, U64Be, R>,
//! }
//!
//! impl Schema for MySchema {
//!     fn cfs(base_options: &rocksdb::Options) -> Vec<sui_consistent_store::CfDescriptor> {
//!         vec![sui_consistent_store::CfDescriptor::new("items", base_options.clone())]
//!     }
//!
//!     fn open(db: &Db) -> Result<Self, OpenError> {
//!         Ok(Self {
//!             items: DbMap::new(db.clone(), "items")?,
//!         })
//!     }
//! }
//!
//! let dir = tempfile::tempdir().unwrap();
//! let (db, schema) = Db::open::<MySchema>(dir.path(), DbOptions::default()).unwrap();
//!
//! // Write the initial state, then take a snapshot at checkpoint 1.
//! let mut batch = db.batch();
//! batch.put(&schema.items, &U64Be(1), &U64Be(100)).unwrap();
//! batch.commit().unwrap();
//! db.take_snapshot(Watermark::for_checkpoint(1));
//!
//! // Mutate after the snapshot.
//! let mut batch = db.batch();
//! batch.put(&schema.items, &U64Be(1), &U64Be(999)).unwrap();
//! batch.commit().unwrap();
//!
//! // Re-bind the items map at the snapshot.
//! let snap = db.at_snapshot(1).unwrap();
//! let items_at_snap = schema.items.at(&snap);
//! assert_eq!(items_at_snap.get(&U64Be(1)).unwrap(), Some(U64Be(100)));
//! // The live binding sees the new value.
//! assert_eq!(schema.items.get(&U64Be(1)).unwrap(), Some(U64Be(999)));
//! ```

use std::fmt;
use std::sync::Arc;

use rocksdb::ReadOptions;

use crate::db::Db;
use crate::db::SnapshotEntry;
use crate::reader::Reader;
use crate::reader::sealed;

/// A cheap-to-clone token referencing a single snapshot of the
/// database that doubles as a [`Reader`].
///
/// Returned by [`Db::at_snapshot`](crate::Db::at_snapshot) and
/// [`Db::latest_snapshot`](crate::Db::latest_snapshot). Pass a
/// reference to a `Snapshot` to [`DbMap::at`](crate::DbMap::at) or
/// [`SchemaAtSnapshot::at`](crate::SchemaAtSnapshot::at) to obtain
/// snapshot-bound read projections. Clones share the same
/// underlying snapshot; cloning is two `Arc` increments and a small
/// struct copy.
pub struct Snapshot {
    // Field declaration order is load-bearing: `entry` must drop
    // before `db` so that the contained `rocksdb::Snapshot`
    // releases its borrow on `DbInner::db` before the `Db` handle's
    // `Arc<DbInner>` ref is decremented.
    entry: Arc<SnapshotEntry>,
    db: Db,
}

impl Snapshot {
    pub(crate) fn new(db: Db, entry: Arc<SnapshotEntry>) -> Self {
        Self { entry, db }
    }

    /// The checkpoint number this snapshot was taken at.
    /// Convenience alias for `self.watermark().checkpoint_hi_inclusive`.
    pub fn checkpoint(&self) -> u64 {
        self.entry.watermark().checkpoint_hi_inclusive
    }

    /// The full [`Watermark`](crate::Watermark) recorded when this
    /// snapshot was taken — checkpoint, epoch, transaction count,
    /// and timestamp. Use this to recover the chain state the
    /// snapshot captured.
    pub fn watermark(&self) -> crate::Watermark {
        self.entry.watermark()
    }

    /// Borrowed handle to the auto-registered
    /// [`FrameworkSchema`](crate::FrameworkSchema), bound to this
    /// snapshot.
    ///
    /// Returns a `FrameworkSchema<&Snapshot>` borrowing `self`.
    /// Zero `Arc` bumps; the returned schema reads the framework
    /// CFs at the snapshot's captured state.
    pub fn framework(&self) -> crate::FrameworkSchema<&Snapshot> {
        crate::FrameworkSchema::new(self)
    }
}

impl sealed::Sealed for Snapshot {}

impl Reader for Snapshot {
    fn db(&self) -> &Db {
        &self.db
    }

    fn read_options(&self) -> ReadOptions {
        let mut opts = ReadOptions::default();
        opts.set_snapshot(self.entry.as_snapshot());
        opts
    }
}

impl Clone for Snapshot {
    fn clone(&self) -> Self {
        Self {
            entry: self.entry.clone(),
            db: self.db.clone(),
        }
    }
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Snapshot")
            .field("checkpoint", &self.checkpoint())
            .finish_non_exhaustive()
    }
}

/// Blanket [`Reader`] impl for `&Snapshot`, the zero-`Arc`-bump
/// counterpart to the owned [`Snapshot`] reader.
///
/// Use a borrowed snapshot via
/// [`DbMap::at_ref`](crate::DbMap::at_ref) when the re-bound
/// [`DbMap`](crate::DbMap) is scoped to a single function body and
/// can borrow a [`Snapshot`] the caller already holds, instead of
/// cloning it (two atomic increments) for every
/// [`DbMap`](crate::DbMap) re-bind. Delegates to [`Snapshot`]'s
/// own [`Reader`] impl, so the semantics are identical.
impl sealed::Sealed for &Snapshot {}

impl Reader for &Snapshot {
    fn db(&self) -> &Db {
        (**self).db()
    }

    fn read_options(&self) -> ReadOptions {
        (**self).read_options()
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::DbOptions;
    use crate::Reader;
    use crate::Schema;
    use crate::SchemaAtSnapshot;
    use crate::Snapshot;
    use crate::Watermark;
    use crate::error::DecodeError;
    use bytes::BufMut;

    use crate::error::EncodeError;
    use crate::error::OpenError;
    use crate::map::DbMap;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct U64Be(u64);

    impl crate::Encode for U64Be {
        fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
            buf.put_slice(&self.0.to_be_bytes());
            Ok(())
        }
    }

    impl crate::Decode for U64Be {
        fn decode<B: bytes::Buf>(buf: &mut B) -> Result<Self, DecodeError> {
            if buf.remaining() != 8 {
                return Err(DecodeError::msg("expected 8 bytes"));
            }
            Ok(Self(buf.get_u64()))
        }
    }

    #[derive(Debug)]
    struct TestSchema<R: Reader = Db> {
        items: DbMap<U64Be, U64Be, R>,
    }

    impl Schema for TestSchema {
        fn cfs(base_options: &rocksdb::Options) -> Vec<crate::CfDescriptor> {
            vec![crate::CfDescriptor::new("items", base_options.clone())]
        }

        fn open(db: &Db) -> Result<Self, OpenError> {
            Ok(Self {
                items: DbMap::new(db.clone(), "items")?,
            })
        }
    }

    impl SchemaAtSnapshot for TestSchema {
        type At = TestSchema<Snapshot>;
        fn at(&self, snap: &Snapshot) -> Self::At {
            TestSchema {
                items: self.items.at(snap),
            }
        }
    }

    fn open_with_capacity(capacity: usize) -> (TempDir, Db, TestSchema) {
        let dir = TempDir::new().unwrap();
        let opts = DbOptions {
            snapshot_capacity: capacity,
            ..DbOptions::default()
        };
        let (db, schema) = Db::open::<TestSchema>(dir.path(), opts).unwrap();
        (dir, db, schema)
    }

    fn open() -> (TempDir, Db, TestSchema) {
        open_with_capacity(32)
    }

    fn put(db: &Db, schema: &TestSchema, key: u64, value: u64) {
        let mut batch = db.batch();
        batch
            .put(&schema.items, &U64Be(key), &U64Be(value))
            .unwrap();
        batch.commit().unwrap();
    }

    #[test]
    fn at_snapshot_returns_none_when_no_snapshot_taken() {
        let (_dir, db, _schema) = open();
        assert!(db.at_snapshot(0).is_none());
    }

    #[test]
    fn latest_snapshot_is_none_when_empty() {
        let (_dir, db, _schema) = open();
        assert!(db.latest_snapshot().is_none());
    }

    #[test]
    fn latest_snapshot_returns_highest_checkpoint() {
        let (_dir, db, _schema) = open();
        db.take_snapshot(Watermark::for_checkpoint(3));
        db.take_snapshot(Watermark::for_checkpoint(10));
        db.take_snapshot(Watermark::for_checkpoint(5));
        let latest = db.latest_snapshot().expect("latest should exist");
        assert_eq!(latest.checkpoint(), 10);
    }

    #[test]
    fn latest_snapshot_after_eviction_reflects_remaining() {
        let (_dir, db, _schema) = open_with_capacity(2);
        db.take_snapshot(Watermark::for_checkpoint(1));
        db.take_snapshot(Watermark::for_checkpoint(2));
        db.take_snapshot(Watermark::for_checkpoint(3));
        // Capacity 2 evicts checkpoint 1; latest is now 3.
        let latest = db.latest_snapshot().expect("latest should exist");
        assert_eq!(latest.checkpoint(), 3);
    }

    #[test]
    fn latest_snapshot_reads_pre_snapshot_state() {
        let (_dir, db, schema) = open();
        put(&db, &schema, 1, 100);
        db.take_snapshot(Watermark::for_checkpoint(1));
        put(&db, &schema, 1, 999);
        let latest = db.latest_snapshot().unwrap();
        assert_eq!(
            schema.items.at(&latest).get(&U64Be(1)).unwrap(),
            Some(U64Be(100)),
        );
    }

    #[test]
    fn snapshot_range_is_none_when_empty() {
        let (_dir, db, _schema) = open();
        assert!(db.snapshot_range().is_none());
    }

    #[test]
    fn take_then_at_returns_handle() {
        let (_dir, db, _schema) = open();
        db.take_snapshot(Watermark::for_checkpoint(7));
        let handle = db.at_snapshot(7).expect("handle should exist");
        assert_eq!(handle.checkpoint(), 7);
    }

    #[test]
    fn snapshot_range_reflects_taken_snapshots() {
        let (_dir, db, _schema) = open();
        db.take_snapshot(Watermark::for_checkpoint(3));
        db.take_snapshot(Watermark::for_checkpoint(10));
        db.take_snapshot(Watermark::for_checkpoint(5));
        assert_eq!(db.snapshot_range(), Some(3..=10));
    }

    #[test]
    fn snapshot_capacity_zero_disables_snapshotting() {
        let (_dir, db, _schema) = open_with_capacity(0);
        db.take_snapshot(Watermark::for_checkpoint(1));
        db.take_snapshot(Watermark::for_checkpoint(2));
        assert!(db.at_snapshot(1).is_none());
        assert!(db.at_snapshot(2).is_none());
        assert!(db.latest_snapshot().is_none());
        assert!(db.snapshot_range().is_none());
    }

    #[test]
    fn snapshot_capacity_evicts_oldest() {
        let (_dir, db, _schema) = open_with_capacity(2);
        db.take_snapshot(Watermark::for_checkpoint(1));
        db.take_snapshot(Watermark::for_checkpoint(2));
        db.take_snapshot(Watermark::for_checkpoint(3));
        assert!(db.at_snapshot(1).is_none());
        assert!(db.at_snapshot(2).is_some());
        assert!(db.at_snapshot(3).is_some());
        assert_eq!(db.snapshot_range(), Some(2..=3));
    }

    #[test]
    fn drop_snapshot_removes_from_buffer() {
        let (_dir, db, _schema) = open();
        db.take_snapshot(Watermark::for_checkpoint(5));
        assert!(db.drop_snapshot(5));
        assert!(db.at_snapshot(5).is_none());
        // Dropping a missing snapshot returns false.
        assert!(!db.drop_snapshot(5));
    }

    #[test]
    fn snapshot_sees_state_at_take_time() {
        let (_dir, db, schema) = open();
        put(&db, &schema, 1, 100);
        db.take_snapshot(Watermark::for_checkpoint(1));
        // Mutate after taking the snapshot.
        put(&db, &schema, 1, 999);

        let snap = db.at_snapshot(1).unwrap();
        assert_eq!(
            schema.items.at(&snap).get(&U64Be(1)).unwrap(),
            Some(U64Be(100)),
        );
        assert_eq!(schema.items.get(&U64Be(1)).unwrap(), Some(U64Be(999)));
    }

    #[test]
    fn snapshot_does_not_see_keys_inserted_after_take() {
        let (_dir, db, schema) = open();
        db.take_snapshot(Watermark::for_checkpoint(0));
        put(&db, &schema, 1, 100);
        let snap = db.at_snapshot(0).unwrap();
        assert!(schema.items.at(&snap).get(&U64Be(1)).unwrap().is_none());
    }

    #[test]
    fn snapshot_get_raw_against_pre_snapshot_state() {
        let (_dir, db, schema) = open();
        put(&db, &schema, 1, 100);
        db.take_snapshot(Watermark::for_checkpoint(1));
        put(&db, &schema, 1, 999);
        let snap = db.at_snapshot(1).unwrap();
        let bytes = schema
            .items
            .at(&snap)
            .get_raw(&U64Be(1))
            .unwrap()
            .expect("value should exist in snapshot");
        assert_eq!(&bytes[..], &100u64.to_be_bytes());
    }

    #[test]
    fn snapshot_contains_key_reflects_pre_snapshot_state() {
        let (_dir, db, schema) = open();
        put(&db, &schema, 1, 100);
        db.take_snapshot(Watermark::for_checkpoint(1));
        // Mutation after snapshot must not affect snapshot's view.
        let mut batch = db.batch();
        batch.delete(&schema.items, &U64Be(1)).unwrap();
        batch.commit().unwrap();
        let snap = db.at_snapshot(1).unwrap();
        assert!(schema.items.at(&snap).contains_key(&U64Be(1)).unwrap());
        assert!(!schema.items.contains_key(&U64Be(1)).unwrap());
    }

    #[test]
    fn snapshot_multi_get_against_pre_snapshot_state() {
        let (_dir, db, schema) = open();
        put(&db, &schema, 1, 10);
        put(&db, &schema, 3, 30);
        db.take_snapshot(Watermark::for_checkpoint(1));
        // After-snapshot writes should not affect snapshot reads.
        put(&db, &schema, 2, 20);
        put(&db, &schema, 1, 999);

        let snap = db.at_snapshot(1).unwrap();
        let keys = [U64Be(1), U64Be(2), U64Be(3)];
        let results = schema.items.at(&snap).multi_get(keys.iter()).unwrap();
        assert_eq!(results[0].as_ref().unwrap(), &Some(U64Be(10)));
        assert_eq!(results[1].as_ref().unwrap(), &None);
        assert_eq!(results[2].as_ref().unwrap(), &Some(U64Be(30)));
    }

    #[test]
    fn snapshot_iter_yields_pre_snapshot_state_in_order() {
        let (_dir, db, schema) = open();
        put(&db, &schema, 1, 10);
        put(&db, &schema, 3, 30);
        db.take_snapshot(Watermark::for_checkpoint(1));
        put(&db, &schema, 2, 20);

        let snap = db.at_snapshot(1).unwrap();
        let snap_items = schema.items.at(&snap);
        let collected: Vec<_> = snap_items.iter(..).unwrap().map(Result::unwrap).collect();
        assert_eq!(
            collected,
            vec![(U64Be(1), U64Be(10)), (U64Be(3), U64Be(30))],
        );
    }

    #[test]
    fn snapshot_iter_rev_yields_pre_snapshot_state_reversed() {
        let (_dir, db, schema) = open();
        put(&db, &schema, 1, 10);
        put(&db, &schema, 2, 20);
        db.take_snapshot(Watermark::for_checkpoint(1));

        let snap = db.at_snapshot(1).unwrap();
        let snap_items = schema.items.at(&snap);
        let collected: Vec<_> = snap_items
            .iter_rev(..)
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            collected,
            vec![(U64Be(2), U64Be(20)), (U64Be(1), U64Be(10))],
        );
    }

    #[test]
    fn snapshot_iter_prefix_filters_against_pre_snapshot_state() {
        let (_dir, db, schema) = open();
        // Use the same encoding as U64Be for prefix; an 8-byte
        // prefix matches an exact key, demonstrating prefix
        // iteration on the snapshot path.
        put(&db, &schema, 1, 10);
        put(&db, &schema, 2, 20);
        db.take_snapshot(Watermark::for_checkpoint(1));
        put(&db, &schema, 1, 999);

        let snap = db.at_snapshot(1).unwrap();
        let snap_items = schema.items.at(&snap);
        let collected: Vec<_> = snap_items
            .iter_prefix(&U64Be(1))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(collected, vec![(U64Be(1), U64Be(10))]);
    }

    #[test]
    fn snapshot_survives_drop_snapshot() {
        let (_dir, db, schema) = open();
        put(&db, &schema, 1, 100);
        db.take_snapshot(Watermark::for_checkpoint(1));
        let snap = db.at_snapshot(1).unwrap();
        // Remove the snapshot from the buffer; the token still works.
        assert!(db.drop_snapshot(1));
        assert!(db.at_snapshot(1).is_none());
        assert_eq!(
            schema.items.at(&snap).get(&U64Be(1)).unwrap(),
            Some(U64Be(100)),
        );
    }

    #[test]
    fn snapshot_clones_share_underlying_snapshot() {
        let (_dir, db, schema) = open();
        put(&db, &schema, 1, 100);
        db.take_snapshot(Watermark::for_checkpoint(1));
        let snap_a = db.at_snapshot(1).unwrap();
        let snap_b = snap_a.clone();
        // Drop the buffer ref; both clones still see the same state.
        db.drop_snapshot(1);
        put(&db, &schema, 1, 999);
        assert_eq!(
            schema.items.at(&snap_a).get(&U64Be(1)).unwrap(),
            Some(U64Be(100)),
        );
        assert_eq!(
            schema.items.at(&snap_b).get(&U64Be(1)).unwrap(),
            Some(U64Be(100)),
        );
    }

    #[test]
    fn snapshot_outlives_schema() {
        let (_dir, db, schema) = open();
        put(&db, &schema, 1, 100);
        db.take_snapshot(Watermark::for_checkpoint(1));
        let snap = db.at_snapshot(1).unwrap();
        // Schema (and its DbMap) drops; the snapshot token still
        // co-owns Arc<Db>, so the underlying database is alive.
        // We re-open a temporary DbMap pointed at the same CF and
        // re-bind it at the snapshot to exercise reads.
        let items: DbMap<U64Be, U64Be> = DbMap::new(db.clone(), "items").unwrap();
        drop(schema);
        assert_eq!(items.at(&snap).get(&U64Be(1)).unwrap(), Some(U64Be(100)));
    }

    #[test]
    fn taking_snapshot_at_existing_checkpoint_replaces() {
        let (_dir, db, schema) = open();
        put(&db, &schema, 1, 100);
        db.take_snapshot(Watermark::for_checkpoint(1));
        put(&db, &schema, 1, 200);
        // Re-take at the same checkpoint; the new snapshot reflects
        // the updated state.
        db.take_snapshot(Watermark::for_checkpoint(1));
        let snap = db.at_snapshot(1).unwrap();
        assert_eq!(
            schema.items.at(&snap).get(&U64Be(1)).unwrap(),
            Some(U64Be(200)),
        );
    }

    #[test]
    fn snapshot_keeps_underlying_snapshot_alive_through_eviction() {
        // Mirrors alt's test_iteration_snapshot_keepalive. A
        // `Snapshot` (here standing in for an iterator built from it)
        // co-owns the SnapshotEntry; capacity-based eviction of the
        // buffer must not break reads through the token.
        let (_dir, db, schema) = open_with_capacity(2);
        put(&db, &schema, 1, 100);
        db.take_snapshot(Watermark::for_checkpoint(1));
        // Hold a Snapshot (and through it, an Arc<SnapshotEntry>).
        let snap = db.at_snapshot(1).unwrap();
        // Push two more snapshots so checkpoint 1 evicts.
        db.take_snapshot(Watermark::for_checkpoint(2));
        db.take_snapshot(Watermark::for_checkpoint(3));
        assert!(db.at_snapshot(1).is_none(), "snapshot 1 should evict");
        // Reads through the held Snapshot still work.
        assert_eq!(
            schema.items.at(&snap).get(&U64Be(1)).unwrap(),
            Some(U64Be(100)),
        );
    }

    #[test]
    fn iterator_keeps_snapshot_alive_through_eviction() {
        // Stronger version of the above: an active Iter built from
        // the snapshot survives buffer eviction. The Iter borrows
        // from the snapshot-bound DbMap, which owns a clone of the
        // `Snapshot`, which holds an Arc<SnapshotEntry>.
        let (_dir, db, schema) = open_with_capacity(2);
        put(&db, &schema, 1, 10);
        put(&db, &schema, 2, 20);
        put(&db, &schema, 3, 30);
        db.take_snapshot(Watermark::for_checkpoint(1));

        let snap = db.at_snapshot(1).unwrap();
        let snap_items = schema.items.at(&snap);
        let mut iter = snap_items.iter(..).unwrap();
        // Consume one element.
        assert_eq!(iter.next().unwrap().unwrap(), (U64Be(1), U64Be(10)));

        // Force eviction of snapshot 1.
        db.take_snapshot(Watermark::for_checkpoint(2));
        db.take_snapshot(Watermark::for_checkpoint(3));
        assert!(db.at_snapshot(1).is_none());

        // The iterator continues to yield the snapshot's data.
        let rest: Vec<_> = (&mut iter).map(Result::unwrap).collect();
        assert_eq!(rest, vec![(U64Be(2), U64Be(20)), (U64Be(3), U64Be(30))]);
    }

    /// Demonstrates the whole-schema re-binding pattern via
    /// `SchemaAtSnapshot::at`. The projected schema's reads see the
    /// captured snapshot state for every CF.
    #[test]
    fn schema_at_snapshot_projects_all_fields() {
        let (_dir, db, schema) = open();
        put(&db, &schema, 1, 100);
        db.take_snapshot(Watermark::for_checkpoint(1));
        put(&db, &schema, 1, 999);

        let snap = db.at_snapshot(1).unwrap();
        let snap_schema = schema.at(&snap);
        assert_eq!(snap_schema.items.get(&U64Be(1)).unwrap(), Some(U64Be(100)),);
    }

    #[test]
    fn at_ref_reads_pre_snapshot_state() {
        // SnapshotRef variant of `snapshot_sees_state_at_take_time`:
        // re-bind the map at a borrowed snapshot instead of an
        // owned (cloned) one, then confirm reads still see the
        // captured state.
        let (_dir, db, schema) = open();
        put(&db, &schema, 1, 100);
        db.take_snapshot(Watermark::for_checkpoint(1));
        put(&db, &schema, 1, 999);

        let snap = db.at_snapshot(1).unwrap();
        let snap_items = schema.items.at_ref(&snap);
        assert_eq!(snap_items.get(&U64Be(1)).unwrap(), Some(U64Be(100)));
    }

    #[test]
    fn at_ref_does_not_clone_the_snapshot() {
        // The borrowed re-bind must not bump the SnapshotEntry's
        // refcount. Take a snapshot, hold one extra clone (refcount
        // = 2), re-bind via at_ref, and verify the refcount stays
        // at 2 — proving no Arc clone happened in `at_ref`.
        let (_dir, db, schema) = open();
        put(&db, &schema, 1, 100);
        db.take_snapshot(Watermark::for_checkpoint(1));
        let snap = db.at_snapshot(1).unwrap();
        let _extra = snap.clone();
        // The buffer also holds an Arc<SnapshotEntry>, so the
        // entry's strong count is 3 here (snap + extra + buffer).
        let before = std::sync::Arc::strong_count(&snap.entry);
        let _items_ref = schema.items.at_ref(&snap);
        let after = std::sync::Arc::strong_count(&snap.entry);
        assert_eq!(before, after, "at_ref must not bump the entry refcount");
    }

    #[test]
    fn at_ref_iter_yields_pre_snapshot_state() {
        // Iteration through a SnapshotRef-bound map should produce
        // the same view as iteration through a Snapshot-bound one.
        let (_dir, db, schema) = open();
        put(&db, &schema, 1, 10);
        put(&db, &schema, 3, 30);
        db.take_snapshot(Watermark::for_checkpoint(1));
        put(&db, &schema, 2, 20);

        let snap = db.at_snapshot(1).unwrap();
        let snap_items = schema.items.at_ref(&snap);
        let collected: Vec<_> = snap_items.iter(..).unwrap().map(Result::unwrap).collect();
        assert_eq!(
            collected,
            vec![(U64Be(1), U64Be(10)), (U64Be(3), U64Be(30))],
        );
    }

    #[test]
    #[should_panic(expected = "snapshot was taken on a different Db")]
    fn at_ref_panics_when_snapshot_is_from_a_different_db() {
        // Mirrors the owned-snapshot version: cross-Db re-binding
        // is a programmer error on the borrowed path too.
        let (_dir_a, db_a, schema_a) = open();
        let (_dir_b, db_b, _schema_b) = open();
        db_b.take_snapshot(Watermark::for_checkpoint(1));
        let snap_b = db_b.at_snapshot(1).unwrap();
        let _ = &db_a;
        let _ = schema_a.items.at_ref(&snap_b);
    }
}
