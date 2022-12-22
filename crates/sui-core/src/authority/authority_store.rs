// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{authority_store_tables::AuthorityPerpetualTables, *};
use crate::authority::authority_per_epoch_store::{
    AuthorityPerEpochStore, ExecutionIndicesWithHash,
};
use arc_swap::ArcSwap;
use once_cell::sync::OnceCell;
use rocksdb::Options;
use std::iter;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use sui_storage::{
    lock_service::ObjectLockStatus,
    mutex_table::{LockGuard, MutexTable},
    LockService,
};
use sui_types::object::Owner;
use sui_types::object::PACKAGE_VERSION;
use sui_types::storage::{ChildObjectResolver, ObjectKey};
use sui_types::{base_types::SequenceNumber, storage::ParentSync};
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tracing::{debug, info, trace};
use typed_store::rocks::DBBatch;
use typed_store::traits::Map;

const NUM_SHARDS: usize = 4096;
const SHARD_SIZE: usize = 128;

/// ALL_OBJ_VER determines whether we want to store all past
/// versions of every object in the store. Authority doesn't store
/// them, but other entities such as replicas will.
/// S is a template on Authority signature state. This allows SuiDataStore to be used on either
/// authorities or non-authorities. Specifically, when storing transactions and effects,
/// S allows SuiDataStore to either store the authority signed version or unsigned version.
pub struct AuthorityStore {
    /// The LockService this store depends on for locking functionality
    lock_service: LockService,

    /// Internal vector of locks to manage concurrent writes to the database
    mutex_table: MutexTable<ObjectDigest>,

    pub(crate) perpetual_tables: AuthorityPerpetualTables,
    epoch_store: ArcSwap<AuthorityPerEpochStore>,

    // needed for re-opening epoch db.
    path: PathBuf,
    db_options: Option<Options>,

    // Implementation detail to support notify_read_effects().
    pub(crate) effects_notify_read: NotifyRead<TransactionDigest, SignedTransactionEffects>,
}

impl AuthorityStore {
    /// Open an authority store by directory path.
    /// If the store is empty, initialize it using genesis.
    pub async fn open(
        path: &Path,
        db_options: Option<Options>,
        genesis: &Genesis,
        committee_store: &Arc<CommitteeStore>,
    ) -> SuiResult<Self> {
        let perpetual_tables = AuthorityPerpetualTables::open(path, db_options.clone());
        if perpetual_tables.database_is_empty()? {
            perpetual_tables.set_recovery_epoch(0)?;
        }
        let cur_epoch = perpetual_tables.get_recovery_epoch_at_restart()?;
        let committee = match committee_store.get_committee(&cur_epoch)? {
            Some(committee) => committee,
            None => {
                // If we cannot find the corresponding committee from the committee store, we must
                // be at genesis. This is because we always first insert to the committee store
                // before updating the current epoch in the perpetual tables.
                assert_eq!(cur_epoch, 0);
                genesis.committee()?
            }
        };
        Self::open_inner(path, db_options, genesis, perpetual_tables, committee).await
    }

    pub async fn open_with_committee_for_testing(
        path: &Path,
        db_options: Option<Options>,
        committee: &Committee,
        genesis: &Genesis,
    ) -> SuiResult<Self> {
        // TODO: Since we always start at genesis, the committee should be technically the same
        // as the genesis committee.
        assert_eq!(committee.epoch, 0);
        let perpetual_tables = AuthorityPerpetualTables::open(path, db_options.clone());
        Self::open_inner(
            path,
            db_options,
            genesis,
            perpetual_tables,
            committee.clone(),
        )
        .await
    }

    async fn open_inner(
        path: &Path,
        db_options: Option<Options>,
        genesis: &Genesis,
        perpetual_tables: AuthorityPerpetualTables,
        committee: Committee,
    ) -> SuiResult<Self> {
        let epoch_tables = Arc::new(AuthorityPerEpochStore::new(
            committee,
            path,
            db_options.clone(),
        ));

        // For now, create one LockService for each SuiDataStore, and we use a specific
        // subdir of the data store directory
        let lockdb_path: PathBuf = path.join("lockdb");
        let lock_service =
            LockService::new(lockdb_path, None).expect("Could not initialize lockdb");

        let store = Self {
            lock_service,
            mutex_table: MutexTable::new(NUM_SHARDS, SHARD_SIZE),
            perpetual_tables,
            epoch_store: epoch_tables.into(),
            path: path.into(),
            db_options,
            effects_notify_read: NotifyRead::new(),
        };
        // Only initialize an empty database.
        if store
            .database_is_empty()
            .expect("Database read should not fail at init.")
        {
            store
                .bulk_object_insert(&genesis.objects().iter().collect::<Vec<_>>())
                .await
                .expect("Cannot bulk insert genesis objects");
        }

        Ok(store)
    }

