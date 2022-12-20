// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::{select, Either};
use futures::FutureExt;
use narwhal_executor::ExecutionIndices;
use parking_lot::Mutex;
use parking_lot::RwLock;
use rocksdb::Options;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::future::Future;
use std::iter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sui_storage::default_db_options;
use sui_storage::mutex_table::LockGuard;
use sui_storage::write_ahead_log::{DBWriteAheadLog, TxGuard, WriteAheadLog};
use sui_types::base_types::{AuthorityName, EpochId, ObjectID, SequenceNumber, TransactionDigest};
use sui_types::committee::Committee;
use sui_types::crypto::AuthoritySignInfo;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{
    ConsensusTransaction, ConsensusTransactionKey, ConsensusTransactionKind, SenderSignedData,
    SignedTransactionEffects, TrustedCertificate, VerifiedCertificate, VerifiedSignedTransaction,
};
use tracing::debug;
use typed_store::rocks::{DBBatch, DBMap, DBOptions, TypedStoreError};
use typed_store::traits::TypedStoreDebug;

use crate::authority::authority_notify_read::NotifyRead;
use crate::authority::{CertTxGuard, MAX_TX_RECOVERY_RETRY};
use crate::epoch::reconfiguration::ReconfigState;
use crate::notify_once::NotifyOnce;
use crate::stake_aggregator::StakeAggregator;
use sui_types::message_envelope::TrustedEnvelope;
use sui_types::temporary_store::InnerTemporaryStore;
use typed_store::Map;
use typed_store_derive::DBMapUtils;

/// The key where the latest consensus index is stored in the database.
// TODO: Make a single table (e.g., called `variables`) storing all our lonely variables in one place.
const LAST_CONSENSUS_INDEX_ADDR: u64 = 0;
const RECONFIG_STATE_INDEX: u64 = 0;
const FINAL_EPOCH_CHECKPOINT_INDEX: u64 = 0;

pub struct CertLockGuard(LockGuard);

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionIndicesWithHash {
    pub index: ExecutionIndices,
    pub hash: u64,
}

pub struct AuthorityPerEpochStore {
    committee: Committee,
    tables: AuthorityEpochTables,
    /// In-memory cache of the content from the reconfig_state db table.
    reconfig_state_mem: RwLock<ReconfigState>,
    consensus_notify_read: NotifyRead<ConsensusTransactionKey, ()>,
    /// This is used to notify all epoch specific tasks that epoch has ended.
    epoch_alive_notify: NotifyOnce,
    /// This lock acts as a barrier for tasks that should not be executed in parallel with reconfiguration
    /// See comments in AuthorityPerEpochStore::epoch_terminated() on how this is used
    /// Crash recovery note: we write next epoch in the database first, and then use this lock to
    /// wait for in-memory tasks for the epoch to finish. If node crashes at this stage validator
    /// will start with the new epoch(and will open instance of per-epoch store for a new epoch).
    epoch_alive: tokio::sync::RwLock<bool>,
    end_of_publish: Mutex<StakeAggregator<(), true>>,
    /// Pending certificates that we are waiting to be sequenced by consensus.
    /// This is an in-memory 'index' of a AuthorityPerEpochTables::pending_consensus_transactions.
    /// We need to keep track of those in order to know when to send EndOfPublish message.
    /// Lock ordering: this is a 'leaf' lock, no other locks should be acquired in the scope of this lock
    /// In particular, this lock is always acquired after taking read or write lock on reconfig state
    pending_consensus_certificates: Mutex<HashSet<TransactionDigest>>,
    /// A write-ahead/recovery log used to ensure we finish fully processing certs after errors or
    /// crashes.
    wal: Arc<DBWriteAheadLog<TrustedCertificate, (InnerTemporaryStore, SignedTransactionEffects)>>,
}

