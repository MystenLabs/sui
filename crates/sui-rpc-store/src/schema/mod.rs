// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Column-family layout for `sui-rpc-store`.
//!
//! Each CF lives in its own submodule that declares:
//!
//! - `NAME` — the on-disk column-family name.
//! - `Key` — the key type with `Encode` / `Decode` pinning its
//!   on-disk layout.
//! - `Value` — the value type, typically `Protobuf<…>`.
//! - `options(resolver)` — per-CF `rocksdb::Options`, obtained from
//!   the [`CfOptionsResolver`] with the CF's merge operator and
//!   compaction filter (if any) layered on top.
//!
//! [`RpcStoreSchema`] aggregates these into the schema passed to
//! [`sui_consistent_store::Db::open`]. Keys reused across multiple
//! CFs live in [`primitives`].

pub mod balance;
pub mod checkpoint_contents;
pub mod checkpoint_seq_by_digest;
pub mod checkpoint_summary;
pub mod effects;
pub mod epochs;
pub mod event_bitmap;
pub mod events;
pub mod object_by_owner;
pub mod object_by_type;
pub mod object_version_by_checkpoint;
pub mod objects;
pub mod package_versions;
pub mod primitives;
pub mod pruning_watermark;
pub mod transaction_bitmap;
pub mod transactions;
pub mod tx_metadata_by_seq;
pub mod tx_seq_by_digest;
pub mod type_filter;

use std::collections::BTreeMap;

use sui_consistent_store::CfDescriptor;
use sui_consistent_store::CfOptionsResolver;
use sui_consistent_store::CfTuning;
use sui_consistent_store::Compression;
use sui_consistent_store::Db;
use sui_consistent_store::DbMap;
use sui_consistent_store::DbWideConfig;
use sui_consistent_store::RocksDbConfig;
use sui_consistent_store::Schema;
use sui_consistent_store::SchemaAtSnapshot;
use sui_consistent_store::Snapshot;
use sui_consistent_store::WriteStallConfig;
use sui_consistent_store::error::OpenError;
use sui_consistent_store::reader::Reader;

/// Typed handles to every CF in the `sui-rpc-store` layout.
pub struct RpcStoreSchema<R: Reader = Db> {
    /// Per-epoch metadata: protocol version, gas price, start and
    /// end timestamps, and the epoch's final checkpoint.
    pub epochs: DbMap<epochs::Key, epochs::Value, R>,

    /// Signed checkpoint headers. The lightweight metadata served
    /// by most "fetch a checkpoint" requests; the heavier contents
    /// list lives in a separate CF.
    pub checkpoint_summary: DbMap<checkpoint_summary::Key, checkpoint_summary::Value, R>,

    /// The ordered list of executed transaction digests in each
    /// checkpoint.
    pub checkpoint_contents: DbMap<checkpoint_contents::Key, checkpoint_contents::Value, R>,

    /// Resolves a checkpoint digest to its sequence number, which
    /// is then the key for every other checkpoint-keyed CF.
    pub checkpoint_seq_by_digest:
        DbMap<checkpoint_seq_by_digest::Key, checkpoint_seq_by_digest::Value, R>,

    /// Signed transactions, keyed by their assigned tx_seq.
    pub transactions: DbMap<transactions::Key, transactions::Value, R>,

    /// Resolves a transaction digest to its assigned tx_seq.
    pub tx_seq_by_digest: DbMap<tx_seq_by_digest::Key, tx_seq_by_digest::Value, R>,

    /// Per-transaction metadata: digest, the containing
    /// checkpoint, position within that checkpoint, event count,
    /// and the checkpoint's timestamp.
    pub tx_metadata_by_seq: DbMap<tx_metadata_by_seq::Key, tx_metadata_by_seq::Value, R>,

    /// The effects produced by each transaction, together with the
    /// set of objects loaded but unchanged during execution.
    pub effects: DbMap<effects::Key, effects::Value, R>,

    /// The events emitted by each transaction.
    pub events: DbMap<events::Key, events::Value, R>,

    /// Every version of every object that has ever existed. A
    /// prefix scan on the object id walks all versions in ascending
    /// order; a reverse prefix scan resolves the latest version (the
    /// greatest `(id, version)` row), the way the validator perpetual
    /// store serves "latest object" reads.
    pub objects: DbMap<objects::Key, objects::Value, R>,

