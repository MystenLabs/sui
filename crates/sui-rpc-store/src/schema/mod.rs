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
//! - `options(base)` — per-CF `rocksdb::Options` (merge operators,
//!   compaction filters) layered on the shared base.
//!
//! [`RpcStoreSchema`] aggregates these into the schema passed to
//! [`sui_consistent_store::Db::open`]. Keys reused across multiple
//! CFs live in [`keys`].

pub mod balance;
pub mod checkpoint_contents;
pub mod checkpoint_seq_by_digest;
pub mod checkpoint_summary;
pub mod committees;
pub mod effects;
pub mod epochs;
pub mod event_bitmap;
pub mod events;
pub mod keys;
pub mod live_objects;
pub mod object_by_owner;
pub mod object_by_type;
pub mod objects;
pub mod package_versions;
pub mod pruning_watermark;
pub mod transaction_bitmap;
pub mod transactions;
pub mod tx_metadata_by_seq;
pub mod tx_seq_by_digest;

use sui_consistent_store::CfDescriptor;
use sui_consistent_store::Db;
use sui_consistent_store::DbMap;
use sui_consistent_store::Schema;
use sui_consistent_store::SchemaAtSnapshot;
use sui_consistent_store::Snapshot;
use sui_consistent_store::error::OpenError;
use sui_consistent_store::reader::Reader;

/// Typed handles to every CF in the `sui-rpc-store` layout.
pub struct RpcStoreSchema<R: Reader = Db> {
    /// Per-epoch metadata: protocol version, gas price, start and
    /// end timestamps, and the epoch's final checkpoint.
    pub epochs: DbMap<epochs::Key, epochs::Value, R>,

    /// The validator committee active during each epoch.
    pub committees: DbMap<committees::Key, committees::Value, R>,

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
    /// prefix scan on the object id walks all versions in
    /// ascending order.
    pub objects: DbMap<objects::Key, objects::Value, R>,

    /// The latest live version of each object — point lookups
    /// avoid an iteration over the multi-version `objects` CF.
    pub live_objects: DbMap<live_objects::Key, live_objects::Value, R>,

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
    fn cfs(base_options: &rocksdb::Options) -> Vec<CfDescriptor> {
        vec![
            CfDescriptor::new(epochs::NAME, epochs::options(base_options)),
            CfDescriptor::new(committees::NAME, committees::options(base_options)),
            CfDescriptor::new(
                checkpoint_summary::NAME,
                checkpoint_summary::options(base_options),
            ),
            CfDescriptor::new(
                checkpoint_contents::NAME,
                checkpoint_contents::options(base_options),
            ),
            CfDescriptor::new(
                checkpoint_seq_by_digest::NAME,
                checkpoint_seq_by_digest::options(base_options),
            ),
            CfDescriptor::new(transactions::NAME, transactions::options(base_options)),
            CfDescriptor::new(
                tx_seq_by_digest::NAME,
                tx_seq_by_digest::options(base_options),
            ),
            CfDescriptor::new(
                tx_metadata_by_seq::NAME,
                tx_metadata_by_seq::options(base_options),
            ),
            CfDescriptor::new(effects::NAME, effects::options(base_options)),
            CfDescriptor::new(events::NAME, events::options(base_options)),
            CfDescriptor::new(objects::NAME, objects::options(base_options)),
            CfDescriptor::new(live_objects::NAME, live_objects::options(base_options)),
            CfDescriptor::new(
                object_by_owner::NAME,
                object_by_owner::options(base_options),
            ),
            CfDescriptor::new(object_by_type::NAME, object_by_type::options(base_options)),
            CfDescriptor::new(balance::NAME, balance::options(base_options)),
            CfDescriptor::new(
                package_versions::NAME,
                package_versions::options(base_options),
            ),
            CfDescriptor::new(
                transaction_bitmap::NAME,
                transaction_bitmap::options(base_options),
            ),
            CfDescriptor::new(event_bitmap::NAME, event_bitmap::options(base_options)),
            CfDescriptor::new(
                pruning_watermark::NAME,
                pruning_watermark::options(base_options),
            ),
        ]
    }

    fn open(db: &Db) -> Result<Self, OpenError> {
        Ok(Self {
            epochs: DbMap::new(db.clone(), epochs::NAME)?,
            committees: DbMap::new(db.clone(), committees::NAME)?,
            checkpoint_summary: DbMap::new(db.clone(), checkpoint_summary::NAME)?,
            checkpoint_contents: DbMap::new(db.clone(), checkpoint_contents::NAME)?,
            checkpoint_seq_by_digest: DbMap::new(db.clone(), checkpoint_seq_by_digest::NAME)?,
            transactions: DbMap::new(db.clone(), transactions::NAME)?,
            tx_seq_by_digest: DbMap::new(db.clone(), tx_seq_by_digest::NAME)?,
            tx_metadata_by_seq: DbMap::new(db.clone(), tx_metadata_by_seq::NAME)?,
            effects: DbMap::new(db.clone(), effects::NAME)?,
            events: DbMap::new(db.clone(), events::NAME)?,
            objects: DbMap::new(db.clone(), objects::NAME)?,
            live_objects: DbMap::new(db.clone(), live_objects::NAME)?,
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
            committees: self.committees.at(snap),
            checkpoint_summary: self.checkpoint_summary.at(snap),
            checkpoint_contents: self.checkpoint_contents.at(snap),
            checkpoint_seq_by_digest: self.checkpoint_seq_by_digest.at(snap),
            transactions: self.transactions.at(snap),
            tx_seq_by_digest: self.tx_seq_by_digest.at(snap),
            tx_metadata_by_seq: self.tx_metadata_by_seq.at(snap),
            effects: self.effects.at(snap),
            events: self.events.at(snap),
            objects: self.objects.at(snap),
            live_objects: self.live_objects.at(snap),
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
                .get(&keys::UnitKey)
                .unwrap()
                .is_none()
        );
    }
}
