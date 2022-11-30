// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use narwhal_executor::ExecutionIndices;
use rocksdb::Options;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use sui_storage::default_db_options;
use sui_types::base_types::{AuthorityName, EpochId, ObjectID, SequenceNumber, TransactionDigest};
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{
    ConsensusTransaction, ConsensusTransactionKey, SenderSignedData, TrustedCertificate,
    VerifiedCertificate,
};
use typed_store::rocks::{DBBatch, DBMap, DBOptions, TypedStoreError};
use typed_store::traits::TypedStoreDebug;

use crate::authority::authority_notify_read::{NotifyRead, Registration};
use sui_types::message_envelope::{TrustedEnvelope, VerifiedEnvelope};
use typed_store::Map;
use typed_store_derive::DBMapUtils;

/// The key where the latest consensus index is stored in the database.
// TODO: Make a single table (e.g., called `variables`) storing all our lonely variables in one place.
const LAST_CONSENSUS_INDEX_ADDR: u64 = 0;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionIndicesWithHash {
    pub index: ExecutionIndices,
    pub hash: u64,
}

pub struct AuthorityPerEpochStore<S> {
    #[allow(dead_code)]
    epoch_id: EpochId,
    tables: AuthorityEpochTables<S>,
    consensus_notify_read: NotifyRead<ConsensusTransactionKey, ()>,
}

/// AuthorityEpochTables contains tables that contain data that is only valid within an epoch.
#[derive(DBMapUtils)]
pub struct AuthorityEpochTables<S> {
    /// This is map between the transaction digest and transactions found in the `transaction_lock`.
    #[default_options_override_fn = "transactions_table_default_config"]
    transactions: DBMap<TransactionDigest, TrustedEnvelope<SenderSignedData, S>>,

    /// Hold the lock for shared objects. These locks are written by a single task: upon receiving a valid
    /// certified transaction from consensus, the authority assigns a lock to each shared objects of the
    /// transaction. Note that all authorities are guaranteed to assign the same lock to these objects.
    /// TODO: These two maps should be merged into a single one (no reason to have two).
    assigned_object_versions: DBMap<(TransactionDigest, ObjectID), SequenceNumber>,
    next_object_versions: DBMap<ObjectID, SequenceNumber>,

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
    /// sure to advance next_object_versions exactly once for each transaction we receive from
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

    /// Validators that have sent EndOfPublish message in this epoch
    end_of_publish: DBMap<AuthorityName, ()>,
}

impl<S> AuthorityEpochTables<S>
where
    S: std::fmt::Debug + Serialize + for<'de> Deserialize<'de>,
{
    pub fn open(epoch: EpochId, parent_path: &Path, db_options: Option<Options>) -> Self {
        Self::open_tables_read_write(
            AuthorityEpochTables::<S>::path(epoch, parent_path),
            db_options,
            None,
        )
    }

    pub fn open_readonly(epoch: EpochId, parent_path: &Path) -> AuthorityEpochTablesReadOnly<S> {
        Self::get_read_only_handle(Self::path(epoch, parent_path), None, None)
    }

    pub fn path(epoch: EpochId, parent_path: &Path) -> PathBuf {
        parent_path.join(format!("epoch_{}", epoch))
    }
}