/// AuthorityEpochTables contains tables that contain data that is only valid within an epoch.
#[derive(DBMapUtils)]
pub struct AuthorityEpochTables {
    /// This is map between the transaction digest and transactions found in the `transaction_lock`.
    #[default_options_override_fn = "transactions_table_default_config"]
    transactions: DBMap<TransactionDigest, TrustedEnvelope<SenderSignedData, AuthoritySignInfo>>,

    /// The two tables below manage shared object locks / versions. There are two ways they can be
    /// updated:
    /// 1. Upon receiving a certified transaction from consensus, the authority assigns the next
    /// version to each shared object of the transaction. The next versions of the shared objects
    /// are updated as well.
    /// 2. Upon receiving a certified effect, the authority assigns the shared object versions from
    /// the effect to the transaction of the effect. Next object versions are not updated.
    ///
    /// REQUIRED: all authorities must assign the same shared object versions for each transaction.
    assigned_shared_object_versions: DBMap<TransactionDigest, Vec<(ObjectID, SequenceNumber)>>,
    next_shared_object_versions: DBMap<ObjectID, SequenceNumber>,

    /// Certificates that have been received from clients or received from consensus, but not yet
    /// executed. Entries are cleared after execution.
    /// This table is critical for crash recovery, because usually the consensus output progress
    /// is updated after a certificate is committed into this table.
    ///
    /// If theory, this table may be superseded by storing consensus and checkpoint execution
    /// progress. But it is more complex, because it would be necessary to track inflight
    /// executions not ordered by indices. For now, tracking inflight certificates as a map
    /// seems easier.
    pending_certificates: DBMap<TransactionDigest, TrustedCertificate>,

    /// Track which transactions have been processed in handle_consensus_transaction. We must be
    /// sure to advance next_shared_object_versions exactly once for each transaction we receive from
    /// consensus. But, we may also be processing transactions from checkpoints, so we need to
    /// track this state separately.
    ///
    /// Entries in this table can be garbage collected whenever we can prove that we won't receive
    /// another handle_consensus_transaction call for the given digest. This probably means at
    /// epoch change.
    consensus_message_processed: DBMap<ConsensusTransactionKey, bool>,

    /// Map stores pending transactions that this authority submitted to consensus
    pending_consensus_transactions: DBMap<ConsensusTransactionKey, ConsensusTransaction>,

    /// This is an inverse index for consensus_message_processed - it allows to select
    /// all transactions at the specific consensus range
    ///
    /// The consensus position for the transaction is defined as first position at which valid
    /// certificate for this transaction is seen in consensus
    consensus_message_order: DBMap<ExecutionIndices, TransactionDigest>,

    /// The following table is used to store a single value (the corresponding key is a constant). The value
    /// represents the index of the latest consensus message this authority processed. This field is written
    /// by a single process acting as consensus (light) client. It is used to ensure the authority processes
    /// every message output by consensus (and in the right order).
    last_consensus_index: DBMap<u64, ExecutionIndicesWithHash>,

    /// This table lists all checkpoint boundaries in the consensus sequence
    ///
    /// The key in this table is incremental index and value is corresponding narwhal
    /// consensus output index
    checkpoint_boundary: DBMap<u64, u64>,

    /// This table contains current reconfiguration state for validator for current epoch
    reconfig_state: DBMap<u64, ReconfigState>,

    /// Validators that have sent EndOfPublish message in this epoch
    end_of_publish: DBMap<AuthorityName, ()>,

    // todo - if we move processing of entire nw commit into single DB batch,
    // we can potentially get rid of this table
    /// Records narwhal consensus output index of the final checkpoint in epoch
    /// This is a single entry table with key FINAL_EPOCH_CHECKPOINT_INDEX
    final_epoch_checkpoint: DBMap<u64, u64>,
}

impl AuthorityEpochTables {
    pub fn open(epoch: EpochId, parent_path: &Path, db_options: Option<Options>) -> Self {
        Self::open_tables_read_write(Self::path(epoch, parent_path), db_options, None)
    }

