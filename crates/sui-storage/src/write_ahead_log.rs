// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! WriteAheadLog is used by authority.rs / authority_store.rs for safe updates of the datastore.
//! It is currently implemented using a rocksdb table, but the interface is designed to be
//! compatible with a true log.

use async_trait::async_trait;

use crate::mutex_table::{LockGuard, MutexTable};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::Mutex;
use sui_types::base_types::TransactionDigest;
use typed_store::traits::DBMapTableUtil;
use typed_store_macros::DBMapUtils;

use sui_types::error::{SuiError, SuiResult};

use typed_store::{rocks::DBMap, traits::Map};

use tracing::{debug, error, instrument, trace, warn};

/// TxGuard is a handle on an in-progress transaction.
///
/// TxGuard must implement Drop, which should mark the tx as unfinished
/// if the guard is dropped without commit_tx being called.
#[allow(drop_bounds)]
pub trait TxGuard<'a>: Drop {
    /// Return the transaction digest.
    fn tx_id(&self) -> TransactionDigest;

    /// Mark the TX as completed.
    fn commit_tx(self);

    /// Mark the TX as abandoned/aborted but not requiring any recovery or rollback.
    fn release(self);
}

// WriteAheadLog is parameterized on the value type (C) because:
// - it's a pain to make a ConfirmationTransaction in tests.
// - we might end up storing either a ConfirmationTransaction or a (sequence, ConfirmationTransaction)
//   tuple, or something else
#[async_trait]
pub trait WriteAheadLog<'a, C> {
    type Guard: TxGuard<'a>;
    type LockGuard;

    /// Begin a confirmation transaction identified by its digest, with the associated cert.
    ///
    /// The possible return values mean:
    ///
    ///   Ok(None) => There was a concurrent instance of the same tx in progress, but it ended
    ///   without being committed. The caller may not proceed processing that tx. A TxGuard for
    ///   that tx can be (eventually) obtained by calling read_one_recoverable_tx().
    ///
    ///   Ok(Some(TxGuard)) => No other concurrent instance of the same tx is in progress, nor can
    ///   one start while the guard is held. However, a prior instance of the same tx could have
    ///   just finished, so the caller may want to check if the tx is already sequenced before
    ///   proceeding.
    ///
    ///   Err(e) => An error occurred.
    #[must_use]
    async fn begin_tx<'b>(
        &'a self,
        tx: &'b TransactionDigest,
        cert: &'b C,
    ) -> SuiResult<Option<Self::Guard>>;

    #[must_use]
    async fn acquire_lock(&'a self, tx: &TransactionDigest) -> Self::LockGuard;

    /// Recoverable TXes are TXes that we find in the log at start up (which indicates we crashed
    /// while processing them) or implicitly dropped TXes (which can happen because we errored
    /// out of the write path and implicitly dropped the TxGuard).
    ///
    /// This method pops one recoverable tx from that log, acquires the lock for that tx,
    /// and returns a TxGuard.
    ///
    /// The caller is responsible for running the tx to completion.
    ///
    /// Recoverable TXes will remain in the on-disk log until they are explicitly committed.
    #[must_use]
    async fn read_one_recoverable_tx(&'a self) -> Option<Self::Guard>;

    /// Get the data associated with a given digest - returns an error if no such transaction is
    /// currently open.
    /// Requires a TxGuard to prevent asking about transactions that aren't in the log.
    fn get_tx_data(&self, g: &Self::Guard) -> SuiResult<(C, u32)>;
}

pub struct DBTxGuard<'a, C: Serialize + DeserializeOwned + Debug> {
    tx: TransactionDigest,
    _mutex_guard: LockGuard,
    wal: &'a DBWriteAheadLog<C>,
    dead: bool,
}

impl<'a, C> DBTxGuard<'a, C>
where
    C: Serialize + DeserializeOwned + Debug,
{
    fn new(tx: &TransactionDigest, _mutex_guard: LockGuard, wal: &'a DBWriteAheadLog<C>) -> Self {
        Self {
            tx: *tx,
            _mutex_guard,
            wal,
            dead: false,
        }
    }
}

impl<'a, C> TxGuard<'a> for DBTxGuard<'a, C>
where
    C: Serialize + DeserializeOwned + Debug,
{
    fn tx_id(&self) -> TransactionDigest {
        self.tx
    }

    fn commit_tx(mut self) {
        self.dead = true;
        // Note: if commit_tx fails, the tx will still be in the log and will re-enter
        // recoverable_txes when we restart. But the tx is fully processed at that point, so at
        // worst we will do a needless retry.
        if let Err(e) = self.wal.commit_tx(&self.tx) {
            warn!(digest = ?self.tx, "Couldn't write tx completion to WriteAheadLog: {}", e);
        }
    }

    // Identical to commit_tx (for now), but we provide different names to make intent clearer.
    fn release(self) {
        self.commit_tx()
    }
}