    /// An object's version as of a checkpoint: keyed by
    /// `(object id, checkpoint)`, a reverse prefix scan resolves the
    /// version live at the end of the most recent checkpoint, at or
    /// before the one queried, in which the object changed. Backs
    /// checkpoint-pinned historical reads that the version-keyed
    /// `objects` CF cannot answer.
    pub object_version_by_checkpoint:
        DbMap<object_version_by_checkpoint::Key, object_version_by_checkpoint::Value, R>,

    /// Supports listing an owner's objects, optionally filtered by
    /// Move type. Coin-like objects sort richest-first within
    /// each `(owner, type)` group so paginating valuable holdings
    /// is a forward prefix scan.
    pub object_by_owner: DbMap<object_by_owner::Key, object_by_owner::Value, R>,

    /// Supports listing every live object of a given Move type,
    /// regardless of owner.
    pub object_by_type: DbMap<object_by_type::Key, object_by_type::Value, R>,

    /// Tracks an account's balance per coin type, combining the
    /// coin-derived component (sum of owned `Coin<T>` balances)
    /// and the accumulator-derived component into a single row
    /// merged from independent indexer pipelines.
    pub balance: DbMap<balance::Key, balance::Value, R>,

    /// Tracks every published version of a Move package and the
    /// storage id under which each version lives.
    pub package_versions: DbMap<package_versions::Key, package_versions::Value, R>,

    /// Inverted bitmap index over transaction-sequence space,
    /// supporting filtered transaction queries by indexed fields
    /// such as sender, called function, or input/changed object.
    pub transaction_bitmap: DbMap<transaction_bitmap::Key, transaction_bitmap::Value, R>,

    /// Inverted bitmap index over packed event-sequence space,
    /// supporting filtered event queries by event type, emitting
    /// module, sender, and similar indexed fields.
    pub event_bitmap: DbMap<event_bitmap::Key, event_bitmap::Value, R>,

    // --- Bookkeeping ---
    /// Singleton holding the lowest still-available `tx_seq`,
    /// `checkpoint_seq`, and object version. Drives compaction
    /// filters and feeds `available_range` responses.
    pub pruning_watermark: DbMap<pruning_watermark::Key, pruning_watermark::Value, R>,
}

impl Schema for RpcStoreSchema {
    fn cfs(opts: &CfOptionsResolver) -> Vec<CfDescriptor> {
        vec![
            CfDescriptor::new(epochs::NAME, epochs::options(opts)),
            CfDescriptor::new(checkpoint_summary::NAME, checkpoint_summary::options(opts)),
            CfDescriptor::new(
                checkpoint_contents::NAME,
                checkpoint_contents::options(opts),
            ),
            CfDescriptor::new(
                checkpoint_seq_by_digest::NAME,
                checkpoint_seq_by_digest::options(opts),
            ),
            CfDescriptor::new(transactions::NAME, transactions::options(opts)),
            CfDescriptor::new(tx_seq_by_digest::NAME, tx_seq_by_digest::options(opts)),
            CfDescriptor::new(tx_metadata_by_seq::NAME, tx_metadata_by_seq::options(opts)),
            CfDescriptor::new(effects::NAME, effects::options(opts)),
            CfDescriptor::new(events::NAME, events::options(opts)),
            CfDescriptor::new(objects::NAME, objects::options(opts)),
            CfDescriptor::new(
                object_version_by_checkpoint::NAME,
                object_version_by_checkpoint::options(opts),
            ),
            CfDescriptor::new(object_by_owner::NAME, object_by_owner::options(opts)),
            CfDescriptor::new(object_by_type::NAME, object_by_type::options(opts)),
            CfDescriptor::new(balance::NAME, balance::options(opts)),
            CfDescriptor::new(package_versions::NAME, package_versions::options(opts)),
            CfDescriptor::new(transaction_bitmap::NAME, transaction_bitmap::options(opts)),
            CfDescriptor::new(event_bitmap::NAME, event_bitmap::options(opts)),
            CfDescriptor::new(pruning_watermark::NAME, pruning_watermark::options(opts)),
        ]
    }