    pub fn open_readonly(epoch: EpochId, parent_path: &Path) -> AuthorityEpochTablesReadOnly {
        Self::get_read_only_handle(Self::path(epoch, parent_path), None, None)
    }

    pub fn path(epoch: EpochId, parent_path: &Path) -> PathBuf {
        parent_path.join(format!("epoch_{}", epoch))
    }

    fn load_reconfig_state(&self) -> SuiResult<ReconfigState> {
        let state = self
            .reconfig_state
            .get(&RECONFIG_STATE_INDEX)?
            .unwrap_or_default();
        Ok(state)
    }

    pub fn get_all_pending_consensus_transactions(&self) -> Vec<ConsensusTransaction> {
        self.pending_consensus_transactions
            .iter()
            .map(|(_k, v)| v)
            .collect()
    }
}

impl AuthorityPerEpochStore {
    pub fn new(committee: Committee, parent_path: &Path, db_options: Option<Options>) -> Self {
        let epoch_id = committee.epoch;
        let tables = AuthorityEpochTables::open(epoch_id, parent_path, db_options);
        let end_of_publish =
            StakeAggregator::from_iter(committee.clone(), tables.end_of_publish.iter());
        let reconfig_state = tables
            .load_reconfig_state()
            .expect("Load reconfig state at initialization cannot fail");
        let wal_path = AuthorityEpochTables::path(epoch_id, parent_path).join("recovery_log");
        let wal = Arc::new(DBWriteAheadLog::new(wal_path));
        let epoch_alive_notify = NotifyOnce::new();
        let pending_consensus_transactions = tables.get_all_pending_consensus_transactions();
        let pending_consensus_certificates: HashSet<_> = pending_consensus_transactions
            .iter()
            .filter_map(|transaction| {
                if let ConsensusTransactionKind::UserTransaction(certificate) = &transaction.kind {
                    Some(*certificate.digest())
                } else {
                    None
                }
            })
            .collect();
        Self {
            committee,
            tables,
            reconfig_state_mem: RwLock::new(reconfig_state),
            epoch_alive_notify,
            epoch_alive: tokio::sync::RwLock::new(true),
            consensus_notify_read: NotifyRead::new(),
            end_of_publish: Mutex::new(end_of_publish),
            pending_consensus_certificates: Mutex::new(pending_consensus_certificates),
            wal,
        }
    }

    pub fn wal(
        &self,
    ) -> &Arc<DBWriteAheadLog<TrustedCertificate, (InnerTemporaryStore, SignedTransactionEffects)>>
    {
        &self.wal
    }

    pub fn committee(&self) -> &Committee {
        &self.committee
    }

    pub fn epoch(&self) -> EpochId {
        self.committee.epoch
    }

    pub async fn acquire_tx_guard(&self, cert: &VerifiedCertificate) -> SuiResult<CertTxGuard> {
        let digest = cert.digest();
        let guard = self.wal.begin_tx(digest, cert.serializable_ref()).await?;

        if guard.retry_num() > MAX_TX_RECOVERY_RETRY {
            // If the tx has been retried too many times, it could be a poison pill, and we should
            // prevent the client from continually retrying it.
            let err = "tx has exceeded the maximum retry limit for transient errors".to_owned();
            debug!(?digest, "{}", err);
            return Err(SuiError::ErrorWhileProcessingCertificate { err });
        }

        Ok(guard)
    }

    /// Acquire the lock for a tx without writing to the WAL.
    pub async fn acquire_tx_lock(&self, digest: &TransactionDigest) -> CertLockGuard {
        CertLockGuard(self.wal.acquire_lock(digest).await)
    }

    pub fn store_reconfig_state(&self, new_state: &ReconfigState) -> SuiResult {
        self.tables
            .reconfig_state
            .insert(&RECONFIG_STATE_INDEX, new_state)?;
        Ok(())
    }

    fn store_reconfig_state_batch(
        &self,
        new_state: &ReconfigState,
        batch: DBBatch,
    ) -> SuiResult<DBBatch> {
        Ok(batch.insert_batch(
            &self.tables.reconfig_state,
            [(&RECONFIG_STATE_INDEX, new_state)],
        )?)
    }

