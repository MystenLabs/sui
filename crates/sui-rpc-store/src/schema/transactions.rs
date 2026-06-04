// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `tx_seq` → `StoredTransaction`.

use sui_consistent_store::Protobuf;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;
use sui_types::signature::GenericSignature;
use sui_types::transaction::TransactionData;

use crate::proto::StoredTransaction;
use crate::schema::keys::U64Be;

pub const NAME: &str = "transactions";

pub type Key = U64Be;
pub type Value = Protobuf<StoredTransaction>;

pub fn options(resolver: &sui_consistent_store::CfOptionsResolver) -> rocksdb::Options {
    resolver.options(NAME)
}

/// Build a `StoredTransaction` row from a transaction's data and
/// signatures.
///
/// BCS-encode failures here would indicate either OOM or a bug in
/// the types' `Serialize` impls; we panic rather than thread a
/// `Result` through every call site.
pub fn store(transaction: &TransactionData, signatures: &[GenericSignature]) -> Value {
    let transaction_bcs = bcs::to_bytes(transaction).expect("bcs encode TransactionData");
    let signatures_bcs = bcs::to_bytes(signatures).expect("bcs encode Vec<GenericSignature>");
    Protobuf(StoredTransaction {
        transaction_bcs: transaction_bcs.into(),
        signatures_bcs: signatures_bcs.into(),
    })
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Look up the transaction data and signatures at the given
    /// assigned `tx_seq`.
    ///
    /// Callers resolving from a `TransactionDigest` should first
    /// translate through `tx_seq_by_digest`.
    pub fn get_transaction(
        &self,
        tx_seq: u64,
    ) -> Result<Option<(TransactionData, Vec<GenericSignature>)>, Error> {
        let Some(stored) = self.transactions.get(&U64Be(tx_seq))? else {
            return Ok(None);
        };
        let stored = stored.into_inner();
        let transaction: TransactionData = bcs::from_bytes(&stored.transaction_bcs)
            .map_err(|e| DecodeError::with_source("bcs decode TransactionData", e))?;
        let signatures: Vec<GenericSignature> = bcs::from_bytes(&stored.signatures_bcs)
            .map_err(|e| DecodeError::with_source("bcs decode Vec<GenericSignature>", e))?;
        Ok(Some((transaction, signatures)))
    }
}

#[cfg(test)]
mod tests {
    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_types::base_types::FullObjectRef;
    use sui_types::base_types::SuiAddress;
    use sui_types::base_types::random_object_ref;

    use super::*;
    use crate::RpcStoreSchema;

    fn fresh_db() -> (tempfile::TempDir, sui_consistent_store::Db, RpcStoreSchema) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    fn dummy_data() -> TransactionData {
        TransactionData::new_transfer(
            SuiAddress::ZERO,
            FullObjectRef::from_fastpath_ref(random_object_ref()),
            SuiAddress::ZERO,
            random_object_ref(),
            1_000_000,
            1_000,
        )
    }

    #[test]
    fn get_returns_none_for_unknown_seq() {
        let (_dir, _db, schema) = fresh_db();
        assert!(schema.get_transaction(7).unwrap().is_none());
    }

    #[test]
    fn store_then_get_round_trips() {
        let (_dir, db, schema) = fresh_db();
        let data = dummy_data();
        let sigs: Vec<GenericSignature> = vec![];
        let expected_data_bcs = bcs::to_bytes(&data).unwrap();
        let expected_sigs_bcs = bcs::to_bytes(&sigs).unwrap();

        let mut batch = db.batch();
        batch
            .put(&schema.transactions, &U64Be(42), &store(&data, &sigs))
            .unwrap();
        batch.commit().unwrap();

        let (read_data, read_sigs) = schema
            .get_transaction(42)
            .unwrap()
            .expect("transaction present");
        assert_eq!(bcs::to_bytes(&read_data).unwrap(), expected_data_bcs);
        assert_eq!(bcs::to_bytes(&read_sigs).unwrap(), expected_sigs_bcs);
    }

    #[test]
    fn overwrite_replaces_previous() {
        let (_dir, db, schema) = fresh_db();
        let first = dummy_data();
        let later = dummy_data();
        let later_bcs = bcs::to_bytes(&later).unwrap();

        let mut batch = db.batch();
        batch
            .put(&schema.transactions, &U64Be(42), &store(&first, &[]))
            .unwrap();
        batch
            .put(&schema.transactions, &U64Be(42), &store(&later, &[]))
            .unwrap();
        batch.commit().unwrap();

        let (read_data, _) = schema
            .get_transaction(42)
            .unwrap()
            .expect("transaction present");
        assert_eq!(bcs::to_bytes(&read_data).unwrap(), later_bcs);
    }
}
