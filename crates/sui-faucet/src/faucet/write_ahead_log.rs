// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use serde::{Deserialize, Serialize};
use sui_types::base_types::SuiAddress;
use sui_types::{base_types::ObjectID, messages::TransactionData};
use typed_store::rocks::{DBMap, TypedStoreError};
use typed_store::traits::{TableSummary, TypedStoreDebug};
use typed_store::Map;

use typed_store_derive::DBMapUtils;
use uuid::Uuid;

/// Persistent log of transactions paying out sui from the faucet, keyed by the coin serving the
/// request.  Transactions are expected to be written to the log before they are sent to full-node,
/// and removed after receiving a response back, before the coin becomes available for subsequent
/// writes.
///
/// This allows the faucet to go down and back up, and not forget which requests were in-flight that
/// it needs to confirm succeeded or failed.
#[derive(DBMapUtils)]
pub struct WriteAheadLog {
    log: DBMap<ObjectID, Entry>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub uuid: uuid::Bytes,
    pub recipient: SuiAddress,
    pub tx: TransactionData,
}

impl WriteAheadLog {
    pub(crate) fn open(path: &Path) -> Self {
        Self::open_tables_read_write(
            path.to_path_buf(),
            typed_store::rocks::MetricConf::default(),
            None,
            None,
        )
    }

    /// Mark `coin` as reserved for transaction `tx` sending coin to `recipient`. Fails if `coin` is
    /// already in the WAL pointing to an existing transaction.
    pub(crate) fn reserve(
        &mut self,
        uuid: Uuid,
        coin: ObjectID,
        recipient: SuiAddress,
        tx: TransactionData,
    ) -> Result<(), TypedStoreError> {
        if self.log.contains_key(&coin)? {
            // Don't permit multiple writes against the same coin
            // TODO: Use a better error type than `TypedStoreError`.
            return Err(TypedStoreError::SerializationError(format!(
                "Duplicate WAL entry for coin {coin:?}",
            )));
        }

        let uuid = *uuid.as_bytes();
        self.log.insert(
            &coin,
            &Entry {
                uuid,
                recipient,
                tx,
            },
        )
    }

    /// Check whether `coin` has a pending transaction in the WAL.  Returns `Ok(Some(entry))` if a
    /// pending transaction exists, `Ok(None)` if not, and `Err(_)` if there was an internal error
    /// accessing the WAL.
    pub(crate) fn reclaim(&self, coin: ObjectID) -> Result<Option<Entry>, TypedStoreError> {
        self.log.get(&coin)
    }

    /// Indicate that the transaction in flight for `coin` has landed, and the entry in the WAL can
    /// be removed.
    pub(crate) fn commit(&mut self, coin: ObjectID) -> Result<(), TypedStoreError> {
        self.log.remove(&coin)
    }
}

#[cfg(test)]
mod tests {
    use sui_types::{
        base_types::{random_object_ref, ObjectRef},
        messages::TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
    };

    use super::*;

    #[tokio::test]
    async fn reserve_reclaim_reclaim() {
        let tmp = tempfile::tempdir().unwrap();
        let mut wal = WriteAheadLog::open(&tmp.path().join("wal"));

        let uuid = Uuid::new_v4();
        let coin = random_object_ref();
        let (recv, tx) = random_request(coin);

        assert!(wal.reserve(uuid, coin.0, recv, tx.clone()).is_ok());

        // Reclaim once
        let Some(entry) = wal.reclaim(coin.0).unwrap() else {
            panic!("Entry not found for {}", coin.0);
        };

        assert_eq!(uuid, Uuid::from_bytes(entry.uuid));
        assert_eq!(recv, entry.recipient);
        assert_eq!(tx, entry.tx);

        // Reclaim again, should still be there.
        let Some(entry) = wal.reclaim(coin.0).unwrap() else {
            panic!("Entry not found for {}", coin.0);
        };

        assert_eq!(uuid, Uuid::from_bytes(entry.uuid));
        assert_eq!(recv, entry.recipient);
        assert_eq!(tx, entry.tx);
    }

    #[tokio::test]
    async fn reserve_reserve() {
        let tmp = tempfile::tempdir().unwrap();
        let mut wal = WriteAheadLog::open(&tmp.path().join("wal"));

        let uuid = Uuid::new_v4();
        let coin = random_object_ref();
        let (recv0, tx0) = random_request(coin);
        let (recv1, tx1) = random_request(coin);

        // First write goes through
        wal.reserve(uuid, coin.0, recv0, tx0).unwrap();

        // Second write fails because it tries to write to the same coin
        assert!(matches!(
            wal.reserve(uuid, coin.0, recv1, tx1),
            Err(TypedStoreError::SerializationError(_)),
        ));
    }

    #[tokio::test]
    async fn reserve_reclaim_commit_reclaim() {
        let tmp = tempfile::tempdir().unwrap();
        let mut wal = WriteAheadLog::open(&tmp.path().join("wal"));

        let uuid = Uuid::new_v4();
        let coin = random_object_ref();
        let (recv, tx) = random_request(coin);

        wal.reserve(uuid, coin.0, recv, tx.clone()).unwrap();

        // Reclaim to show that the entry is there
        let Some(entry) = wal.reclaim(coin.0).unwrap() else {
            panic!("Entry not found for {}", coin.0);
        };

        assert_eq!(uuid, Uuid::from_bytes(entry.uuid));
        assert_eq!(recv, entry.recipient);
        assert_eq!(tx, entry.tx);

        // Commit the transaction, which removes it from the log.
        wal.commit(coin.0).unwrap();

        // Expect it to now be gone
        assert_eq!(Ok(None), wal.reclaim(coin.0));
    }

    #[tokio::test]
    async fn reserve_commit_reserve() {
        let tmp = tempfile::tempdir().unwrap();
        let mut wal = WriteAheadLog::open(&tmp.path().join("wal"));

        let uuid = Uuid::new_v4();
        let coin = random_object_ref();
        let (recv0, tx0) = random_request(coin);
        let (recv1, tx1) = random_request(coin);

        // Write the transaction
        wal.reserve(uuid, coin.0, recv0, tx0).unwrap();

        // Commit the transaction, which removes it from the log.
        wal.commit(coin.0).unwrap();

        // Write a fresh transaction, which should now pass
        wal.reserve(uuid, coin.0, recv1, tx1).unwrap();
    }

    fn random_request(coin: ObjectRef) -> (SuiAddress, TransactionData) {
        let gas_price = 1;
        let send = SuiAddress::random_for_testing_only();
        let recv = SuiAddress::random_for_testing_only();
        (
            recv,
            TransactionData::new_pay_sui(
                send,
                vec![coin],
                vec![recv],
                vec![1000],
                coin,
                gas_price * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
                gas_price,
            )
            .unwrap(),
        )
    }
}
