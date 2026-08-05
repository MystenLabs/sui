// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `()` → `PruningWatermarks`.
//!
//! Singleton row that holds the lowest still-available `tx_seq`
//! and `checkpoint_seq`. It is the durable authority for the
//! bitmap CFs' compaction filters and feeds `available_range`
//! requests.
//!
//! Each open RPC-store schema owns an `Arc<AtomicU64>` whose clones
//! are captured by that database's bitmap compaction filters. Schema
//! construction loads the persisted `tx_seq` floor into the atomic.
//! Pruning and restore paths commit the singleton row before
//! publishing its value to the matching database-local atomic.

use std::sync::atomic::Ordering;

use sui_consistent_store::Protobuf;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;

use crate::proto::PruningWatermarks;
use crate::schema::primitives::UnitKey;

pub const NAME: &str = "pruning_watermark";

pub type Key = UnitKey;
pub type Value = Protobuf<PruningWatermarks>;

pub fn options(resolver: &sui_consistent_store::CfOptionsResolver) -> rocksdb::Options {
    resolver.options(NAME)
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
}

impl super::RpcStoreSchema {
    /// Publish the `tx_seq` floor used by this database's bitmap
    /// compaction filters.
    ///
    /// Callers publish only a committed `tx_seq_lo`, or zero
    /// immediately after durably clearing the persisted watermark.
    /// Restores may intentionally install a lower committed floor
    /// before history backfill writes begin.
    pub fn set_pruning_floor(&self, tx_seq_lo: u64) {
        self.tx_seq_pruning_floor
            .store(tx_seq_lo, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub(crate) fn current_pruning_floor(&self) -> u64 {
        self.tx_seq_pruning_floor.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;

    use super::*;
    use crate::RpcStoreSchema;
    use crate::schema::event_bitmap;
    use crate::schema::transaction_bitmap;

    fn fresh_db() -> (tempfile::TempDir, Db, RpcStoreSchema) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    fn put_materialized_bucket_zero(db: &Db, schema: &RpcStoreSchema, dimension: &[u8]) {
        let (tx_key, tx_value) = transaction_bitmap::store_match(dimension.to_vec(), 5);
        let (event_key, event_value) = event_bitmap::store_match(dimension.to_vec(), 5, 0);
        let mut batch = db.batch();
        batch
            .put(&schema.transaction_bitmap, &tx_key, &tx_value)
            .unwrap();
        batch
            .put(&schema.event_bitmap, &event_key, &event_value)
            .unwrap();
        batch.commit().unwrap();
        db.flush().unwrap();
    }

    #[test]
    fn fresh_empty_db_starts_with_zero_pruning_floor() {
        let (_dir, _db, schema) = fresh_db();
        assert!(schema.get_pruning_watermarks().unwrap().is_none());
        assert_eq!(schema.current_pruning_floor(), 0);
    }

    #[test]
    fn reopen_loads_persisted_pruning_floor() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        let floor = transaction_bitmap::TX_BUCKET_SIZE;
        let (watermark_key, watermark_value) = store(&Watermarks {
            tx_seq_lo: floor,
            checkpoint_lo: 1,
        });
        let mut batch = db.batch();
        batch
            .put(&schema.pruning_watermark, &watermark_key, &watermark_value)
            .unwrap();
        batch.commit().unwrap();
        put_materialized_bucket_zero(&db, &schema, b"reopen");
        assert_eq!(schema.current_pruning_floor(), 0);

        drop(schema);
        drop(db);

        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        assert_eq!(schema.current_pruning_floor(), floor);
        db.compact_range_cf(transaction_bitmap::NAME, None, None)
            .unwrap();
        db.compact_range_cf(event_bitmap::NAME, None, None).unwrap();
        assert!(
            schema
                .get_transaction_bitmap(b"reopen".to_vec(), 0)
                .unwrap()
                .is_none()
        );
        assert!(
            schema
                .get_event_bitmap(b"reopen".to_vec(), 0)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn bitmap_pruning_floors_are_isolated_per_database() {
        let (_dir_a, db_a, schema_a) = fresh_db();
        let (_dir_b, db_b, schema_b) = fresh_db();
        let floor = transaction_bitmap::TX_BUCKET_SIZE;

        let (watermark_key, watermark_value) = store(&Watermarks {
            tx_seq_lo: floor,
            checkpoint_lo: 1,
        });
        let mut batch = db_a.batch();
        batch
            .put(
                &schema_a.pruning_watermark,
                &watermark_key,
                &watermark_value,
            )
            .unwrap();
        batch.commit().unwrap();
        schema_a.set_pruning_floor(floor);

        put_materialized_bucket_zero(&db_a, &schema_a, b"isolated");
        put_materialized_bucket_zero(&db_b, &schema_b, b"isolated");

        for db in [&db_a, &db_b] {
            db.compact_range_cf(transaction_bitmap::NAME, None, None)
                .unwrap();
            db.compact_range_cf(event_bitmap::NAME, None, None).unwrap();
        }

        assert!(
            schema_a
                .get_transaction_bitmap(b"isolated".to_vec(), 0)
                .unwrap()
                .is_none()
        );
        assert!(
            schema_a
                .get_event_bitmap(b"isolated".to_vec(), 0)
                .unwrap()
                .is_none()
        );
        assert!(
            schema_b
                .get_transaction_bitmap(b"isolated".to_vec(), 0)
                .unwrap()
                .is_some()
        );
        assert!(
            schema_b
                .get_event_bitmap(b"isolated".to_vec(), 0)
                .unwrap()
                .is_some()
        );
        assert!(schema_b.get_pruning_watermarks().unwrap().is_none());
        assert_eq!(schema_b.current_pruning_floor(), 0);
    }
}
