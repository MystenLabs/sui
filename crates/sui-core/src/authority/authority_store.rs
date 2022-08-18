// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{authority_store_tables::AuthorityStoreTables, *};
use crate::gateway_state::GatewayTxSeqNumber;
use narwhal_executor::ExecutionIndices;
use rocksdb::Options;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::iter;
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::{fmt::Debug, path::PathBuf};
use sui_storage::{
    mutex_table::{LockGuard, MutexTable},
    write_ahead_log::{DBWriteAheadLog, WriteAheadLog},
    LockService,
};
use sui_types::batch::{SignedBatch, TxSequenceNumber};
use sui_types::crypto::{AuthoritySignInfo, AuthoritySignInfoTrait, EmptySignInfo};
use sui_types::object::{Owner, OBJECT_START_VERSION};
use sui_types::{base_types::SequenceNumber, storage::ParentSync};
use tokio::sync::Notify;
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tracing::{debug, error, info, trace};
use typed_store::rocks::{DBBatch, DBMap};
use typed_store::traits::{DBMapTableUtil, Map};

pub type AuthorityStore = SuiDataStore<AuthoritySignInfo>;
pub type GatewayStore = SuiDataStore<EmptySignInfo>;

pub type InternalSequenceNumber = u64;

pub struct CertLockGuard(LockGuard);

const NUM_SHARDS: usize = 4096;
const SHARD_SIZE: usize = 128;

/// The key where the latest consensus index is stored in the database.
// TODO: Make a single table (e.g., called `variables`) storing all our lonely variables in one place.
const LAST_CONSENSUS_INDEX_ADDR: u64 = 0;

/// ALL_OBJ_VER determines whether we want to store all past
/// versions of every object in the store. Authority doesn't store
/// them, but other entities such as replicas will.
/// S is a template on Authority signature state. This allows SuiDataStore to be used on either
/// authorities or non-authorities. Specifically, when storing transactions and effects,
/// S allows SuiDataStore to either store the authority signed version or unsigned version.
pub struct SuiDataStore<S>
where
    S: AuthoritySignInfoTrait,
{
    /// A write-ahead/recovery log used to ensure we finish fully processing certs after errors or
    /// crashes.
    pub wal: Arc<DBWriteAheadLog<CertifiedTransaction>>,

    /// The LockService this store depends on for locking functionality
    lock_service: LockService,

    /// Internal vector of locks to manage concurrent writes to the database
    mutex_table: MutexTable<ObjectDigest>,

    // The next sequence number.
    next_pending_seq: AtomicU64,
    // A notifier for new pending certificates
    pending_notifier: Arc<Notify>,

    pub(crate) tables: AuthorityStoreTables<S>,
}

