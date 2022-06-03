// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! WriteAheadLog is used by authority.rs / authority_store.rs for safe updates of the datastore.
//! It is currently implemented using a rocksdb table, but the interface is designed to be
//! compatible with a true log.

use async_trait::async_trait;

use serde::{de::DeserializeOwned, Serialize};

use std::path::Path;
use std::sync::Mutex;

use crate::{default_db_options, mutex_table::MutexTable};
use sui_types::base_types::TransactionDigest;

use sui_types::error::{SuiError, SuiResult};

use typed_store::{rocks::DBMap, traits::Map};

use tracing::{debug, error};

/// TxGuard is a handle on an in-progress transaction.
///
/// TxGuard must implement Drop, which should mark the tx as unfinished
/// if the guard is dropped without commit_tx being called.
#[allow(drop_bounds)]
pub trait TxGuard<'a>: Drop {
    fn tx_id(&self) -> TransactionDigest;
    fn commit_tx(self) -> SuiResult;
}

// WriteAheadLog is parameterized on the value type (C) because:
// - it's a pain to make a ConfirmationTransaction in tests.
// - we might end up storing either a ConfirmationTransaction or a (sequence, ConfirmationTransaction)
//   tuple, or something else
#[async_trait]
pub trait WriteAheadLog<'a, C> {
    type Guard: TxGuard<'a>;

    /// Begin a confirmation transaction identified by its digest, with the associated cert
    ///
    /// If a transaction with the given digest is already in progress, return None.
    /// Otherwise return a TxGuard, which is used to commit the tx.
    #[must_use]
    async fn begin_tx(&'a self, tx: &TransactionDigest, cert: &C)
        -> SuiResult<Option<Self::Guard>>;

    /// Recoverable TXes are TXes that we find in the log at start up (which indicates we crashed
    /// while processing them) or implicitly dropped TXes (which can happen because we errored
    /// out of the write path and implicitly dropped the TxGuard).
    ///
    /// This method takes and clears the current recoverable txes list.
    /// A vector of Guard is returned, which means the txes will return to the recoverable_txes
    /// list if not explicitly committed.
    ///
    /// The caller is responsible for running each tx to completion.
    ///
    /// Recoverable TXes will remain in the on-disk log until they are explicitly committed.
    #[must_use]
    fn take_recoverable_txes(&'a self) -> Vec<Self::Guard>;

    /// Get the data associated with a given digest - returns an error if no such transaction is
    /// currently open.
    /// Requires a TxGuard to prevent asking about transactions that aren't in the log.
    fn get_tx_data(&self, g: &Self::Guard) -> SuiResult<C>;
}

pub struct DBTxGuard<'a, C: Serialize + DeserializeOwned> {
    tx: TransactionDigest,
    wal: &'a DBWriteAheadLog<C>,
    dead: bool,
}

impl<'a, C> DBTxGuard<'a, C>
where
    C: Serialize + DeserializeOwned,
{
    fn new(tx: &TransactionDigest, wal: &'a DBWriteAheadLog<C>) -> Self {
        Self {
            tx: *tx,
            wal,
            dead: false,
        }
    }
}

impl<'a, C> TxGuard<'a> for DBTxGuard<'a, C>
where
    C: Serialize + DeserializeOwned,
{
    fn tx_id(&self) -> TransactionDigest {
        self.tx
    }

    fn commit_tx(mut self) -> SuiResult {
        self.dead = true;
        self.wal.commit_tx(&self.tx)
    }
}

impl<C> Drop for DBTxGuard<'_, C>
where
    C: Serialize + DeserializeOwned,
{
    fn drop(&mut self) {
        if !self.dead {
            let tx = self.tx;
            error!(digest = ?tx, "DBTxGuard dropped without explicit commit");
            self.wal.implicit_drop_tx(&tx);
        }
    }
}

// A WriteAheadLog implementation built on rocksdb.
pub struct DBWriteAheadLog<C> {
    log: DBMap<TransactionDigest, C>,

    // Can't use tokio Mutex - must be accessible synchronously from drop trait.
    recoverable_txes: Mutex<Vec<TransactionDigest>>,

    // Guards the get/set in begin_tx
    mutex_table: MutexTable<TransactionDigest>,
}

const MUTEX_TABLE_SIZE: usize = 1024;

