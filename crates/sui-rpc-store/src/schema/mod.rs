// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Column-family layout for `sui-rpc-store`.
//!
//! [`RpcStoreSchema`] declares every CF the crate registers on a
//! [`sui_consistent_store::Db`], grouped by concern in source order:
//!
//! - Raw chain data — objects, transactions, effects, events,
//!   checkpoints, committees, and the `tx_seq` <-> digest bijection.
//! - Derived indexes — owner, type, dynamic-field, coin, balance,
//!   package version, epoch info, ledger-history bitmaps.
//! - A singleton `pruning_watermark` row that drives compaction
//!   filters and feeds `available_range` queries.
//!
//! The crate's `Db` is opened with this schema via
//! `Db::open::<RpcStoreSchema>(path, opts)`.

use sui_consistent_store::CfDescriptor;
use sui_consistent_store::Db;
use sui_consistent_store::DbMap;
use sui_consistent_store::Protobuf;
use sui_consistent_store::Schema;
use sui_consistent_store::SchemaAtSnapshot;
use sui_consistent_store::Snapshot;
use sui_consistent_store::error::OpenError;
use sui_consistent_store::reader::Reader;

use crate::keys::BalanceKey;
use crate::keys::BitmapIndexKey;
use crate::keys::CkptDigestKey;
use crate::keys::CoinTypeKey;
use crate::keys::DynamicFieldKey;
use crate::keys::ObjectIdKey;
use crate::keys::ObjectVersionKey;
use crate::keys::OwnerIndexKey;
use crate::keys::PackageVersionKey;
use crate::keys::TxDigestKey;
use crate::keys::TypeIndexKey;
use crate::keys::U64Be;
use crate::keys::UnitKey;
use crate::proto::BalanceDelta;
use crate::proto::BitmapBlob;
use crate::proto::CoinInfo;
use crate::proto::DynamicFieldInfo;
use crate::proto::EpochInfo;
use crate::proto::LiveObjectRef;
use crate::proto::PackageVersionInfo;
use crate::proto::PruningWatermarks;
use crate::proto::StoredCheckpointContents;
use crate::proto::StoredCheckpointSummary;
use crate::proto::StoredCommittee;
use crate::proto::StoredEffects;
use crate::proto::StoredEvents;
use crate::proto::StoredObject;
use crate::proto::StoredTransaction;
use crate::proto::TxMeta;
use crate::proto::VersionDigest;

// === Raw chain CFs ==============================================

pub const CF_OBJECTS: &str = "objects";
pub const CF_LIVE_OBJECTS: &str = "live_objects";
pub const CF_TRANSACTIONS: &str = "transactions";
pub const CF_EFFECTS: &str = "effects";
pub const CF_EVENTS: &str = "events";
pub const CF_CHECKPOINT_SUMMARY: &str = "checkpoint_summary";
pub const CF_CHECKPOINT_CONTENTS: &str = "checkpoint_contents";
pub const CF_CHECKPOINT_SEQ_BY_DIGEST: &str = "checkpoint_seq_by_digest";
pub const CF_COMMITTEES: &str = "committees";
pub const CF_TX_SEQ_BY_DIGEST: &str = "tx_seq_by_digest";
pub const CF_TX_META_BY_SEQ: &str = "tx_meta_by_seq";

// === Derived-index CFs ==========================================

pub const CF_OWNER_INDEX: &str = "owner_index";
pub const CF_TYPE_INDEX: &str = "type_index";
pub const CF_DYNAMIC_FIELDS: &str = "dynamic_fields";
pub const CF_COIN_INDEX: &str = "coin_index";
pub const CF_BALANCE: &str = "balance";
pub const CF_ADDRESS_BALANCE: &str = "address_balance";
pub const CF_PACKAGE_VERSIONS: &str = "package_versions";
pub const CF_EPOCH_INFO: &str = "epoch_info";
pub const CF_TRANSACTION_BITMAP: &str = "transaction_bitmap";
pub const CF_EVENT_BITMAP: &str = "event_bitmap";

// === Bookkeeping CFs ============================================

pub const CF_PRUNING_WATERMARK: &str = "pruning_watermark";

