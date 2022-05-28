// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::epoch::EpochInfoLocals;
use crate::gateway_state::GatewayTxSeqNumber;
use narwhal_executor::ExecutionIndices;
use rocksdb::Options;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::collections::BTreeSet;
use std::path::Path;
use sui_storage::{default_db_options, LockService};
use sui_types::base_types::SequenceNumber;
use sui_types::batch::{SignedBatch, TxSequenceNumber};
use sui_types::committee::EpochId;
use sui_types::crypto::{AuthoritySignInfo, EmptySignInfo};
use sui_types::object::{Owner, OBJECT_START_VERSION};
use tracing::{debug, error, trace};
use typed_store::rocks::{DBBatch, DBMap};
use typed_store::{reopen, traits::Map};

pub type AuthorityStore = SuiDataStore<false, true, AuthoritySignInfo>;
#[allow(dead_code)]
pub type ReplicaStore = SuiDataStore<true, false, EmptySignInfo>;
pub type GatewayStore = SuiDataStore<false, true, EmptySignInfo>;

const NUM_SHARDS: usize = 4096;

/// The key where the latest consensus index is stored in the database.
// TODO: Make a single table (e.g., called `variables`) storing all our lonely variables in one place.
const LAST_CONSENSUS_INDEX_ADDR: u64 = 0;

/// ALL_OBJ_VER determines whether we want to store all past
/// versions of every object in the store. Authority doesn't store
/// them, but other entities such as replicas will.
/// USE_LOCKS determines whether we maintain locks when updating state
/// this is because replicas for example don't currently require locks`
/// S is a template on Authority signature state. This allows SuiDataStore to be used on either
/// authorities or non-authorities. Specifically, when storing transactions and effects,
/// S allows SuiDataStore to either store the authority signed version or unsigned version.
pub struct SuiDataStore<const ALL_OBJ_VER: bool, const USE_LOCKS: bool, S> {
    /// This is a map between the object (ID, version) and the latest state of the object, namely the
    /// state that is needed to process new transactions. If an object is deleted its entry is
    /// removed from this map.
    ///
    /// Note that while this map can store all versions of an object, in practice it only stores
    /// the most recent version.
    objects: DBMap<ObjectKey, Object>,

    /// Stores all history versions of all objects.
    /// This is not needed by an authority, but is needed by a replica.
    #[allow(dead_code)]
    all_object_versions: DBMap<ObjectKey, Object>,

    /// The LockService this store depends on for locking functionality
    lock_service: LockService,

    /// Internal vector of locks to manage concurrent writes to the database
    lock_table: Vec<tokio::sync::Mutex<()>>,

    /// This is a an index of object references to currently existing objects, indexed by the
    /// composite key of the SuiAddress of their owner and the object ID of the object.
    /// This composite index allows an efficient iterator to list all objected currently owned
    /// by a specific user, and their object reference.
    owner_index: DBMap<(Owner, ObjectID), ObjectInfo>,

    /// This is map between the transaction digest and transactions found in the `transaction_lock`.
    /// NOTE: after a lock is deleted (after a certificate is processed) the corresponding entry here
    /// could be deleted, but right now this is only done on gateways, not done on authorities.
    transactions: DBMap<TransactionDigest, TransactionEnvelope<S>>,

    /// This is a map between the transaction digest and the corresponding certificate for all
    /// certificates that have been successfully processed by this authority. This set of certificates
    /// along with the genesis allows the reconstruction of all other state, and a full sync to this
    /// authority.
    certificates: DBMap<TransactionDigest, CertifiedTransaction>,

    /// The map between the object ref of objects processed at all versions and the transaction
    /// digest of the certificate that lead to the creation of this version of the object.
    ///
    /// When an object is deleted we include an entry into this table for its next version and
    /// a digest of ObjectDigest::deleted(), along with a link to the transaction that deleted it.
    parent_sync: DBMap<ObjectRef, TransactionDigest>,

    /// A map between the transaction digest of a certificate that was successfully processed
    /// (ie in `certificates`) and the effects its execution has on the authority state. This
    /// structure is used to ensure we do not double process a certificate, and that we can return
    /// the same response for any call after the first (ie. make certificate processing idempotent).
    effects: DBMap<TransactionDigest, TransactionEffectsEnvelope<S>>,

