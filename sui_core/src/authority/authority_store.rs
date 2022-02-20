// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;

use rocksdb::Options;
use std::collections::BTreeSet;
use std::convert::TryInto;
use std::path::Path;
use typed_store::rocks::{open_cf, DBMap};
use typed_store::traits::Map;

pub struct AuthorityStore {
    /// This is a map between the object ID and the latest state of the object, namely the
    /// state that is needed to process new orders. If an object is deleted its entry is
    /// removed from this map.
    objects: DBMap<ObjectID, Object>,

    /// This is a map between object references of currently active objects that can be mutated,
    /// and the order that they are lock on for use by this specific authority. Where an object
    /// lock exists for an object version, but no order has been seen using it the lock is set
    /// to None. The safety of consistent broadcast depend on each honest authority never changing
    /// the lock once it is set. After a certificate for this object is processed it can be
    /// forgotten.
    order_lock: DBMap<ObjectRef, Option<TransactionDigest>>,

    /// This is a an index of object references to currently existing objects, indexed by the
    /// composite key of the SuiAddress of their owner and the object ID of the object.
    /// This composite index allows an efficient iterator to list all objected currently owned
    /// by a specific user, and their object reference.
    owner_index: DBMap<(SuiAddress, ObjectID), ObjectRef>,

    /// This is map between the transaction digest and signed orders found in the `order_lock`.
    /// NOTE: after a lock is deleted the corresponding entry here could be deleted, but right
    /// now this is not done. If a certificate is processed (see `certificates`) the signed
    /// order can also be deleted from this structure.
    signed_orders: DBMap<TransactionDigest, SignedOrder>,

    /// This is a map between the tranbsaction digest and the corresponding certificate for all
    /// certificates that have been successfully processed by this authority. This set of certificates
    /// along with the genesis allows the reconstruction of all other state, and a full sync to this
    /// authority.
    certificates: DBMap<TransactionDigest, CertifiedOrder>,

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
    signed_effects: DBMap<TransactionDigest, SignedOrderEffects>,

    /// Internal vector of locks to manage concurrent writes to the database
    lock_table: Vec<parking_lot::Mutex<()>>,
}

impl AuthorityStore {
    /// Open an authority store by directory path
    pub fn open<P: AsRef<Path>>(path: P, db_options: Option<Options>) -> AuthorityStore {
        let db = open_cf(
            &path,
            db_options,
            &[
                "objects",
                "owner_index",
                "order_lock",
                "signed_orders",
                "certificates",
                "parent_sync",
                "signed_effects",
            ],
        )
        .expect("Cannot open DB.");
        AuthorityStore {
            objects: DBMap::reopen(&db, Some("objects")).expect("Cannot open CF."),
            owner_index: DBMap::reopen(&db, Some("owner_index")).expect("Cannot open CF."),
            order_lock: DBMap::reopen(&db, Some("order_lock")).expect("Cannot open CF."),
            signed_orders: DBMap::reopen(&db, Some("signed_orders")).expect("Cannot open CF."),
            certificates: DBMap::reopen(&db, Some("certificates")).expect("Cannot open CF."),
            parent_sync: DBMap::reopen(&db, Some("parent_sync")).expect("Cannot open CF."),
            signed_effects: DBMap::reopen(&db, Some("signed_effects")).expect("Cannot open CF."),
            lock_table: (0..1024)
                .into_iter()
                .map(|_| parking_lot::Mutex::new(()))
                .collect(),
        }
    }

    /// A function that aquires all locks associated with the objects (in order to avoid deadlocks.)
    fn aqcuire_locks(&self, _input_objects: &[ObjectRef]) -> Vec<parking_lot::MutexGuard<'_, ()>> {
        let num_locks = self.lock_table.len();
        // TODO: randomize the lock mapping based on a secet to avoid DoS attacks.
        let lock_number: BTreeSet<usize> = _input_objects
            .iter()
            .map(|(_, _, digest)| {
                usize::from_le_bytes(digest.0[0..8].try_into().unwrap()) % num_locks
            })
            .collect();
        // Note: we need to iterate over the sorted unique elements, hence the use of a Set
        //       in order to prevent deadlocks when trying to aquire many locks.
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