    fn open(db: &Db) -> Result<Self, OpenError> {
        Ok(Self {
            epochs: DbMap::new(db.clone(), epochs::NAME)?,
            checkpoint_summary: DbMap::new(db.clone(), checkpoint_summary::NAME)?,
            checkpoint_contents: DbMap::new(db.clone(), checkpoint_contents::NAME)?,
            checkpoint_seq_by_digest: DbMap::new(db.clone(), checkpoint_seq_by_digest::NAME)?,
            transactions: DbMap::new(db.clone(), transactions::NAME)?,
            tx_seq_by_digest: DbMap::new(db.clone(), tx_seq_by_digest::NAME)?,
            tx_metadata_by_seq: DbMap::new(db.clone(), tx_metadata_by_seq::NAME)?,
            effects: DbMap::new(db.clone(), effects::NAME)?,
            events: DbMap::new(db.clone(), events::NAME)?,
            objects: DbMap::new(db.clone(), objects::NAME)?,
            object_version_by_checkpoint: DbMap::new(
                db.clone(),
                object_version_by_checkpoint::NAME,
            )?,
            object_by_owner: DbMap::new(db.clone(), object_by_owner::NAME)?,
            object_by_type: DbMap::new(db.clone(), object_by_type::NAME)?,
            balance: DbMap::new(db.clone(), balance::NAME)?,
            package_versions: DbMap::new(db.clone(), package_versions::NAME)?,
            transaction_bitmap: DbMap::new(db.clone(), transaction_bitmap::NAME)?,
            event_bitmap: DbMap::new(db.clone(), event_bitmap::NAME)?,
            pruning_watermark: DbMap::new(db.clone(), pruning_watermark::NAME)?,
        })
    }
}

impl SchemaAtSnapshot for RpcStoreSchema {
    type At = RpcStoreSchema<Snapshot>;
    fn at(&self, snap: &Snapshot) -> Self::At {
        RpcStoreSchema {
            epochs: self.epochs.at(snap),
            checkpoint_summary: self.checkpoint_summary.at(snap),
            checkpoint_contents: self.checkpoint_contents.at(snap),
            checkpoint_seq_by_digest: self.checkpoint_seq_by_digest.at(snap),
            transactions: self.transactions.at(snap),
            tx_seq_by_digest: self.tx_seq_by_digest.at(snap),
            tx_metadata_by_seq: self.tx_metadata_by_seq.at(snap),
            effects: self.effects.at(snap),
            events: self.events.at(snap),
            objects: self.objects.at(snap),
            object_version_by_checkpoint: self.object_version_by_checkpoint.at(snap),
            object_by_owner: self.object_by_owner.at(snap),
            object_by_type: self.object_by_type.at(snap),
            balance: self.balance.at(snap),
            package_versions: self.package_versions.at(snap),
            transaction_bitmap: self.transaction_bitmap.at(snap),
            event_bitmap: self.event_bitmap.at(snap),
            pruning_watermark: self.pruning_watermark.at(snap),
        }
    }
}

