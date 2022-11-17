// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! WriteAheadLog is used by authority.rs / authority_store.rs for safe updates of the datastore.
//! It is currently implemented using a rocksdb table, but the interface is designed to be
//! compatible with a true log.

use async_trait::async_trait;

use crate::mutex_table::{LockGuard, MutexTable};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::Mutex;
use sui_types::base_types::TransactionDigest;
use sui_types::messages::SignedTransactionEffects;
use sui_types::temporary_store::InnerTemporaryStore;
use typed_store::traits::TypedStoreDebug;
use typed_store_derive::DBMapUtils;

use sui_types::error::{SuiError, SuiResult};

use typed_store::{rocks::DBMap, traits::Map};

use tracing::{debug, error, instrument, trace, warn};

use tap::TapFallible;

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

    /// How many times has this tx previously been attempted?
    fn retry_num(&self) -> u32;

    /// Mark the TX as abandoned/aborted but not requiring any recovery or rollback.
    fn release(self);
}

/// Denotes a transaction commit phase.
///
/// `Uncommitted`: The initial state where the tx digest and certificate
/// exist in the WAL, but the transaction has not yet been both executed
/// and perssited to permanent storage.
///
/// `CommittedToWal(...)`: The state that the transaction enters when it has been
/// executed and its resultant objects and effects written to the WAL. This is
/// a recoverable state, and the arguments of this variant can be used to retry
/// writing to permanent storage.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub enum TransactionCommitPhase {
    Uncommitted,
    CommittedToWal(InnerTemporaryStore, SignedTransactionEffects),
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
    async fn begin_tx<'b>(
        &'a self,
        tx: &'b TransactionDigest,
        cert: &'b C,
    ) -> SuiResult<Self::Guard>;

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
    async fn read_one_recoverable_tx(
        &'a self,
    ) -> SuiResult<Option<(C, TransactionCommitPhase, Self::Guard)>>;
}

pub struct DBTxGuard<'a, C: Serialize + DeserializeOwned + Debug> {
    tx: TransactionDigest,
    retry_num: u32,
    _mutex_guard: LockGuard,
    wal: &'a DBWriteAheadLog<C>,
    dead: bool,
}

