// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::gateway_state::GatewayTxSeqNumber;
use narwhal_executor::ExecutionIndices;
use rocksdb::Options;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::convert::TryInto;
use std::path::Path;
use sui_types::base_types::SequenceNumber;
use sui_types::batch::{SignedBatch, TxSequenceNumber};
use sui_types::crypto::{AuthoritySignInfo, EmptySignInfo};
use sui_types::object::OBJECT_START_VERSION;
use tracing::warn;
use typed_store::rocks::{DBBatch, DBMap};

use typed_store::{reopen, traits::Map};

pub type AuthorityStore = SuiDataStore<false, AuthoritySignInfo>;
#[allow(dead_code)]
pub type ReplicaStore = SuiDataStore<true, EmptySignInfo>;
pub type GatewayStore = SuiDataStore<false, EmptySignInfo>;

const NUM_SHARDS: usize = 4096;

/// The key where the latest consensus index is stored in the database.
// TODO: Make a single table (e.g., called `variables`) storing all our lonely variables in one place.
const LAST_CONSENSUS_INDEX_ADDR: u64 = 0;

/// ALL_OBJ_VER determines whether we want to store all past
/// versions of every object in the store. Authority doesn't store
/// them, but other entities such as replicas will.
/// S is a template on Authority signature state. This allows SuiDataStore to be used on either
/// authorities or non-authorities. Specifically, when storing transactions and effects,
/// S allows SuiDataStore to either store the authority signed version or unsigned version.
pub struct SuiDataStore<const ALL_OBJ_VER: bool, S> {
    /// This is a map between the object ID and the latest state of the object, namely the
    /// state that is needed to process new transactions. If an object is deleted its entry is
    /// removed from this map.
    objects: DBMap<ObjectID, Object>,

    /// Stores all history versions of all objects.
    /// This is not needed by an authority, but is needed by a replica.
    #[allow(dead_code)]
    all_object_versions: DBMap<(ObjectID, SequenceNumber), Object>,

    /// This is a map between object references of currently active objects that can be mutated,
    /// and the transaction that they are lock on for use by this specific authority. Where an object
    /// lock exists for an object version, but no transaction has been seen using it the lock is set
    /// to None. The safety of consistent broadcast depend on each honest authority never changing
    /// the lock once it is set. After a certificate for this object is processed it can be
    /// forgotten.
    transaction_lock: DBMap<ObjectRef, Option<TransactionDigest>>,

    /// This is a an index of object references to currently existing objects, indexed by the
    /// composite key of the SuiAddress of their owner and the object ID of the object.
    /// This composite index allows an efficient iterator to list all objected currently owned
    /// by a specific user, and their object reference.
    owner_index: DBMap<(SuiAddress, ObjectID), ObjectRef>,

    /// This is map between the transaction digest and transactions found in the `transaction_lock`.
    /// NOTE: after a lock is deleted the corresponding entry here could be deleted, but right
    /// now this is not done. If a certificate is processed (see `certificates`) the
    /// transaction can also be deleted from this structure.
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

    /// Internal vector of locks to manage concurrent writes to the database
    lock_table: Vec<parking_lot::Mutex<()>>,

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
}

