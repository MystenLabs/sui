// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::iter;
use std::path::Path;
use std::sync::Arc;
use std::{fmt::Debug, path::PathBuf};

use arc_swap::ArcSwap;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use once_cell::sync::OnceCell;
use rocksdb::Options;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tracing::{debug, info, trace};

use narwhal_executor::ExecutionIndices;
use sui_storage::{
    lock_service::ObjectLockStatus,
    mutex_table::{LockGuard, MutexTable},
    LockService,
};
use sui_types::crypto::AuthoritySignInfo;
use sui_types::dynamic_field::{DynamicFieldInfo, DynamicFieldType};
use sui_types::message_envelope::VerifiedEnvelope;
use sui_types::object::Owner;
use sui_types::storage::ChildObjectResolver;
use sui_types::{base_types::SequenceNumber, storage::ParentSync};
use sui_types::{batch::TxSequenceNumber, object::PACKAGE_VERSION};
use typed_store::rocks::DBBatch;
use typed_store::traits::Map;

use crate::authority::authority_per_epoch_store::{
    AuthorityPerEpochStore, ExecutionIndicesWithHash,
};
use crate::authority::authority_store_tables::ExecutionIndicesWithHash;

use super::{
    authority_store_tables::{AuthorityEpochTables, AuthorityPerpetualTables},
    *,
};

pub type AuthorityStore = SuiDataStore<AuthoritySignInfo>;

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

    module_cache: RwLock<BTreeMap<ModuleId, Arc<CompiledModule>>>,
}

impl AuthorityStore {
    /// Open an authority store by directory path.
    /// If the store is empty, initialize it using genesis objects.
    pub async fn open(
        path: &Path,
        db_options: Option<Options>,
        genesis: &Genesis,
    ) -> SuiResult<Self> {
        let perpetual_tables = AuthorityPerpetualTables::open(path, db_options.clone());
        let committee = if perpetual_tables.database_is_empty()? {
            genesis.committee()?
        } else {
            perpetual_tables.get_committee()?
        };
        Self::open_inner(path, db_options, genesis, perpetual_tables, committee).await
    }