/// Typed handles to every CF in the `sui-rpc-store` layout.
pub struct RpcStoreSchema<R: Reader = Db> {
    // --- Raw chain data ---
    /// `(ObjectID, version)` → `StoredObject`.
    pub objects: DbMap<ObjectVersionKey, Protobuf<StoredObject>, R>,
    /// `ObjectID` → `LiveObjectRef` (latest live version + digest).
    pub live_objects: DbMap<ObjectIdKey, Protobuf<LiveObjectRef>, R>,
    /// `tx_seq` → `StoredTransaction`.
    pub transactions: DbMap<U64Be, Protobuf<StoredTransaction>, R>,
    /// `tx_seq` → `StoredEffects`.
    pub effects: DbMap<U64Be, Protobuf<StoredEffects>, R>,
    /// `tx_seq` → `StoredEvents`.
    pub events: DbMap<U64Be, Protobuf<StoredEvents>, R>,
    /// `checkpoint_seq` → `StoredCheckpointSummary`.
    pub checkpoint_summary: DbMap<U64Be, Protobuf<StoredCheckpointSummary>, R>,
    /// `checkpoint_seq` → `StoredCheckpointContents`.
    pub checkpoint_contents: DbMap<U64Be, Protobuf<StoredCheckpointContents>, R>,
    /// `CheckpointDigest` → `checkpoint_seq`.
    pub checkpoint_seq_by_digest: DbMap<CkptDigestKey, U64Be, R>,
    /// `EpochId` → `StoredCommittee`.
    pub committees: DbMap<U64Be, Protobuf<StoredCommittee>, R>,
    /// `TransactionDigest` → `tx_seq`.
    pub tx_seq_by_digest: DbMap<TxDigestKey, U64Be, R>,
    /// `tx_seq` → `TxMeta`.
    pub tx_meta_by_seq: DbMap<U64Be, Protobuf<TxMeta>, R>,

    // --- Derived indexes ---
    /// `OwnerIndexKey` → `VersionDigest`. Owner-and-type filtering
    /// with optional balance ordering.
    pub owner_index: DbMap<OwnerIndexKey, Protobuf<VersionDigest>, R>,
    /// `TypeIndexKey` → `VersionDigest`. Type-only filtering (no
    /// owner constraint).
    pub type_index: DbMap<TypeIndexKey, Protobuf<VersionDigest>, R>,
    /// `(parent, field_id)` → `DynamicFieldInfo`.
    pub dynamic_fields: DbMap<DynamicFieldKey, Protobuf<DynamicFieldInfo>, R>,
    /// `coin_type StructTag` → `CoinInfo`.
    pub coin_index: DbMap<CoinTypeKey, Protobuf<CoinInfo>, R>,
    /// `(owner, coin_type)` → coin-derived balance delta. Uses an
    /// associative merge operator; compaction drops zeros.
    pub balance: DbMap<BalanceKey, Protobuf<BalanceDelta>, R>,
    /// `(owner, type)` → accumulator-derived balance. Same merge /
    /// compaction setup as `balance`.
    pub address_balance: DbMap<BalanceKey, Protobuf<BalanceDelta>, R>,
    /// `(original_id, version)` → `PackageVersionInfo`.
    pub package_versions: DbMap<PackageVersionKey, Protobuf<PackageVersionInfo>, R>,
    /// `EpochId` → `EpochInfo`.
    pub epoch_info: DbMap<U64Be, Protobuf<EpochInfo>, R>,
    /// Inverted bitmap index over tx_seq space.
    pub transaction_bitmap: DbMap<BitmapIndexKey, Protobuf<BitmapBlob>, R>,
    /// Inverted bitmap index over packed-event-seq space.
    pub event_bitmap: DbMap<BitmapIndexKey, Protobuf<BitmapBlob>, R>,

    // --- Bookkeeping ---
    /// Singleton holding the lowest still-available `tx_seq`,
    /// `checkpoint_seq`, and `object_version`. Drives compaction
    /// filters and serves `available_range` requests.
    pub pruning_watermark: DbMap<UnitKey, Protobuf<PruningWatermarks>, R>,
}

