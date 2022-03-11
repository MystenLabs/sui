// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;

use rocksdb::Options;
use std::collections::BTreeSet;
use std::convert::TryInto;
use std::path::Path;

use std::sync::atomic::AtomicU64;
use sui_types::base_types::SequenceNumber;
use sui_types::batch::{SignedBatch, TxSequenceNumber};
use tracing::warn;
use typed_store::rocks::{open_cf, DBBatch, DBMap};

use std::sync::atomic::Ordering;
use typed_store::{reopen, traits::Map};

pub type AuthorityStore = SuiDataStore<true>;
#[allow(dead_code)]
pub type ReplicaStore = SuiDataStore<false>;

/// ALL_OBJ_VER determines whether we want to store all past
/// versions of every object in the store. Authority doesn't store
/// them, but other entities such as replicas will.
pub struct SuiDataStore<const ALL_OBJ_VER: bool> {
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

    /// This is map between the transaction digest and signed transactions found in the `transaction_lock`.
    /// NOTE: after a lock is deleted the corresponding entry here could be deleted, but right
    /// now this is not done. If a certificate is processed (see `certificates`) the signed
    /// transaction can also be deleted from this structure.
    signed_transactions: DBMap<TransactionDigest, SignedTransaction>,

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
    signed_effects: DBMap<TransactionDigest, SignedTransactionEffects>,

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

    /// The next available sequence number to use in the `executed sequence` table.
    pub next_sequence_number: AtomicU64,
}