    /// Hold the lock for shared objects. These locks are written by a single task: upon receiving a valid
    /// certified transaction from consensus, the authority assigns a lock to each shared objects of the
    /// transaction. Note that all authorities are guaranteed to assign the same lock to these objects.
    /// TODO: These two maps should be merged into a single one (no reason to have two).
    sequenced: DBMap<(TransactionDigest, ObjectID), SequenceNumber>,
    schedule: DBMap<ObjectID, SequenceNumber>,

    // Tables used for authority batch structure
    /// A sequence on all executed certificates and effects.
    pub executed_sequence: DBMap<TxSequenceNumber, TransactionDigest>,

    /// A sequence of batches indexing into the sequence of executed transactions.
    pub batches: DBMap<TxSequenceNumber, SignedBatch>,

    /// The following table is used to store a single value (the corresponding key is a constant). The value
    /// represents the index of the latest consensus message this authority processed. This field is written
    /// by a single process acting as consensus (light) client. It is used to ensure the authority processes
    /// every message output by consensus (and in the right order).
    last_consensus_index: DBMap<u64, ExecutionIndices>,

    /// Map from each epoch ID to the epoch information.
    epochs: DBMap<EpochId, EpochInfoLocals>,
}

impl<
        const ALL_OBJ_VER: bool,
        const USE_LOCKS: bool,
        S: Eq + Serialize + for<'de> Deserialize<'de>,
    > SuiDataStore<ALL_OBJ_VER, USE_LOCKS, S>
{
    /// Open an authority store by directory path
    pub fn open<P: AsRef<Path>>(path: P, db_options: Option<Options>) -> Self {
        let (options, point_lookup) = default_db_options(db_options);

        let db = {
            let path = &path;
            let db_options = Some(options.clone());
            let opt_cfs: &[(&str, &rocksdb::Options)] = &[
                ("objects", &point_lookup),
                ("all_object_versions", &options),
                ("transactions", &point_lookup),
                ("owner_index", &options),
                ("certificates", &point_lookup),
                ("parent_sync", &options),
                ("effects", &point_lookup),
                ("sequenced", &options),
                ("schedule", &options),
                ("executed_sequence", &options),
                ("batches", &options),
                ("last_consensus_index", &options),
                ("epochs", &options),
            ];
            typed_store::rocks::open_cf_opts(path, db_options, opt_cfs)
        }
        .expect("Cannot open DB.");

        let executed_sequence =
            DBMap::reopen(&db, Some("executed_sequence")).expect("Cannot open CF.");

        let (
            objects,
            all_object_versions,
            owner_index,
            transactions,
            certificates,
            parent_sync,
            effects,
            sequenced,
            schedule,
            batches,
            last_consensus_index,
            epochs,
        ) = reopen! (
            &db,
            "objects";<ObjectKey, Object>,
            "all_object_versions";<ObjectKey, Object>,
            "owner_index";<(Owner, ObjectID), ObjectInfo>,
            "transactions";<TransactionDigest, TransactionEnvelope<S>>,
            "certificates";<TransactionDigest, CertifiedTransaction>,
            "parent_sync";<ObjectRef, TransactionDigest>,
            "effects";<TransactionDigest, TransactionEffectsEnvelope<S>>,
            "sequenced";<(TransactionDigest, ObjectID), SequenceNumber>,
            "schedule";<ObjectID, SequenceNumber>,
            "batches";<TxSequenceNumber, SignedBatch>,
            "last_consensus_index";<u64, ExecutionIndices>,
            "epochs";<EpochId, EpochInfoLocals>
        );

        // For now, create one LockService for each SuiDataStore, and we use a specific
        // subdir of the data store directory
        let lockdb_path = path.as_ref().join("lockdb");
        let lock_service =
            LockService::new(lockdb_path, None).expect("Could not initialize lockdb");

        Self {
            objects,
            all_object_versions,
            lock_service,
            lock_table: (0..NUM_SHARDS)
                .into_iter()
                .map(|_| tokio::sync::Mutex::new(()))
                .collect(),
            owner_index,
            transactions,
            certificates,
            parent_sync,
            effects,
            sequenced,
            schedule,
            executed_sequence,
            batches,
            last_consensus_index,
            epochs,
        }
    }

    /// Returns the TransactionEffects if we have an effects structure for this transaction digest
    pub fn get_effects(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> SuiResult<TransactionEffects> {
        self.effects
            .get(transaction_digest)?
            .map(|data| data.effects)
            .ok_or(SuiError::TransactionNotFound {
                digest: *transaction_digest,
            })
    }

    /// Returns true if we have an effects structure for this transaction digest
    pub fn effects_exists(&self, transaction_digest: &TransactionDigest) -> SuiResult<bool> {
        self.effects
            .contains_key(transaction_digest)
            .map_err(|e| e.into())
    }

    /// Returns true if we have a transaction structure for this transaction digest
    pub fn transaction_exists(&self, transaction_digest: &TransactionDigest) -> SuiResult<bool> {
        self.transactions
            .contains_key(transaction_digest)
            .map_err(|e| e.into())
    }

    /// Returns true if there are no objects in the database
    pub fn database_is_empty(&self) -> SuiResult<bool> {
        Ok(self
            .objects
            .iter()
            .skip_to(&ObjectKey::ZERO)?
            .next()
            .is_none())
    }

    pub fn next_sequence_number(&self) -> Result<TxSequenceNumber, SuiError> {
        Ok(self
            .executed_sequence
            .iter()
            .skip_prior_to(&TxSequenceNumber::MAX)?
            .next()
            .map(|(v, _)| v + 1u64)
            .unwrap_or(0))
    }

    #[cfg(test)]
    pub fn side_sequence(&self, seq: TxSequenceNumber, digest: &TransactionDigest) {
        self.executed_sequence.insert(&seq, digest).unwrap();
    }

    /// A function that acquires all locks associated with the objects (in order to avoid deadlocks).
    async fn acquire_locks<'a, 'b>(
        &'a self,
        input_objects: &'b [ObjectRef],
    ) -> Vec<tokio::sync::MutexGuard<'a, ()>> {
        if !USE_LOCKS {
            return vec![];
        }

        let num_locks = self.lock_table.len();
        // TODO: randomize the lock mapping based on a secret to avoid DoS attacks.
        let lock_numbers: BTreeSet<usize> = input_objects
            .iter()
            .map(|(_, _, digest)| {
                usize::from_le_bytes(digest.0[0..8].try_into().unwrap()) % num_locks
            })
            .collect();
        // Note: we need to iterate over the sorted unique elements, hence the use of a Set
        //       in order to prevent deadlocks when trying to acquire many locks.
        let mut locks = Vec::new();
        for lock_seq in lock_numbers {
            locks.push(self.lock_table[lock_seq].lock().await);
        }
        locks
    }

    // Methods to read the store
    pub fn get_owner_objects(&self, owner: Owner) -> Result<Vec<ObjectInfo>, SuiError> {
        debug!(?owner, "get_owner_objects");
        Ok(self
            .owner_index
            .iter()
            // The object id 0 is the smallest possible
            .skip_to(&(owner, ObjectID::ZERO))?
            .take_while(|((object_owner, _), _)| (object_owner == &owner))
            .map(|(_, object_info)| object_info)
            .collect())
    }

    pub fn get_object_version(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        Ok(self.objects.get(&ObjectKey(*object_id, version))?)
    }

    /// Read an object and return it, or Err(ObjectNotFound) if the object was not found.
    pub fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        let obj_entry = self
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
        // last, so that the write to self.parent_sync gets the last word.
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

    /// Read a transaction envelope via lock or returns Err(TransactionLockDoesNotExist) if the lock does not exist.
    pub async fn get_transaction_envelope(
        &self,
        object_ref: &ObjectRef,
    ) -> Result<Option<TransactionEnvelope<S>>, SuiError> {
        let transaction_option = self
            .lock_service
            .get_lock(*object_ref)
            .await?
            .ok_or(SuiError::TransactionLockDoesNotExist)?;

        match transaction_option {
            Some(tx_digest) => Ok(Some(
                self.transactions
                    .get(&tx_digest)?
                    .expect("Stored a lock without storing transaction?"),
            )),
            None => Ok(None),
        }
    }

    /// Read a certificate and return an option with None if it does not exist.
    pub fn read_certificate(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<CertifiedTransaction>, SuiError> {
        self.certificates.get(digest).map_err(|e| e.into())
    }

    /// Read the transactionDigest that is the parent of an object reference
    /// (ie. the transaction that created an object at this version.)
    pub fn parent(&self, object_ref: &ObjectRef) -> Result<Option<TransactionDigest>, SuiError> {
        self.parent_sync.get(object_ref).map_err(|e| e.into())
    }

    /// Batch version of `parent` function.
    pub fn multi_get_parents(
        &self,
        object_refs: &[ObjectRef],
    ) -> Result<Vec<Option<TransactionDigest>>, SuiError> {
        self.parent_sync
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
    pub fn sequenced<'a>(
        &self,
        transaction_digest: &TransactionDigest,
        object_ids: impl Iterator<Item = &'a ObjectID>,
    ) -> Result<Vec<Option<SequenceNumber>>, SuiError> {
        let keys = object_ids.map(|objid| (*transaction_digest, *objid));

        self.sequenced.multi_get(keys).map_err(SuiError::from)
    }

    /// Read a lock for a specific (transaction, shared object) pair.
    pub fn all_shared_locks(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> Result<Vec<(ObjectID, SequenceNumber)>, SuiError> {
        Ok(self
            .sequenced
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
        self.objects.insert(&object_ref.into(), object)?;

        // Update the index
        if object.get_single_owner().is_some() {
            self.owner_index.insert(
                &(object.owner, object_ref.0),
                &ObjectInfo::new(&object_ref, object),
            )?;
        }

        // Update the parent
        self.parent_sync
            .insert(&object_ref, &object.previous_transaction)?;

        self.lock_service
            .initialize_locks(&[object_ref], false /* is_force_reset */)
            .await?;

        Ok(())
    }

    /// This function is used by the bench.rs script, and should not be used in other contexts
    /// In particular it does not check the old locks before inserting new ones, so the objects
    /// must be new.
    pub async fn bulk_object_insert(&self, objects: &[&Object]) -> SuiResult<()> {
        let batch = self.objects.batch();
        let ref_and_objects: Vec<_> = objects
            .iter()
            .map(|o| (o.compute_object_reference(), o))
            .collect();

        batch
            .insert_batch(
                &self.objects,
                ref_and_objects
                    .iter()
                    .map(|(oref, o)| (ObjectKey::from(oref), **o)),
            )?
            .insert_batch(
                &self.owner_index,
                ref_and_objects
                    .iter()
                    .map(|(oref, o)| ((o.owner, oref.0), ObjectInfo::new(oref, o))),
            )?
            .insert_batch(
                &self.parent_sync,
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
        self.transactions.insert(&tx_digest, &transaction)?;

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
    pub async fn update_state<BackingPackageStore>(
        &self,
        temporary_store: AuthorityTemporaryStore<BackingPackageStore>,
        certificate: &CertifiedTransaction,
        effects: &TransactionEffectsEnvelope<S>,
        sequence_number: Option<TxSequenceNumber>,
    ) -> SuiResult {
        // Extract the new state from the execution
        // TODO: events are already stored in the TxDigest -> TransactionEffects store. Is that enough?
        let mut write_batch = self.certificates.batch();

        // Store the certificate indexed by transaction digest
        let transaction_digest: &TransactionDigest = certificate.digest();
        write_batch = write_batch.insert_batch(
            &self.certificates,
            std::iter::once((transaction_digest, certificate)),
        )?;

        // Store the signed effects of the transaction
        write_batch = write_batch.insert_batch(
            &self.effects,
            std::iter::once((transaction_digest, effects)),
        )?;

        // Cleanup the lock of the shared objects.
        let write_batch =
            self.remove_shared_objects_locks(write_batch, transaction_digest, certificate)?;

        // Safe to unwrap since the "true" flag ensures we get a sequence value back.
        self.batch_update_objects(
            write_batch,
            temporary_store,
            *transaction_digest,
            sequence_number,
        )
        .await
    }

    /// Persist temporary storage to DB for genesis modules
    pub async fn update_objects_state_for_genesis<BackingPackageStore>(
        &self,
        temporary_store: AuthorityTemporaryStore<BackingPackageStore>,
        transaction_digest: TransactionDigest,
    ) -> Result<(), SuiError> {
        debug_assert_eq!(transaction_digest, TransactionDigest::genesis());
        let write_batch = self.certificates.batch();
        self.batch_update_objects(write_batch, temporary_store, transaction_digest, None)
            .await
    }

    /// This is used by the Gateway to update its local store after a transaction succeeded
    /// on the authorities. Since we don't yet have local execution on the gateway, we will
    /// need to recreate the temporary store based on the inputs and effects to update it properly.
    pub async fn update_gateway_state(
        &self,
        active_inputs: Vec<(InputObjectKind, Object)>,
        mutated_objects: HashMap<ObjectRef, Object>,
        certificate: CertifiedTransaction,
        effects: TransactionEffectsEnvelope<S>,
        sequence_number: GatewayTxSeqNumber,
    ) -> SuiResult {
        let transaction_digest = certificate.digest();
        let mut temporary_store =
            AuthorityTemporaryStore::new(Arc::new(&self), active_inputs, *transaction_digest);
        for (_, object) in mutated_objects {
            temporary_store.write_object(object);
        }
        for obj_ref in &effects.effects.deleted {
            temporary_store.delete_object(&obj_ref.0, obj_ref.1, DeleteKind::Normal);
        }
        for obj_ref in &effects.effects.wrapped {
            temporary_store.delete_object(&obj_ref.0, obj_ref.1, DeleteKind::Wrap);
        }

        let mut write_batch = self.certificates.batch();

        // Store the certificate indexed by transaction digest
        write_batch = write_batch.insert_batch(
            &self.certificates,
            std::iter::once((transaction_digest, &certificate)),
        )?;

        // Store the unsigned effects of the transaction
        write_batch = write_batch.insert_batch(
            &self.effects,
            std::iter::once((transaction_digest, effects)),
        )?;

        // Once a transaction is done processing and effects committed, we no longer
        // need it in the transactions table. This also allows us to track pending
        // transactions.
        write_batch =
            write_batch.delete_batch(&self.transactions, std::iter::once(transaction_digest))?;
        self.batch_update_objects(
            write_batch,
            temporary_store,
            *transaction_digest,
            Some(sequence_number),
        )
        .await
    }

    /// Helper function for updating the objects in the state
    async fn batch_update_objects<BackingPackageStore>(
        &self,
        mut write_batch: DBBatch,
        temporary_store: AuthorityTemporaryStore<BackingPackageStore>,
        transaction_digest: TransactionDigest,
        seq_opt: Option<TxSequenceNumber>,
    ) -> Result<(), SuiError> {
        let (objects, active_inputs, written, deleted, _events) = temporary_store.into_inner();
        trace!(written =? written.values().map(|((obj_id, ver, _), _)| (obj_id, ver)).collect::<Vec<_>>(),
               "batch_update_objects: temp store written");

        let owned_inputs: Vec<_> = active_inputs
            .iter()
            .filter(|(id, _, _)| objects.get(id).unwrap().is_owned())
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
        write_batch = write_batch.delete_batch(&self.owner_index, old_object_owners)?;

        // Index the certificate by the objects mutated
        write_batch = write_batch.insert_batch(
            &self.parent_sync,
            written
                .iter()
                .map(|(_, (object_ref, _object_))| (object_ref, transaction_digest)),
        )?;

        // Index the certificate by the objects deleted
        write_batch = write_batch.insert_batch(
            &self.parent_sync,
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

        if ALL_OBJ_VER {
            // Keep all versions of every object if ALL_OBJ_VER is true.
            write_batch = write_batch.insert_batch(
                &self.all_object_versions,
                written
                    .iter()
                    .map(|(_, (object_ref, object))| (ObjectKey::from(object_ref), object)),
            )?;
        }

        // Update the indexes of the objects written
        write_batch = write_batch.insert_batch(
            &self.owner_index,
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
            &self.objects,
            written
                .iter()
                .map(|(_, (obj_ref, new_object))| (ObjectKey::from(obj_ref), new_object)),
        )?;

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
            if USE_LOCKS {
                self.lock_service.locks_exist(owned_inputs.clone()).await?;
            }

            if let Some(next_seq) = seq_opt {
                // Now we are sure we are going to execute, add to the sequence
                // number and insert into authority sequence.
                //
                // NOTE: it is possible that we commit to the database transactions
                //       out of order with respect to their sequence number. It is also
                //       possible for the authority to crash without committing the
                //       full sequence, and the batching logic needs to deal with this.
                write_batch = write_batch.insert_batch(
                    &self.executed_sequence,
                    std::iter::once((next_seq, transaction_digest)),
                )?;
            }

            // Atomic write of all data other than locks
            write_batch.write()?;
            trace!("Finished writing batch");

            if USE_LOCKS {
                // Initialize object locks for new objects.  After this point, transactions on new objects
                // can run.  So it is critical this is done AFTER objects are done writing.
                // TODO: add retries https://github.com/MystenLabs/sui/issues/1993
                let new_locks_to_init: Vec<_> = written
                    .iter()
                    .filter_map(|(_, (object_ref, new_object))| {
                        if new_object.is_owned() {
                            Some(*object_ref)
                        } else {
                            None
                        }
                    })
                    .collect();
                self.lock_service
                    .initialize_locks(&new_locks_to_init, false /* is_force_reset */)
                    .await?;

                // Remove the old lock - timing of this matters less
                let locks_to_remove = owned_inputs;
                self.lock_service.remove_locks(locks_to_remove).await?;
            }

            // implicit: drop(_mutexes);
        }

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

    /// Remove the shared objects locks. This function is not safety-critical and is only need to cleanup the store.
    pub fn remove_shared_objects_locks(
        &self,
        mut write_batch: DBBatch,
        transaction_digest: &TransactionDigest,
        transaction: &CertifiedTransaction,
    ) -> SuiResult<DBBatch> {
        let mut sequenced_to_delete = Vec::new();
        let mut schedule_to_delete = Vec::new();
        for object_id in transaction.shared_input_objects() {
            sequenced_to_delete.push((*transaction_digest, *object_id));
            if self.get_object(object_id)?.is_none() {
                schedule_to_delete.push(*object_id);
            }
        }
        write_batch = write_batch.delete_batch(&self.sequenced, sequenced_to_delete)?;
        write_batch = write_batch.delete_batch(&self.schedule, schedule_to_delete)?;
        Ok(write_batch)
    }

    /// Lock a sequence number for the shared objects of the input transaction. Also update the
    /// last consensus index.
    pub fn persist_certificate_and_lock_shared_objects(
        &self,
        certificate: CertifiedTransaction,
        consensus_index: ExecutionIndices,
    ) -> Result<(), SuiError> {
        // Make an iterator to save the certificate.
        let transaction_digest = *certificate.digest();
        let certificate_to_write = std::iter::once((transaction_digest, &certificate));

        // Make an iterator to update the locks of the transaction's shared objects.
        let ids = certificate.shared_input_objects();
        let versions = self.schedule.multi_get(ids)?;

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

        // Make an iterator to update the last consensus index.
        let index_to_write = std::iter::once((LAST_CONSENSUS_INDEX_ADDR, consensus_index));

        // Atomically store all elements.
        let mut write_batch = self.sequenced.batch();
        write_batch = write_batch.insert_batch(&self.certificates, certificate_to_write)?;
        write_batch = write_batch.insert_batch(&self.sequenced, sequenced_to_write)?;
        write_batch = write_batch.insert_batch(&self.schedule, schedule_to_write)?;
        write_batch = write_batch.insert_batch(&self.last_consensus_index, index_to_write)?;
        write_batch.write().map_err(SuiError::from)
    }

    pub fn transactions_in_seq_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> SuiResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self
            .executed_sequence
            .iter()
            .skip_to(&start)?
            .take_while(|(seq, _tx)| *seq < end)
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
    #[allow(clippy::type_complexity)]
    pub fn batches_and_transactions(
        &self,
        start: u64,
        end: u64,
    ) -> Result<(Vec<SignedBatch>, Vec<(TxSequenceNumber, TransactionDigest)>), SuiError> {
        /*
        Get all batches that include requested transactions. This includes the signed batch
        prior to the first requested transaction, the batch including the last requested
        transaction and all batches in between.

        So for example if we got a request for start: 3 end: 9 and we have:
        B0 T0 T1 B2 T2 T3 B3 T3 T4 T5 B6 T6 T8 T9

        This will return B2, B3, B6


        */
        let batches: Vec<SignedBatch> = self
            .batches
            .iter()
            .skip_prior_to(&start)?
            .take_while(|(_seq, batch)| batch.batch.initial_sequence_number < end)
            .map(|(_, batch)| batch)
            .collect();

        /*
        Get transactions in the retrieved batches. The first batch is included
        without transactions, so get transactions of all subsequent batches, or
        until the end of the sequence if the last batch does not contain the
        requested end sequence number.

        So for example if we got a request for start: 3 end: 9 and we have:
        B0 T0 T1 B2 T2 T3 B3 T3 T4 T5 B6 T6 T8 T9

        The code below will return T2 .. T6

        Note: T8 is out of order so the sequence returned ends at T6.

        */

        let first_seq = batches
            .first()
            .ok_or(SuiError::NoBatchesFoundError)?
            .batch
            .next_sequence_number;
        let mut last_seq = batches
            .last()
            .unwrap() // if the first exists the last exists too
            .batch
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

        let transactions: Vec<(TxSequenceNumber, TransactionDigest)> = self
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
        self.last_consensus_index
            .get(&LAST_CONSENSUS_INDEX_ADDR)
            .map(|x| x.unwrap_or_default())
            .map_err(SuiError::from)
    }

    pub fn get_transaction(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> SuiResult<Option<TransactionEnvelope<S>>> {
        let transaction = self.transactions.get(transaction_digest)?;
        Ok(transaction)
    }

    pub fn get_certified_transaction(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> SuiResult<Option<CertifiedTransaction>> {
        let transaction = self.certificates.get(transaction_digest)?;
        Ok(transaction)
    }

    pub fn insert_new_epoch_info(&self, epoch_info: EpochInfoLocals) -> SuiResult {
        self.epochs
            .insert(&epoch_info.committee.epoch(), &epoch_info)?;
        Ok(())
    }

    pub fn get_last_epoch_info(&self) -> SuiResult<EpochInfoLocals> {
        // unwrap safe since we guarantee to insert an epoch entry at genesis.
        Ok(self.epochs.iter().skip_to_last().next().unwrap().1)
    }

    #[cfg(test)]
    /// Provide read access to the `schedule` table (useful for testing).
    pub fn get_schedule(&self, object_id: &ObjectID) -> SuiResult<Option<SequenceNumber>> {
        self.schedule.get(object_id).map_err(SuiError::from)
    }
}

impl<const A: bool, const B: bool> SuiDataStore<A, B, AuthoritySignInfo> {
    pub fn get_signed_transaction_info(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        Ok(TransactionInfoResponse {
            signed_transaction: self.transactions.get(transaction_digest)?,
            certified_transaction: self.certificates.get(transaction_digest)?,
            signed_effects: self.effects.get(transaction_digest)?,
        })
    }
}

impl<const A: bool, const B: bool> SuiDataStore<A, B, EmptySignInfo> {
    pub fn pending_transactions(&self) -> &DBMap<TransactionDigest, Transaction> {
        &self.transactions
    }
}

impl<const A: bool, const B: bool, S: Eq + Serialize + for<'de> Deserialize<'de>>
    BackingPackageStore for SuiDataStore<A, B, S>
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

impl<const A: bool, const B: bool, S: Eq + Serialize + for<'de> Deserialize<'de>> ModuleResolver
    for SuiDataStore<A, B, S>
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

// The primary key type for object storage.
#[serde_as]
#[derive(Eq, PartialEq, Clone, Copy, PartialOrd, Ord, Hash, Serialize, Deserialize)]
struct ObjectKey(pub ObjectID, pub VersionNumber);

impl ObjectKey {
    pub const ZERO: ObjectKey = ObjectKey(ObjectID::ZERO, VersionNumber::MIN);

    pub fn max_for_id(id: &ObjectID) -> Self {
        Self(*id, VersionNumber::MAX)
    }
}

impl From<ObjectRef> for ObjectKey {
    fn from(object_ref: ObjectRef) -> Self {
        (&object_ref).into()
    }
}

impl From<&ObjectRef> for ObjectKey {
    fn from(object_ref: &ObjectRef) -> Self {
        Self(object_ref.0, object_ref.1)
    }
}

/// Persist the Genesis package to DB along with the side effects for module initialization
pub async fn store_package_and_init_modules_for_genesis<
    const A: bool,
    const B: bool,
    S: Eq + Serialize + for<'de> Deserialize<'de>,
>(
    store: &Arc<SuiDataStore<A, B, S>>,
    native_functions: &NativeFunctionTable,
    ctx: &mut TxContext,
    modules: Vec<CompiledModule>,
) -> SuiResult {
    let inputs = Transaction::input_objects_in_compiled_modules(&modules);
    let ids: Vec<_> = inputs.iter().map(|kind| kind.object_id()).collect();
    let input_objects = store.get_objects(&ids[..])?;
    // When publishing genesis packages, since the std framework packages all have
    // non-zero addresses, [`Transaction::input_objects_in_compiled_modules`] will consider
    // them as dependencies even though they are not. Hence input_objects contain objects
    // that don't exist on-chain because they are yet to be published.
    #[cfg(debug_assertions)]
    {
        let to_be_published_addresses: HashSet<_> = modules
            .iter()
            .map(|module| *module.self_id().address())
            .collect();
        assert!(
            // An object either exists on-chain, or is one of the packages to be published.
            inputs
                .iter()
                .zip(input_objects.iter())
                .all(|(kind, obj_opt)| obj_opt.is_some()
                    || to_be_published_addresses.contains(&kind.object_id()))
        );
    }
    let filtered = inputs
        .into_iter()
        .zip(input_objects.into_iter())
        .filter_map(|(input, object_opt)| object_opt.map(|object| (input, object)))
        .collect::<Vec<_>>();

    debug_assert!(ctx.digest() == TransactionDigest::genesis());
    let mut temporary_store = AuthorityTemporaryStore::new(store.clone(), filtered, ctx.digest());
    let package_id = ObjectID::from(*modules[0].self_id().address());
    let natives = native_functions.clone();
    let mut gas_status = SuiGasStatus::new_unmetered();
    let vm = adapter::verify_and_link(
        &temporary_store,
        &modules,
        package_id,
        natives,
        &mut gas_status,
    )?;
    adapter::store_package_and_init_modules(
        &mut temporary_store,
        &vm,
        modules,
        ctx,
        &mut gas_status,
    )?;
    store
        .update_objects_state_for_genesis(temporary_store, ctx.digest())
        .await
}

pub async fn generate_genesis_system_object<
    const A: bool,
    const B: bool,
    S: Eq + Serialize + for<'de> Deserialize<'de>,
>(
    store: &Arc<SuiDataStore<A, B, S>>,
    move_vm: &Arc<MoveVM>,
    committee: &Committee,
    genesis_ctx: &mut TxContext,
) -> SuiResult {
    let genesis_digest = genesis_ctx.digest();
    let mut temporary_store = AuthorityTemporaryStore::new(store.clone(), vec![], genesis_digest);
    let pubkeys: Vec<Vec<u8>> = committee
        .expanded_keys
        .values()
        .map(|pk| pk.to_bytes().to_vec())
        .collect();
    // TODO: May use separate sui address than derived from pubkey.
    let sui_addresses: Vec<AccountAddress> = committee
        .voting_rights
        .keys()
        .map(|pk| SuiAddress::from(pk).into())
        .collect();
    // TODO: Allow config to specify human readable validator names.
    let names: Vec<Vec<u8>> = (0..sui_addresses.len())
        .map(|i| Vec::from(format!("Validator{}", i).as_bytes()))
        .collect();
    // TODO: Change voting_rights to use u64 instead of usize.
    let stakes: Vec<u64> = committee
        .voting_rights
        .values()
        .map(|v| *v as u64)
        .collect();
    adapter::execute(
        move_vm,
        &mut temporary_store,
        ModuleId::new(SUI_FRAMEWORK_ADDRESS, ident_str!("Genesis").to_owned()),
        &ident_str!("create").to_owned(),
        vec![],
        vec![
            CallArg::Pure(bcs::to_bytes(&pubkeys).unwrap()),
            CallArg::Pure(bcs::to_bytes(&sui_addresses).unwrap()),
            CallArg::Pure(bcs::to_bytes(&names).unwrap()),
            // TODO: below is netaddress, for now just use names as we don't yet want to expose them.
            CallArg::Pure(bcs::to_bytes(&names).unwrap()),
            CallArg::Pure(bcs::to_bytes(&stakes).unwrap()),
        ],
        &mut SuiGasStatus::new_unmetered(),
        genesis_ctx,
    )?;
    store
        .update_objects_state_for_genesis(temporary_store, genesis_digest)
        .await
}