    pub fn get_order_info(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> Result<OrderInfoResponse, SuiError> {
        Ok(OrderInfoResponse {
            signed_order: self.signed_orders.get(transaction_digest)?,
            certified_order: self.certificates.get(transaction_digest)?,
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

    /// Read a lock or returns Err(OrderLockDoesNotExist) if the lock does not exist.
    pub fn get_order_lock(&self, object_ref: &ObjectRef) -> Result<Option<SignedOrder>, SuiError> {
        let order_option = self
            .order_lock
            .get(object_ref)?
            .ok_or(SuiError::OrderLockDoesNotExist)?;

        match order_option {
            Some(tx_digest) => Ok(Some(
                self.signed_orders
                    .get(&tx_digest)?
                    .expect("Stored a lock without storing order?"),
            )),
            None => Ok(None),
        }
    }

    /// Read a certificate and return an option with None if it does not exist.
    pub fn read_certificate(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<CertifiedOrder>, SuiError> {
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

    // Methods to mutate the store

    /// Insert an object
    pub fn insert_object(&self, object: Object) -> Result<(), SuiError> {
        self.objects.insert(&object.id(), &object)?;

        // Update the index
        self.owner_index
            .insert(&(object.owner, object.id()), &object.to_object_reference())?;

        // Update the parent
        self.parent_sync
            .insert(&object.to_object_reference(), &object.previous_transaction)?;

        // We only side load objects with a genesis parent transaction.
        debug_assert!(object.previous_transaction == TransactionDigest::genesis());

        Ok(())
    }

    /// Initialize a lock to an object reference to None, but keep it
    /// as it is if a value is already present.
    pub fn init_order_lock(&self, object_ref: ObjectRef) -> Result<(), SuiError> {
        self.order_lock.get_or_insert(&object_ref, || None)?;
        Ok(())
    }

    /// Set the order lock to a specific transaction
    ///
    /// This function checks all locks exist, are either None or equal to the passed order
    /// and then sets them to the order. Otherwise an Err is returned. Locks are set
    /// atomically in this implementation.
    ///
    pub fn set_order_lock(
        &self,
        mutable_input_objects: &[ObjectRef],
        signed_order: SignedOrder,
    ) -> Result<(), SuiError> {
        let tx_digest = signed_order.order.digest();
        let lock_batch = self
            .order_lock
            .batch()
            .insert_batch(
                &self.order_lock,
                mutable_input_objects
                    .iter()
                    .map(|obj_ref| (obj_ref, Some(tx_digest))),
            )?
            .insert_batch(
                &self.signed_orders,
                std::iter::once((tx_digest, signed_order)),
            )?;

        // This is the critical region: testing the locks and writing the
        // new locks must be atomic, and not writes should happen in between.
        {
            // Aquire the lock to ensure no one else writes when we are in here.
            let _mutexes = self.aqcuire_locks(mutable_input_objects);

            let locks = self.order_lock.multi_get(mutable_input_objects)?;

            for (obj_ref, lock) in mutable_input_objects.iter().zip(locks) {
                // The object / version must exist, and therefore lock initialized.
                let lock = lock.ok_or(SuiError::OrderLockDoesNotExist)?;

                if let Some(previous_tx_digest) = lock {
                    if previous_tx_digest != tx_digest {
                        let prev_order = self
                            .get_order_lock(obj_ref)?
                            .expect("If we have a lock we should have an order.");

                        // TODO: modify ConflictingOrder to only return the order digest here.
                        return Err(SuiError::ConflictingOrder {
                            pending_order: prev_order.order,
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
    /// version, and then writes locks, objects, certificates, parents atomicaly.
    pub fn update_state(
        &self,
        temporary_store: AuthorityTemporaryStore,
        certificate: CertifiedOrder,
        signed_effects: SignedOrderEffects,
    ) -> Result<OrderInfoResponse, SuiError> {
        // Extract the new state from the execution
        // TODO: events are already stored in the TxDigest -> TransactionEffects store. Is that enough?
        let (objects, active_inputs, written, deleted, _events) = temporary_store.into_inner();
        let mut write_batch = self.order_lock.batch();

        // Archive the old lock.
        write_batch = write_batch.delete_batch(&self.order_lock, active_inputs.iter())?;

        // Store the certificate indexed by transaction digest
        let transaction_digest: TransactionDigest = certificate.order.digest();
        write_batch = write_batch.insert_batch(
            &self.certificates,
            std::iter::once((transaction_digest, &certificate)),
        )?;

        // Store the signed effects of the order
        write_batch = write_batch.insert_batch(
            &self.signed_effects,
            std::iter::once((transaction_digest, &signed_effects)),
        )?;

        // Delete objects
        write_batch = write_batch.delete_batch(&self.objects, deleted.iter())?;

        // Make an iterator over all objects that are either deleted or have changed owner,
        // along with their old owner.  This is used to update the owner index.
        let old_object_owners =
            deleted
                .iter()
                .map(|id| (objects[id].owner, *id))
                .chain(
                    written
                        .iter()
                        .filter_map(|(id, new_object)| match objects.get(id) {
                            Some(old_object) if old_object.owner != new_object.owner => {
                                Some((old_object.owner, *id))
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
            deleted.iter().map(|object_id| {
                (
                    (
                        *object_id,
                        objects[object_id].version().increment(),
                        ObjectDigest::deleted(),
                    ),
                    transaction_digest,
                )
            }),
        )?;

        // Create locks for new objects, if they are not immutable
        write_batch = write_batch.insert_batch(
            &self.order_lock,
            written.iter().filter_map(|(_, new_object)| {
                if !new_object.is_read_only() {
                    Some((new_object.to_object_reference(), None))
                } else {
                    None
                }
            }),
        )?;

        // Update the indexes of the objects written
        write_batch = write_batch.insert_batch(
            &self.owner_index,
            written.iter().map(|(id, new_object)| {
                ((new_object.owner, *id), new_object.to_object_reference())
            }),
        )?;

        // Insert each output object into the stores
        write_batch = write_batch.insert_batch(&self.objects, written.iter())?;

        // Update the indexes of the objects written

        // This is the critical region: testing the locks and writing the
        // new locks must be atomic, and no writes should happen in between.
        {
            // Aquire the lock to ensure no one else writes when we are in here.
            let _mutexes = self.aqcuire_locks(&active_inputs[..]);

            // Check the locks are still active
            // TODO: maybe we could just check if the certificate is there instead?
            let locks = self.order_lock.multi_get(&active_inputs[..])?;
            for object_lock in locks {
                object_lock.ok_or(SuiError::OrderLockDoesNotExist)?;
            }

            // Atomic write of all locks & other data
            write_batch.write()?;

            // implict: drop(_mutexes);
        } // End of critical region

        Ok(OrderInfoResponse {
            signed_order: self.signed_orders.get(&transaction_digest)?,
            certified_order: Some(certificate),
            signed_effects: Some(signed_effects),
        })
    }

    /// Returns the last entry we have for this object in the parents_sync index used
    /// to facilitate client and authority sync. In turn the latest entry provides the
    /// latest object_reference, and also the latest tranaction that has interacted with
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
            .skip_prior_to(&(object_id, SEQUENCE_NUMBER_MAX, OBJECT_DIGEST_MAX))?;

        Ok(iterator.next().and_then(|(obj_ref, tx_digest)| {
            if obj_ref.0 == object_id {
                Some((obj_ref, tx_digest))
            } else {
                None
            }
        }))
    }
}

impl ModuleResolver for AuthorityStore {
    type Error = SuiError;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        match self.get_object(&ObjectID::from(*module_id.address()))? {
            Some(o) => match &o.data {
                Data::Package(c) => Ok(c
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
