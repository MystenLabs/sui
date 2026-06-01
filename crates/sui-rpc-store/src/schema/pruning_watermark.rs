// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `()` → `PruningWatermarks`.
//!
//! Singleton row that holds the lowest still-available `tx_seq`
//! and `checkpoint_seq`. Drives the bitmap CFs' compaction
//! filters and feeds `available_range` requests.
//!
//! The bitmap CFs need to know the current `tx_seq` floor at
//! compaction time, which runs in a RocksDB background thread
//! without access to the schema. The pattern used here:
//!
//! 1. A process-wide `Arc<AtomicU64>` ([`tx_seq_floor`]) holds the
//!    current floor.
//! 2. Bitmap CF [`options`](super::transaction_bitmap::options)
//!    install compaction filters that clone the `Arc` and read the
//!    atomic on every key they consider.
//! 3. Indexer pipelines that advance pruning call
//!    [`RpcStoreSchema::set_pruning_floor`] after their batch
//!    commits, so the on-disk row and the atomic agree.
//! 4. On startup callers run
//!    [`RpcStoreSchema::refresh_pruning_atomics`] once to load the
//!    persisted floor into the atomic.

use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use sui_consistent_store::Protobuf;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;

use crate::proto::PruningWatermarks;
use crate::schema::keys::UnitKey;

pub const NAME: &str = "pruning_watermark";

pub type Key = UnitKey;
pub type Value = Protobuf<PruningWatermarks>;

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}

/// Caller-facing view of the pruning watermarks.
///
/// `tx_seq_lo` is the lowest `tx_seq` whose downstream rows
/// (`tx_metadata_by_seq`, `transactions`, `effects`, `events`,
/// and the bitmap CFs) are still present. Everything strictly
/// below it has been pruned.
///
/// `checkpoint_lo` is the analogous floor for the
/// `checkpoint_summary` / `checkpoint_contents` CFs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Watermarks {
    pub tx_seq_lo: u64,
    pub checkpoint_lo: u64,
}

/// Build the singleton `(Key, Value)` pair recording the current
/// pruning floor.
pub fn store(watermarks: &Watermarks) -> (Key, Value) {
    (
        UnitKey,
        Protobuf(PruningWatermarks {
            tx_seq_lo: watermarks.tx_seq_lo,
            checkpoint_lo: watermarks.checkpoint_lo,
        }),
    )
}

/// Process-wide `tx_seq` pruning floor used by the bitmap CFs'
/// compaction filters. Lazily allocated on first access.
///
/// The atomic carries the *exclusive* floor: every `tx_seq <
/// floor` is considered pruned. Bitmap buckets that fit entirely
/// below the floor become removable on the next compaction sweep.
pub fn tx_seq_floor() -> &'static Arc<AtomicU64> {
    static FLOOR: OnceLock<Arc<AtomicU64>> = OnceLock::new();
    FLOOR.get_or_init(|| Arc::new(AtomicU64::new(0)))
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Read the persisted pruning watermarks from disk.
    pub fn get_pruning_watermarks(&self) -> Result<Option<Watermarks>, Error> {
        let Some(stored) = self.pruning_watermark.get(&UnitKey)? else {
            return Ok(None);
        };
        let stored = stored.into_inner();
        Ok(Some(Watermarks {
            tx_seq_lo: stored.tx_seq_lo,
            checkpoint_lo: stored.checkpoint_lo,
        }))
    }

    /// Update the `tx_seq` floor used by the bitmap CFs'
    /// compaction filters.
    ///
    /// Callers that advance pruning should:
    ///
    /// 1. Stage a write to the `pruning_watermark` CF via
    ///    [`store`].
    /// 2. Commit the batch so the new watermarks are durable.
    /// 3. Call this method with the new `tx_seq_lo` so the
    ///    in-memory floor matches what's on disk.
    pub fn set_pruning_floor(&self, tx_seq_lo: u64) {
        tx_seq_floor().store(tx_seq_lo, Ordering::Relaxed);
    }

    /// Load the persisted pruning watermarks from disk into the
    /// in-memory bitmap floor.
    ///
    /// Call once on startup so the bitmap compaction filters see
    /// the persisted floor instead of starting at zero (where
    /// they'd prune nothing).
    pub fn refresh_pruning_atomics(&self) -> Result<(), Error> {
        if let Some(watermarks) = self.get_pruning_watermarks()? {
            self.set_pruning_floor(watermarks.tx_seq_lo);
        }
        Ok(())
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
    fn get_returns_none_when_empty() {
        let (_dir, _db, schema) = fresh_db();
        assert!(schema.get_pruning_watermarks().unwrap().is_none());
    }

    #[test]
    fn store_then_get_round_trips() {
        let (_dir, db, schema) = fresh_db();
        let watermarks = Watermarks {
            tx_seq_lo: 1_000,
            checkpoint_lo: 50,
        };

        let (k, v) = store(&watermarks);
        let mut batch = db.batch();
        batch.put(&schema.pruning_watermark, &k, &v).unwrap();
        batch.commit().unwrap();

        let read = schema
            .get_pruning_watermarks()
            .unwrap()
            .expect("watermarks present");
        assert_eq!(read, watermarks);
    }

    #[test]
    fn set_pruning_floor_updates_atomic() {
        // Take a baseline so this test isn't affected by ordering
        // against other tests sharing the process-wide atomic.
        let baseline = tx_seq_floor().load(Ordering::Relaxed);
        let (_dir, _db, schema) = fresh_db();
        let target = baseline.wrapping_add(12_345);
        schema.set_pruning_floor(target);
        assert_eq!(tx_seq_floor().load(Ordering::Relaxed), target);
        // Restore the floor so we don't leak state across tests.
        tx_seq_floor().store(baseline, Ordering::Relaxed);
    }

    #[test]
    fn refresh_pulls_disk_watermarks_into_atomic() {
        let baseline = tx_seq_floor().load(Ordering::Relaxed);
        let (_dir, db, schema) = fresh_db();
        let target = baseline.wrapping_add(67_890);

        let (k, v) = store(&Watermarks {
            tx_seq_lo: target,
            checkpoint_lo: 0,
        });
        let mut batch = db.batch();
        batch.put(&schema.pruning_watermark, &k, &v).unwrap();
        batch.commit().unwrap();

        schema.refresh_pruning_atomics().unwrap();
        assert_eq!(tx_seq_floor().load(Ordering::Relaxed), target);
        tx_seq_floor().store(baseline, Ordering::Relaxed);
    }

    #[test]
    fn refresh_is_a_no_op_when_disk_is_empty() {
        let baseline = tx_seq_floor().load(Ordering::Relaxed);
        let (_dir, _db, schema) = fresh_db();
        // No write — refresh should not touch the atomic.
        schema.refresh_pruning_atomics().unwrap();
        assert_eq!(tx_seq_floor().load(Ordering::Relaxed), baseline);
    }
}