    pub async fn open_with_committee(
        path: &Path,
        db_options: Option<Options>,
        committee: &Committee,
        genesis: &Genesis,
    ) -> SuiResult<Self> {
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
            module_cache: Default::default(),
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

    pub(crate) fn reopen_epoch_db(&self, new_committee: Committee) {
        info!(new_epoch = ?new_committee.epoch, "re-opening AuthorityEpochTables for new epoch");
        let epoch_tables = Arc::new(AuthorityPerEpochStore::new(
            new_committee,
            &self.path,
            self.db_options.clone(),
        ));
        let previous_store = self.epoch_store.swap(epoch_tables);
        previous_store.epoch_terminated();
    }

    pub fn epoch_store(&self) -> Guard<Arc<AuthorityPerEpochStore>> {
        self.epoch_store.load()
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

    pub fn next_sequence_number(&self) -> Result<TxSequenceNumber, SuiError> {
        Ok(self
            .perpetual_tables
            .executed_sequence
            .iter()
            .skip_prior_to(&TxSequenceNumber::MAX)?
            .next()
            .map(|(v, _)| v + 1u64)
            .unwrap_or(0))
    }

    #[cfg(test)]
    pub fn side_sequence(&self, seq: TxSequenceNumber, digest: &ExecutionDigests) {
        self.perpetual_tables
            .executed_sequence
            .insert(&seq, digest)
            .unwrap();
    }

    /// A function that acquires all locks associated with the objects (in order to avoid deadlocks).
    async fn acquire_locks(&self, input_objects: &[ObjectRef]) -> Vec<LockGuard> {
        self.mutex_table
            .acquire_locks(input_objects.iter().map(|(_, _, digest)| *digest))
            .await
    }

    // Methods to read the store
    pub fn get_owner_objects(&self, owner: SuiAddress) -> Result<Vec<ObjectInfo>, SuiError> {
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

    // Methods to read the store
    pub fn get_dynamic_fields(&self, object: ObjectID) -> Result<Vec<DynamicFieldInfo>, SuiError> {
        debug!(?object, "get_dynamic_fields");
        Ok(self
            .perpetual_tables
            .dynamic_field_index
            .iter()
            // The object id 0 is the smallest possible
            .skip_to(&(object, ObjectID::ZERO))?
            .take_while(|((object_owner, _), _)| (object_owner == &object))
            .map(|(_, object_info)| object_info)
            .collect())
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

    pub async fn get_tx_sequence(
        &self,
        tx: TransactionDigest,
    ) -> SuiResult<Option<TxSequenceNumber>> {
        self.lock_service.get_tx_sequence(tx).await
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
            match object.owner {
                Owner::AddressOwner(addr) => self
                    .perpetual_tables
                    .owner_index
                    .insert(&(addr, object_ref.0), &ObjectInfo::new(&object_ref, object))?,
                Owner::ObjectOwner(object_id) => {
                    if let Some(info) = self.try_create_dynamic_field_info(
                        &object_ref,
                        object,
                        &Default::default(),
                    )? {
                        self.perpetual_tables
                            .dynamic_field_index
                            .insert(&(ObjectID::from(object_id), object_ref.0), &info)?
                    }
                }
                _ => {}
            }
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

    fn try_create_dynamic_field_info(
        &self,
        oref: &ObjectRef,
        o: &Object,
        uncommitted_objects: &BTreeMap<ObjectID, &Object>,
    ) -> SuiResult<Option<DynamicFieldInfo>> {
        // Skip if not a move object
        let Some(move_object) = o.data.try_as_move() else {
            return Ok(None);
        };
        // We only index dynamic field objects
        if !DynamicFieldInfo::is_dynamic_field(&move_object.type_) {
            return Ok(None);
        }
        let move_struct =
            move_object.to_move_struct_with_resolver(ObjectFormatOptions::default(), &self)?;

        let Some((name, type_, object_id)) = DynamicFieldInfo::parse_move_object(&move_struct)? else{
            return Ok(None)
        };

        Ok(Some(match type_ {
            DynamicFieldType::DynamicObject => {
                // Find the actual object from storage using the object id obtained from the wrapper.
                let (object_type, version, digest) =
                    if let Some(o) = uncommitted_objects.get(&object_id) {
                        o.data
                            .type_()
                            .map(|type_| (type_.clone(), o.version(), o.digest()))
                    } else if let Ok(Some(o)) = self.get_object(&object_id) {
                        o.data
                            .type_()
                            .map(|type_| (type_.clone(), o.version(), o.digest()))
                    } else {
                        None
                    }
                    .ok_or_else(|| SuiError::ObjectDeserializationError {
                        error: format!("Cannot found data for dynamic object {object_id}"),
                    })?;

                DynamicFieldInfo {
                    name,
                    type_,
                    object_type: object_type.to_string(),
                    object_id,
                    version,
                    digest,
                }
            }
            DynamicFieldType::DynamicField { .. } => DynamicFieldInfo {
                name,
                type_,
                object_type: move_object.type_.type_params[1].to_string(),
                object_id: oref.0,
                version: oref.1,
                digest: oref.2,
            },
        }))
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

        let (owner_insert, dynamic_field_insert): (Vec<_>, Vec<_>) = ref_and_objects
            .iter()
            .map(|(object_ref, o)| match o.owner {
                Owner::AddressOwner(address) => (
                    Some(((address, o.id()), ObjectInfo::new(object_ref, o))),
                    None,
                ),
                Owner::ObjectOwner(object_id) => (
                    None,
                    Some(((ObjectID::from(object_id), o.id()), (object_ref, o))),
                ),
                _ => (None, None),
            })
            .unzip();

        let owner_insert = owner_insert.into_iter().flatten();

        let uncommitted_objects = ref_and_objects
            .iter()
            .map(|((id, ..), o)| (*id, **o))
            .collect();

        let dynamic_field_insert = dynamic_field_insert
            .into_iter()
            .flatten()
            .flat_map(|(key, (oref, o))| {
                self.try_create_dynamic_field_info(oref, o, &uncommitted_objects)
                    .transpose()
                    .map(|info| info.map(|info| (key, info)))
            })
            .collect::<SuiResult<Vec<_>>>()?;

        batch
            .insert_batch(
                &self.perpetual_tables.objects,
                ref_and_objects
                    .iter()
                    .map(|(oref, o)| (ObjectKey::from(oref), **o)),
            )?
            .insert_batch(&self.perpetual_tables.owner_index, owner_insert)?
            .insert_batch(
                &self.perpetual_tables.dynamic_field_index,
                dynamic_field_insert,
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
        proposed_seq: TxSequenceNumber,
        effects: &SignedTransactionEffects,
        effects_digest: &TransactionEffectsDigest,
    ) -> SuiResult<TxSequenceNumber> {
        // Extract the new state from the execution
        // TODO: events are already stored in the TxDigest -> TransactionEffects store. Is that enough?
        let mut write_batch = self.perpetual_tables.certificates.batch();

        // Store the certificate indexed by transaction digest
        let transaction_digest: &TransactionDigest = certificate.digest();
        write_batch = write_batch.insert_batch(
            &self.perpetual_tables.certificates,
            iter::once((transaction_digest, certificate.serializable_ref())),
        )?;

        let seq = self
            .sequence_tx(
                write_batch,
                inner_temporary_store,
                transaction_digest,
                proposed_seq,
                effects,
                effects_digest,
            )
            .await?;

        self.effects_notify_read.notify(transaction_digest, effects);

        // Clean up the locks of shared objects. This should be done after we write effects, as
        // effects_exists is used as the guard to avoid re-writing locks for a previously
        // executed transaction. Otherwise, there can be left-over locks in the tables.
        // However, the issue is benign because epoch tables are cleaned up for each epoch.
        let deleted_objects: BTreeSet<_> = effects
            .deleted
            .iter()
            .map(|(object_id, _, _)| object_id)
            .collect();
        let mut deleted_shared_objects = Vec::new();
        for (object_id, _) in certificate.shared_input_objects() {
            if deleted_objects.contains(object_id) {
                deleted_shared_objects.push(*object_id);
            }
        }
        self.epoch_store()
            .delete_shared_object_versions(certificate.digest(), &deleted_shared_objects)?;

        Ok(seq)
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
        proposed_seq: TxSequenceNumber,
        effects: &SignedTransactionEffects,
        effects_digest: &TransactionEffectsDigest,
    ) -> SuiResult<TxSequenceNumber> {
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

        // Writing to executed_sequence must be done *after* writing to effects, so that we never
        // broadcast a sequenced transaction (via the batch system) for which no effects can be
        // retrieved.
        //
        // Currently we write both effects and executed_sequence in the same batch to avoid
        // consistency issues between the two (see #4395 for more details).
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
        let batch = batch.insert_batch(
            &self.perpetual_tables.executed_sequence,
            [(
                assigned_seq,
                ExecutionDigests::new(*transaction_digest, *effects_digest),
            )]
            .into_iter(),
        )?;

        batch.write()?;

        Ok(assigned_seq)
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
        let (old_object_owners, old_dynamic_fields): (Vec<_>, Vec<_>) =
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
                ))
                .map(|(owner, id)| match owner {
                    Owner::AddressOwner(address) => (Some((address, id)), None),
                    Owner::ObjectOwner(object_id) => (None, Some((ObjectID::from(object_id), id))),
                    _ => (None, None),
                })
                .unzip();

        let old_object_owners = old_object_owners.into_iter().flatten();
        let old_dynamic_fields = old_dynamic_fields.into_iter().flatten();

        // Delete the old owner index entries
        write_batch =
            write_batch.delete_batch(&self.perpetual_tables.owner_index, old_object_owners)?;
        write_batch = write_batch.delete_batch(
            &self.perpetual_tables.dynamic_field_index,
            old_dynamic_fields,
        )?;

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
        let uncommitted_objects = written.iter().map(|(id, (_, o, _))| (*id, o)).collect();
        // Update the indexes of the objects written
        let (owner_written, dynamic_field_written): (Vec<_>, Vec<_>) = written
            .iter()
            .map(|(id, (object_ref, new_object, _))| match new_object.owner {
                Owner::AddressOwner(address) => (
                    Some(((address, *id), ObjectInfo::new(object_ref, new_object))),
                    None,
                ),
                Owner::ObjectOwner(object_id) => (
                    None,
                    Some(((ObjectID::from(object_id), *id), (object_ref, new_object))),
                ),
                _ => (None, None),
            })
            .unzip();

        let owner_written = owner_written.into_iter().flatten();
        let dynamic_field_written = dynamic_field_written
            .into_iter()
            .flatten()
            .flat_map(|(key, (oref, o))| {
                self.try_create_dynamic_field_info(oref, o, &uncommitted_objects)
                    .transpose()
                    .map(|info| info.map(|info| (key, info)))
            })
            .collect::<SuiResult<Vec<_>>>()?;

        write_batch =
            write_batch.insert_batch(&self.perpetual_tables.owner_index, owner_written)?;
        write_batch = write_batch.insert_batch(
            &self.perpetual_tables.dynamic_field_index,
            dynamic_field_written,
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
                .filter_map(|(_, (object_ref, new_object, _kind))| {
                    if new_object.is_address_owned() {
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
        let (owners_to_delete, dynamic_field_to_delete): (Vec<_>, Vec<_>) = effects
            .created
            .iter()
            .chain(effects.unwrapped.iter())
            .chain(effects.mutated.iter())
            .map(|((id, _, _), owner)| match owner {
                Owner::AddressOwner(addr) => (Some((*addr, *id)), None),
                Owner::ObjectOwner(object_id) => (None, Some((ObjectID::from(*object_id), *id))),
                _ => (None, None),
            })
            .unzip();

        let owners_to_delete = owners_to_delete.into_iter().flatten();
        let dynamic_field_to_delete = dynamic_field_to_delete.into_iter().flatten();

        write_batch =
            write_batch.delete_batch(&self.perpetual_tables.owner_index, owners_to_delete)?;
        write_batch = write_batch.delete_batch(
            &self.perpetual_tables.dynamic_field_index,
            dynamic_field_to_delete,
        )?;
        let modified_object_keys = effects
            .modified_at_versions
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
        let (old_objects_and_locks, old_dynamic_fields): (Vec<_>, Vec<_>) = self
            .perpetual_tables
            .objects
            .multi_get(modified_object_keys)?
            .into_iter()
            .map(|obj_opt| {
                let obj = obj_opt.expect("Older object version not found");
                match obj.owner {
                    Owner::AddressOwner(addr) => {
                        let oref = obj.compute_object_reference();
                        (
                            Some((((addr, obj.id()), ObjectInfo::new(&oref, &obj)), oref)),
                            None,
                        )
                    }
                    Owner::ObjectOwner(object_id) => {
                        (None, Some(((ObjectID::from(object_id), obj.id()), obj)))
                    }
                    _ => (None, None),
                }
            })
            .unzip();

        let (old_objects, old_locks): (Vec<_>, Vec<_>) =
            old_objects_and_locks.into_iter().flatten().unzip();
        let old_dynamic_fields = old_dynamic_fields
            .into_iter()
            .flatten()
            .flat_map(|(key, o)| {
                self.try_create_dynamic_field_info(
                    &o.compute_object_reference(),
                    &o,
                    &Default::default(),
                )
                .transpose()
                .map(|info| info.map(|info| (key, info)))
            })
            .collect::<SuiResult<Vec<_>>>()?;

        write_batch =
            write_batch.insert_batch(&self.perpetual_tables.owner_index, old_modified_objects)?;
        write_batch = write_batch.insert_batch(
            &self.perpetual_tables.dynamic_field_index,
            old_dynamic_fields,
        )?;

        write_batch.write()?;

        self.lock_service.initialize_locks(&old_locks, true).await?;
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
        if self.effects_exists(certificate.digest())? {
            return Ok(());
        }
        self.epoch_store().set_assigned_shared_object_versions(
            certificate.digest(),
            &effects
                .shared_objects
                .iter()
                .map(|(id, version, _)| (*id, *version))
                .collect(),
        )
    }

    pub fn consensus_message_processed(
        &self,
        key: &ConsensusTransactionKey,
    ) -> Result<bool, SuiError> {
        self.epoch_store().is_consensus_message_processed(key)
    }

    pub fn sent_end_of_publish(&self, authority: &AuthorityName) -> SuiResult<bool> {
        self.epoch_store().has_sent_end_of_publish(authority)
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

    /// Caller is responsible to call consensus_message_processed before this method
    pub async fn record_owned_object_cert_from_consensus(
        &self,
        transaction: &ConsensusTransaction,
        certificate: &VerifiedCertificate,
        consensus_index: ExecutionIndicesWithHash,
    ) -> Result<(), SuiError> {
        let key = transaction.key();
        self.epoch_store()
            .finish_consensus_certificate_process(key, certificate, consensus_index)
    }

    /// Locks a sequence number for the shared objects of the input transaction. Also updates the
    /// last consensus index, consensus_message_processed and pending_certificates tables.
    /// This function must only be called from the consensus task (i.e. from handle_consensus_transaction).
    ///
    /// Caller is responsible to call consensus_message_processed before this method
    pub async fn record_shared_object_cert_from_consensus(
        &self,
        transaction: &ConsensusTransaction,
        certificate: &VerifiedCertificate,
        consensus_index: ExecutionIndicesWithHash,
    ) -> Result<(), SuiError> {
        // Make an iterator to save the certificate.
        let transaction_digest = *certificate.digest();

        // Make an iterator to update the locks of the transaction's shared objects.
        let ids = certificate.shared_input_objects().map(|(id, _)| id);
        let epoch_store = self.epoch_store();
        let versions = epoch_store.multi_get_next_shared_object_versions(ids)?;

        let mut input_object_keys = certificate_input_object_keys(certificate)?;
        let mut assigned_versions = Vec::new();
        for ((id, initial_shared_version), v) in
            certificate.shared_input_objects().zip(versions.iter())
        {
            // On epoch changes, the `next_shared_object_versions` table will be empty, and we rely on
            // parent sync to recover the current version of the object.  However, if an object was
            // previously aware of the object as owned, and it was upgraded to shared, the version
            // in parent sync may be out of date, causing a fork.  In that case, we know that the
            // `initial_shared_version` will be greater than the version in parent sync, and we can
            // use that.  It is the version that the object was shared at, and can be trusted
            // because it has been checked and signed by a quorum of other validators when creating
            // the certificate.
            let version = match v {
                Some(v) => *v,
                None => *initial_shared_version.max(
                    &self
                        // TODO: if we use an eventually consistent object store in the future,
                        // we must make this read strongly consistent somehow!
                        .get_latest_parent_entry(*id)?
                        .map(|(objref, _)| objref.1)
                        .unwrap_or_default(),
                ),
            };

            assigned_versions.push((*id, version));
            input_object_keys.push(ObjectKey(*id, version));
        }

        let next_version =
            SequenceNumber::lamport_increment(input_object_keys.iter().map(|obj| obj.1));
        let next_versions: Vec<_> = assigned_versions
            .iter()
            .map(|(id, _)| (*id, next_version))
            .collect();

        trace!(tx_digest = ?transaction_digest,
               ?assigned_versions, ?next_version,
               "locking shared objects");

        // Make an iterator to update the last consensus index.

        // Holding _tx_lock avoids the following race:
        // - we check effects_exist, returns false
        // - another task (starting from handle_node_sync_certificate) writes effects,
        //    and then deletes locks from assigned_shared_object_versions
        // - we write to assigned_object versions, re-creating the locks that were just deleted
        // - now it's possible to run a new tx against old versions of the shared objects.
        let _tx_lock = epoch_store.acquire_tx_lock(&transaction_digest).await;

        // Note: if we crash here we are not in an inconsistent state since
        //       it is ok to just update the pending list without updating the sequence.

        epoch_store.finish_assign_shared_object_versions(
            transaction.key(),
            certificate,
            consensus_index,
            assigned_versions,
            next_versions,
        )
    }

    /// Returns transaction digests from consensus_message_order table in the "checkpoint range".
    ///
    /// Checkpoint range is defined from the last seen checkpoint(excluded) to the provided
    /// to_height (included)
    pub fn last_checkpoint(
        &self,
        to_height_included: u64,
    ) -> SuiResult<Option<(u64, Vec<TransactionDigest>)>> {
        let epoch_tables = self.epoch_store();

        let Some((index, from_height_excluded)) = epoch_tables.get_last_checkpoint_boundary() else {
            return Ok(None);
        };
        if from_height_excluded >= to_height_included {
            // Due to crash recovery we might enter this function twice for same boundary
            debug!("Not returning last checkpoint - already processed");
            return Ok(None);
        }

        let roots = epoch_tables
            .get_transactions_in_checkpoint_range(from_height_excluded, to_height_included)?;

        debug!(
            "Selected {} roots between narwhal commit rounds {} and {}",
            roots.len(),
            from_height_excluded,
            to_height_included
        );

        Ok(Some((index, roots)))
    }

    pub fn final_epoch_checkpoint(&self) -> SuiResult<Option<u64>> {
        self.epoch_store().final_epoch_checkpoint()
    }

    pub fn record_checkpoint_boundary(&self, commit_round: u64) -> SuiResult {
        let epoch_tables = self.epoch_store();

        if let Some((index, height)) = epoch_tables.get_last_checkpoint_boundary() {
            if height >= commit_round {
                // Due to crash recovery we might see same boundary twice
                debug!("Not recording checkpoint boundary - already updated");
            } else {
                let index = index + 1;
                debug!(
                    "Recording checkpoint boundary {} at {}",
                    index, commit_round
                );
                epoch_tables.insert_checkpoint_boundary(index, commit_round)?;
            }
        } else {
            // Table is empty
            debug!("Recording first checkpoint boundary at {}", commit_round);
            epoch_tables.insert_checkpoint_boundary(0, commit_round)?;
        }
        Ok(())
    }

    pub fn transactions_in_seq_range(
        &self,
        start: u64,
        end: u64,
    ) -> SuiResult<Vec<(u64, ExecutionDigests)>> {
        Ok(self
            .perpetual_tables
            .executed_sequence
            .iter()
            .skip_to(&start)?
            .take_while(|(seq, _tx)| *seq < end)
            .collect())
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

impl<S: Eq + Debug + Serialize + for<'de> Deserialize<'de>> GetModule for &SuiDataStore<S> {
    type Error = SuiError;
    type Item = Arc<CompiledModule>;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<Arc<CompiledModule>>, Self::Error> {
        if let Some(compiled_module) = self.module_cache.read().get(id) {
            return Ok(Some(compiled_module.clone()));
        }

        if let Some(module_bytes) = self.get_module(id)? {
            let module = Arc::new(CompiledModule::deserialize(&module_bytes).map_err(|e| {
                SuiError::ModuleDeserializationFailure {
                    error: e.to_string(),
                }
            })?);

            self.module_cache.write().insert(id.clone(), module.clone());
            Ok(Some(module))
        } else {
            Ok(None)
        }
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
pub struct ObjectKey(pub ObjectID, pub VersionNumber);

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

/// Fetch the `ObjectKey`s (IDs and versions) for non-shared input objects.  Includes owned,
/// and immutable objects as well as the gas objects, but not move packages or shared objects.
fn certificate_input_object_keys(certificate: &VerifiedCertificate) -> SuiResult<Vec<ObjectKey>> {
    Ok(certificate
        .data()
        .intent_message
        .value
        .input_objects()?
        .into_iter()
        .filter_map(|object| {
            use InputObjectKind::*;
            match object {
                MovePackage(_) | SharedMoveObject { .. } => None,
                ImmOrOwnedMoveObject(obj) => Some(obj.into()),
            }
        })
        .collect())
}