    /// This function is called at the very end of the epoch.
    /// This step is required before updating new epoch in the db and calling reopen_epoch_db.
    pub(crate) async fn revert_uncommitted_epoch_transactions(&self) -> SuiResult {
        let epoch_store = self.epoch_store.load();
        {
            let state = epoch_store.get_reconfig_state_write_lock_guard();
            if state.should_accept_user_certs() {
                // Need to change this so that consensus adapter do not accept certificates from user.
                // This can happen if our local validator did not initiate epoch change locally,
                // but 2f+1 nodes already concluded the epoch.
                //
                // This lock is essentially a barrier for
                // `epoch_store.pending_consensus_certificates` table we are reading on the line after this block
                epoch_store.close_user_certs(state);
            }
            // lock is dropped here
        }
        let pending_certificates = epoch_store.pending_consensus_certificates();
        debug!(
            "Reverting {} locally executed transactions that was not included in the epoch",
            pending_certificates.len()
        );
        for digest in pending_certificates {
            debug!("Reverting {} at the end of epoch", digest);
            self.revert_state_update(&digest).await?;
        }
        debug!("All uncommitted local transactions reverted");
        Ok(())
    }

    pub(crate) async fn reopen_epoch_db(&self, new_committee: Committee) {
        info!(new_epoch = ?new_committee.epoch, "re-opening AuthorityEpochTables for new epoch");
        let epoch_tables = Arc::new(AuthorityPerEpochStore::new(
            new_committee,
            &self.path,
            self.db_options.clone(),
        ));
        let previous_store = self.epoch_store.swap(epoch_tables);
        previous_store.epoch_terminated().await;
    }

    // TODO: Deprecate this once we replace all calls with load_epoch_store.
    pub fn epoch_store(&self) -> Guard<Arc<AuthorityPerEpochStore>> {
        self.epoch_store.load()
    }

    pub fn load_epoch_store(
        &self,
        intended_epoch: EpochId,
    ) -> SuiResult<Guard<Arc<AuthorityPerEpochStore>>> {
        let store = self.epoch_store.load();
        fp_ensure!(
            store.epoch() == intended_epoch,
            SuiError::StoreAccessEpochMismatch {
                store_epoch: store.epoch(),
                intended_epoch,
            }
        );
        Ok(store)
    }

    /// Returns the TransactionEffects if we have an effects structure for this transaction digest
    pub fn get_effects(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> SuiResult<TransactionEffects> {
        self.perpetual_tables
            .executed_effects
            .get(transaction_digest)?
            .map(|data| data.into_data())
            .ok_or(SuiError::TransactionNotFound {
                digest: *transaction_digest,
            })
    }

    /// Returns true if we have an effects structure for this transaction digest
    pub fn effects_exists(&self, transaction_digest: &TransactionDigest) -> SuiResult<bool> {
        self.perpetual_tables
            .executed_effects
            .contains_key(transaction_digest)
            .map_err(|e| e.into())
    }

    /// Returns true if there are no objects in the database
    pub fn database_is_empty(&self) -> SuiResult<bool> {
        self.perpetual_tables.database_is_empty()
    }

    /// A function that acquires all locks associated with the objects (in order to avoid deadlocks).
    async fn acquire_locks(&self, input_objects: &[ObjectRef]) -> Vec<LockGuard> {
        self.mutex_table
            .acquire_locks(input_objects.iter().map(|(_, _, digest)| *digest))
            .await
    }

    // Methods to read the store
    pub fn get_owner_objects(&self, owner: Owner) -> Result<Vec<ObjectInfo>, SuiError> {
        debug!(?owner, "get_owner_objects");
        Ok(self.get_owner_objects_iterator(owner)?.collect())
    }

    // Methods to read the store
    pub fn get_owner_objects_iterator(
        &self,
        owner: Owner,
    ) -> Result<impl Iterator<Item = ObjectInfo> + '_, SuiError> {
        debug!(?owner, "get_owner_objects");
        Ok(self
            .perpetual_tables
            .owner_index
            .iter()
            // The object id 0 is the smallest possible
            .skip_to(&(owner, ObjectID::ZERO))?
            .take_while(move |((object_owner, _), _)| (object_owner == &owner))
            .map(|(_, object_info)| object_info))
    }