impl<C> Drop for DBTxGuard<'_, C>
where
    C: Serialize + DeserializeOwned + Debug,
{
    fn drop(&mut self) {
        if !self.dead {
            let tx = self.tx;
            error!(digest = ?tx, "DBTxGuard dropped without explicit commit");
            self.wal.implicit_drop_tx(&tx);
        }
    }
}

#[derive(DBMapUtils)]
pub struct DBWriteAheadLogTables<C> {
    log: DBMap<TransactionDigest, C>,
    // We use two tables, because if we instead have one table mapping digest -> (C, u32), we have
    // to clone C to make a tuple ref to pass to insert.
    retry_count: DBMap<TransactionDigest, u32>,
}

// A WriteAheadLog implementation built on rocksdb.
pub struct DBWriteAheadLog<C> {
    tables: DBWriteAheadLogTables<C>,

    // Can't use tokio Mutex - must be accessible synchronously from drop trait.
    // Only acquire this lock in sync functions to make sure we don't hold it across an await.
    recoverable_txes: Mutex<Vec<TransactionDigest>>,

    // Guards the get/set in begin_tx
    mutex_table: MutexTable<TransactionDigest>,
}

const MUTEX_TABLE_SIZE: usize = 1024;
const MUTEX_TABLE_SHARD_SIZE: usize = 128;

impl<C> DBWriteAheadLog<C>
where
    C: Serialize + DeserializeOwned + Debug,
{
    pub fn new(path: PathBuf) -> Self {
        let tables = DBWriteAheadLogTables::open_tables_read_write(path, None, None);

        // Read in any digests that were left in the log, e.g. due to a crash.
        //
        // This list will normally be small - it will typically only include txes that were
        // in-progress when we crashed.
        //
        // If, however, we were hitting repeated errors while trying to store txes, we could have
        // accumulated many txes in this list.
        let recoverable_txes: Vec<_> = tables.log.iter().map(|(tx, _)| tx).collect();

        Self {
            tables,
            recoverable_txes: Mutex::new(recoverable_txes),
            mutex_table: MutexTable::new(MUTEX_TABLE_SIZE, MUTEX_TABLE_SHARD_SIZE),
        }
    }

    fn commit_tx(&self, tx: &TransactionDigest) -> SuiResult {
        debug!(digest = ?tx, "committing tx");
        let write_batch = self.tables.log.batch();
        let write_batch = write_batch.delete_batch(&self.tables.log, std::iter::once(tx))?;
        let write_batch =
            write_batch.delete_batch(&self.tables.retry_count, std::iter::once(tx))?;
        write_batch.write().map_err(SuiError::from)
    }

    fn increment_retry_count(&self, tx: &TransactionDigest) -> SuiResult {
        let cur = self.tables.retry_count.get(tx)?.unwrap_or(0);
        self.tables
            .retry_count
            .insert(tx, &(cur + 1))
            .map_err(SuiError::from)
    }

    fn implicit_drop_tx(&self, tx: &TransactionDigest) {
        // this function should be called very rarely so contention should not be an issue.
        // unwrap ok because it is not safe to continue running if the mutex is poisoned.
        self.recoverable_txes.lock().unwrap().push(*tx);
    }

    fn pop_one_tx(&self) -> Option<TransactionDigest> {
        // Only acquire this lock inside a sync function to make sure we don't accidentally
        // hold it across an .await - unwrap okay because we should crash if a mutex is
        // poisoned.
        let recoverable_txes = &mut self.recoverable_txes.lock().unwrap();

        while let Some(tx) = recoverable_txes.pop() {
            if let Err(e) = self.increment_retry_count(&tx) {
                // Note that this does not remove the tx from the log, so we will find it again
                // next time we restart. But we will never retry a tx that we can't increment the
                // retry count for.
                error!(digest = ?tx,
                       "Failed to increment retry count for recovered tx. \
                       refusing to return it to avoid possible infinite \
                       crash loop. Error: {}", e);
                continue;
            } else {
                return Some(tx);
            }
        }

        None
    }
}