/// The tuned [`RocksDbConfig`] this crate ships as its baseline for
/// the `sui-rpc-store` column families.
///
/// Operators layer their own overrides on top via
/// [`RocksDbConfig::merge_over`]; anything they leave unset falls back
/// to these values. The defaults port the production-proven settings
/// from `typed_store` and bake in a "no write stalls, generous
/// compaction parallelism" policy: the pending-compaction stall limits
/// are disabled and the L0 triggers raised so neither the bulk restore
/// nor steady-state indexing throttles on compaction debt, while the
/// L0 stop trigger still bounds a runaway backlog.
pub fn default_rocksdb_config() -> RocksDbConfig {
    let write_stall = WriteStallConfig {
        soft_pending_compaction_bytes_limit_mb: Some(0),
        hard_pending_compaction_bytes_limit_mb: Some(0),
        level0_file_num_compaction_trigger: Some(4),
        level0_slowdown_writes_trigger: Some(512),
        level0_stop_writes_trigger: Some(1024),
    };

    let default_cf = CfTuning {
        write_buffer_size_mb: Some(64),
        max_write_buffer_number: Some(6),
        compression: Some(Compression::Lz4),
        bottommost_compression: Some(Compression::Zstd),
        block_size_kb: Some(16),
        bloom_filter_bits: None,
        memtable_prefix_bloom_ratio: Some(0.02),
        target_file_size_mb: Some(128),
        write_stall,
    };

    let mut column_family = BTreeMap::new();

    // Point-lookup CFs: a whole-key bloom filter lets reads skip SSTs
    // that cannot contain the requested key.
    let point_lookup = CfTuning {
        bloom_filter_bits: Some(10.0),
        ..Default::default()
    };
    for name in [tx_seq_by_digest::NAME, checkpoint_seq_by_digest::NAME] {
        column_family.insert(name.to_string(), point_lookup.clone());
    }

    // Bitmap CFs accumulate large merge-blob values; a bigger memtable
    // amortizes the merge-and-flush churn.
    let bitmap = CfTuning {
        write_buffer_size_mb: Some(256),
        ..Default::default()
    };
    for name in [transaction_bitmap::NAME, event_bitmap::NAME] {
        column_family.insert(name.to_string(), bitmap.clone());
    }

    RocksDbConfig {
        db: DbWideConfig {
            parallelism: Some(8),
            max_background_jobs: None,
            // RocksDB's default (`-1`) keeps every SST open, which
            // exhausts the process file-descriptor budget on a
            // large DB (a formal-snapshot restore writes thousands
            // of SSTs and fails with "Too many open files"). Mirror
            // `typed_store::default_db_options`: raise the fd limit
            // toward the hard cap and bound the table cache to an
            // eighth of it. `None` on platforms without the syscall
            // (e.g. Windows), leaving the RocksDB default.
            max_open_files: fdlimit::raise_fd_limit()
                .map(|limit| (limit / 8).try_into().unwrap_or(i32::MAX)),
            db_write_buffer_size_mb: Some(1024),
            max_total_wal_size_mb: Some(1024),
            enable_pipelined_write: Some(true),
            table_cache_num_shard_bits: Some(10),
            block_cache_size_mb: Some(1024),
            block_cache_hyper_clock: Some(true),
            block_cache_estimated_entry_charge_kb: Some(16),
        },
        default_cf,
        column_family,
    }
}

#[cfg(test)]
mod tests {
    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SequenceNumber;

    use super::*;

    #[test]
    fn opens_with_all_cfs() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        // Empty database — every typed handle is constructed; a
        // miss on any of them returns None instead of an open-time
        // missing-CF error.
        assert!(
            schema
                .objects
                .get(&objects::Key {
                    id: ObjectID::ZERO,
                    version: SequenceNumber::from_u64(0),
                })
                .unwrap()
                .is_none()
        );
        assert!(
            schema
                .pruning_watermark
                .get(&primitives::UnitKey)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn default_config_validates() {
        default_rocksdb_config()
            .validate()
            .expect("shipped default config must be internally consistent");
    }

    #[test]
    fn default_config_opens_every_cf() {
        // Exercises the full resolve path: validation, the shared
        // block cache, and every CF's merge operator / compaction
        // filter layered on the tuned per-CF options.
        let dir = tempfile::tempdir().unwrap();
        let opts = DbOptions {
            rocksdb: default_rocksdb_config(),
            snapshot_capacity: 32,
        };
        let (_db, schema) = Db::open::<RpcStoreSchema>(dir.path(), opts).unwrap();
        assert!(
            schema
                .pruning_watermark
                .get(&primitives::UnitKey)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn default_config_sets_per_cf_deviations() {
        let cfg = default_rocksdb_config();
        assert_eq!(cfg.db.block_cache_hyper_clock, Some(true));
        assert_eq!(cfg.db.block_cache_estimated_entry_charge_kb, Some(16));
        // Point-lookup CFs get a bloom filter.
        assert_eq!(
            cfg.column_family[tx_seq_by_digest::NAME].bloom_filter_bits,
            Some(10.0)
        );
        assert_eq!(
            cfg.column_family[checkpoint_seq_by_digest::NAME].bloom_filter_bits,
            Some(10.0)
        );
        // Bitmap CFs get a larger write buffer.
        assert_eq!(
            cfg.column_family[transaction_bitmap::NAME].write_buffer_size_mb,
            Some(256)
        );
    }
}
