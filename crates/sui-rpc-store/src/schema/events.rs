// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `tx_seq` → `StoredEvents`.

use sui_consistent_store::Protobuf;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;
use sui_types::effects::TransactionEvents;

use crate::proto::StoredEvents;
use crate::schema::keys::U64Be;

pub const NAME: &str = "events";

pub type Key = U64Be;
pub type Value = Protobuf<StoredEvents>;

pub fn options(resolver: &sui_consistent_store::CfOptionsResolver) -> rocksdb::Options {
    resolver.options(NAME)
}

/// Build a `StoredEvents` row from a transaction's events.
///
/// BCS-encode failures here would indicate either OOM or a bug in
/// the type's `Serialize` impl; we panic rather than thread a
/// `Result` through every call site.
pub fn store(events: &TransactionEvents) -> Value {
    let bcs = bcs::to_bytes(events).expect("bcs encode TransactionEvents");
    Protobuf(StoredEvents { bcs: bcs.into() })
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Look up the events emitted by the transaction at the given
    /// assigned `tx_seq`.
    pub fn get_events(&self, tx_seq: u64) -> Result<Option<TransactionEvents>, Error> {
        let Some(stored) = self.events.get(&U64Be(tx_seq))? else {
            return Ok(None);
        };
        let events: TransactionEvents = bcs::from_bytes(&stored.into_inner().bcs)
            .map_err(|e| DecodeError::with_source("bcs decode TransactionEvents", e))?;
        Ok(Some(events))
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
    fn get_returns_none_for_unknown_seq() {
        let (_dir, _db, schema) = fresh_db();
        assert!(schema.get_events(7).unwrap().is_none());
    }

    #[test]
    fn empty_events_round_trip() {
        let (_dir, db, schema) = fresh_db();
        let events = TransactionEvents::default();

        let mut batch = db.batch();
        batch
            .put(&schema.events, &U64Be(42), &store(&events))
            .unwrap();
        batch.commit().unwrap();

        let read = schema.get_events(42).unwrap().expect("events present");
        assert_eq!(read, events);
    }
}