    pub fn insert_transaction(&self, transaction: VerifiedSignedTransaction) -> SuiResult {
        Ok(self
            .tables
            .transactions
            .insert(transaction.digest(), transaction.serializable_ref())?)
    }

    pub fn get_transaction(
        &self,
        tx_digest: &TransactionDigest,
    ) -> SuiResult<Option<VerifiedSignedTransaction>> {
        Ok(self.tables.transactions.get(tx_digest)?.map(|t| t.into()))
    }

    pub fn multi_get_next_shared_object_versions<'a>(
        &self,
        ids: impl Iterator<Item = &'a ObjectID>,
    ) -> SuiResult<Vec<Option<SequenceNumber>>> {
        Ok(self.tables.next_shared_object_versions.multi_get(ids)?)
    }

    pub fn get_last_checkpoint_boundary(&self) -> (u64, Option<u64>) {
        match self.tables.checkpoint_boundary.iter().skip_to_last().next() {
            Some((idx, height)) => (idx, Some(height)),
            None => (0, None),
        }
    }

    pub fn get_last_consensus_index(&self) -> SuiResult<ExecutionIndicesWithHash> {
        self.tables
            .last_consensus_index
            .get(&LAST_CONSENSUS_INDEX_ADDR)
            .map(|x| x.unwrap_or_default())
            .map_err(SuiError::from)
    }

    pub fn get_transactions_in_checkpoint_range(
        &self,
        from_height_excluded: Option<u64>,
        to_height_included: u64,
    ) -> SuiResult<Vec<TransactionDigest>> {
        let mut iter = self.tables.consensus_message_order.iter();
        if let Some(from_height_excluded) = from_height_excluded {
            let last_previous = ExecutionIndices::end_for_commit(from_height_excluded);
            iter = iter.skip_to(&last_previous)?;
        }
        // skip_to lands to key the last_key or key after it
        // technically here we need to check if first item in stream has a key equal to last_previous
        // however in practice this can not happen because number of batches in certificate is
        // limited and is less then u64::MAX
        let roots: Vec<_> = iter
            .take_while(|(idx, _tx)| idx.last_committed_round <= to_height_included)
            .map(|(_idx, tx)| tx)
            .collect();
        Ok(roots)
    }

    /// Gets one pending certificate.
    pub fn get_pending_certificate(
        &self,
        tx: &TransactionDigest,
    ) -> Result<Option<VerifiedCertificate>, TypedStoreError> {
        Ok(self.tables.pending_certificates.get(tx)?.map(|c| c.into()))
    }

    pub fn multi_get_pending_certificate(
        &self,
        transaction_digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<VerifiedCertificate>>> {
        Ok(self
            .tables
            .pending_certificates
            .multi_get(transaction_digests)?
            .into_iter()
            .map(|o| o.map(|c| c.into()))
            .collect())
    }

    /// Gets all pending certificates. Used during recovery.
    pub fn all_pending_certificates(&self) -> SuiResult<Vec<VerifiedCertificate>> {
        Ok(self
            .tables
            .pending_certificates
            .iter()
            .map(|(_, cert)| cert.into())
            .collect())
    }

    /// Deletes one pending certificate.
    pub fn remove_pending_certificate(&self, digest: &TransactionDigest) -> SuiResult<()> {
        self.tables.pending_certificates.remove(digest)?;
        Ok(())
    }

    pub fn get_all_pending_consensus_transactions(&self) -> Vec<ConsensusTransaction> {
        self.tables.get_all_pending_consensus_transactions()
    }

    /// Read shared object locks / versions for a specific transaction.
    pub fn get_shared_locks(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> Result<Vec<(ObjectID, SequenceNumber)>, SuiError> {
        Ok(self
            .tables
            .assigned_shared_object_versions
            .get(transaction_digest)?
            .unwrap_or_default())
    }

    #[cfg(test)]
    pub fn get_next_object_version(&self, obj: &ObjectID) -> Option<SequenceNumber> {
        self.tables.next_shared_object_versions.get(obj).unwrap()
    }

    pub fn delete_shared_object_versions(
        &self,
        executed_transaction: &TransactionDigest,
        deleted_objects: &[ObjectID],
    ) -> SuiResult {
        let mut write_batch = self.tables.assigned_shared_object_versions.batch();
        write_batch = write_batch.delete_batch(
            &self.tables.assigned_shared_object_versions,
            iter::once(executed_transaction),
        )?;
        write_batch =
            write_batch.delete_batch(&self.tables.next_shared_object_versions, deleted_objects)?;
        write_batch.write()?;
        Ok(())
    }

    pub fn set_assigned_shared_object_versions(
        &self,
        transaction_digest: &TransactionDigest,
        assigned_versions: &Vec<(ObjectID, SequenceNumber)>,
    ) -> SuiResult {
        self.tables
            .assigned_shared_object_versions
            .insert(transaction_digest, assigned_versions)?;
        Ok(())
    }

    pub fn insert_checkpoint_boundary(&self, index: u64, height: u64) -> SuiResult {
        self.tables.checkpoint_boundary.insert(&index, &height)?;
        Ok(())
    }

    pub fn insert_pending_consensus_transactions(
        &self,
        transaction: &ConsensusTransaction,
    ) -> SuiResult {
        self.tables
            .pending_consensus_transactions
            .insert(&transaction.key(), transaction)?;
        if let ConsensusTransactionKind::UserTransaction(cert) = &transaction.kind {
            self.pending_consensus_certificates
                .lock()
                .insert(*cert.digest());
        }
        Ok(())
    }

    pub fn remove_pending_consensus_transaction(&self, key: &ConsensusTransactionKey) -> SuiResult {
        self.tables.pending_consensus_transactions.remove(key)?;
        if let ConsensusTransactionKey::Certificate(cert) = key {
            self.pending_consensus_certificates
                .lock()
                .remove(cert.as_ref());
        }
        Ok(())
    }

    pub fn pending_consensus_certificates_empty(&self) -> bool {
        self.pending_consensus_certificates.lock().is_empty()
    }

    /// Stores a list of pending certificates to be executed.
    pub fn insert_pending_certificates(
        &self,
        certs: &[VerifiedCertificate],
    ) -> Result<(), TypedStoreError> {
        let batch = self.tables.pending_certificates.batch().insert_batch(
            &self.tables.pending_certificates,
            certs
                .iter()
                .map(|cert| (*cert.digest(), cert.clone().serializable())),
        )?;
        batch.write()?;
        Ok(())
    }

    pub fn is_consensus_message_processed(&self, key: &ConsensusTransactionKey) -> SuiResult<bool> {
        Ok(self.tables.consensus_message_processed.contains_key(key)?)
    }

    pub async fn consensus_message_processed_notify(
        &self,
        key: ConsensusTransactionKey,
    ) -> Result<(), SuiError> {
        let registration = self.consensus_notify_read.register_one(&key);
        if self.is_consensus_message_processed(&key)? {
            return Ok(());
        }
        registration.await;
        Ok(())
    }

    pub fn has_sent_end_of_publish(&self, authority: &AuthorityName) -> SuiResult<bool> {
        Ok(self
            .end_of_publish
            .try_lock()
            .expect("No contention on end_of_publish lock")
            .contains_key(authority))
    }

    /// Returns Ok(true) if 2f+1 end of publish messages were recorded at this point
    pub fn record_end_of_publish(
        &self,
        authority: AuthorityName,
        key: ConsensusTransactionKey,
        consensus_index: ExecutionIndicesWithHash,
    ) -> SuiResult {
        let mut write_batch = self.tables.last_consensus_index.batch();
        // It is ok to just release lock here as this function is the only place that transition into RejectAllCerts state
        // And this function itself is always executed from consensus task
        let collected_end_of_publish = if self
            .get_reconfig_state_read_lock_guard()
            .should_accept_consensus_certs()
        {
            write_batch =
                write_batch.insert_batch(&self.tables.end_of_publish, [(authority, ())])?;
            self.end_of_publish.try_lock()
                .expect("No contention on Authority::end_of_publish as it is only accessed from consensus handler")
                .insert(authority, ()).is_quorum_reached()
        } else {
            // If we past the stage where we are accepting consensus certificates we also don't record end of publish messages
            debug!("Ignoring end of publish message from validator {:?} as we already collected enough end of publish messages", authority.concise());
            false
        };
        let _lock = if collected_end_of_publish {
            debug!(
                "Collected enough end_of_publish messages with last message from validator {:?}",
                authority.concise()
            );
            let mut lock = self.get_reconfig_state_write_lock_guard();
            lock.close_all_certs();
            // We store reconfig_state and end_of_publish in same batch to avoid dealing with inconsistency here on restart
            write_batch = self.store_reconfig_state_batch(&lock, write_batch)?;
            write_batch = write_batch.insert_batch(
                &self.tables.final_epoch_checkpoint,
                [(
                    &FINAL_EPOCH_CHECKPOINT_INDEX,
                    &consensus_index.index.last_committed_round,
                )],
            )?;
            // Holding this lock until end of this function where we write batch to DB
            Some(lock)
        } else {
            None
        };
        // Important: we actually rely here on fact that ConsensusHandler panics if it's operation returns error
        // If some day we won't panic in ConsensusHandler on error we need to figure out here how
        // to revert in-memory state of .end_of_publish and .reconfig_state when write fails
        self.finish_consensus_transaction_process_with_batch(write_batch, key, consensus_index)
    }

    pub fn finish_consensus_transaction_process(
        &self,
        key: ConsensusTransactionKey,
        consensus_index: ExecutionIndicesWithHash,
    ) -> SuiResult {
        let write_batch = self.tables.last_consensus_index.batch();
        self.finish_consensus_transaction_process_with_batch(write_batch, key, consensus_index)
    }

    pub fn finish_consensus_certificate_process(
        &self,
        key: ConsensusTransactionKey,
        certificate: &VerifiedCertificate,
        consensus_index: ExecutionIndicesWithHash,
    ) -> SuiResult {
        let write_batch = self.tables.last_consensus_index.batch();
        self.finish_consensus_certificate_process_with_batch(
            write_batch,
            key,
            certificate,
            consensus_index,
        )
    }

    pub fn finish_assign_shared_object_versions(
        &self,
        key: ConsensusTransactionKey,
        certificate: &VerifiedCertificate,
        consensus_index: ExecutionIndicesWithHash,
        assigned_versions: Vec<(ObjectID, SequenceNumber)>,
        next_versions: Vec<(ObjectID, SequenceNumber)>,
    ) -> SuiResult {
        // Atomically store all elements.
        // TODO: clear the shared object locks per transaction after ensuring consistency.
        let mut write_batch = self.tables.assigned_shared_object_versions.batch();

        write_batch = write_batch.insert_batch(
            &self.tables.assigned_shared_object_versions,
            iter::once((certificate.digest(), assigned_versions)),
        )?;

        write_batch =
            write_batch.insert_batch(&self.tables.next_shared_object_versions, next_versions)?;

        self.finish_consensus_certificate_process_with_batch(
            write_batch,
            key,
            certificate,
            consensus_index,
        )
    }

    /// When we finish processing certificate from consensus we record this information.
    /// Tables updated:
    ///  * consensus_message_processed - indicate that this certificate was processed by consensus
    ///  * last_consensus_index - records last processed position in consensus stream
    ///  * consensus_message_order - records at what position this transaction was first seen in consensus
    /// Self::consensus_message_processed returns true after this call for given certificate
    fn finish_consensus_transaction_process_with_batch(
        &self,
        batch: DBBatch,
        key: ConsensusTransactionKey,
        consensus_index: ExecutionIndicesWithHash,
    ) -> SuiResult {
        let batch = batch.insert_batch(
            &self.tables.last_consensus_index,
            [(LAST_CONSENSUS_INDEX_ADDR, consensus_index)],
        )?;
        let batch = batch.insert_batch(&self.tables.consensus_message_processed, [(key, true)])?;
        batch.write()?;
        self.consensus_notify_read.notify(&key, &());
        Ok(())
    }

    fn finish_consensus_certificate_process_with_batch(
        &self,
        batch: DBBatch,
        key: ConsensusTransactionKey,
        certificate: &VerifiedCertificate,
        consensus_index: ExecutionIndicesWithHash,
    ) -> SuiResult {
        let transaction_digest = *certificate.digest();
        let batch = batch.insert_batch(
            &self.tables.consensus_message_order,
            [(consensus_index.index, transaction_digest)],
        )?;
        let batch = batch.insert_batch(
            &self.tables.pending_certificates,
            [(*certificate.digest(), certificate.clone().serializable())],
        )?;
        self.finish_consensus_transaction_process_with_batch(batch, key, consensus_index)
    }

    pub fn final_epoch_checkpoint(&self) -> SuiResult<Option<u64>> {
        Ok(self
            .tables
            .final_epoch_checkpoint
            .get(&FINAL_EPOCH_CHECKPOINT_INDEX)?)
    }

    pub fn get_reconfig_state_read_lock_guard(
        &self,
    ) -> parking_lot::RwLockReadGuard<ReconfigState> {
        self.reconfig_state_mem.read()
    }

    pub fn get_reconfig_state_write_lock_guard(
        &self,
    ) -> parking_lot::RwLockWriteGuard<ReconfigState> {
        self.reconfig_state_mem.write()
    }

    // This method can only be called from ConsensusAdapter::begin_reconfiguration
    pub fn close_user_certs(
        &self,
        mut lock_guard: parking_lot::RwLockWriteGuard<'_, ReconfigState>,
    ) {
        lock_guard.close_user_certs();
        self.store_reconfig_state(&lock_guard)
            .expect("Updating reconfig state cannot fail");
    }

    /// Notify epoch is terminated, can only be called once on epoch store
    pub async fn epoch_terminated(&self) {
        // Notify interested tasks that epoch has ended
        self.epoch_alive_notify
            .notify()
            .expect("epoch_terminated called twice on same epoch store");
        // This `write` acts as a barrier - it waits for futures executing in
        // `within_alive_epoch` to terminate before we can continue here
        debug!("Epoch terminated - waiting for pending tasks to complete");
        *self.epoch_alive.write().await = false;
        debug!("All pending epoch tasks completed");
    }

    /// Waits for the notification about epoch termination
    pub async fn wait_epoch_terminated(&self) {
        self.epoch_alive_notify.wait().await
    }

    /// This function executes given future until epoch_terminated is called
    /// If future finishes before epoch_terminated is called, future result is returned
    /// If epoch_terminated is called before future is resolved, error is returned
    ///
    /// In addition to the early termination guarantee, this function also prevents epoch_terminated()
    /// if future is being executed.
    #[allow(clippy::result_unit_err)]
    pub async fn within_alive_epoch<F: Future + Send>(&self, f: F) -> Result<F::Output, ()> {
        // This guard is kept in the future until it resolves, preventing `epoch_terminated` to
        // acquire a write lock
        let guard = self.epoch_alive.read().await;
        if !*guard {
            return Err(());
        }
        let terminated = self.wait_epoch_terminated().boxed();
        let f = f.boxed();
        match select(terminated, f).await {
            Either::Left((_, _f)) => Err(()),
            Either::Right((result, _)) => Ok(result),
        }
    }
}

fn transactions_table_default_config() -> DBOptions {
    default_db_options(None, None).1
}