impl<C> DBWriteAheadLog<C>
where
    C: Serialize + DeserializeOwned,
{
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let (options, _) = default_db_options(None);
        let db = {
            let path = &path;
            let db_options = Some(options.clone());
            let opt_cfs: &[(&str, &rocksdb::Options)] = &[("tx_write_ahead_log", &options)];
            typed_store::rocks::open_cf_opts(path, db_options, opt_cfs)
        }
        .expect("Cannot open DB.");

        let log: DBMap<TransactionDigest, C> =
            DBMap::reopen(&db, Some("tx_write_ahead_log")).expect("Cannot open CF.");

        // Read in any digests that were left in the log, e.g. due to a crash.
        //
        // This list will normally be small - it will typically only include txes that were
        // in-progress when we crashed.
        //
        // If, however, we were hitting repeated errors while trying to store txes, we could have
        // accumulated many txes in this list.
        let recoverable_txes: Vec<_> = log.iter().map(|(tx, _)| tx).collect();

        Self {
            log,
            recoverable_txes: Mutex::new(recoverable_txes),
            mutex_table: MutexTable::new(MUTEX_TABLE_SIZE),
        }
    }

    fn commit_tx(&self, tx: &TransactionDigest) -> SuiResult {
        debug!(digest = ?tx, "committing tx");
        self.log.remove(tx).map_err(|e| e.into())
    }

    fn implicit_drop_tx(&self, tx: &TransactionDigest) {
        // this function should be called very rarely so contention should not be an issue.
        // unwrap ok because it is not safe to continue running if the mutex is poisoned.
        let mut r = self.recoverable_txes.lock().unwrap();
        r.push(*tx);
    }
}

#[async_trait]
impl<'a, C: 'a> WriteAheadLog<'a, C> for DBWriteAheadLog<C>
where
    C: Serialize + DeserializeOwned + std::marker::Send + std::marker::Sync,
{
    type Guard = DBTxGuard<'a, C>;

    #[must_use]
    async fn begin_tx(
        &'a self,
        tx: &TransactionDigest,
        cert: &C,
    ) -> SuiResult<Option<DBTxGuard<'a, C>>> {
        let _mutex_guard = self.mutex_table.acquire_lock(tx).await;

        if self.log.contains_key(tx)? {
            // We return None instead of a guard, to signal that a tx with this digest is already
            // in progress.
            //
            // TODO: It may turn out to be better to hold the lock until the other guard is
            // dropped - this should become clear once this code is being used.
            return Ok(None);
        }

        self.log.insert(tx, cert)?;

        Ok(Some(DBTxGuard::new(tx, self)))
    }

    #[must_use]
    fn take_recoverable_txes(&'a self) -> Vec<DBTxGuard<'a, C>> {
        // unwrap ok because we should absolutely crash if the mutex is poisoned
        let mut v = self.recoverable_txes.lock().unwrap();
        let mut new = Vec::new();
        std::mem::swap(&mut *v, &mut new);
        let ret = new;
        ret.iter()
            .map(|digest| DBTxGuard::new(digest, self))
            .collect()
    }

    fn get_tx_data(&self, g: &DBTxGuard<'a, C>) -> SuiResult<C> {
        self.log
            .get(&g.tx)
            .map_err(SuiError::from)?
            .ok_or(SuiError::TransactionNotFound { digest: g.tx })
    }
}

#[cfg(test)]
mod tests {

    use crate::write_ahead_log::{DBWriteAheadLog, TxGuard, WriteAheadLog};
    use anyhow;
    use sui_types::base_types::TransactionDigest;

    #[tokio::test]
    async fn test_write_ahead_log() -> Result<(), anyhow::Error> {
        let working_dir = tempfile::tempdir()?;

        let tx1_id = TransactionDigest::random();
        let tx2_id = TransactionDigest::random();
        let tx3_id = TransactionDigest::random();

        {
            let log: DBWriteAheadLog<u32> = DBWriteAheadLog::new(&working_dir);
            let r = log.take_recoverable_txes();
            assert!(r.is_empty());

            let tx1 = log.begin_tx(&tx1_id, &1).await?.unwrap();
            tx1.commit_tx().unwrap();

            let tx2 = log.begin_tx(&tx2_id, &2).await?.unwrap();
            tx2.commit_tx().unwrap();

            {
                let _tx3 = log.begin_tx(&tx3_id, &3).await?.unwrap();
                // implicit drop
            }

            let r = log.take_recoverable_txes();
            // tx3 in recoverable txes because we dropped the guard.
            assert_eq!(r.len(), 1);
            assert_eq!(r[0].tx_id(), tx3_id);

            // verify previous call emptied the recoverable list
            let r_empty = log.take_recoverable_txes();
            assert!(r_empty.is_empty());
        }

        {
            // recover the log
            let log: DBWriteAheadLog<u32> = DBWriteAheadLog::new(&working_dir);

            // recoverable txes still there
            let mut r = log.take_recoverable_txes();
            assert_eq!(r.len(), 1);
            let g = r.pop().unwrap();
            assert_eq!(g.tx_id(), tx3_id);

            assert_eq!(log.get_tx_data(&g).unwrap(), 3);

            // commit the recoverable tx
            g.commit_tx().unwrap();
        }

        {
            // recover the log again
            let log: DBWriteAheadLog<u32> = DBWriteAheadLog::new(&working_dir);
            let r = log.take_recoverable_txes();
            // empty, because we committed the tx before.
            assert!(r.is_empty());
        }

        Ok(())
    }
}