impl<'a, C> DBTxGuard<'a, C>
where
    C: Serialize + DeserializeOwned + Debug,
{
    fn new(
        tx: &TransactionDigest,
        retry_num: u32,
        _mutex_guard: LockGuard,
        wal: &'a DBWriteAheadLog<C>,
    ) -> Self {
        Self {
            tx: *tx,
            retry_num,
            _mutex_guard,
            wal,
            dead: false,
        }
    }

    fn commit_tx_impl(mut self, is_commit: bool) {
        self.dead = true;
        // Note: if commit_tx fails, the tx will still be in the log and will re-enter
        // recoverable_txes when we restart. But the tx is fully processed at that point, so at
        // worst we will do a needless retry.
        if let Err(e) = self.wal.commit_tx(&self.tx, is_commit) {
            warn!(digest = ?self.tx, "Couldn't write tx completion to WriteAheadLog: {}", e);
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

    fn commit_tx(self) {
        self.commit_tx_impl(true)
    }

    fn retry_num(&self) -> u32 {
        self.retry_num
    }

    // Identical to commit_tx (for now), but we provide different names to make intent clearer.
    fn release(self) {
        self.commit_tx_impl(false)
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
    log: DBMap<TransactionDigest, (C, TransactionCommitPhase)>,
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

    pub fn get_tx(&self, tx: &TransactionDigest) -> SuiResult<Option<(C, TransactionCommitPhase)>> {
        self.tables.log.get(tx).map_err(SuiError::from)
    }

    pub fn get_intermediate_objs(
        &self,
        tx: &TransactionDigest,
    ) -> SuiResult<Option<(InnerTemporaryStore, SignedTransactionEffects)>> {
        match self.get_tx(tx)? {
            Some((_cert, TransactionCommitPhase::CommittedToWal(store, fx))) => {
                Ok(Some((store, fx)))
            }
            Some((_cert, TransactionCommitPhase::Uncommitted)) => Ok(None),
            None => Ok(None),
        }
    }

    pub fn write_intermediate_objs(
        &self,
        tx: &TransactionDigest,
        store: InnerTemporaryStore,
        fx: SignedTransactionEffects,
    ) -> SuiResult {
        match self.get_tx(tx)? {
            Some((cert, TransactionCommitPhase::Uncommitted)) => {
                self.tables
                    .log
                    .insert(
                        tx,
                        &(cert, TransactionCommitPhase::CommittedToWal(store, fx)),
                    )
                    .map_err(SuiError::from)?;
                Ok(())
            }
            // We skip rather than overwriting, with the assumption that the
            // same tx cert digest must produce the same output
            // should return error here instead
            Some((_cert, TransactionCommitPhase::CommittedToWal(_, _))) => Ok(()),
            None => Err(SuiError::TransactionNotFound { digest: *tx }),
        }
    }

    fn commit_tx(&self, tx: &TransactionDigest, is_commit: bool) -> SuiResult {
        if is_commit {
            debug!(digest = ?tx, "committing tx");
        }
        let write_batch = self.tables.log.batch();
        let write_batch = write_batch.delete_batch(&self.tables.log, std::iter::once(tx))?;
        let write_batch =
            write_batch.delete_batch(&self.tables.retry_count, std::iter::once(tx))?;
        write_batch.write().map_err(SuiError::from)
    }

    fn get_retry_count(&self, tx: &TransactionDigest) -> SuiResult<u32> {
        Ok(self.tables.retry_count.get(tx)?.unwrap_or(0))
    }

    fn increment_retry_count(&self, tx: &TransactionDigest) -> SuiResult<u32> {
        let retry_count = self.get_retry_count(tx)? + 1;
        self.tables
            .retry_count
            .insert(tx, &retry_count)
            .map_err(SuiError::from)?;
        Ok(retry_count)
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
        recoverable_txes.pop()
    }
}

#[async_trait]
impl<'a, C: 'a> WriteAheadLog<'a, C> for DBWriteAheadLog<C>
where
    C: Serialize + DeserializeOwned + Send + Sync + Debug + Clone,
{
    type Guard = DBTxGuard<'a, C>;
    type LockGuard = LockGuard;

    #[instrument(level = "debug", name = "begin_tx", skip_all)]
    async fn begin_tx<'b>(
        &'a self,
        tx: &'b TransactionDigest,
        cert: &'b C,
    ) -> SuiResult<DBTxGuard<'a, C>> {
        let mutex_guard = self.mutex_table.acquire_lock(*tx).await;
        trace!(digest = ?tx, "acquired tx lock");

        let retry_count = if self.tables.log.contains_key(tx)? {
            self.increment_retry_count(tx).tap_err(|e| {
                error!(digest = ?tx,
                       "Failed to increment retry count tx. \
                       Refusing to return tx guard to avoid possible infinite \
                       crash loop. Error: {}", e)
            })?
        } else {
            self.tables
                .log
                .insert(tx, &(cert.clone(), TransactionCommitPhase::Uncommitted))?;
            0
        };

        Ok(DBTxGuard::new(tx, retry_count, mutex_guard, self))
    }

    #[must_use]
    async fn acquire_lock(&'a self, tx: &TransactionDigest) -> Self::LockGuard {
        let res = self.mutex_table.acquire_lock(*tx).await;
        trace!(digest = ?tx, "acquired tx lock");
        res
    }

    async fn read_one_recoverable_tx(
        &'a self,
    ) -> SuiResult<Option<(C, TransactionCommitPhase, DBTxGuard<'a, C>)>> {
        let candidate = self.pop_one_tx();

        match candidate {
            None => Ok(None),
            Some(digest) => {
                let (cert, commit_phase) = self
                    .tables
                    .log
                    .get(&digest)?
                    .ok_or(SuiError::TransactionNotFound { digest })?;

                let guard = self.begin_tx(&digest, &cert).await?;
                Ok(Some((cert, commit_phase, guard)))
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use std::time::Duration;

    use crate::write_ahead_log::{DBWriteAheadLog, TransactionCommitPhase, TxGuard, WriteAheadLog};
    use anyhow;
    use sui_types::base_types::TransactionDigest;

    async fn recover_queue_empty(log: &DBWriteAheadLog<u32>) -> bool {
        log.read_one_recoverable_tx().await.unwrap().is_none()
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

            let tx1 = log.begin_tx(&tx1_id, &1).await.unwrap();
            tx1.commit_tx();

            let tx2 = log.begin_tx(&tx2_id, &2).await.unwrap();
            tx2.commit_tx();

            {
                let _tx3 = log.begin_tx(&tx3_id, &3).await.unwrap();
                // implicit drop
            }

            let (_, commit_phase, r) = log.read_one_recoverable_tx().await.unwrap().unwrap();
            // tx3 in recoverable txes because we dropped the guard.
            assert_eq!(r.tx_id(), tx3_id);

            assert!(matches!(commit_phase, TransactionCommitPhase::Uncommitted));

            // verify previous call emptied the recoverable list
            assert!(recover_queue_empty(&log).await);
        }

        // TODO: The right fix is to invoke some function on DBMap and release the rocksdb arc references
        // being held in the background thread but this will suffice for now
        tokio::time::sleep(Duration::from_secs(1)).await;

        {
            // recover the log
            let log: DBWriteAheadLog<u32> = DBWriteAheadLog::new(working_dir.path().to_path_buf());

            // recoverable txes still there
            let (_, commit_phase, r) = log.read_one_recoverable_tx().await.unwrap().unwrap();
            assert_eq!(r.tx_id(), tx3_id);
            assert_eq!(r.retry_num(), 2);
            assert!(recover_queue_empty(&log).await);
            assert!(matches!(commit_phase, TransactionCommitPhase::Uncommitted));

            // commit the recoverable tx
            r.commit_tx();
        }

        // TODO: The right fix is to invoke some function on DBMap and release the rocksdb arc references
        // being held in the background thread but this will suffice for now
        tokio::time::sleep(Duration::from_secs(1)).await;

        {
            // recover the log again
            let log: DBWriteAheadLog<u32> = DBWriteAheadLog::new(working_dir.path().to_path_buf());
            // empty, because we committed the tx before.
            assert!(recover_queue_empty(&log).await);
        }

        // TODO(william): Add another recovery case where we commit to WAL but not permanent storage.

        Ok(())
    }
}