impl<S: Eq + Debug + Serialize + for<'de> Deserialize<'de> + AuthoritySignInfoTrait>
    SuiDataStore<S>
{
    /// Open an authority store by directory path
    pub fn open(path: &Path, db_options: Option<Options>) -> Self {
        let tables =
            AuthorityStoreTables::open_tables_read_write(path.to_path_buf(), db_options, None);

        // For now, create one LockService for each SuiDataStore, and we use a specific
        // subdir of the data store directory
        let lockdb_path: PathBuf = path.join("lockdb");
        let lock_service =
            LockService::new(lockdb_path, None).expect("Could not initialize lockdb");

        let wal_path = path.join("recovery_log");
        let wal = Arc::new(DBWriteAheadLog::new(wal_path));

        // Get the last sequence item
        let pending_seq = tables
            .pending_execution
            .iter()
            .skip_to_last()
            .next()
            .map(|(seq, _)| seq + 1)
            .unwrap_or(0);
        let next_pending_seq = AtomicU64::new(pending_seq);

        Self {
            wal,
            lock_service,
            mutex_table: MutexTable::new(NUM_SHARDS, SHARD_SIZE),
            next_pending_seq,
            pending_notifier: Arc::new(Notify::new()),
            tables,
        }
    }

    pub async fn acquire_tx_guard<'a, 'b>(
        &'a self,
        cert: &'b CertifiedTransaction,
    ) -> SuiResult<CertTxGuard<'a>> {
        let digest = cert.digest();
        let guard = self.wal.begin_tx(digest, cert).await?;

        if guard.retry_num() > MAX_TX_RECOVERY_RETRY {
            // If the tx has been retried too many times, it could be a poison pill, and we should
            // prevent the client from continually retrying it.
            let err = "tx has exceeded the maximum retry limit for transient errors".to_owned();
            debug!(?digest, "{}", err);
            return Err(SuiError::ErrorWhileProcessingConfirmationTransaction { err });
        }

        Ok(guard)
    }

    /// Acquire the lock for a tx without writing to the WAL.
    pub async fn acquire_tx_lock<'a, 'b>(&'a self, digest: &'b TransactionDigest) -> CertLockGuard {
        CertLockGuard(self.wal.acquire_lock(digest).await)
    }

    // TODO: Async retry method, using tokio-retry crate.

    /// Await a new pending certificate to be added
    pub async fn wait_for_new_pending(&self) {
        self.pending_notifier.notified().await
    }

    /// Returns the TransactionEffects if we have an effects structure for this transaction digest
    pub fn get_effects(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> SuiResult<TransactionEffects> {
        self.tables
            .effects
            .get(transaction_digest)?
            .map(|data| data.effects().clone())
            .ok_or(SuiError::TransactionNotFound {
                digest: *transaction_digest,
            })
    }

    /// Returns true if we have an effects structure for this transaction digest
    pub fn effects_exists(&self, transaction_digest: &TransactionDigest) -> SuiResult<bool> {
        self.tables
            .effects
            .contains_key(transaction_digest)
            .map_err(|e| e.into())
    }

    /// Returns true if we have a transaction structure for this transaction digest
    pub fn transaction_exists(&self, transaction_digest: &TransactionDigest) -> SuiResult<bool> {
        self.tables
            .transactions
            .contains_key(transaction_digest)
            .map_err(|e| e.into())
    }

    /// Returns true if there are no objects in the database
    pub fn database_is_empty(&self) -> SuiResult<bool> {
        Ok(self
            .tables
            .objects
            .iter()
            .skip_to(&ObjectKey::ZERO)?
            .next()
            .is_none())
    }

    pub fn next_sequence_number(&self) -> Result<TxSequenceNumber, SuiError> {
        Ok(self
            .tables
            .executed_sequence
            .iter()
            .skip_prior_to(&TxSequenceNumber::MAX)?
            .next()
            .map(|(v, _)| v + 1u64)
            .unwrap_or(0))
    }

    #[cfg(test)]
    pub fn side_sequence(&self, seq: TxSequenceNumber, digest: &ExecutionDigests) {
        self.tables.executed_sequence.insert(&seq, digest).unwrap();
    }

    #[cfg(test)]
    pub fn get_next_object_version(&self, obj: &ObjectID) -> Option<SequenceNumber> {
        self.tables.next_object_versions.get(obj).unwrap()
    }

    /// Add a number of certificates to the pending transactions as well as the
    /// certificates structure if they are not already executed.
    /// Certificates are optional, and if not provided, they will be eventually
    /// downloaded in the execution driver.
    ///
    /// This function may be run concurrently: it increases atomically an internal index
    /// by the number of certificates passed, and then records the certificates and their
    /// index. If two instanced run concurrently, the indexes are guaranteed to not overlap
    /// although some certificates may be included twice in the `pending_execution`, and
    /// the same certificate may be written twice (but that is OK since it is valid.)
    pub fn add_pending_certificates(
        &self,
        certs: Vec<(TransactionDigest, Option<CertifiedTransaction>)>,
    ) -> SuiResult<()> {
        let first_index = self
            .next_pending_seq
            .fetch_add(certs.len() as u64, Ordering::Relaxed);

        let batch = self.tables.pending_execution.batch();
        let batch = batch.insert_batch(
            &self.tables.pending_execution,
            certs
                .iter()
                .enumerate()
                .map(|(num, (digest, _))| ((num as u64) + first_index, digest)),
        )?;
        let batch = batch.insert_batch(
            &self.tables.certificates,
            certs
                .into_iter()
                .filter_map(|(digest, cert_opt)| cert_opt.map(|cert| (digest, cert))),
        )?;
        batch.write()?;

        // now notify there is a pending certificate
        self.pending_notifier.notify_one();

        Ok(())
    }

    /// Get all stored certificate digests
    pub fn get_pending_digests(
        &self,
    ) -> SuiResult<Vec<(InternalSequenceNumber, TransactionDigest)>> {
        Ok(self.tables.pending_execution.iter().collect())
    }

    /// Remove entries from pending certificates
    pub fn remove_pending_certificates(&self, seqs: Vec<InternalSequenceNumber>) -> SuiResult<()> {
        let batch = self.tables.pending_execution.batch();
        let batch = batch.delete_batch(&self.tables.pending_execution, seqs.iter())?;
        batch.write()?;
        Ok(())
    }

    // Empty the pending_execution table, and remove the certs from the certificates table.
    pub fn remove_all_pending_certificates(&self) -> SuiResult {
        let all_pending_tx = self.get_pending_digests()?;
        let mut batch = self.tables.pending_execution.batch();
        batch = batch.delete_batch(
            &self.tables.certificates,
            all_pending_tx.iter().map(|(_, digest)| digest),
        )?;
        batch.write()?;
        self.tables.pending_execution.clear()?;

        Ok(())
    }

    /// A function that acquires all locks associated with the objects (in order to avoid deadlocks).
    async fn acquire_locks<'a, 'b>(&'a self, input_objects: &'b [ObjectRef]) -> Vec<LockGuard> {
        self.mutex_table
            .acquire_locks(input_objects.iter().map(|(_, _, digest)| *digest))
            .await
    }

    // Methods to read the store
    pub fn get_owner_objects(&self, owner: Owner) -> Result<Vec<ObjectInfo>, SuiError> {
        debug!(?owner, "get_owner_objects");
        Ok(self
            .tables
            .owner_index
            .iter()
            // The object id 0 is the smallest possible
            .skip_to(&(owner, ObjectID::ZERO))?
            .take_while(|((object_owner, _), _)| (object_owner == &owner))
            .map(|(_, object_info)| object_info)
            .collect())
    }

    pub fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        Ok(self.tables.objects.get(&ObjectKey(*object_id, version))?)
    }

    /// Read an object and return it, or Err(ObjectNotFound) if the object was not found.
    pub fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        let obj_entry = self
            .tables
            .objects
            .iter()
            .skip_prior_to(&ObjectKey::max_for_id(object_id))?
            .next();

        let obj = match obj_entry {
            Some((ObjectKey(obj_id, _), obj)) if obj_id == *object_id => obj,
            _ => return Ok(None),
        };

        // Note that the two reads in this function are (obviously) not atomic, and the
        // object may be deleted after we have read it. Hence we check get_latest_parent_entry
        // last, so that the write to self.tables.parent_sync gets the last word.
        //
        // TODO: verify this race is ok.
        //
        // I believe it is - Even if the reads were atomic, calls to this function would still
        // race with object deletions (the object could be deleted between when the function is
        // called and when the first read takes place, which would be indistinguishable to the
        // caller with the case in which the object is deleted in between the two reads).
        let parent_entry = self.get_latest_parent_entry(*object_id)?;

        match parent_entry {
            None => {
                error!(
                    ?object_id,
                    "Object is missing parent_sync entry, data store is inconsistent"
                );
                Ok(None)
            }
            Some((obj_ref, _)) if obj_ref.2.is_alive() => Ok(Some(obj)),
            _ => Ok(None),
        }
    }

    /// Get many objects
    pub fn get_objects(&self, objects: &[ObjectID]) -> Result<Vec<Option<Object>>, SuiError> {
        let mut result = Vec::new();
        for id in objects {
            result.push(self.get_object(id)?);
        }
        Ok(result)
    }

    /// Get many objects by their (id, version number) key.
    pub fn get_input_objects(
        &self,
        objects: &[InputObjectKind],
    ) -> Result<Vec<Option<Object>>, SuiError> {
        let mut result = Vec::new();
        for kind in objects {
            let obj = match kind {
                InputObjectKind::MovePackage(id) | InputObjectKind::SharedMoveObject(id) => {
                    self.get_object(id)?
                }
                InputObjectKind::ImmOrOwnedMoveObject(objref) => {
                    self.get_object_by_key(&objref.0, objref.1)?
                }
            };
            result.push(obj);
        }
        Ok(result)
    }

    /// Read a transaction envelope via lock or returns Err(TransactionLockDoesNotExist) if the lock does not exist.
    pub async fn get_transaction_envelope(
        &self,
        object_ref: &ObjectRef,
    ) -> SuiResult<Option<TransactionEnvelope<S>>> {
        let transaction_option = self
            .lock_service
            .get_lock(*object_ref)
            .await?
            .ok_or(SuiError::TransactionLockDoesNotExist)?;

        // Returns None if either no TX with the lock, or TX present but no entry in transactions table.
        // However we retry a couple times because the TX is written after the lock is acquired, so it might
        // just be a race.
        match transaction_option {
            Some(tx_digest) => {
                let mut retry_strategy = ExponentialBackoff::from_millis(2)
                    .factor(10)
                    .map(jitter)
                    .take(3);
                let mut tx_option = self.tables.transactions.get(&tx_digest)?;
                while tx_option.is_none() {
                    if let Some(duration) = retry_strategy.next() {
                        // Wait to retry
                        tokio::time::sleep(duration).await;
                        trace!(?tx_digest, "Retrying getting pending transaction");
                    } else {
                        // No more retries, just quit
                        break;
                    }
                    tx_option = self.tables.transactions.get(&tx_digest)?;
                }
                Ok(tx_option)
            }
            None => Ok(None),
        }
    }

    /// Read a certificate and return an option with None if it does not exist.
    pub fn read_certificate(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<CertifiedTransaction>, SuiError> {
        self.tables.certificates.get(digest).map_err(|e| e.into())
    }

    /// Read the transactionDigest that is the parent of an object reference
    /// (ie. the transaction that created an object at this version.)
    pub fn parent(&self, object_ref: &ObjectRef) -> Result<Option<TransactionDigest>, SuiError> {
        self.tables
            .parent_sync
            .get(object_ref)
            .map_err(|e| e.into())
    }

    /// Batch version of `parent` function.
    pub fn multi_get_parents(
        &self,
        object_refs: &[ObjectRef],
    ) -> Result<Vec<Option<TransactionDigest>>, SuiError> {
        self.tables
            .parent_sync
            .multi_get(object_refs)
            .map_err(|e| e.into())
    }

    /// Returns all parents (object_ref and transaction digests) that match an object_id, at
    /// any object version, or optionally at a specific version.
    pub fn get_parent_iterator(
        &self,
        object_id: ObjectID,
        seq: Option<SequenceNumber>,
    ) -> Result<impl Iterator<Item = (ObjectRef, TransactionDigest)> + '_, SuiError> {
        let seq_inner = seq.unwrap_or_else(|| SequenceNumber::from(0));
        let obj_dig_inner = ObjectDigest::new([0; 32]);

        Ok(self
            .tables
            .parent_sync
            .iter()
            // The object id [0; 16] is the smallest possible
            .skip_to(&(object_id, seq_inner, obj_dig_inner))?
            .take_while(move |((id, iseq, _digest), _txd)| {
                let mut flag = id == &object_id;
                if let Some(seq_num) = seq {
                    flag &= seq_num == *iseq;
                }
                flag
            }))
    }

    /// Read a lock for a specific (transaction, shared object) pair.
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

    /// Read a lock for a specific (transaction, shared object) pair.
    pub fn all_shared_locks(
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

    // Methods to mutate the store

    /// Insert a genesis object.
    pub async fn insert_genesis_object(&self, object: Object) -> SuiResult {
        // We only side load objects with a genesis parent transaction.
        debug_assert!(object.previous_transaction == TransactionDigest::genesis());
        let object_ref = object.compute_object_reference();
        self.insert_object_direct(object_ref, &object).await
    }

    /// Insert an object directly into the store, and also update relevant tables
    /// NOTE: does not handle transaction lock.
    /// This is used by the gateway to insert object directly.
    /// TODO: We need this today because we don't have another way to sync an account.
    pub async fn insert_object_direct(&self, object_ref: ObjectRef, object: &Object) -> SuiResult {
        // Insert object
        self.tables.objects.insert(&object_ref.into(), object)?;

        // Update the index
        if object.get_single_owner().is_some() {
            self.tables.owner_index.insert(
                &(object.owner, object_ref.0),
                &ObjectInfo::new(&object_ref, object),
            )?;
            // Only initialize lock for owned objects.
            // TODO: Skip this for quasi-shared objects.
            self.lock_service
                .initialize_locks(&[object_ref], false /* is_force_reset */)
                .await?;
        }

        // Update the parent
        self.tables
            .parent_sync
            .insert(&object_ref, &object.previous_transaction)?;

        Ok(())
    }

    /// This function is used by the bench.rs script, and should not be used in other contexts
    /// In particular it does not check the old locks before inserting new ones, so the objects
    /// must be new.
    pub async fn bulk_object_insert(&self, objects: &[&Object]) -> SuiResult<()> {
        let batch = self.tables.objects.batch();
        let ref_and_objects: Vec<_> = objects
            .iter()
            .map(|o| (o.compute_object_reference(), o))
            .collect();

        batch
            .insert_batch(
                &self.tables.objects,
                ref_and_objects
                    .iter()
                    .map(|(oref, o)| (ObjectKey::from(oref), **o)),
            )?
            .insert_batch(
                &self.tables.owner_index,
                ref_and_objects
                    .iter()
                    .map(|(oref, o)| ((o.owner, oref.0), ObjectInfo::new(oref, o))),
            )?
            .insert_batch(
                &self.tables.parent_sync,
                ref_and_objects
                    .iter()
                    .map(|(oref, o)| (oref, o.previous_transaction)),
            )?
            .write()?;

        let refs: Vec<_> = ref_and_objects.iter().map(|(oref, _)| *oref).collect();
        self.lock_service
            .initialize_locks(&refs, false /* is_force_reset */)
            .await?;

        Ok(())
    }

    /// Acquires the transaction lock for a specific transaction, writing the transaction
    /// to the transaction column family if acquiring the lock succeeds.
    /// The lock service is used to atomically acquire locks.
    pub async fn lock_and_write_transaction(
        &self,
        owned_input_objects: &[ObjectRef],
        transaction: TransactionEnvelope<S>,
    ) -> Result<(), SuiError> {
        let tx_digest = *transaction.digest();

        // Acquire the lock on input objects
        self.lock_service
            .acquire_locks(owned_input_objects.to_owned(), tx_digest)
            .await?;

        // TODO: we should have transaction insertion be atomic with lock acquisition, or retry.
        // For now write transactions after because if we write before, there is a chance the lock can fail
        // and this can cause invalid transactions to be inserted in the table.
        // https://github.com/MystenLabs/sui/issues/1990
        self.tables.transactions.insert(&tx_digest, &transaction)?;

        Ok(())
    }

    /// This function should only be used by the gateway.
    /// It's called when we could not get a transaction to successfully execute,
    /// and have to roll back.
    pub async fn reset_transaction_lock(&self, owned_input_objects: &[ObjectRef]) -> SuiResult {
        self.lock_service
            .initialize_locks(owned_input_objects, true /* is_force_reset */)
            .await?;
        Ok(())
    }

    /// Updates the state resulting from the execution of a certificate.
    ///
    /// Internally it checks that all locks for active inputs are at the correct
    /// version, and then writes locks, objects, certificates, parents atomically.
    pub async fn update_state(
        &self,
        inner_temporary_store: InnerTemporaryStore,
        certificate: &CertifiedTransaction,
        proposed_seq: TxSequenceNumber,
        effects: &TransactionEffectsEnvelope<S>,
        effects_digest: &TransactionEffectsDigest,
    ) -> SuiResult {
        // Extract the new state from the execution
        // TODO: events are already stored in the TxDigest -> TransactionEffects store. Is that enough?
        let mut write_batch = self.tables.certificates.batch();

        // Store the certificate indexed by transaction digest
        let transaction_digest: &TransactionDigest = certificate.digest();
        write_batch = write_batch.insert_batch(
            &self.tables.certificates,
            std::iter::once((transaction_digest, certificate)),
        )?;

        self.sequence_tx(
            write_batch,
            inner_temporary_store,
            transaction_digest,
            proposed_seq,
            effects,
            effects_digest,
        )
        .await?;

        // Cleanup the lock of the shared objects. This must be done after we write effects, as
        // effects_exists is used as the guard to avoid re-locking objects for a previously
        // executed tx. remove_shared_objects_locks.
        self.remove_shared_objects_locks(transaction_digest, certificate)
    }

    /// Persist temporary storage to DB for genesis modules
    pub async fn update_objects_state_for_genesis(
        &self,
        inner_temporary_store: InnerTemporaryStore,
        transaction_digest: TransactionDigest,
    ) -> Result<(), SuiError> {
        debug_assert_eq!(transaction_digest, TransactionDigest::genesis());
        let write_batch = self.tables.certificates.batch();
        self.batch_update_objects(
            write_batch,
            inner_temporary_store,
            transaction_digest,
            UpdateType::Genesis,
        )
        .await?;
        Ok(())
    }

    /// This is used by the Gateway to update its local store after a transaction succeeded
    /// on the authorities. Since we don't yet have local execution on the gateway, we will
    /// need to recreate the temporary store based on the inputs and effects to update it properly.
    pub async fn update_gateway_state(
        &self,
        input_objects: InputObjects,
        mutated_objects: HashMap<ObjectRef, Object>,
        certificate: CertifiedTransaction,
        proposed_seq: TxSequenceNumber,
        effects: TransactionEffectsEnvelope<S>,
        effects_digest: &TransactionEffectsDigest,
    ) -> SuiResult {
        let transaction_digest = certificate.digest();
        let mut temporary_store =
            TemporaryStore::new(Arc::new(&self), input_objects, *transaction_digest);
        for (_, object) in mutated_objects {
            temporary_store.write_object(object);
        }
        for obj_ref in &effects.effects().deleted {
            temporary_store.delete_object(&obj_ref.0, obj_ref.1, DeleteKind::Normal);
        }
        for obj_ref in &effects.effects().wrapped {
            temporary_store.delete_object(&obj_ref.0, obj_ref.1, DeleteKind::Wrap);
        }

        let mut write_batch = self.tables.certificates.batch();

        // Store the certificate indexed by transaction digest
        write_batch = write_batch.insert_batch(
            &self.tables.certificates,
            std::iter::once((transaction_digest, &certificate)),
        )?;

        let (inner_temporary_store, _events) = temporary_store.into_inner();
        self.sequence_tx(
            write_batch,
            inner_temporary_store,
            transaction_digest,
            proposed_seq,
            &effects,
            effects_digest,
        )
        .await
    }

    async fn sequence_tx(
        &self,
        write_batch: DBBatch,
        inner_temporary_store: InnerTemporaryStore,
        transaction_digest: &TransactionDigest,
        proposed_seq: TxSequenceNumber,
        effects: &TransactionEffectsEnvelope<S>,
        effects_digest: &TransactionEffectsDigest,
    ) -> SuiResult {
        // Safe to unwrap since UpdateType::Transaction ensures we get a sequence number back.
        let assigned_seq = self
            .batch_update_objects(
                write_batch,
                inner_temporary_store,
                *transaction_digest,
                UpdateType::Transaction(proposed_seq, *effects_digest),
            )
            .await?
            .unwrap();

        // Store the signed effects of the transaction
        // We can't write this until after sequencing succeeds (which happens in
        // batch_update_objects), as effects_exists is used as a check in many places
        // for "did the tx finish".
        self.tables.effects.insert(transaction_digest, effects)?;

        // Writing to executed_sequence must be done *after* writing to effects, so that we never
        // broadcast a sequenced transaction (via the batch system) for which no effects can be
        // retrieved.
        //
        // Note that this write may be done repeatedly when retrying a tx. The
        // sequence_transaction call in batch_update_objects assigns a sequence number to
        // the transaction the first time it is called and will return that same sequence
        // on subsequent calls.
        trace!(
            ?assigned_seq,
            tx_digest = ?transaction_digest,
            ?effects_digest,
            "storing sequence number to executed_sequence"
        );
        self.tables.executed_sequence.insert(
            &assigned_seq,
            &ExecutionDigests::new(*transaction_digest, *effects_digest),
        )?;

        Ok(())
    }

    /// Helper function for updating the objects in the state
    async fn batch_update_objects(
        &self,
        mut write_batch: DBBatch,
        inner_temporary_store: InnerTemporaryStore,
        transaction_digest: TransactionDigest,
        update_type: UpdateType,
    ) -> SuiResult<Option<TxSequenceNumber>> {
        let InnerTemporaryStore {
            objects,
            mutable_inputs: active_inputs,
            written,
            deleted,
        } = inner_temporary_store;
        trace!(written =? written.values().map(|((obj_id, ver, _), _)| (obj_id, ver)).collect::<Vec<_>>(),
               "batch_update_objects: temp store written");

        let owned_inputs: Vec<_> = active_inputs
            .iter()
            .filter(|(id, _, _)| objects.get(id).unwrap().is_owned_or_quasi_shared())
            .cloned()
            .collect();

        // Make an iterator over all objects that are either deleted or have changed owner,
        // along with their old owner.  This is used to update the owner index.
        // For wrapped objects, although their owners technically didn't change, we will lose track
        // of them and there is no guarantee on their owner in the future. Hence we treat them
        // the same as deleted.
        let old_object_owners = deleted
            .iter()
            // We need to call get() on objects because some object that were just deleted may not
            // be in the objects list. This can happen if these deleted objects were wrapped in the past,
            // and hence will not show up in the input objects.
            .filter_map(|(id, _)| objects.get(id).and_then(Object::get_owner_and_id))
            .chain(
                written
                    .iter()
                    .filter_map(|(id, (_, new_object))| match objects.get(id) {
                        Some(old_object) if old_object.owner != new_object.owner => {
                            old_object.get_owner_and_id()
                        }
                        _ => None,
                    }),
            );

        // Delete the old owner index entries
        write_batch = write_batch.delete_batch(&self.tables.owner_index, old_object_owners)?;

        // Index the certificate by the objects mutated
        write_batch = write_batch.insert_batch(
            &self.tables.parent_sync,
            written
                .iter()
                .map(|(_, (object_ref, _object_))| (object_ref, transaction_digest)),
        )?;

        // Index the certificate by the objects deleted
        write_batch = write_batch.insert_batch(
            &self.tables.parent_sync,
            deleted.iter().map(|(object_id, (version, kind))| {
                (
                    (
                        *object_id,
                        *version,
                        if kind == &DeleteKind::Wrap {
                            ObjectDigest::OBJECT_DIGEST_WRAPPED
                        } else {
                            ObjectDigest::OBJECT_DIGEST_DELETED
                        },
                    ),
                    transaction_digest,
                )
            }),
        )?;

        // Once a transaction is done processing and effects committed, we no longer
        // need it in the transactions table. This also allows us to track pending
        // transactions.
        write_batch = write_batch.delete_batch(
            &self.tables.transactions,
            std::iter::once(transaction_digest),
        )?;

        // Update the indexes of the objects written
        write_batch = write_batch.insert_batch(
            &self.tables.owner_index,
            written
                .iter()
                .filter_map(|(_id, (object_ref, new_object))| {
                    trace!(?object_ref, owner =? new_object.owner, "Updating owner_index");
                    new_object
                        .get_owner_and_id()
                        .map(|owner_id| (owner_id, ObjectInfo::new(object_ref, new_object)))
                }),
        )?;

        // Insert each output object into the stores
        write_batch = write_batch.insert_batch(
            &self.tables.objects,
            written
                .iter()
                .map(|(_, (obj_ref, new_object))| (ObjectKey::from(obj_ref), new_object)),
        )?;

        // Atomic write of all data other than locks
        write_batch.write()?;
        trace!("Finished writing batch");

        // Need to have a critical section for now because we need to prevent execution of older
        // certs which may overwrite newer objects with older ones.  This can be removed once we have
        // an object storage supporting multiple object versions at once, then there is idempotency and
        // old writes would be OK.
        let assigned_seq = {
            // Acquire the lock to ensure no one else writes when we are in here.
            let _mutexes = self.acquire_locks(&owned_inputs[..]).await;

            // NOTE: We just check here that locks exist, not that they are locked to a specific TX.  Why?
            // 1. Lock existence prevents re-execution of old certs when objects have been upgraded
            // 2. Not all validators lock, just 2f+1, so transaction should proceed regardless
            //    (But the lock should exist which means previous transactions finished)
            // 3. Equivocation possible (different TX) but as long as 2f+1 approves current TX its fine
            // 4. Locks may have existed when we started processing this tx, but could have since
            //    been deleted by a concurrent tx that finished first. In that case, check if the tx effects exist.
            let new_locks_to_init: Vec<_> = written
                .iter()
                .filter_map(|(_, (object_ref, new_object))| {
                    if new_object.is_owned_or_quasi_shared() {
                        Some(*object_ref)
                    } else {
                        None
                    }
                })
                .collect();

            match update_type {
                UpdateType::Transaction(seq, _) => {
                    // sequence_transaction atomically assigns a sequence number to the tx and
                    // initializes locks for the output objects.
                    // It also (not atomically) deletes the locks for input objects.
                    // After this call completes, new txes can run on the output locks, so all
                    // output objects must be written already.
                    Some(
                        self.lock_service
                            .sequence_transaction(
                                transaction_digest,
                                seq,
                                owned_inputs,
                                new_locks_to_init,
                            )
                            .await?,
                    )
                }
                UpdateType::Genesis => {
                    info!("Creating locks for genesis objects");
                    self.lock_service
                        .create_locks_for_genesis_objects(new_locks_to_init)
                        .await?;
                    None
                }
            }

            // implicit: drop(_mutexes);
        };

        Ok(assigned_seq)
    }

    /// This function is called at the end of epoch for each transaction that's
    /// executed locally on the validator but didn't make to the last checkpoint.
    /// The effects of the execution is reverted here.
    /// The following things are reverted:
    /// 1. Certificate and effects are deleted.
    /// 2. Latest parent_sync entries for each mutated object are deleted.
    /// 3. All new object states are deleted.
    /// 4. owner_index table change is reverted.
    pub fn revert_state_update(&self, tx_digest: &TransactionDigest) -> SuiResult {
        let effects = self.get_effects(tx_digest)?;
        let mut write_batch = self.tables.certificates.batch();
        write_batch = write_batch.delete_batch(&self.tables.certificates, iter::once(tx_digest))?;
        write_batch = write_batch.delete_batch(&self.tables.effects, iter::once(tx_digest))?;

        let all_new_refs = effects
            .mutated
            .iter()
            .chain(effects.created.iter())
            .chain(effects.unwrapped.iter())
            .map(|(r, _)| r)
            .chain(effects.deleted.iter())
            .chain(effects.wrapped.iter());
        write_batch = write_batch.delete_batch(&self.tables.parent_sync, all_new_refs)?;

        let all_new_object_keys = effects
            .mutated
            .iter()
            .chain(effects.created.iter())
            .chain(effects.unwrapped.iter())
            .map(|((id, version, _), _)| ObjectKey(*id, *version));
        write_batch = write_batch.delete_batch(&self.tables.objects, all_new_object_keys)?;

        // Reverting the change to the owner_index table is most complex.
        // For each newly created (i.e. created and unwrapped) object, the entry in owner_index
        // needs to be deleted; for each mutated object, we need to query the object state of
        // the older version, and then rewrite the entry with the old object info.
        // TODO: Validators should not need to maintain owner_index.
        // This is dependent on https://github.com/MystenLabs/sui/issues/2629.
        let owners_to_delete = effects
            .created
            .iter()
            .chain(effects.unwrapped.iter())
            .chain(effects.mutated.iter())
            .map(|((id, _, _), owner)| (*owner, *id));
        write_batch = write_batch.delete_batch(&self.tables.owner_index, owners_to_delete)?;
        let mutated_objects = effects
            .mutated
            .iter()
            .map(|(r, _)| r)
            .chain(effects.deleted.iter())
            .chain(effects.wrapped.iter())
            .map(|(id, version, _)| {
                ObjectKey(
                    *id,
                    version
                        .decrement()
                        .expect("version revert should never fail"),
                )
            });
        let old_objects = self
            .tables
            .objects
            .multi_get(mutated_objects)?
            .into_iter()
            .map(|obj_opt| {
                let obj = obj_opt.expect("Older object version not found");
                (
                    (obj.owner, obj.id()),
                    ObjectInfo::new(&obj.compute_object_reference(), &obj),
                )
            });
        write_batch = write_batch.insert_batch(&self.tables.owner_index, old_objects)?;

        write_batch.write()?;
        Ok(())
    }

    /// Returns the last entry we have for this object in the parents_sync index used
    /// to facilitate client and authority sync. In turn the latest entry provides the
    /// latest object_reference, and also the latest transaction that has interacted with
    /// this object.
    ///
    /// This parent_sync index also contains entries for deleted objects (with a digest of
    /// ObjectDigest::deleted()), and provides the transaction digest of the certificate
    /// that deleted the object. Note that a deleted object may re-appear if the deletion
    /// was the result of the object being wrapped in another object.
    ///
    /// If no entry for the object_id is found, return None.
    pub fn get_latest_parent_entry(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<(ObjectRef, TransactionDigest)>, SuiError> {
        let mut iterator = self
            .tables
            .parent_sync
            .iter()
            // Make the max possible entry for this object ID.
            .skip_prior_to(&(object_id, SequenceNumber::MAX, ObjectDigest::MAX))?;

        Ok(iterator.next().and_then(|(obj_ref, tx_digest)| {
            if obj_ref.0 == object_id {
                Some((obj_ref, tx_digest))
            } else {
                None
            }
        }))
    }

    /// Remove the shared objects locks.
    pub fn remove_shared_objects_locks(
        &self,
        transaction_digest: &TransactionDigest,
        transaction: &CertifiedTransaction,
    ) -> SuiResult {
        let mut sequenced_to_delete = Vec::new();
        let mut schedule_to_delete = Vec::new();
        for object_id in transaction.shared_input_objects() {
            sequenced_to_delete.push((*transaction_digest, *object_id));
            if self.get_object(object_id)?.is_none() {
                schedule_to_delete.push(*object_id);
            }
        }
        let mut write_batch = self.tables.assigned_object_versions.batch();
        write_batch =
            write_batch.delete_batch(&self.tables.assigned_object_versions, sequenced_to_delete)?;
        write_batch =
            write_batch.delete_batch(&self.tables.next_object_versions, schedule_to_delete)?;
        write_batch.write()?;
        Ok(())
    }

    /// Lock a sequence number for the shared objects of the input transaction based on the effects
    /// of that transaction. Used by the nodes, which don't listen to consensus.
    pub fn acquire_shared_locks_from_effects(
        &self,
        certificate: &CertifiedTransaction,
        effects: &TransactionEffects,
        // Do not remove unused arg - ensures that this function is not called without holding a
        // lock.
        _tx_guard: &CertTxGuard<'_>,
    ) -> SuiResult {
        let digest = *certificate.digest();

        let sequenced: Vec<_> = effects
            .shared_objects
            .iter()
            .map(|(id, version, _)| ((digest, *id), *version))
            .collect();
        debug!(?sequenced, "Shared object locks sequenced from effects");

        let mut write_batch = self.tables.assigned_object_versions.batch();
        write_batch = write_batch.insert_batch(&self.tables.assigned_object_versions, sequenced)?;
        write_batch.write()?;

        Ok(())
    }

    /// Lock a sequence number for the shared objects of the input transaction. Also update the
    /// last consensus index.
    /// This function must only be called from the consensus task (i.e. from handle_consensus_transaction).
    pub async fn persist_certificate_and_lock_shared_objects(
        &self,
        certificate: CertifiedTransaction,
        consensus_index: ExecutionIndices,
    ) -> Result<(), SuiError> {
        // Make an iterator to save the certificate.
        let transaction_digest = *certificate.digest();

        // Ensure that we only advance next_object_versions exactly once for every cert received from
        // consensus.
        if self
            .tables
            .consensus_message_processed
            .contains_key(&transaction_digest)?
        {
            return Ok(());
        }

        // Make an iterator to update the locks of the transaction's shared objects.
        let ids = certificate.shared_input_objects();
        let versions = self.tables.next_object_versions.multi_get(ids)?;

        let ids = certificate.shared_input_objects();
        let (sequenced_to_write, schedule_to_write): (Vec<_>, Vec<_>) = ids
            .zip(versions.iter())
            .map(|(id, v)| {
                // If it is the first time the shared object has been sequenced, assign it the default
                // sequence number (`OBJECT_START_VERSION`). Otherwise use the `scheduled` map to
                // to assign the next sequence number.
                let version = v.unwrap_or_else(|| OBJECT_START_VERSION);
                let next_version = version.increment();

                let sequenced = ((transaction_digest, *id), version);
                let scheduled = (id, next_version);

                (sequenced, scheduled)
            })
            .unzip();

        trace!(tx_digest = ?transaction_digest,
               ?sequenced_to_write, ?schedule_to_write,
               "locking shared objects");

        // Make an iterator to update the last consensus index.
        let index_to_write = std::iter::once((LAST_CONSENSUS_INDEX_ADDR, consensus_index));

        // Schedule the certificate for execution
        self.add_pending_certificates(vec![(transaction_digest, Some(certificate.clone()))])?;

        // Holding _tx_lock avoids the following race:
        // - we check effects_exist, returns false
        // - another task (starting from handle_node_sync_certificate) writes effects,
        //    and then deletes locks from assigned_object_versions
        // - we write to assigned_object versions, re-creating the locks that were just deleted
        // - now it's possible to run a new tx against old versions of the shared objects.
        let _tx_lock = self.acquire_tx_lock(&transaction_digest).await;

        // Note: if we crash here we are not in an inconsistent state since
        //       it is ok to just update the pending list without updating the sequence.

        // Atomically store all elements.
        let mut write_batch = self.tables.assigned_object_versions.batch();

        // If the tx has already been executed, we don't need to populate assigned_object_versions,
        // and indeed we shouldn't because it will never be cleaned up.
        //
        // It may appear that this can cause assigned_object_versions to be inconsistent with
        // next_object_versions - however there is no real inconsistency. assigned_object_versions
        // effectively contains different snapshots of next_object_versions at different points in
        // time.
        //
        // When we set assigned_object_versions from next_object_versions, we are taking a
        // snapshot.
        //
        // When we set it from a TransactionEffects structure, we are instead copying the
        // snapshot that was taken by another validator at the time at which the transaction was
        // sequenced.
        //
        // (Both checkpoints and the node follower system ensure that at least one
        // honest validator has vouched for the TransactionEffects that were used).
        if !self.effects_exists(&transaction_digest)? {
            write_batch = write_batch
                .insert_batch(&self.tables.assigned_object_versions, sequenced_to_write)?;
        }

        write_batch =
            write_batch.insert_batch(&self.tables.next_object_versions, schedule_to_write)?;
        write_batch =
            write_batch.insert_batch(&self.tables.last_consensus_index, index_to_write)?;
        write_batch = write_batch.insert_batch(
            &self.tables.consensus_message_processed,
            std::iter::once((transaction_digest, true)),
        )?;
        write_batch.write().map_err(SuiError::from)
    }

    pub fn transactions_in_seq_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> SuiResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self
            .tables
            .executed_sequence
            .iter()
            .skip_to(&start)?
            .take_while(|(seq, _tx)| *seq < end)
            .map(|(seq, exec)| (seq, exec.transaction))
            .collect())
    }

    /// Retrieves batches including transactions within a range.
    ///
    /// This function returns all signed batches that enclose the requested transaction
    /// including the batch preceding the first requested transaction, the batch including
    /// the last requested transaction (if there is one) and all batches in between.
    ///
    /// Transactions returned include all transactions within the batch that include the
    /// first requested transaction, all the way to at least all the transactions that are
    /// included in the last batch returned. If the last requested transaction is outside a
    /// batch (one has not yet been generated) the function returns all transactions at the
    /// end of the sequence that are in TxSequenceOrder (and ignores any that are out of
    /// order.)
    // TODO: Why include the transaction prior to `start`?
    #[allow(clippy::type_complexity)]
    pub fn batches_and_transactions(
        &self,
        start: TxSequenceNumber,
        end: TxSequenceNumber,
    ) -> Result<(Vec<SignedBatch>, Vec<(TxSequenceNumber, ExecutionDigests)>), SuiError> {
        /*
        Get all batches that include requested transactions. This includes the signed batch
        prior to the first requested transaction, the batch including the last requested
        transaction and all batches in between.

        So for example if we got a request for start: 3 end: 9 and we have:
        B0 T0 T1 B2 T2 B3 T3 T4 T5 B6 T6 T8 T9

        This will return B2, B3, B6

        */
        let batches: Vec<SignedBatch> = self
            .tables
            .batches
            .iter()
            .skip_prior_to(&start)?
            .take_while(|(_seq, batch)| batch.data().initial_sequence_number < end)
            .map(|(_, batch)| batch)
            .collect();

        /*
        Get transactions in the retrieved batches. The first batch is included
        without transactions, so get transactions of all subsequent batches, or
        until the end of the sequence if the last batch does not contain the
        requested end sequence number.

        So for example if we got a request for start: 3 end: 9 and we have:
        B0 T0 T1 B2 T2 B3 T3 T4 T5 B6 T6 T8 T9

        The code below will return T2 .. T6

        Note: T8 is out of order so the sequence returned ends at T6.

        */

        let first_seq = batches
            .first()
            .ok_or(SuiError::NoBatchesFoundError)?
            .data()
            .next_sequence_number;
        let mut last_seq = batches
            .last()
            .unwrap() // if the first exists the last exists too
            .data()
            .next_sequence_number;

        let mut in_sequence = last_seq;
        let in_sequence_ptr = &mut in_sequence;

        if last_seq < end {
            // This means that the request needs items beyond the end of the
            // last batch, so we include all items.
            last_seq = TxSequenceNumber::MAX;
        }

        /* Since the database writes are asynchronous it may be the case that the tail end of the
        sequence misses items. This will confuse calling logic, so we filter them out and allow
        callers to use the subscription API to catch the latest items in order. */

        let transactions: Vec<(TxSequenceNumber, ExecutionDigests)> = self
            .tables
            .executed_sequence
            .iter()
            .skip_to(&first_seq)?
            .take_while(|(seq, _tx)| {
                // Before the end of the last batch we want everything.
                if *seq < *in_sequence_ptr {
                    return true;
                };

                // After the end of the last batch we only take items in sequence.
                if *seq < last_seq && *seq == *in_sequence_ptr {
                    *in_sequence_ptr += 1;
                    return true;
                }

                // If too large or out of sequence after the last batch
                // we stop taking items.
                false
            })
            .collect();

        Ok((batches, transactions))
    }

    /// Return the latest consensus index. It is used to bootstrap the consensus client.
    pub fn last_consensus_index(&self) -> SuiResult<ExecutionIndices> {
        self.tables
            .last_consensus_index
            .get(&LAST_CONSENSUS_INDEX_ADDR)
            .map(|x| x.unwrap_or_default())
            .map_err(SuiError::from)
    }

    pub fn get_transaction(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> SuiResult<Option<TransactionEnvelope<S>>> {
        let transaction = self.tables.transactions.get(transaction_digest)?;
        Ok(transaction)
    }

    pub fn get_certified_transaction(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> SuiResult<Option<CertifiedTransaction>> {
        let transaction = self.tables.certificates.get(transaction_digest)?;
        Ok(transaction)
    }

    pub fn multi_get_certified_transaction(
        &self,
        transaction_digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<CertifiedTransaction>>> {
        Ok(self.tables.certificates.multi_get(transaction_digests)?)
    }

    pub fn get_sui_system_state_object(&self) -> SuiResult<SuiSystemState>
    where
        S: Eq + Serialize + for<'de> Deserialize<'de>,
    {
        let sui_system_object = self
            .get_object(&SUI_SYSTEM_STATE_OBJECT_ID)?
            .expect("Sui System State object must always exist");
        let move_object = sui_system_object
            .data
            .try_as_move()
            .expect("Sui System State object must be a Move object");
        let result = bcs::from_bytes::<SuiSystemState>(move_object.contents())
            .expect("Sui System State object deserialization cannot fail");
        Ok(result)
    }
}

impl SuiDataStore<AuthoritySignInfo> {
    pub fn get_signed_transaction_info(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        Ok(TransactionInfoResponse {
            signed_transaction: self.tables.transactions.get(transaction_digest)?,
            certified_transaction: self.tables.certificates.get(transaction_digest)?,
            signed_effects: self.tables.effects.get(transaction_digest)?,
        })
    }
}

impl SuiDataStore<EmptySignInfo> {
    pub fn pending_transactions(&self) -> &DBMap<TransactionDigest, Transaction> {
        &self.tables.transactions
    }
}

impl<S: Eq + Debug + Serialize + for<'de> Deserialize<'de> + AuthoritySignInfoTrait>
    BackingPackageStore for SuiDataStore<S>
{
    fn get_package(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        let package = self.get_object(package_id)?;
        if let Some(obj) = &package {
            fp_ensure!(
                obj.is_package(),
                SuiError::BadObjectType {
                    error: format!("Package expected, Move object found: {package_id}"),
                }
            );
        }
        Ok(package)
    }
}

impl<S: Eq + Debug + Serialize + for<'de> Deserialize<'de> + AuthoritySignInfoTrait> ParentSync
    for SuiDataStore<S>
{
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        Ok(self
            .get_latest_parent_entry(object_id)?
            .map(|(obj_ref, _)| obj_ref))
    }
}

impl<S: Eq + Debug + Serialize + for<'de> Deserialize<'de> + AuthoritySignInfoTrait> ModuleResolver
    for SuiDataStore<S>
{
    type Error = SuiError;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        // TODO: We should cache the deserialized modules to avoid
        // fetching from the store / re-deserializing them everytime.
        // https://github.com/MystenLabs/sui/issues/809
        Ok(self
            .get_package(&ObjectID::from(*module_id.address()))?
            .and_then(|package| {
                // unwrap safe since get_package() ensures it's a package object.
                package
                    .data
                    .try_as_package()
                    .unwrap()
                    .serialized_module_map()
                    .get(module_id.name().as_str())
                    .cloned()
            }))
    }
}

