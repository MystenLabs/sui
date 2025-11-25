// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! WritePathPendingTransactionLog is used in TransactionOrchestrator
//! to deduplicate transaction submission processing. It helps to achieve:
//! 1. At one time, a transaction is only processed once.
//! 2. When Fullnode crashes and restarts, the pending transaction will be loaded and retried.

use std::collections::HashSet;
use std::path::PathBuf;

use parking_lot::Mutex;
use sui_types::base_types::TransactionDigest;
use sui_types::crypto::EmptySignInfo;
use sui_types::error::{SuiError, SuiResult};
use sui_types::message_envelope::TrustedEnvelope;
use sui_types::transaction::{SenderSignedData, VerifiedTransaction};
use typed_store::DBMapUtils;
use typed_store::rocks::MetricConf;
use typed_store::{rocks::DBMap, traits::Map};

#[derive(DBMapUtils)]
struct WritePathPendingTransactionTable {
    logs: DBMap<TransactionDigest, TrustedEnvelope<SenderSignedData, EmptySignInfo>>,
}

pub struct WritePathPendingTransactionLog {
    // Disk storage for pending transactions.
    pending_transactions: WritePathPendingTransactionTable,
    // In-memory set of pending transactions.
    transactions_set: Mutex<HashSet<TransactionDigest>>,
}

impl WritePathPendingTransactionLog {
    pub fn new(path: PathBuf) -> Self {
        let pending_transactions = WritePathPendingTransactionTable::open_tables_read_write(
            path,
            MetricConf::new("pending_tx_log"),
            None,
            None,
        );
        Self {
            pending_transactions,
            transactions_set: Mutex::new(HashSet::new()),
        }
    }

    // Returns whether the table currently has this transaction in record.
    // If not, write the transaction and return true; otherwise return false.
    pub fn write_pending_transaction_maybe(&self, tx: &VerifiedTransaction) -> bool {
        let tx_digest = tx.digest();
        let mut transactions_set = self.transactions_set.lock();
        if transactions_set.contains(tx_digest) {
            return false;
        }
        // Hold the lock while inserting into the logs to avoid race conditions.
        self.pending_transactions
            .logs
            .insert(tx_digest, tx.serializable_ref())
            .unwrap();
        transactions_set.insert(*tx_digest);
        true
    }

    pub fn finish_transaction(&self, tx: &TransactionDigest) -> SuiResult {
        let mut transactions_set = self.transactions_set.lock();
        // Hold the lock while removing from the logs to avoid race conditions.
        let mut write_batch = self.pending_transactions.logs.batch();
        write_batch.delete_batch(&self.pending_transactions.logs, std::iter::once(tx))?;
        write_batch.write().map_err(SuiError::from)?;
        transactions_set.remove(tx);
        Ok(())
    }

    pub fn load_all_pending_transactions(&self) -> SuiResult<Vec<VerifiedTransaction>> {
        let mut transactions_set = self.transactions_set.lock();
        let transactions = self
            .pending_transactions
            .logs
            .safe_iter()
            .map(|item| item.map(|(_tx_digest, tx)| VerifiedTransaction::from(tx)))
            .collect::<Result<Vec<_>, _>>()?;
        transactions_set.extend(transactions.iter().map(|t| *t.digest()));
        Ok(transactions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow;
    use std::collections::HashSet;
    use sui_types::utils::create_fake_transaction;

    #[tokio::test]
    async fn test_pending_tx_log_basic() -> anyhow::Result<()> {
        let temp_dir = tempfile::tempdir().unwrap();
        let pending_txes = WritePathPendingTransactionLog::new(temp_dir.path().to_path_buf());
        let tx = VerifiedTransaction::new_unchecked(create_fake_transaction());
        let tx_digest = *tx.digest();
        assert!(pending_txes.write_pending_transaction_maybe(&tx));
        // The second write will return false
        assert!(!pending_txes.write_pending_transaction_maybe(&tx));

        let loaded_txes = pending_txes.load_all_pending_transactions()?;
        assert_eq!(vec![tx], loaded_txes);

        pending_txes.finish_transaction(&tx_digest).unwrap();
        let loaded_txes = pending_txes.load_all_pending_transactions()?;
        assert!(loaded_txes.is_empty());

        // It's ok to finish an already finished transaction
        pending_txes.finish_transaction(&tx_digest).unwrap();

        // Test writing and finishing more transactions
        let txes: Vec<_> = (0..10)
            .map(|_| VerifiedTransaction::new_unchecked(create_fake_transaction()))
            .collect();
        for tx in txes.iter().take(10) {
            assert!(pending_txes.write_pending_transaction_maybe(tx));
        }
        let loaded_tx_digests: HashSet<_> = pending_txes
            .load_all_pending_transactions()?
            .iter()
            .map(|t| *t.digest())
            .collect();
        assert_eq!(
            txes.iter().map(|t| *t.digest()).collect::<HashSet<_>>(),
            loaded_tx_digests
        );

        for tx in txes.iter().take(5) {
            pending_txes.finish_transaction(tx.digest()).unwrap();
        }
        let loaded_tx_digests: HashSet<_> = pending_txes
            .load_all_pending_transactions()?
            .iter()
            .map(|t| *t.digest())
            .collect();
        assert_eq!(
            txes.iter()
                .skip(5)
                .map(|t| *t.digest())
                .collect::<HashSet<_>>(),
            loaded_tx_digests
        );

        Ok(())
    }
}
