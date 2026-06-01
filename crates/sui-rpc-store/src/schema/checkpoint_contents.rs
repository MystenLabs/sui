// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `checkpoint_seq` → `StoredCheckpointContents`.
//!
//! Holds the ordered list of executed transaction digests for each
//! checkpoint.

use sui_consistent_store::Protobuf;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::proto::StoredCheckpointContents;
use crate::schema::keys::U64Be;

pub const NAME: &str = "checkpoint_contents";

pub type Key = U64Be;
pub type Value = Protobuf<StoredCheckpointContents>;

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}

/// Build a `StoredCheckpointContents` row from canonical
/// `CheckpointContents`.
///
/// BCS-encode failures here would indicate either OOM or a bug in
/// the type's `Serialize` impl; we panic rather than thread a
/// `Result` through every call site.
pub fn store(contents: &CheckpointContents) -> Value {
    let bcs = bcs::to_bytes(contents).expect("bcs encode CheckpointContents");
    Protobuf(StoredCheckpointContents { bcs: bcs.into() })
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Look up the contents of a checkpoint by sequence number.
    pub fn get_checkpoint_contents(
        &self,
        seq: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointContents>, Error> {
        let Some(stored) = self.checkpoint_contents.get(&U64Be(seq))? else {
            return Ok(None);
        };
        let contents: CheckpointContents = bcs::from_bytes(&stored.into_inner().bcs)
            .map_err(|e| DecodeError::with_source("bcs decode CheckpointContents", e))?;
        Ok(Some(contents))
    }
}

#[cfg(test)]
mod tests {
    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_types::base_types::ExecutionDigests;

    use super::*;
    use crate::RpcStoreSchema;

    fn fresh_db() -> (tempfile::TempDir, sui_consistent_store::Db, RpcStoreSchema) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    fn dummy_contents(n: usize) -> CheckpointContents {
        let digests: Vec<_> = (0..n).map(|_| ExecutionDigests::random()).collect();
        CheckpointContents::new_with_digests_only_for_tests(digests)
    }

    #[test]
    fn get_returns_none_for_unknown_seq() {
        let (_dir, _db, schema) = fresh_db();
        assert!(schema.get_checkpoint_contents(7).unwrap().is_none());
    }

    #[test]
    fn store_then_get_round_trips() {
        let (_dir, db, schema) = fresh_db();
        let contents = dummy_contents(3);

        let mut batch = db.batch();
        batch
            .put(&schema.checkpoint_contents, &U64Be(42), &store(&contents))
            .unwrap();
        batch.commit().unwrap();

        let read = schema
            .get_checkpoint_contents(42)
            .unwrap()
            .expect("contents present");
        assert_eq!(read, contents);
    }

    #[test]
    fn overwrite_replaces_previous() {
        let (_dir, db, schema) = fresh_db();
        let first = dummy_contents(1);
        let later = dummy_contents(2);

        let mut batch = db.batch();
        batch
            .put(&schema.checkpoint_contents, &U64Be(42), &store(&first))
            .unwrap();
        batch
            .put(&schema.checkpoint_contents, &U64Be(42), &store(&later))
            .unwrap();
        batch.commit().unwrap();

        let read = schema
            .get_checkpoint_contents(42)
            .unwrap()
            .expect("contents present");
        assert_eq!(read, later);
    }
}
