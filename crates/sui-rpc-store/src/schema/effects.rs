// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `tx_seq` → `StoredEffects`.

use sui_consistent_store::Protobuf;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;
use sui_types::effects::TransactionEffects;
use sui_types::storage::ObjectKey;

use crate::proto::StoredEffects;
use crate::schema::keys::U64Be;

pub const NAME: &str = "effects";

pub type Key = U64Be;
pub type Value = Protobuf<StoredEffects>;

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}

/// Build a `StoredEffects` row from a transaction's effects and
/// the set of objects loaded but not modified during execution.
///
/// BCS-encode failures here would indicate either OOM or a bug in
/// the types' `Serialize` impls; we panic rather than thread a
/// `Result` through every call site.
pub fn store(effects: &TransactionEffects, unchanged_loaded: &[ObjectKey]) -> Value {
    let bcs = bcs::to_bytes(effects).expect("bcs encode TransactionEffects");
    let unchanged_loaded_bcs =
        bcs::to_bytes(unchanged_loaded).expect("bcs encode Vec<ObjectKey>");
    Protobuf(StoredEffects {
        bcs: bcs.into(),
        unchanged_loaded_bcs: unchanged_loaded_bcs.into(),
    })
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Look up the effects produced by the transaction at the
    /// given assigned `tx_seq`, along with the set of objects
    /// loaded during execution but not modified by the tx.
    pub fn get_effects(
        &self,
        tx_seq: u64,
    ) -> Result<Option<(TransactionEffects, Vec<ObjectKey>)>, Error> {
        let Some(stored) = self.effects.get(&U64Be(tx_seq))? else {
            return Ok(None);
        };
        let stored = stored.into_inner();
        let effects: TransactionEffects = bcs::from_bytes(&stored.bcs)
            .map_err(|e| DecodeError::with_source("bcs decode TransactionEffects", e))?;
        let unchanged_loaded: Vec<ObjectKey> = bcs::from_bytes(&stored.unchanged_loaded_bcs)
            .map_err(|e| DecodeError::with_source("bcs decode Vec<ObjectKey>", e))?;
        Ok(Some((effects, unchanged_loaded)))
    }
}

#[cfg(test)]
mod tests {
    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SequenceNumber;

    use super::*;
    use crate::RpcStoreSchema;

    fn fresh_db() -> (tempfile::TempDir, sui_consistent_store::Db, RpcStoreSchema) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    fn dummy_unchanged_loaded() -> Vec<ObjectKey> {
        vec![
            ObjectKey(ObjectID::random(), SequenceNumber::from_u64(7)),
            ObjectKey(ObjectID::random(), SequenceNumber::from_u64(13)),
        ]
    }

    #[test]
    fn get_returns_none_for_unknown_seq() {
        let (_dir, _db, schema) = fresh_db();
        assert!(schema.get_effects(7).unwrap().is_none());
    }

    #[test]
    fn store_then_get_round_trips() {
        let (_dir, db, schema) = fresh_db();
        let effects = TransactionEffects::default();
        let loaded = dummy_unchanged_loaded();

        let mut batch = db.batch();
        batch
            .put(&schema.effects, &U64Be(42), &store(&effects, &loaded))
            .unwrap();
        batch.commit().unwrap();

        let (read_effects, read_loaded) =
            schema.get_effects(42).unwrap().expect("effects present");
        assert_eq!(read_effects, effects);
        assert_eq!(read_loaded, loaded);
    }

    #[test]
    fn empty_unchanged_loaded_round_trips() {
        let (_dir, db, schema) = fresh_db();
        let effects = TransactionEffects::default();

        let mut batch = db.batch();
        batch
            .put(&schema.effects, &U64Be(42), &store(&effects, &[]))
            .unwrap();
        batch.commit().unwrap();

        let (_, read_loaded) = schema.get_effects(42).unwrap().expect("effects present");
        assert!(read_loaded.is_empty());
    }

    #[test]
    fn overwrite_replaces_previous() {
        let (_dir, db, schema) = fresh_db();
        let effects = TransactionEffects::default();
        let first = dummy_unchanged_loaded();
        let later = dummy_unchanged_loaded();

        let mut batch = db.batch();
        batch
            .put(&schema.effects, &U64Be(42), &store(&effects, &first))
            .unwrap();
        batch
            .put(&schema.effects, &U64Be(42), &store(&effects, &later))
            .unwrap();
        batch.commit().unwrap();

        let (_, read_loaded) = schema.get_effects(42).unwrap().expect("effects present");
        assert_eq!(read_loaded, later);
    }
}