#[async_trait]
impl<'a, C: 'a> WriteAheadLog<'a, C> for DBWriteAheadLog<C>
where
    C: Serialize + DeserializeOwned + std::marker::Send + std::marker::Sync + Debug,
{
    type Guard = DBTxGuard<'a, C>;
    type LockGuard = LockGuard;

    #[must_use]
    #[instrument(level = "debug", name = "begin_tx", skip_all)]
    async fn begin_tx<'b>(
        &'a self,
        tx: &'b TransactionDigest,
        cert: &'b C,
    ) -> SuiResult<Option<DBTxGuard<'a, C>>> {
        let mutex_guard = self.mutex_table.acquire_lock(*tx).await;
        trace!(digest = ?tx, "acquired tx lock");

        if self.tables.log.contains_key(tx)? {
            // A concurrent tx will have held the mutex guard until it finished. If the tx is
            // committed it is removed from the log. This means that if the tx is still in the
            // log, it was dropped (errored out) and not committed. Return None to indicate
            // that the caller does not hold a guard on this tx and cannot proceed.
            //
            // (The dropped tx must be retried later by calling read_one_recoverable_tx() and
            // obtaining a TxGuard).
            return Ok(None);
        }

        self.tables.log.insert(tx, cert)?;

        Ok(Some(DBTxGuard::new(tx, mutex_guard, self)))
    }

    #[must_use]
    async fn acquire_lock(&'a self, tx: &TransactionDigest) -> Self::LockGuard {
        let res = self.mutex_table.acquire_lock(*tx).await;
        trace!(digest = ?tx, "acquired tx lock");
        res
    }

    #[must_use]
    async fn read_one_recoverable_tx(&'a self) -> Option<DBTxGuard<'a, C>> {
        let candidate = self.pop_one_tx();

        match candidate {
            None => None,
            Some(digest) => {
                let guard = self.mutex_table.acquire_lock(digest).await;
                Some(DBTxGuard::new(&digest, guard, self))
            }
        }
    }

    fn get_tx_data(&self, g: &DBTxGuard<'a, C>) -> SuiResult<(C, u32)> {
        let cert = self
            .tables
            .log
            .get(&g.tx)?
            .ok_or(SuiError::TransactionNotFound { digest: g.tx })?;
        let attempt_num = self.tables.retry_count.get(&g.tx)?.unwrap_or(0);
        Ok((cert, attempt_num))
    }
}

#[cfg(test)]
mod tests {

    use crate::write_ahead_log::{DBWriteAheadLog, TxGuard, WriteAheadLog};
    use anyhow;
    use sui_types::base_types::TransactionDigest;

    async fn recover_queue_empty(log: &DBWriteAheadLog<u32>) -> bool {
        log.read_one_recoverable_tx().await.is_none()
    }

    #[tokio::test]
    async fn test_write_ahead_log() -> Result<(), anyhow::Error> {
        let working_dir = tempfile::tempdir()?;

        let tx1_id = TransactionDigest::random();
        let tx2_id = TransactionDigest::random();
        let tx3_id = TransactionDigest::random();

        {
            let log: DBWriteAheadLog<u32> = DBWriteAheadLog::new(working_dir.path().to_path_buf());
            assert!(recover_queue_empty(&log).await);

            let tx1 = log.begin_tx(&tx1_id, &1).await?.unwrap();
            tx1.commit_tx();

            let tx2 = log.begin_tx(&tx2_id, &2).await?.unwrap();
            tx2.commit_tx();

            {
                let _tx3 = log.begin_tx(&tx3_id, &3).await?.unwrap();
                // implicit drop
            }

            let r = log.read_one_recoverable_tx().await.unwrap();
            // tx3 in recoverable txes because we dropped the guard.
            assert_eq!(r.tx_id(), tx3_id);

            // verify previous call emptied the recoverable list
            assert!(recover_queue_empty(&log).await);
        }

        {
            // recover the log
            let log: DBWriteAheadLog<u32> = DBWriteAheadLog::new(working_dir.path().to_path_buf());

            // recoverable txes still there
            let r = log.read_one_recoverable_tx().await.unwrap();
            assert_eq!(r.tx_id(), tx3_id);
            assert_eq!(log.get_tx_data(&r).unwrap(), (3, 2 /* retry */));
            assert!(recover_queue_empty(&log).await);

            // commit the recoverable tx
            r.commit_tx();
        }

        {
            // recover the log again
            let log: DBWriteAheadLog<u32> = DBWriteAheadLog::new(working_dir.path().to_path_buf());
            // empty, because we committed the tx before.
            assert!(recover_queue_empty(&log).await);
        }

        Ok(())
    }
}