impl Schema for RpcStoreSchema {
    fn cfs(base_options: &rocksdb::Options) -> Vec<CfDescriptor> {
        // Per-CF rocksdb options (merge operators, compaction
        // filters) will land alongside the indexer that populates
        // each CF. For now they share the base options; the
        // schema's job here is only to register the CFs.
        // TODO: install balance merge operator + zero-compaction.
        // TODO: install bitmap union merge operator + bucket
        // compaction filter for the two bitmap CFs.
        vec![
            CfDescriptor::new(CF_OBJECTS, base_options.clone()),
            CfDescriptor::new(CF_LIVE_OBJECTS, base_options.clone()),
            CfDescriptor::new(CF_TRANSACTIONS, base_options.clone()),
            CfDescriptor::new(CF_EFFECTS, base_options.clone()),
            CfDescriptor::new(CF_EVENTS, base_options.clone()),
            CfDescriptor::new(CF_CHECKPOINT_SUMMARY, base_options.clone()),
            CfDescriptor::new(CF_CHECKPOINT_CONTENTS, base_options.clone()),
            CfDescriptor::new(CF_CHECKPOINT_SEQ_BY_DIGEST, base_options.clone()),
            CfDescriptor::new(CF_COMMITTEES, base_options.clone()),
            CfDescriptor::new(CF_TX_SEQ_BY_DIGEST, base_options.clone()),
            CfDescriptor::new(CF_TX_META_BY_SEQ, base_options.clone()),
            CfDescriptor::new(CF_OWNER_INDEX, base_options.clone()),
            CfDescriptor::new(CF_TYPE_INDEX, base_options.clone()),
            CfDescriptor::new(CF_DYNAMIC_FIELDS, base_options.clone()),
            CfDescriptor::new(CF_COIN_INDEX, base_options.clone()),
            CfDescriptor::new(CF_BALANCE, base_options.clone()),
            CfDescriptor::new(CF_ADDRESS_BALANCE, base_options.clone()),
            CfDescriptor::new(CF_PACKAGE_VERSIONS, base_options.clone()),
            CfDescriptor::new(CF_EPOCH_INFO, base_options.clone()),
            CfDescriptor::new(CF_TRANSACTION_BITMAP, base_options.clone()),
            CfDescriptor::new(CF_EVENT_BITMAP, base_options.clone()),
            CfDescriptor::new(CF_PRUNING_WATERMARK, base_options.clone()),
        ]
    }

    fn open(db: &Db) -> Result<Self, OpenError> {
        Ok(Self {
            objects: DbMap::new(db.clone(), CF_OBJECTS)?,
            live_objects: DbMap::new(db.clone(), CF_LIVE_OBJECTS)?,
            transactions: DbMap::new(db.clone(), CF_TRANSACTIONS)?,
            effects: DbMap::new(db.clone(), CF_EFFECTS)?,
            events: DbMap::new(db.clone(), CF_EVENTS)?,
            checkpoint_summary: DbMap::new(db.clone(), CF_CHECKPOINT_SUMMARY)?,
            checkpoint_contents: DbMap::new(db.clone(), CF_CHECKPOINT_CONTENTS)?,
            checkpoint_seq_by_digest: DbMap::new(db.clone(), CF_CHECKPOINT_SEQ_BY_DIGEST)?,
            committees: DbMap::new(db.clone(), CF_COMMITTEES)?,
            tx_seq_by_digest: DbMap::new(db.clone(), CF_TX_SEQ_BY_DIGEST)?,
            tx_meta_by_seq: DbMap::new(db.clone(), CF_TX_META_BY_SEQ)?,
            owner_index: DbMap::new(db.clone(), CF_OWNER_INDEX)?,
            type_index: DbMap::new(db.clone(), CF_TYPE_INDEX)?,
            dynamic_fields: DbMap::new(db.clone(), CF_DYNAMIC_FIELDS)?,
            coin_index: DbMap::new(db.clone(), CF_COIN_INDEX)?,
            balance: DbMap::new(db.clone(), CF_BALANCE)?,
            address_balance: DbMap::new(db.clone(), CF_ADDRESS_BALANCE)?,
            package_versions: DbMap::new(db.clone(), CF_PACKAGE_VERSIONS)?,
            epoch_info: DbMap::new(db.clone(), CF_EPOCH_INFO)?,
            transaction_bitmap: DbMap::new(db.clone(), CF_TRANSACTION_BITMAP)?,
            event_bitmap: DbMap::new(db.clone(), CF_EVENT_BITMAP)?,
            pruning_watermark: DbMap::new(db.clone(), CF_PRUNING_WATERMARK)?,
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
            coin_index: self.coin_index.at(snap),
            balance: self.balance.at(snap),
            address_balance: self.address_balance.at(snap),
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

    use super::*;

    #[test]
    fn opens_with_all_cfs() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, schema) =
            Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        // Empty database — every typed handle is constructed; a
        // miss on any of them returns None instead of an open-time
        // missing-CF error.
        assert!(
            schema
                .objects
                .get(&crate::keys::ObjectVersionKey {
                    id: sui_types::base_types::ObjectID::ZERO,
                    version: sui_types::base_types::SequenceNumber::from_u64(0),
                })
                .unwrap()
                .is_none()
        );
        assert!(schema.pruning_watermark.get(&UnitKey).unwrap().is_none());
    }
}