    pub fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        Ok(self
            .perpetual_tables
            .objects
            .get(&ObjectKey(*object_id, version))?)
    }

    pub fn object_version_exists(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<bool, SuiError> {
        Ok(self
            .perpetual_tables
            .objects
            .contains_key(&ObjectKey(*object_id, version))?)
    }

    /// Read an object and return it, or Err(ObjectNotFound) if the object was not found.
    pub fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        self.perpetual_tables.get_object(object_id)
    }

    /// Get many objects
    pub fn get_objects(&self, objects: &[ObjectID]) -> Result<Vec<Option<Object>>, SuiError> {
        let mut result = Vec::new();
        for id in objects {
            result.push(self.get_object(id)?);
        }
        Ok(result)
    }

    pub fn check_input_objects(
        &self,
        objects: &[InputObjectKind],
    ) -> Result<Vec<Object>, SuiError> {
        let mut result = Vec::new();
        let mut errors = Vec::new();
        for kind in objects {
            let obj = match kind {
                InputObjectKind::MovePackage(id) | InputObjectKind::SharedMoveObject { id, .. } => {
                    self.get_object(id)?
                }
                InputObjectKind::ImmOrOwnedMoveObject(objref) => {
                    self.get_object_by_key(&objref.0, objref.1)?
                }
            };
            match obj {
                Some(obj) => result.push(obj),
                None => errors.push(kind.object_not_found_error()),
            }
        }
        if !errors.is_empty() {
            Err(SuiError::TransactionInputObjectsErrors { errors })
        } else {
            Ok(result)
        }
    }

    /// When making changes, please see if check_sequenced_input_objects() below needs
    /// similar changes as well.
    pub async fn get_missing_input_objects(
        &self,
        digest: &TransactionDigest,
        objects: &[InputObjectKind],
    ) -> Result<Vec<ObjectKey>, SuiError> {
        let shared_locks_cell: OnceCell<HashMap<_, _>> = OnceCell::new();

        let mut missing = Vec::new();
        let mut probe_lock_exists = Vec::new();
        for kind in objects {
            match kind {
                InputObjectKind::SharedMoveObject { id, .. } => {
                    let shared_locks = shared_locks_cell.get_or_try_init(|| {
                        Ok::<HashMap<ObjectID, SequenceNumber>, SuiError>(
                            self.epoch_store()
                                .get_shared_locks(digest)?
                                .into_iter()
                                .collect(),
                        )
                    })?;
                    match shared_locks.get(id) {
                        Some(version) => {
                            if !self.object_version_exists(id, *version)? {
                                // When this happens, other transactions that use smaller versions of
                                // this shared object haven't finished execution.
                                missing.push(ObjectKey(*id, *version));
                            }
                        }
                        None => {
                            // Abort the function because the lock should have been set.
                            return Err(SuiError::SharedObjectLockNotSetError);
                        }
                    };
                }
                InputObjectKind::MovePackage(id) => {
                    if !self.object_version_exists(id, PACKAGE_VERSION)? {
                        // The cert cannot have been formed if immutable inputs were missing.
                        missing.push(ObjectKey(*id, PACKAGE_VERSION));
                    }
                }
                InputObjectKind::ImmOrOwnedMoveObject(objref) => {
                    if let Some(obj) = self.get_object_by_key(&objref.0, objref.1)? {
                        if !obj.is_immutable() {
                            probe_lock_exists.push(*objref);
                        }
                    } else {
                        missing.push(ObjectKey::from(objref));
                    }
                }
            };
        }

        if !probe_lock_exists.is_empty() {
            // It is possible that we probed the objects after they are written, but before the
            // locks are created. In that case, if we attempt to execute the transaction, it will
            // fail. Because the objects_committed() call is made only after the locks are written,
            // the tx manager will be awoken after the locks are written.
            missing.extend(
                self.lock_service
                    .get_missing_locks(probe_lock_exists)
                    .await?
                    .into_iter()
                    .map(ObjectKey::from),
            );
        }

        Ok(missing)
    }

    /// When making changes, please see if get_missing_input_objects() above needs
    /// similar changes as well.
    pub fn check_sequenced_input_objects(
        &self,
        digest: &TransactionDigest,
        objects: &[InputObjectKind],
    ) -> Result<Vec<Object>, SuiError> {
        let shared_locks_cell: OnceCell<HashMap<_, _>> = OnceCell::new();

        let mut result = Vec::new();
        let mut errors = Vec::new();
        for kind in objects {
            let obj = match kind {
                InputObjectKind::SharedMoveObject { id, .. } => {
                    let shared_locks = shared_locks_cell.get_or_try_init(|| {
                        Ok::<HashMap<ObjectID, SequenceNumber>, SuiError>(
                            self.epoch_store()
                                .get_shared_locks(digest)?
                                .into_iter()
                                .collect(),
                        )
                    })?;
                    match shared_locks.get(id) {
                        Some(version) => {
                            if let Some(obj) = self.get_object_by_key(id, *version)? {
                                result.push(obj);
                            } else {
                                // When this happens, other transactions that use smaller versions of
                                // this shared object haven't finished execution.
                                errors.push(SuiError::SharedObjectPriorVersionsPendingExecution {
                                    object_id: *id,
                                    version_not_ready: *version,
                                });
                            }
                            continue;
                        }
                        None => {
                            errors.push(SuiError::SharedObjectLockNotSetError);
                            continue;
                        }
                    }
                }
                InputObjectKind::MovePackage(id) => self.get_object(id)?,
                InputObjectKind::ImmOrOwnedMoveObject(objref) => {
                    self.get_object_by_key(&objref.0, objref.1)?
                }
            };
            // SharedMoveObject should not reach here
            match obj {
                Some(obj) => result.push(obj),
                None => errors.push(kind.object_not_found_error()),
            }
        }
        if !errors.is_empty() {
            Err(SuiError::TransactionInputObjectsErrors { errors })
        } else {
            Ok(result)
        }
    }

    /// Get the TransactionEnvelope that currently locks the given object, if any.
    /// Since object locks are only valid for one epoch, we also need the epoch_id in the query.
    /// Returns SuiError::ObjectNotFound if no lock records for the given object can be found.
    /// Returns SuiError::ObjectVersionUnavailableForConsumption if the object record is at a different version.
    /// Returns Some(VerifiedEnvelope) if the given ObjectRef is locked by a certain transaction.
    /// Returns None if the a lock record is initialized for the given ObjectRef but not yet locked by any transaction,
    ///     or cannot find the transaction in transaction table, because of data race etc.
    pub async fn get_object_locking_transaction(
        &self,
        object_ref: &ObjectRef,
        epoch_id: EpochId,
    ) -> SuiResult<Option<VerifiedSignedTransaction>> {
        let lock_info = self.lock_service.get_lock(*object_ref, epoch_id).await?;
        let lock_info = match lock_info {
            ObjectLockStatus::LockedAtDifferentVersion { locked_ref } => {
                return Err(SuiError::ObjectVersionUnavailableForConsumption {
                    provided_obj_ref: *object_ref,
                    current_version: locked_ref.1,
                })
            }
            ObjectLockStatus::Initialized => {
                return Ok(None);
            }
            ObjectLockStatus::LockedToTx { locked_by_tx } => locked_by_tx,
        };
        // Returns None if either no TX with the lock, or TX present but no entry in transactions table.
        // However we retry a couple times because the TX is written after the lock is acquired, so it might
        // just be a race.
        let tx_digest = &lock_info.tx_digest;
        let mut retry_strategy = ExponentialBackoff::from_millis(2)
            .factor(10)
            .map(jitter)
            .take(3);

        let mut tx_option = self.get_transaction(tx_digest)?;
        while tx_option.is_none() {
            if let Some(duration) = retry_strategy.next() {
                // Wait to retry
                tokio::time::sleep(duration).await;
                trace!(?tx_digest, "Retrying getting pending transaction");
            } else {
                // No more retries, just quit
                break;
            }
            tx_option = self.get_transaction(tx_digest)?;
        }
        Ok(tx_option)
    }

    /// Read a certificate and return an option with None if it does not exist.
    pub fn read_certificate(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<VerifiedCertificate>, SuiError> {
        self.perpetual_tables
            .certificates
            .get(digest)
            .map(|r| r.map(|c| c.into()))
            .map_err(|e| e.into())
    }

    /// Read the transactionDigest that is the parent of an object reference
    /// (ie. the transaction that created an object at this version.)
    pub fn parent(&self, object_ref: &ObjectRef) -> Result<Option<TransactionDigest>, SuiError> {
        self.perpetual_tables
            .parent_sync
            .get(object_ref)
            .map_err(|e| e.into())
    }

    /// Batch version of `parent` function.
    pub fn multi_get_parents(
        &self,
        object_refs: &[ObjectRef],
    ) -> Result<Vec<Option<TransactionDigest>>, SuiError> {
        self.perpetual_tables
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
            .perpetual_tables
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

    pub async fn check_owned_locks(&self, owned_object_refs: &[ObjectRef]) -> SuiResult {
        self.lock_service
            .locks_exist(owned_object_refs.into())
            .await
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
    /// This is used to insert genesis objects
    pub async fn insert_object_direct(&self, object_ref: ObjectRef, object: &Object) -> SuiResult {
        // Insert object
        self.perpetual_tables
            .objects
            .insert(&object_ref.into(), object)?;

        // Update the index
        if object.get_single_owner().is_some() {
            self.perpetual_tables.owner_index.insert(
                &(object.owner, object_ref.0),
                &ObjectInfo::new(&object_ref, object),
            )?;
            // Only initialize lock for address owned objects.
            if !object.is_child_object() {
                self.lock_service
                    .initialize_locks(&[object_ref], false /* is_force_reset */)
                    .await?;
            }
        }

        // Update the parent
        self.perpetual_tables
            .parent_sync
            .insert(&object_ref, &object.previous_transaction)?;

        Ok(())
    }

    /// This function is used by the bench.rs script, and should not be used in other contexts
    /// In particular it does not check the old locks before inserting new ones, so the objects
    /// must be new.
    pub async fn bulk_object_insert(&self, objects: &[&Object]) -> SuiResult<()> {
        let batch = self.perpetual_tables.objects.batch();
        let ref_and_objects: Vec<_> = objects
            .iter()
            .map(|o| (o.compute_object_reference(), o))
            .collect();

        batch
            .insert_batch(
                &self.perpetual_tables.objects,
                ref_and_objects
                    .iter()
                    .map(|(oref, o)| (ObjectKey::from(oref), **o)),
            )?
            .insert_batch(
                &self.perpetual_tables.owner_index,
                ref_and_objects
                    .iter()
                    .map(|(oref, o)| ((o.owner, oref.0), ObjectInfo::new(oref, o))),
            )?
            .insert_batch(
                &self.perpetual_tables.parent_sync,
                ref_and_objects
                    .iter()
                    .map(|(oref, o)| (oref, o.previous_transaction)),
            )?
            .write()?;

        let non_child_object_refs: Vec<_> = ref_and_objects
            .iter()
            .filter(|(_, object)| !object.is_child_object())
            .map(|(oref, _)| *oref)
            .collect();
        self.lock_service
            .initialize_locks(&non_child_object_refs, false /* is_force_reset */)
            .await?;

        Ok(())
    }

    /// Acquires the transaction lock for a specific transaction, writing the transaction
    /// to the transaction column family if acquiring the lock succeeds.
    /// The lock service is used to atomically acquire locks.
    pub async fn lock_and_write_transaction(
        &self,
        epoch: EpochId,
        owned_input_objects: &[ObjectRef],
        transaction: VerifiedSignedTransaction,
    ) -> Result<(), SuiError> {
        let tx_digest = *transaction.digest();

        // Acquire the lock on input objects
        self.lock_service
            .acquire_locks(epoch, owned_input_objects.to_owned(), tx_digest)
            .await?;

        // TODO: we should have transaction insertion be atomic with lock acquisition, or retry.
        // For now write transactions after because if we write before, there is a chance the lock can fail
        // and this can cause invalid transactions to be inserted in the table.
        // https://github.com/MystenLabs/sui/issues/1990
        self.epoch_store().insert_transaction(transaction)?;

        Ok(())
    }

    /// Updates the state resulting from the execution of a certificate.
    ///
    /// Internally it checks that all locks for active inputs are at the correct
    /// version, and then writes objects, certificates, parents and clean up locks atomically.
    pub async fn update_state(
        &self,
        inner_temporary_store: InnerTemporaryStore,
        certificate: &VerifiedCertificate,
        effects: &SignedTransactionEffects,
        effects_digest: &TransactionEffectsDigest,
    ) -> SuiResult {
        // Extract the new state from the execution
        // TODO: events are already stored in the TxDigest -> TransactionEffects store. Is that enough?
        let mut write_batch = self.perpetual_tables.certificates.batch();

        // Store the certificate indexed by transaction digest
        let transaction_digest: &TransactionDigest = certificate.digest();
        write_batch = write_batch.insert_batch(
            &self.perpetual_tables.certificates,
            iter::once((transaction_digest, certificate.serializable_ref())),
        )?;

        self.sequence_tx(
            write_batch,
            inner_temporary_store,
            transaction_digest,
            effects,
            effects_digest,
        )
        .await?;

        self.effects_notify_read.notify(transaction_digest, effects);

        Ok(())
    }

    /// Persist temporary storage to DB for genesis modules
    pub async fn update_objects_state_for_genesis(
        &self,
        inner_temporary_store: InnerTemporaryStore,
        transaction_digest: TransactionDigest,
    ) -> Result<(), SuiError> {
        debug_assert_eq!(transaction_digest, TransactionDigest::genesis());
        let write_batch = self.perpetual_tables.certificates.batch();
        self.batch_update_objects(
            write_batch,
            inner_temporary_store,
            transaction_digest,
            UpdateType::Genesis,
        )
        .await?;
        Ok(())
    }

    async fn sequence_tx(
        &self,
        write_batch: DBBatch,
        inner_temporary_store: InnerTemporaryStore,
        transaction_digest: &TransactionDigest,
        effects: &SignedTransactionEffects,
        effects_digest: &TransactionEffectsDigest,
    ) -> SuiResult {
        // Safe to unwrap since UpdateType::Transaction ensures we get a sequence number back.
        self.batch_update_objects(
            write_batch,
            inner_temporary_store,
            *transaction_digest,
            UpdateType::Transaction(*effects_digest),
        )
        .await?;

        // Store the signed effects of the transaction
        // We can't write this until after sequencing succeeds (which happens in
        // batch_update_objects), as effects_exists is used as a check in many places
        // for "did the tx finish".
        let batch = self.perpetual_tables.executed_effects.batch();
        let batch = batch
            .insert_batch(
                &self.perpetual_tables.executed_effects,
                [(transaction_digest, effects)],
            )?
            .insert_batch(
                &self.perpetual_tables.effects,
                [(effects_digest, effects.data())],
            )?;

        batch.write()?;

        Ok(())
    }

    /// Helper function for updating the objects in the state
    async fn batch_update_objects(
        &self,
        mut write_batch: DBBatch,
        inner_temporary_store: InnerTemporaryStore,
        transaction_digest: TransactionDigest,
        update_type: UpdateType,
    ) -> SuiResult {
        let InnerTemporaryStore {
            objects,
            mutable_inputs: active_inputs,
            written,
            deleted,
        } = inner_temporary_store;
        trace!(written =? written.values().map(|((obj_id, ver, _), _, _)| (obj_id, ver)).collect::<Vec<_>>(),
               "batch_update_objects: temp store written");

        let owned_inputs: Vec<_> = active_inputs
            .iter()
            .filter(|(id, _, _)| objects.get(id).unwrap().is_address_owned())
            .cloned()
            .collect();

        // Make an iterator over all objects that are either deleted or have changed owner,
        // along with their old owner.  This is used to update the owner index.
        // For wrapped objects, although their owners technically didn't change, we will lose track
        // of them and there is no guarantee on their owner in the future. Hence we treat them
        // the same as deleted.
        let old_object_owners =
            deleted
                .iter()
                // We need to call get() on objects because some object that were just deleted may not
                // be in the objects list. This can happen if these deleted objects were wrapped in the past,
                // and hence will not show up in the input objects.
                .filter_map(|(id, _)| objects.get(id).and_then(Object::get_owner_and_id))
                .chain(written.iter().filter_map(
                    |(id, (_, new_object, _))| match objects.get(id) {
                        Some(old_object) if old_object.owner != new_object.owner => {
                            old_object.get_owner_and_id()
                        }
                        _ => None,
                    },
                ));

        // Delete the old owner index entries
        write_batch =
            write_batch.delete_batch(&self.perpetual_tables.owner_index, old_object_owners)?;

        // Index the certificate by the objects mutated
        write_batch = write_batch.insert_batch(
            &self.perpetual_tables.parent_sync,
            written
                .iter()
                .map(|(_, (object_ref, _object, _kind))| (object_ref, transaction_digest)),
        )?;

        // Index the certificate by the objects deleted
        write_batch = write_batch.insert_batch(
            &self.perpetual_tables.parent_sync,
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

        // Update the indexes of the objects written
        write_batch = write_batch.insert_batch(
            &self.perpetual_tables.owner_index,
            written
                .iter()
                .filter_map(|(_id, (object_ref, new_object, _kind))| {
                    trace!(?object_ref, owner =? new_object.owner, "Updating owner_index");
                    new_object
                        .get_owner_and_id()
                        .map(|owner_id| (owner_id, ObjectInfo::new(object_ref, new_object)))
                }),
        )?;

        // Insert each output object into the stores
        write_batch = write_batch.insert_batch(
            &self.perpetual_tables.objects,
            written
                .iter()
                .map(|(_, (obj_ref, new_object, _kind))| (ObjectKey::from(obj_ref), new_object)),
        )?;

        // Atomic write of all data other than locks
        write_batch.write()?;
        trace!("Finished writing batch");

        // Need to have a critical section for now because we need to prevent execution of older
        // certs which may overwrite newer objects with older ones.  This can be removed once we have
        // an object storage supporting multiple object versions at once, then there is idempotency and
        // old writes would be OK.
        {
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
                .filter_map(|(_, (object_ref, new_object, _kind))| {
                    if new_object.is_address_owned() {
                        Some(*object_ref)
                    } else {
                        None
                    }
                })
                .collect();

            match update_type {
                UpdateType::Transaction(_) => {
                    // After this call completes, new txes can run on the output locks, so all
                    // output objects must be written already.
                    self.lock_service
                        .commit_transaction(transaction_digest, owned_inputs, new_locks_to_init)
                        .await?;
                }
                UpdateType::Genesis => {
                    info!("Creating locks for genesis objects");
                    self.lock_service
                        .create_locks_for_genesis_objects(new_locks_to_init)
                        .await?;
                }
            }

            // implicit: drop(_mutexes);
        }

        Ok(())
    }

    /// This function is called at the end of epoch for each transaction that's
    /// executed locally on the validator but didn't make to the last checkpoint.
    /// The effects of the execution is reverted here.
    /// The following things are reverted:
    /// 1. Certificate and effects are deleted.
    /// 2. Latest parent_sync entries for each mutated object are deleted.
    /// 3. All new object states are deleted.
    /// 4. owner_index table change is reverted.
    pub async fn revert_state_update(&self, tx_digest: &TransactionDigest) -> SuiResult {
        let effects = self.get_effects(tx_digest)?;

        let mut write_batch = self.perpetual_tables.certificates.batch();
        write_batch =
            write_batch.delete_batch(&self.perpetual_tables.certificates, iter::once(tx_digest))?;
        write_batch = write_batch.delete_batch(
            &self.perpetual_tables.executed_effects,
            iter::once(tx_digest),
        )?;

        let all_new_refs = effects
            .mutated
            .iter()
            .chain(effects.created.iter())
            .chain(effects.unwrapped.iter())
            .map(|(r, _)| r)
            .chain(effects.deleted.iter())
            .chain(effects.wrapped.iter());
        write_batch = write_batch.delete_batch(&self.perpetual_tables.parent_sync, all_new_refs)?;

        let all_new_object_keys = effects
            .mutated
            .iter()
            .chain(effects.created.iter())
            .chain(effects.unwrapped.iter())
            .map(|((id, version, _), _)| ObjectKey(*id, *version));
        write_batch =
            write_batch.delete_batch(&self.perpetual_tables.objects, all_new_object_keys)?;

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
        write_batch =
            write_batch.delete_batch(&self.perpetual_tables.owner_index, owners_to_delete)?;

        let modified_object_keys = effects
            .modified_at_versions
            .iter()
            .map(|(id, version)| ObjectKey(*id, *version));

        let (old_modified_objects, old_locks): (Vec<_>, Vec<_>) = self
            .perpetual_tables
            .objects
            .multi_get(modified_object_keys)?
            .into_iter()
            .filter_map(|obj_opt| {
                let obj = obj_opt.expect("Older object version not found");

                if obj.is_immutable() {
                    return None;
                }

                let obj_ref = obj.compute_object_reference();
                Some((
                    ((obj.owner, obj.id()), ObjectInfo::new(&obj_ref, &obj)),
                    obj.is_address_owned().then_some(obj_ref),
                ))
            })
            .unzip();

        let old_locks: Vec<_> = old_locks.into_iter().flatten().collect();

        write_batch =
            write_batch.insert_batch(&self.perpetual_tables.owner_index, old_modified_objects)?;

        write_batch.write()?;

        self.lock_service.initialize_locks(&old_locks, true).await?;
        Ok(())
    }
    /// Return the object with version less then or eq to the provided seq number.
    /// This is used by indexer to find the correct version of dynamic field child object.
    /// We do not store the version of the child object, but because of lamport timestamp,
    /// we know the child must have version number less then or eq to the parent.
    pub fn find_object_lt_or_eq_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Option<Object> {
        self.perpetual_tables
            .find_object_lt_or_eq_version(object_id, version)
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
        self.perpetual_tables.get_latest_parent_entry(object_id)
    }

    pub fn object_exists(&self, object_id: ObjectID) -> SuiResult<bool> {
        match self.get_latest_parent_entry(object_id)? {
            None => Ok(false),
            Some(entry) => Ok(entry.0 .2.is_alive()),
        }
    }

    /// Lock a sequence number for the shared objects of the input transaction based on the effects
    /// of that transaction. Used by full nodes, which don't listen to consensus.
    pub async fn acquire_shared_locks_from_effects(
        &self,
        certificate: &VerifiedCertificate,
        effects: &TransactionEffects,
    ) -> SuiResult {
        let _tx_lock = self
            .epoch_store()
            .acquire_tx_lock(certificate.digest())
            .await;
        self.epoch_store().set_assigned_shared_object_versions(
            certificate.digest(),
            &effects
                .shared_objects
                .iter()
                .map(|(id, version, _)| (*id, *version))
                .collect(),
        )
    }

    pub async fn record_end_of_publish(
        &self,
        authority: AuthorityName,
        transaction: &ConsensusTransaction,
        consensus_index: ExecutionIndicesWithHash,
    ) -> SuiResult {
        self.epoch_store()
            .record_end_of_publish(authority, transaction.key(), consensus_index)
    }

    pub async fn record_consensus_transaction_processed(
        &self,
        transaction: &ConsensusTransaction,
        consensus_index: ExecutionIndicesWithHash,
    ) -> Result<(), SuiError> {
        // user certificates need to use record_(shared|owned)_object_cert_from_consensus
        assert!(!transaction.is_user_certificate());
        let key = transaction.key();
        self.epoch_store()
            .finish_consensus_transaction_process(key, consensus_index)
    }

    /// Return the latest consensus index. It is used to bootstrap the consensus client.
    pub fn last_consensus_index(&self) -> SuiResult<ExecutionIndicesWithHash> {
        self.epoch_store().get_last_consensus_index()
    }

    pub fn get_transaction(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> SuiResult<Option<VerifiedSignedTransaction>> {
        self.epoch_store().get_transaction(transaction_digest)
    }

    pub fn get_certified_transaction(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> SuiResult<Option<VerifiedCertificate>> {
        let transaction = self.perpetual_tables.certificates.get(transaction_digest)?;
        Ok(transaction.map(|t| t.into()))
    }

    pub fn multi_get_certified_transaction(
        &self,
        transaction_digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<VerifiedCertificate>>> {
        Ok(self
            .perpetual_tables
            .certificates
            .multi_get(transaction_digests)?
            .into_iter()
            .map(|o| o.map(|c| c.into()))
            .collect())
    }

    pub fn get_sui_system_state_object(&self) -> SuiResult<SuiSystemState> {
        self.perpetual_tables.get_sui_system_state_object()
    }

    /// Returns true if we have a transaction structure for this transaction digest
    pub fn transaction_exists(
        &self,
        cur_epoch: EpochId,
        transaction_digest: &TransactionDigest,
    ) -> SuiResult<bool> {
        let tx: Option<VerifiedSignedTransaction> =
            self.epoch_store().get_transaction(transaction_digest)?;
        Ok(if let Some(signed_tx) = tx {
            signed_tx.epoch() == cur_epoch
        } else {
            false
        })
    }

    pub fn get_signed_transaction_info(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> Result<VerifiedTransactionInfoResponse, SuiError> {
        Ok(VerifiedTransactionInfoResponse {
            signed_transaction: self.get_transaction(transaction_digest)?,
            certified_transaction: self
                .perpetual_tables
                .certificates
                .get(transaction_digest)?
                .map(|c| c.into()),
            signed_effects: self
                .perpetual_tables
                .executed_effects
                .get(transaction_digest)?,
        })
    }
}

impl BackingPackageStore for AuthorityStore {
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

impl ChildObjectResolver for AuthorityStore {
    fn read_child_object(&self, parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        let child_object = match self.get_object(child)? {
            None => return Ok(None),
            Some(o) => o,
        };
        let parent = *parent;
        if child_object.owner != Owner::ObjectOwner(parent.into()) {
            return Err(SuiError::InvalidChildObjectAccess {
                object: *child,
                given_parent: parent,
                actual_owner: child_object.owner,
            });
        }
        Ok(Some(child_object))
    }
}

impl ParentSync for AuthorityStore {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        Ok(self
            .get_latest_parent_entry(object_id)?
            .map(|(obj_ref, _)| obj_ref))
    }
}

impl ModuleResolver for AuthorityStore {
    type Error = SuiError;

    // TODO: duplicated code with ModuleResolver for InMemoryStorage in memory_storage.rs.
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

pub enum UpdateType {
    Transaction(TransactionEffectsDigest),
    Genesis,
}

pub trait EffectsStore {
    fn get_effects<'a>(
        &self,
        transactions: impl Iterator<Item = &'a TransactionDigest> + Clone,
    ) -> SuiResult<Vec<Option<SignedTransactionEffects>>>;
}

impl EffectsStore for Arc<AuthorityStore> {
    fn get_effects<'a>(
        &self,
        transactions: impl Iterator<Item = &'a TransactionDigest> + Clone,
    ) -> SuiResult<Vec<Option<SignedTransactionEffects>>> {
        Ok(self
            .perpetual_tables
            .executed_effects
            .multi_get(transactions)?)
    }
}