impl<const ALL_OBJ_VER: bool> SuiDataStore<ALL_OBJ_VER> {
    /// Open an authority store by directory path
    pub fn open<P: AsRef<Path>>(path: P, db_options: Option<Options>) -> AuthorityStore {
        let db = open_cf(
            &path,
            db_options,
            &[
                "objects",
                "all_object_versions",
                "owner_index",
                "transaction_lock",
                "signed_transactions",
                "certificates",
                "parent_sync",
                "signed_effects",
                "sequenced",
                "schedule",
                "executed_sequence",
                "batches",
            ],
        )
        .expect("Cannot open DB.");

        let executed_sequence =
            DBMap::reopen(&db, Some("executed_sequence")).expect("Cannot open CF.");

        // Read the index of the last entry in the sequence of commands
        // to extract the next sequence number or it is zero.
        let next_sequence_number = AtomicU64::new(
            executed_sequence
                .iter()
                .skip_prior_to(&TxSequenceNumber::MAX)
                .expect("Error reading table.")
                .next()
                .map(|(v, _)| v + 1u64)
                .unwrap_or(0),
        );

        let (
            objects,
            all_object_versions,
            owner_index,
            transaction_lock,
            signed_transactions,
            certificates,
            parent_sync,
            signed_effects,
            sequenced,
            schedule,
            batches,
        ) = reopen! (
            &db,
            "objects";<ObjectID, Object>,
            "all_object_versions";<(ObjectID, SequenceNumber), Object>,
            "owner_index";<(SuiAddress, ObjectID), ObjectRef>,
            "transaction_lock";<ObjectRef, Option<TransactionDigest>>,
            "signed_transactions";<TransactionDigest, SignedTransaction>,
            "certificates";<TransactionDigest, CertifiedTransaction>,
            "parent_sync";<ObjectRef, TransactionDigest>,
            "signed_effects";<TransactionDigest, SignedTransactionEffects>,
            "sequenced";<(TransactionDigest, ObjectID), SequenceNumber>,
            "schedule";<ObjectID, SequenceNumber>,
            "batches";<TxSequenceNumber, SignedBatch>
        );
        AuthorityStore {
            objects,
            all_object_versions,
            owner_index,
            transaction_lock,
            signed_transactions,
            certificates,
            parent_sync,
            signed_effects,
            sequenced,
            schedule,
            lock_table: (0..1024)
                .into_iter()
                .map(|_| parking_lot::Mutex::new(()))
                .collect(),
            executed_sequence,
            batches,
            next_sequence_number,
        }
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

    pub fn get_transaction_info(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        Ok(TransactionInfoResponse {
            signed_transaction: self.signed_transactions.get(transaction_digest)?,
            certified_transaction: self.certificates.get(transaction_digest)?,
            signed_effects: self.signed_effects.get(transaction_digest)?,
        })
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
    ) -> Result<Option<SignedTransaction>, SuiError> {
        let transaction_option = self
            .transaction_lock
            .get(object_ref)?
            .ok_or(SuiError::TransactionLockDoesNotExist)?;

        match transaction_option {
            Some(tx_digest) => Ok(Some(
                self.signed_transactions
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
    ) -> Result<Vec<(ObjectRef, TransactionDigest)>, SuiError> {
        let seq_inner = seq.unwrap_or_else(|| SequenceNumber::from(0));
        let obj_dig_inner = ObjectDigest::new([0; 32]);

        Ok(self
            .parent_sync
            .iter()
            // The object id [0; 16] is the smallest possible
            .skip_to(&(object_id, seq_inner, obj_dig_inner))?
            .take_while(|((id, iseq, _digest), _txd)| {
                let mut flag = id == &object_id;
                if seq.is_some() {
                    flag &= seq_inner == *iseq;
                }
                flag
            })
            .collect())
    }

    /// Read a lock for a specific (transaction, shared object) pair.
    pub fn sequenced(
        &self,
        transaction_digest: TransactionDigest,
        object_ids: &[ObjectID],
    ) -> Result<Vec<Option<SequenceNumber>>, SuiError> {
        let keys: Vec<_> = object_ids
            .iter()
            .map(|objid| (transaction_digest, *objid))
            .collect();

        self.sequenced.multi_get(&keys[..]).map_err(SuiError::from)
    }

    /// Read a lock for a specific (transaction, shared object) pair.
    pub fn all_shared_locks(
        &self,
        transaction_digest: TransactionDigest,
    ) -> Result<Vec<(ObjectID, SequenceNumber)>, SuiError> {
        Ok(self
            .sequenced
            .iter()
            .skip_to(&(transaction_digest, ObjectID::ZERO))?
            .take_while(|((tx, _objid), _ver)| *tx == transaction_digest)
            .map(|((_tx, objid), ver)| (objid, ver))
            .collect())
    }

    // Methods to mutate the store

    /// Insert an object
    pub fn insert_object(&self, object: Object) -> Result<(), SuiError> {
        self.objects.insert(&object.id(), &object)?;

        // Update the index
        if let Some(address) = object.get_single_owner() {
            self.owner_index
                .insert(&(address, object.id()), &object.to_object_reference())?;
        }

        // Update the parent
        self.parent_sync
            .insert(&object.to_object_reference(), &object.previous_transaction)?;

        // We only side load objects with a genesis parent transaction.
        debug_assert!(object.previous_transaction == TransactionDigest::genesis());

        Ok(())
    }

    /// Initialize a lock to an object reference to None, but keep it
    /// as it is if a value is already present.
    pub fn init_transaction_lock(&self, object_ref: ObjectRef) -> Result<(), SuiError> {
        self.transaction_lock.get_or_insert(&object_ref, || None)?;
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
        mutable_input_objects: &[ObjectRef],
        signed_transaction: SignedTransaction,
    ) -> Result<(), SuiError> {
        let tx_digest = signed_transaction.transaction.digest();
        let lock_batch = self
            .transaction_lock
            .batch()
            .insert_batch(
                &self.transaction_lock,
                mutable_input_objects
                    .iter()
                    .map(|obj_ref| (obj_ref, Some(tx_digest))),
            )?
            .insert_batch(
                &self.signed_transactions,
                std::iter::once((tx_digest, signed_transaction)),
            )?;

        // This is the critical region: testing the locks and writing the
        // new locks must be atomic, and not writes should happen in between.
        {
            // Aquire the lock to ensure no one else writes when we are in here.
            // MutexGuards are unlocked on drop (ie end of this block)
            let _mutexes = self.acquire_locks(mutable_input_objects);

            let locks = self.transaction_lock.multi_get(mutable_input_objects)?;

            for (obj_ref, lock) in mutable_input_objects.iter().zip(locks) {
                // The object / version must exist, and therefore lock initialized.
                let lock = lock.ok_or(SuiError::TransactionLockDoesNotExist)?;

                if let Some(previous_tx_digest) = lock {
                    if previous_tx_digest != tx_digest {
                        let prev_transaction = self
                            .get_transaction_lock(obj_ref)?
                            .expect("If we have a lock we should have a transaction.");

                        warn!(prev_tx_digest =? previous_tx_digest,
                              cur_tx_digest =? tx_digest,
                              "Conflicting transaction!  Lock state changed in unexpected way");
                        // TODO: modify ConflictingTransaction to only return the transaction digest here.
                        return Err(SuiError::ConflictingTransaction {
                            pending_transaction: prev_transaction.transaction,
                        });
                    }
                }
            }

            // Atomic write of all locks
            lock_batch.write().map_err(|e| e.into())

            // Implicit: drop(_mutexes);
        } // End of critical region
    }

    /// Updates the state resulting from the execution of a certificate.
    ///
    /// Internally it checks that all locks for active inputs are at the correct
    /// version, and then writes locks, objects, certificates, parents atomically.
    pub fn update_state(
        &self,
        temporary_store: AuthorityTemporaryStore,
        certificate: CertifiedTransaction,
        signed_effects: SignedTransactionEffects,
    ) -> Result<(TxSequenceNumber, TransactionInfoResponse), SuiError> {
        // Extract the new state from the execution
        // TODO: events are already stored in the TxDigest -> TransactionEffects store. Is that enough?
        let mut write_batch = self.transaction_lock.batch();

        // Store the certificate indexed by transaction digest
        let transaction_digest: TransactionDigest = certificate.transaction.digest();
        write_batch = write_batch.insert_batch(
            &self.certificates,
            std::iter::once((transaction_digest, &certificate)),
        )?;

        // Store the signed effects of the transaction
        write_batch = write_batch.insert_batch(
            &self.signed_effects,
            std::iter::once((transaction_digest, &signed_effects)),
        )?;

        // Cleanup the lock of the shared objects.
        let write_batch = self.remove_shared_objects_locks(
            write_batch,
            transaction_digest,
            &certificate.transaction,
        )?;

        // Safe to unwrap since the "true" flag ensures we get a sequence value back.
        let seq: TxSequenceNumber = self
            .batch_update_objects(write_batch, temporary_store, transaction_digest, true)?
            .unwrap();

        Ok((
            seq,
            TransactionInfoResponse {
                signed_transaction: self.signed_transactions.get(&transaction_digest)?,
                certified_transaction: Some(certificate),
                signed_effects: Some(signed_effects),
            },
        ))
    }

    /// Persist temporary storage to DB for genesis modules
    pub fn update_objects_state_for_genesis(
        &self,
        temporary_store: AuthorityTemporaryStore,
        transaction_digest: TransactionDigest,
    ) -> Result<(), SuiError> {
        debug_assert_eq!(transaction_digest, TransactionDigest::genesis());
        let write_batch = self.transaction_lock.batch();
        self.batch_update_objects(write_batch, temporary_store, transaction_digest, false)
            .map(|_| ())
    }

    /// Helper function for updating the objects in the state
    fn batch_update_objects(
        &self,
        mut write_batch: DBBatch,
        temporary_store: AuthorityTemporaryStore,
        transaction_digest: TransactionDigest,
        should_sequence: bool,
    ) -> Result<Option<TxSequenceNumber>, SuiError> {
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
                    .filter_map(|(id, new_object)| match objects.get(id) {
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
                .map(|(_, object)| (object.to_object_reference(), transaction_digest)),
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
            written.iter().filter_map(|(_, new_object)| {
                if !new_object.is_read_only() {
                    Some((new_object.to_object_reference(), None))
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
                    .map(|(id, object)| ((*id, object.version()), object)),
            )?;
        }

        // Update the indexes of the objects written
        write_batch = write_batch.insert_batch(
            &self.owner_index,
            written.iter().filter_map(|(_id, new_object)| {
                new_object
                    .get_single_owner_and_id()
                    .map(|owner_id| (owner_id, new_object.to_object_reference()))
            }),
        )?;

        // Insert each output object into the stores
        write_batch = write_batch.insert_batch(&self.objects, written.iter())?;

        // Update the indexes of the objects written

        // This is the critical region: testing the locks and writing the
        // new locks must be atomic, and no writes should happen in between.
        let mut return_seq = None;
        {
            // Acquire the lock to ensure no one else writes when we are in here.
            let _mutexes = self.acquire_locks(&active_inputs[..]);

            // Check the locks are still active
            // TODO: maybe we could just check if the certificate is there instead?
            let locks = self.transaction_lock.multi_get(&active_inputs[..])?;
            for object_lock in locks {
                object_lock.ok_or(SuiError::TransactionLockDoesNotExist)?;
            }

            if should_sequence {
                // Now we are sure we are going to execute, add to the sequence
                // number and insert into authority sequence.
                //
                // NOTE: it is possible that we commit to the database transactions
                //       out of order with respect to their sequence number. It is also
                //       possible for the authority to crash without committing the
                //       full sequence, and the batching logic needs to deal with this.
                let next_seq = self.next_sequence_number.fetch_add(1, Ordering::SeqCst);
                write_batch = write_batch.insert_batch(
                    &self.executed_sequence,
                    std::iter::once((next_seq, transaction_digest)),
                )?;

                return_seq = Some(next_seq);
            }

            // Atomic write of all locks & other data
            write_batch.write()?;

            // implicit: drop(_mutexes);
        } // End of critical region

        Ok(return_seq)
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
        transaction_digest: TransactionDigest,
        transaction: &Transaction,
    ) -> SuiResult<DBBatch> {
        let mut sequenced_to_delete = Vec::new();
        let mut schedule_to_delete = Vec::new();
        for object_id in transaction.shared_input_objects() {
            sequenced_to_delete.push((transaction_digest, *object_id));
            if self.get_object(object_id)?.is_none() {
                schedule_to_delete.push(object_id);
            }
        }
        write_batch = write_batch.delete_batch(&self.sequenced, sequenced_to_delete)?;
        write_batch = write_batch.delete_batch(&self.schedule, schedule_to_delete)?;
        Ok(write_batch)
    }

    /// Lock a sequence number for the shared objects of the input transaction.
    pub fn persist_certificate_and_lock_shared_objects(
        &self,
        transaction_digest: TransactionDigest,
        transaction: &Transaction,
        certificate: CertifiedTransaction,
    ) -> Result<(), SuiError> {
        let certificate_to_write = std::iter::once((transaction_digest, certificate));

        let mut sequenced_to_write = Vec::new();
        let mut schedule_to_write = Vec::new();
        for id in transaction.shared_input_objects() {
            let version = self.schedule.get(id)?.unwrap_or_default();
            sequenced_to_write.push(((transaction_digest, *id), version));
            let next_version = version.increment();
            schedule_to_write.push((id, next_version));
        }

        let mut write_batch = self.sequenced.batch();
        write_batch = write_batch.insert_batch(&self.certificates, certificate_to_write)?;
        write_batch = write_batch.insert_batch(&self.sequenced, sequenced_to_write)?;
        write_batch = write_batch.insert_batch(&self.schedule, schedule_to_write)?;
        write_batch.write().map_err(SuiError::from)
    }

    /// Retrieves batches including transactions within a range.
    ///
    /// This function returns all signed batches that enclose the requested transaction
    /// including the batch preceeding the first requested transaction, the batch including
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
}

impl ModuleResolver for AuthorityStore {
    type Error = SuiError;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        match self.get_object(&ObjectID::from(*module_id.address()))? {
            Some(o) => match &o.data {
                Data::Package(c) => Ok(c
                    .serialized_module_map()
                    .get(module_id.name().as_str())
                    .cloned()
                    .map(|m| m.into_vec())),
                _ => Err(SuiError::BadObjectType {
                    error: "Expected module object".to_string(),
                }),
            },
            None => Ok(None),
        }
    }
}