impl<const ALL_OBJ_VER: bool, S: Eq + Serialize + for<'de> Deserialize<'de>>
    SuiDataStore<ALL_OBJ_VER, S>
{
    /// Open an authority store by directory path
    pub fn open<P: AsRef<Path>>(path: P, db_options: Option<Options>) -> Self {
        let mut options = db_options.unwrap_or_default();

        // One common issue when running tests on Mac is that the default ulimit is too low,
        // leading to I/O errors such as "Too many open files". Raising fdlimit to bypass it.
        options.set_max_open_files((fdlimit::raise_fd_limit().unwrap() / 8) as i32);

        /* The table cache is locked for updates and this determines the number
           of shareds, ie 2^10. Increase in case of lock contentions.
        */
        let row_cache = rocksdb::Cache::new_lru_cache(1_000_000).expect("Cache is ok");
        options.set_row_cache(&row_cache);
        options.set_table_cache_num_shard_bits(10);
        options.set_compression_type(rocksdb::DBCompressionType::None);

        let mut point_lookup = options.clone();
        point_lookup.optimize_for_point_lookup(1024 * 1024);
        point_lookup.set_memtable_whole_key_filtering(true);

        let transform = rocksdb::SliceTransform::create("bytes_8_to_16", |key| &key[8..16], None);
        point_lookup.set_prefix_extractor(transform);
        point_lookup.set_memtable_prefix_bloom_ratio(0.2);

        let db = {
            let path = &path;
            let db_options = Some(options.clone());
            let opt_cfs: &[(&str, &rocksdb::Options)] = &[
                ("objects", &point_lookup),
                ("all_object_versions", &options),
                ("transactions", &point_lookup),
                ("owner_index", &options),
                ("transaction_lock", &point_lookup),
                ("certificates", &point_lookup),
                ("parent_sync", &options),
                ("effects", &point_lookup),
                ("sequenced", &options),
                ("schedule", &options),
                ("executed_sequence", &options),
                ("batches", &options),
                ("last_consensus_index", &options),
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
            transaction_lock,
            transactions,
            certificates,
            parent_sync,
            effects,
            sequenced,
            schedule,
            batches,
            last_consensus_index,
        ) = reopen! (
            &db,
            "objects";<ObjectID, Object>,
            "all_object_versions";<(ObjectID, SequenceNumber), Object>,
            "owner_index";<(SuiAddress, ObjectID), ObjectRef>,
            "transaction_lock";<ObjectRef, Option<TransactionDigest>>,
            "transactions";<TransactionDigest, TransactionEnvelope<S>>,
            "certificates";<TransactionDigest, CertifiedTransaction>,
            "parent_sync";<ObjectRef, TransactionDigest>,
            "effects";<TransactionDigest, TransactionEffectsEnvelope<S>>,
            "sequenced";<(TransactionDigest, ObjectID), SequenceNumber>,
            "schedule";<ObjectID, SequenceNumber>,
            "batches";<TxSequenceNumber, SignedBatch>,
            "last_consensus_index";<u64, ExecutionIndices>
        );
        Self {
            objects,
            all_object_versions,
            owner_index,
            transaction_lock,
            transactions,
            certificates,
            parent_sync,
            effects,
            sequenced,
            schedule,
            lock_table: (0..NUM_SHARDS)
                .into_iter()
                .map(|_| parking_lot::Mutex::new(()))
                .collect(),
            executed_sequence,
            batches,
            last_consensus_index,
        }
    }

    /// Returns true if we have a signed_effects structure for this transaction digest
    pub fn effects_exists(&self, transaction_digest: &TransactionDigest) -> SuiResult<bool> {
        self.effects
            .contains_key(transaction_digest)
            .map_err(|e| e.into())
    }

    /// Returns true if we have a signed_effects structure for this transaction digest
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
            .skip_to(&ObjectID::ZERO)?
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
    fn acquire_locks(&self, _input_objects: &[ObjectRef]) -> Vec<parking_lot::MutexGuard<'_, ()>> {
        let num_locks = self.lock_table.len();
        // TODO: randomize the lock mapping based on a secret to avoid DoS attacks.
        let lock_number: BTreeSet<usize> = _input_objects
            .iter()
            .map(|(_, _, digest)| {
                usize::from_le_bytes(digest.0[0..8].try_into().unwrap()) % num_locks
            })
            .collect();
        // Note: we need to iterate over the sorted unique elements, hence the use of a Set
        //       in order to prevent deadlocks when trying to acquire many locks.
        lock_number
            .into_iter()
            .map(|lock_seq| self.lock_table[lock_seq].lock())
            .collect()
    }

    // Methods to read the store

    pub fn get_account_objects(&self, account: SuiAddress) -> Result<Vec<ObjectRef>, SuiError> {
        Ok(self
            .owner_index
            .iter()
            // The object id 0 is the smallest possible
            .skip_to(&(account, ObjectID::ZERO))?
            .take_while(|((owner, _id), _object_ref)| (owner == &account))
            .map(|((_owner, _id), object_ref)| object_ref)
            .collect())
    }

    /// Read an object and return it, or Err(ObjectNotFound) if the object was not found.
    pub fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        self.objects.get(object_id).map_err(|e| e.into())
    }

    /// Get many objects
    pub fn get_objects(&self, _objects: &[ObjectID]) -> Result<Vec<Option<Object>>, SuiError> {
        self.objects.multi_get(_objects).map_err(|e| e.into())
    }

    /// Read a lock or returns Err(TransactionLockDoesNotExist) if the lock does not exist.
    pub fn get_transaction_lock(
        &self,
        object_ref: &ObjectRef,
    ) -> Result<Option<TransactionEnvelope<S>>, SuiError> {
        let transaction_option = self
            .transaction_lock
            .get(object_ref)?
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
    pub fn insert_genesis_object(&self, object: Object) -> SuiResult {
        // We only side load objects with a genesis parent transaction.
        debug_assert!(object.previous_transaction == TransactionDigest::genesis());
        let object_ref = object.compute_object_reference();
        self.insert_object_direct(object_ref, &object)
    }

    /// Insert an object directly into the store, and also update relevant tables, including
    /// initiating the transaction lock. This is used by the gateway to insert object directly.
    /// TODO: We need this today because we don't have another way to sync an account.
    pub fn insert_object_direct(&self, object_ref: ObjectRef, object: &Object) -> SuiResult {
        // Insert object
        self.objects.insert(&object_ref.0, object)?;

        self.transaction_lock.get_or_insert(&object_ref, || None)?;

        // Update the index
        if let Some(address) = object.get_single_owner() {
            self.owner_index
                .insert(&(address, object_ref.0), &object_ref)?;
        }
        // Update the parent
        self.parent_sync
            .insert(&object_ref, &object.previous_transaction)?;

        Ok(())
    }

    /// This function is used by the bench.rs script, and should not be used in other contexts
    /// In particular it does not check the old locks before inserting new ones, so the objects
    /// must be new.
    pub fn bulk_object_insert(&self, objects: &[&Object]) -> SuiResult<()> {
        let batch = self.objects.batch();
        let ref_and_objects: Vec<_> = objects
            .iter()
            .map(|o| (o.compute_object_reference(), o))
            .collect();

        batch
            .insert_batch(
                &self.objects,
                ref_and_objects.iter().map(|(oref, o)| (oref.0, **o)),
            )?
            .insert_batch(
                &self.transaction_lock,
                ref_and_objects.iter().map(|(oref, _)| (oref, None)),
            )?
            .insert_batch(
                &self.owner_index,
                ref_and_objects.iter().filter_map(|(oref, o)| {
                    o.get_single_owner()
                        .map(|address| ((address, oref.0), oref))
                }),
            )?
            .insert_batch(
                &self.parent_sync,
                ref_and_objects
                    .iter()
                    .map(|(oref, o)| (oref, o.previous_transaction)),
            )?
            .write()?;

        Ok(())
    }

    /// Set the transaction lock to a specific transaction
    ///
    /// This function checks all locks exist, are either None or equal to the passed transaction
    /// and then sets them to the transaction. Otherwise an Err is returned. Locks are set
    /// atomically in this implementation.
    ///
    pub fn set_transaction_lock(
        &self,
        owned_input_objects: &[ObjectRef],
        tx_digest: TransactionDigest,
        transaction: TransactionEnvelope<S>,
    ) -> Result<(), SuiError> {
        let lock_batch = self
            .transaction_lock
            .batch()
            .insert_batch(
                &self.transaction_lock,
                owned_input_objects
                    .iter()
                    .map(|obj_ref| (obj_ref, Some(tx_digest))),
            )?
            .insert_batch(
                &self.transactions,
                std::iter::once((tx_digest, transaction)),
            )?;

        // This is the critical region: testing the locks and writing the
        // new locks must be atomic, and not writes should happen in between.
        let mut need_write = false;
        {
            // Acquire the lock to ensure no one else writes when we are in here.
            // MutexGuards are unlocked on drop (ie end of this block)
            let _mutexes = self.acquire_locks(owned_input_objects);

            let locks = self.transaction_lock.multi_get(owned_input_objects)?;

            for lock in locks {
                // The object / version must exist, and therefore lock initialized.
                let lock = lock.ok_or(SuiError::TransactionLockDoesNotExist)?;

                if let Some(previous_tx_digest) = lock {
                    if previous_tx_digest != tx_digest {
                        warn!(prev_tx_digest =? previous_tx_digest,
                              cur_tx_digest =? tx_digest,
                              "Conflicting transaction!  Lock state changed in unexpected way");
                        return Err(SuiError::ConflictingTransaction {
                            pending_transaction: previous_tx_digest,
                        });
                    }
                } else {
                    need_write |= true;
                }
            }

            if need_write {
                // Atomic write of all locks
                lock_batch.write()?
            }

            // Implicit: drop(_mutexes);
        } // End of critical region

        Ok(())
    }

    /// Updates the state resulting from the execution of a certificate.
    ///
    /// Internally it checks that all locks for active inputs are at the correct
    /// version, and then writes locks, objects, certificates, parents atomically.
    pub fn update_state<BackingPackageStore>(
        &self,
        temporary_store: AuthorityTemporaryStore<BackingPackageStore>,
        certificate: &CertifiedTransaction,
        effects: &TransactionEffectsEnvelope<S>,
        sequence_number: Option<TxSequenceNumber>,
    ) -> SuiResult {
        // Extract the new state from the execution
        // TODO: events are already stored in the TxDigest -> TransactionEffects store. Is that enough?
        let mut write_batch = self.transaction_lock.batch();

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
        let write_batch = self.remove_shared_objects_locks(
            write_batch,
            transaction_digest,
            &certificate.transaction,
        )?;

        // Safe to unwrap since the "true" flag ensures we get a sequence value back.
        self.batch_update_objects(
            write_batch,
            temporary_store,
            *transaction_digest,
            sequence_number,
        )
    }

    /// Persist temporary storage to DB for genesis modules
    pub fn update_objects_state_for_genesis<BackingPackageStore>(
        &self,
        temporary_store: AuthorityTemporaryStore<BackingPackageStore>,
        transaction_digest: TransactionDigest,
    ) -> Result<(), SuiError> {
        debug_assert_eq!(transaction_digest, TransactionDigest::genesis());
        let write_batch = self.transaction_lock.batch();
        self.batch_update_objects(write_batch, temporary_store, transaction_digest, None)
    }

    /// This is used by the Gateway to update its local store after a transaction succeeded
    /// on the authorities. Since we don't yet have local execution on the gateway, we will
    /// need to recreate the temporary store based on the inputs and effects to update it properly.
    pub fn update_gateway_state(
        &self,
        active_inputs: Vec<(InputObjectKind, Object)>,
        mutated_objects: HashMap<ObjectRef, Object>,
        certificate: CertifiedTransaction,
        effects: TransactionEffects,
        sequence_number: GatewayTxSeqNumber,
    ) -> SuiResult {
        let transaction_digest = certificate.digest();
        let mut temporary_store =
            AuthorityTemporaryStore::new(Arc::new(&self), active_inputs, *transaction_digest);
        for (_, object) in mutated_objects {
            temporary_store.write_object(object);
        }
        for obj_ref in &effects.deleted {
            temporary_store.delete_object(&obj_ref.0, obj_ref.1, DeleteKind::Normal);
        }
        for obj_ref in &effects.wrapped {
            temporary_store.delete_object(&obj_ref.0, obj_ref.1, DeleteKind::Wrap);
        }

        let mut write_batch = self.transaction_lock.batch();

        // Store the certificate indexed by transaction digest
        write_batch = write_batch.insert_batch(
            &self.certificates,
            std::iter::once((transaction_digest, &certificate)),
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
    }

    /// Helper function for updating the objects in the state
    fn batch_update_objects<BackingPackageStore>(
        &self,
        mut write_batch: DBBatch,
        temporary_store: AuthorityTemporaryStore<BackingPackageStore>,
        transaction_digest: TransactionDigest,
        seq_opt: Option<TxSequenceNumber>,
    ) -> Result<(), SuiError> {
        let (objects, active_inputs, written, deleted, _events) = temporary_store.into_inner();

        // Archive the old lock.
        write_batch = write_batch.delete_batch(&self.transaction_lock, active_inputs.iter())?;

        // Delete objects.
        // Wrapped objects need to be deleted as well because we can no longer track their
        // content nor use them directly.
        write_batch = write_batch.delete_batch(&self.objects, deleted.iter().map(|(id, _)| *id))?;

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
            .filter_map(|(id, _)| objects.get(id).and_then(Object::get_single_owner_and_id))
            .chain(
                written
                    .iter()
                    .filter_map(|(id, (_, new_object))| match objects.get(id) {
                        Some(old_object) if old_object.owner != new_object.owner => {
                            old_object.get_single_owner_and_id()
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

        // Create locks for new objects, if they are not immutable
        write_batch = write_batch.insert_batch(
            &self.transaction_lock,
            written.iter().filter_map(|(_, (object_ref, new_object))| {
                if !new_object.is_immutable() {
                    Some((object_ref, None))
                } else {
                    None
                }
            }),
        )?;

        if ALL_OBJ_VER {
            // Keep all versions of every object if ALL_OBJ_VER is true.
            write_batch = write_batch.insert_batch(
                &self.all_object_versions,
                written
                    .iter()
                    .map(|(id, (_object_ref, object))| ((*id, object.version()), object)),
            )?;
        }

        // Update the indexes of the objects written
        write_batch = write_batch.insert_batch(
            &self.owner_index,
            written
                .iter()
                .filter_map(|(_id, (object_ref, new_object))| {
                    new_object
                        .get_single_owner_and_id()
                        .map(|owner_id| (owner_id, object_ref))
                }),
        )?;

        // Insert each output object into the stores
        write_batch = write_batch.insert_batch(
            &self.objects,
            written
                .iter()
                .map(|(object_id, (_, new_object))| (object_id, new_object)),
        )?;

        // Update the indexes of the objects written

        // This is the critical region: testing the locks and writing the
        // new locks must be atomic, and no writes should happen in between.
        {
            // Acquire the lock to ensure no one else writes when we are in here.
            let _mutexes = self.acquire_locks(&active_inputs[..]);

            // Check the locks are still active
            // TODO: maybe we could just check if the certificate is there instead?
            let locks = self.transaction_lock.multi_get(&active_inputs[..])?;
            for object_lock in locks {
                object_lock.ok_or(SuiError::TransactionLockDoesNotExist)?;
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

            // Atomic write of all locks & other data
            write_batch.write()?;

            // implicit: drop(_mutexes);
        } // End of critical region

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
        transaction: &Transaction,
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
        let ids = certificate.transaction.shared_input_objects();
        let versions = self.schedule.multi_get(ids)?;

        let ids = certificate.transaction.shared_input_objects();
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

    #[cfg(test)]
    /// Provide read access to the `schedule` table (useful for testing).
    pub fn get_schedule(&self, object_id: &ObjectID) -> SuiResult<Option<SequenceNumber>> {
        self.schedule.get(object_id).map_err(SuiError::from)
    }
}

impl<const A: bool> SuiDataStore<A, AuthoritySignInfo> {
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

impl<const A: bool> SuiDataStore<A, EmptySignInfo> {
    pub fn pending_transactions(&self) -> &DBMap<TransactionDigest, Transaction> {
        &self.transactions
    }
}

impl<const A: bool, S: Eq + Serialize + for<'de> Deserialize<'de>> BackingPackageStore
    for SuiDataStore<A, S>
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

impl<const A: bool, S: Eq + Serialize + for<'de> Deserialize<'de>> ModuleResolver
    for SuiDataStore<A, S>
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
