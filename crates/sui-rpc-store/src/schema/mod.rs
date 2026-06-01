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
pub mod dynamic_fields;
pub mod effects;
pub mod epoch_info;
pub mod event_bitmap;
pub mod events;
pub mod keys;
pub mod live_objects;
pub mod objects;
pub mod owner_index;
pub mod package_versions;
pub mod pruning_watermark;
pub mod transaction_bitmap;
pub mod transactions;
pub mod tx_meta_by_seq;
pub mod tx_seq_by_digest;
pub mod type_index;

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
    // --- Raw chain data ---
    pub objects: DbMap<objects::Key, objects::Value, R>,
    pub live_objects: DbMap<live_objects::Key, live_objects::Value, R>,
    pub transactions: DbMap<transactions::Key, transactions::Value, R>,
    pub effects: DbMap<effects::Key, effects::Value, R>,
    pub events: DbMap<events::Key, events::Value, R>,
    pub checkpoint_summary: DbMap<checkpoint_summary::Key, checkpoint_summary::Value, R>,
    pub checkpoint_contents: DbMap<checkpoint_contents::Key, checkpoint_contents::Value, R>,
    pub checkpoint_seq_by_digest:
        DbMap<checkpoint_seq_by_digest::Key, checkpoint_seq_by_digest::Value, R>,
    pub committees: DbMap<committees::Key, committees::Value, R>,
    pub tx_seq_by_digest: DbMap<tx_seq_by_digest::Key, tx_seq_by_digest::Value, R>,
    pub tx_meta_by_seq: DbMap<tx_meta_by_seq::Key, tx_meta_by_seq::Value, R>,

    // --- Derived indexes ---
    pub owner_index: DbMap<owner_index::Key, owner_index::Value, R>,
    pub type_index: DbMap<type_index::Key, type_index::Value, R>,
    pub dynamic_fields: DbMap<dynamic_fields::Key, dynamic_fields::Value, R>,
    pub balance: DbMap<balance::Key, balance::Value, R>,
    pub package_versions: DbMap<package_versions::Key, package_versions::Value, R>,
    pub epoch_info: DbMap<epoch_info::Key, epoch_info::Value, R>,
    pub transaction_bitmap: DbMap<transaction_bitmap::Key, transaction_bitmap::Value, R>,
    pub event_bitmap: DbMap<event_bitmap::Key, event_bitmap::Value, R>,

    // --- Bookkeeping ---
    pub pruning_watermark: DbMap<pruning_watermark::Key, pruning_watermark::Value, R>,
}

impl Schema for RpcStoreSchema {
    fn cfs(base_options: &rocksdb::Options) -> Vec<CfDescriptor> {
        vec![
            CfDescriptor::new(objects::NAME, objects::options(base_options)),
            CfDescriptor::new(live_objects::NAME, live_objects::options(base_options)),
            CfDescriptor::new(transactions::NAME, transactions::options(base_options)),
            CfDescriptor::new(effects::NAME, effects::options(base_options)),
            CfDescriptor::new(events::NAME, events::options(base_options)),
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
            CfDescriptor::new(committees::NAME, committees::options(base_options)),
            CfDescriptor::new(
                tx_seq_by_digest::NAME,
                tx_seq_by_digest::options(base_options),
            ),
            CfDescriptor::new(tx_meta_by_seq::NAME, tx_meta_by_seq::options(base_options)),
            CfDescriptor::new(owner_index::NAME, owner_index::options(base_options)),
            CfDescriptor::new(type_index::NAME, type_index::options(base_options)),
            CfDescriptor::new(dynamic_fields::NAME, dynamic_fields::options(base_options)),
            CfDescriptor::new(balance::NAME, balance::options(base_options)),
            CfDescriptor::new(
                package_versions::NAME,
                package_versions::options(base_options),
            ),
            CfDescriptor::new(epoch_info::NAME, epoch_info::options(base_options)),
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
            objects: DbMap::new(db.clone(), objects::NAME)?,
            live_objects: DbMap::new(db.clone(), live_objects::NAME)?,
            transactions: DbMap::new(db.clone(), transactions::NAME)?,
            effects: DbMap::new(db.clone(), effects::NAME)?,
            events: DbMap::new(db.clone(), events::NAME)?,
            checkpoint_summary: DbMap::new(db.clone(), checkpoint_summary::NAME)?,
            checkpoint_contents: DbMap::new(db.clone(), checkpoint_contents::NAME)?,
            checkpoint_seq_by_digest: DbMap::new(db.clone(), checkpoint_seq_by_digest::NAME)?,
            committees: DbMap::new(db.clone(), committees::NAME)?,
            tx_seq_by_digest: DbMap::new(db.clone(), tx_seq_by_digest::NAME)?,
            tx_meta_by_seq: DbMap::new(db.clone(), tx_meta_by_seq::NAME)?,
            owner_index: DbMap::new(db.clone(), owner_index::NAME)?,
            type_index: DbMap::new(db.clone(), type_index::NAME)?,
            dynamic_fields: DbMap::new(db.clone(), dynamic_fields::NAME)?,
            balance: DbMap::new(db.clone(), balance::NAME)?,
            package_versions: DbMap::new(db.clone(), package_versions::NAME)?,
            epoch_info: DbMap::new(db.clone(), epoch_info::NAME)?,
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
            objects: self.objects.at(snap),
            live_objects: self.live_objects.at(snap),
            transactions: self.transactions.at(snap),
            effects: self.effects.at(snap),
            events: self.events.at(snap),
            checkpoint_summary: self.checkpoint_summary.at(snap),
            checkpoint_contents: self.checkpoint_contents.at(snap),
            checkpoint_seq_by_digest: self.checkpoint_seq_by_digest.at(snap),
            committees: self.committees.at(snap),
            tx_seq_by_digest: self.tx_seq_by_digest.at(snap),
            tx_meta_by_seq: self.tx_meta_by_seq.at(snap),
            owner_index: self.owner_index.at(snap),
            type_index: self.type_index.at(snap),
            dynamic_fields: self.dynamic_fields.at(snap),
            balance: self.balance.at(snap),
            package_versions: self.package_versions.at(snap),
            epoch_info: self.epoch_info.at(snap),
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