/// A wrapper to make Orphan Rule happy
pub struct ResolverWrapper<T: ModuleResolver>(pub Arc<T>);

impl<T: ModuleResolver> ModuleResolver for ResolverWrapper<T> {
    type Error = T::Error;
    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.0.get_module(module_id)
    }
}

// The primary key type for object storage.
#[serde_as]
#[derive(Eq, PartialEq, Clone, Copy, PartialOrd, Ord, Hash, Serialize, Deserialize, Debug)]
pub(crate) struct ObjectKey(pub ObjectID, pub VersionNumber);

impl ObjectKey {
    pub const ZERO: ObjectKey = ObjectKey(ObjectID::ZERO, VersionNumber::MIN);

    pub fn max_for_id(id: &ObjectID) -> Self {
        Self(*id, VersionNumber::MAX)
    }
}

impl From<ObjectRef> for ObjectKey {
    fn from(object_ref: ObjectRef) -> Self {
        ObjectKey::from(&object_ref)
    }
}

impl From<&ObjectRef> for ObjectKey {
    fn from(object_ref: &ObjectRef) -> Self {
        Self(object_ref.0, object_ref.1)
    }
}

pub enum UpdateType {
    Transaction(TxSequenceNumber, TransactionEffectsDigest),
    Genesis,
}