impl<S> AuthorityPerEpochStore<S>
where
    S: std::fmt::Debug + Serialize + for<'de> Deserialize<'de>,
{
    pub fn new(epoch: EpochId, parent_path: &Path, db_options: Option<Options>) -> Self {
        Self {
            epoch_id: epoch,
            tables: AuthorityEpochTables::open(epoch, parent_path, db_options),
            consensus_notify_read: NotifyRead::new(),
        }
    }

    pub fn insert_transaction(
        &self,
        transaction: VerifiedEnvelope<SenderSignedData, S>,
    ) -> SuiResult {
        Ok(self
            .tables
            .transactions
            .insert(transaction.digest(), transaction.serializable_ref())?)
    }

    pub fn get_transaction(
        &self,
        tx_digest: &TransactionDigest,
    ) -> SuiResult<Option<VerifiedEnvelope<SenderSignedData, S>>> {
        Ok(self.tables.transactions.get(tx_digest)?.map(|t| t.into()))
    }

    pub fn multi_get_next_object_versions<'a>(
        &self,
        ids: impl Iterator<Item = &'a ObjectID>,
    ) -> SuiResult<Vec<Option<SequenceNumber>>> {
        Ok(self.tables.next_object_versions.multi_get(ids)?)
    }

    pub fn get_last_checkpoint_boundary(&self) -> Option<(u64, u64)> {
        self.tables.checkpoint_boundary.iter().skip_to_last().next()
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
        from_height_excluded: u64,
        to_height_included: u64,
    ) -> SuiResult<Vec<TransactionDigest>> {
        let iter = self.tables.consensus_message_order.iter();
        let last_previous = ExecutionIndices::end_for_commit(from_height_excluded);
        let iter = iter.skip_to(&last_previous)?;
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

    /// Gets all pending certificates. Used during recovery.
    pub fn all_pending_certificates(&self) -> SuiResult<Vec<VerifiedCertificate>> {
        Ok(self
            .tables
            .pending_certificates
            .iter()
            .map(|(_, cert)| cert.into())
            .collect())
    }

    /// Checks if a certificate is in the pending queue.
    pub fn pending_certificate_exists(&self, tx: &TransactionDigest) -> Result<bool, SuiError> {
        Ok(self.tables.pending_certificates.contains_key(tx)?)
    }

    /// Deletes one pending certificate.
    pub fn remove_pending_certificate(&self, digest: &TransactionDigest) -> SuiResult<()> {
        self.tables.pending_certificates.remove(digest)?;
        Ok(())
    }

    pub fn get_all_pending_consensus_transactions(&self) -> Vec<ConsensusTransaction> {
        self.tables
            .pending_consensus_transactions
            .iter()
            .map(|(_k, v)| v)
            .collect()
    }

    /// Read a lock for a specific (transaction, shared object) pair.
    pub fn get_all_shared_locks(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> Result<Vec<(ObjectID, SequenceNumber)>, SuiError> {
        Ok(self
            .tables
            .assigned_object_versions
            .iter()
            .skip_to(&(*transaction_digest, ObjectID::ZERO))?
            .take_while(|((tx, _objid), _ver)| tx == transaction_digest)
            .map(|((_tx, objid), ver)| (objid, ver))
            .collect())
    }

    #[cfg(test)]
    pub fn get_next_object_version(&self, obj: &ObjectID) -> Option<SequenceNumber> {
        self.tables.next_object_versions.get(obj).unwrap()
    }

    /// Read a lock for a specific (transaction, shared object) pair.
    #[cfg(test)] // Nothing wrong with this function, but it is not currently used outside of tests
    pub fn get_assigned_object_versions<'a>(
        &self,
        transaction_digest: &TransactionDigest,
        object_ids: impl Iterator<Item = &'a ObjectID>,
    ) -> Result<Vec<Option<SequenceNumber>>, SuiError> {
        let keys = object_ids.map(|objid| (*transaction_digest, *objid));

        self.tables
            .assigned_object_versions
            .multi_get(keys)
            .map_err(SuiError::from)
    }

    pub fn remove_shared_objects_locks(
        &self,
        sequenced_to_delete: &[(TransactionDigest, ObjectID)],
        schedule_to_delete: &[ObjectID],
    ) -> SuiResult {
        let mut write_batch = self.tables.assigned_object_versions.batch();
        write_batch =
            write_batch.delete_batch(&self.tables.assigned_object_versions, sequenced_to_delete)?;
        write_batch =
            write_batch.delete_batch(&self.tables.next_object_versions, schedule_to_delete)?;
        write_batch.write()?;
        Ok(())
    }

    pub fn insert_assigned_shared_object_versions(
        &self,
        sequenced: Vec<((TransactionDigest, ObjectID), SequenceNumber)>,
    ) -> SuiResult {
        let mut write_batch = self.tables.assigned_object_versions.batch();
        write_batch = write_batch.insert_batch(&self.tables.assigned_object_versions, sequenced)?;
        write_batch.write()?;

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
        Ok(())
    }

    pub fn remove_pending_consensus_transaction(&self, key: &ConsensusTransactionKey) -> SuiResult {
        self.tables.pending_consensus_transactions.remove(key)?;
        Ok(())
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

    pub fn has_sent_end_of_publish(&self, authority: &AuthorityName) -> SuiResult<bool> {
        Ok(self.tables.end_of_publish.contains_key(authority)?)
    }

    pub fn register_consensus_message_notify(
        &self,
        key: &ConsensusTransactionKey,
    ) -> Registration<ConsensusTransactionKey, ()> {
        self.consensus_notify_read.register_one(key)
    }

    pub fn record_end_of_publish(
        &self,
        authority: AuthorityName,
        key: ConsensusTransactionKey,
        consensus_index: ExecutionIndicesWithHash,
    ) -> SuiResult {
        let write_batch = self.tables.last_consensus_index.batch();
        let write_batch =
            write_batch.insert_batch(&self.tables.end_of_publish, [(authority, ())])?;
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
        sequenced_to_write: Vec<((TransactionDigest, ObjectID), SequenceNumber)>,
        schedule_to_write: Vec<(ObjectID, SequenceNumber)>,
    ) -> SuiResult {
        // Atomically store all elements.
        // TODO: clear the shared object locks per transaction after ensuring consistency.
        let mut write_batch = self.tables.assigned_object_versions.batch();

        write_batch =
            write_batch.insert_batch(&self.tables.assigned_object_versions, sequenced_to_write)?;

        write_batch =
            write_batch.insert_batch(&self.tables.next_object_versions, schedule_to_write)?;

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
            [(consensus_index.index.clone(), transaction_digest)],
        )?;
        let batch = batch.insert_batch(
            &self.tables.pending_certificates,
            [(*certificate.digest(), certificate.clone().serializable())],
        )?;
        self.finish_consensus_transaction_process_with_batch(batch, key, consensus_index)
    }
}

fn transactions_table_default_config() -> DBOptions {
    default_db_options(None, None).1
}
