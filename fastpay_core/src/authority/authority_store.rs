use super::*;

use std::path::Path;
use store::rocks::{open_cf, DBMap};
use store::traits::Map;

pub struct AuthorityStore {
    objects: DBMap<ObjectID, Object>,
    order_lock: DBMap<ObjectRef, Option<SignedOrder>>,
    certificates: DBMap<TransactionDigest, CertifiedOrder>,
    parent_sync: DBMap<ObjectRef, TransactionDigest>,
}

impl AuthorityStore {
    pub fn open<P: AsRef<Path>>(path: P) -> AuthorityStore {
        let db = open_cf(
            &path,
            None,
            &["objects", "order_lock", "certificates", "parent_sync"],
        )
        .expect("Cannot open DB.");
        AuthorityStore {
            objects: DBMap::reopen(&db, Some("objects")).expect("Cannot open CF."),
            order_lock: DBMap::reopen(&db, Some("order_lock")).expect("Cannot open CF."),
            certificates: DBMap::reopen(&db, Some("certificates")).expect("Cannot open CF."),
            parent_sync: DBMap::reopen(&db, Some("parent_sync")).expect("Cannot open CF."),
        }
    }

    // Methods to read the store

    pub fn object_state(&self, object_id: &ObjectID) -> Result<Object, FastPayError> {
        self.objects
            .get(object_id)
            .map_err(|_| FastPayError::StorageError)?
            .ok_or(FastPayError::UnknownSenderAccount)
    }

    pub fn get_order_lock(
        &self,
        object_ref: &ObjectRef,
    ) -> Result<Option<SignedOrder>, FastPayError> {
        self.order_lock
            .get(object_ref)
            .map_err(|_| FastPayError::StorageError)?
            .ok_or(FastPayError::OrderLockDoesNotExist)
    }

    pub fn read_certificate(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<CertifiedOrder>, FastPayError> {
        self.certificates
            .get(digest)
            .map_err(|_| FastPayError::StorageError)
    }

    pub fn parent(
        &mut self,
        object_ref: &ObjectRef,
    ) -> Result<Option<TransactionDigest>, FastPayError> {
        self.parent_sync
            .get(object_ref)
            .map_err(|_| FastPayError::StorageError)
    }

    // Methods to mutate the store

    pub fn insert_object(&self, object: Object) -> Result<(), FastPayError> {
        self.objects
            .insert(&object.id(), &object)
            .map_err(|_| FastPayError::StorageError)
    }

    pub fn init_order_lock(&mut self, object_ref: ObjectRef) -> Result<(), FastPayError> {
        // TODO: Do we really need the get_or_insert here, or can we just do insert? Presumably we
        //       have checked that there are no conflicts?
        self.order_lock
            .get_or_insert(&object_ref, || None)
            .map_err(|_| FastPayError::StorageError)?;
        Ok(())
    }

    /// Set the order lock to a specific transaction
    pub fn set_order_lock(
        &mut self,
        mutable_input_objects: &[ObjectRef],
        signed_order: SignedOrder,
    ) -> Result<(), FastPayError> {
        // This is the only function that writes as part of the handle_order flow
        // and the only that therefore needs an exclusive write lock on the lock
        // database. Inconsistent / delayed reads of the lock database do not result in safety
        // violations since at the end this function also re-checks that the lock
        // is not set and returns an Err if it is.

        let mut lock_batch = self.order_lock.batch();

        for obj_ref in mutable_input_objects {
            // The object / version must exist, and therefore lock initialized.
            let lock = self
                .order_lock
                .get(obj_ref)
                .map_err(|_| FastPayError::StorageError)?
                .ok_or(FastPayError::OrderLockDoesNotExist)?;

            if let Some(existing_signed_order) = lock {
                if existing_signed_order.order == signed_order.order {
                    // For some reason we are re-inserting the same order. Not optimal but correct.
                    continue;
                } else {
                    // We are trying to set the lock to a different order, this is unsafe.
                    return Err(FastPayError::OrderLockReset);
                }
            }

            // The lock is None, so we replace it with the given order.
            let update = [(*obj_ref, Some(signed_order.clone()))];
            lock_batch = lock_batch
                .insert_batch(&self.order_lock, update.iter().cloned())
                .map_err(|_| FastPayError::StorageError)?;
        }

        // Atomic write of all locks
        lock_batch.write().map_err(|_| FastPayError::StorageError)
    }

    pub fn update_state(
        &mut self,
        temporary_store: AuthorityTemporaryStore,
        certificate: CertifiedOrder,
    ) -> Result<(), FastPayError> {
        // Extract the new state from the execution
        let (mut objects, active_inputs, written, _deleted) = temporary_store.into_inner();
        let mut write_batch = self.order_lock.batch();

        // Archive the old lock.
        for input_ref in active_inputs {
            let old_lock = self
                .order_lock
                .get(&input_ref)
                .map_err(|_| FastPayError::StorageError)?;
            fp_ensure!(old_lock.is_some(), FastPayError::OrderLockDoesNotExist);
            write_batch = write_batch
                .delete_batch(&self.order_lock, [input_ref].iter().cloned()) // TODO: I am sure we can avoid this clone
                .map_err(|_| FastPayError::StorageError)?;
        }

        // Store the certificate indexed by transaction digest
        let transaction_digest: TransactionDigest = certificate.order.digest();
        write_batch = write_batch
            .insert_batch(
                &self.certificates,
                [(transaction_digest, certificate)].iter().cloned(),
            )
            .map_err(|_| FastPayError::StorageError)?;

        for deleted_ref in _deleted {
            // Remove the object
            write_batch = write_batch
                .delete_batch(&self.objects, [deleted_ref.0].iter().copied()) // TODO: I am sure we can avoid this clone
                .map_err(|_| FastPayError::StorageError)?;
        }

        // Insert each output object into the stores, index and make locks for it.
        for output_ref in written {
            // Index the certificate by the objects created
            write_batch = write_batch
                .insert_batch(
                    &self.parent_sync,
                    [(output_ref, transaction_digest)].iter().cloned(),
                )
                .map_err(|_| FastPayError::StorageError)?;

            // Add new object, init locks and remove old ones
            let object = objects
                .remove(&output_ref.0)
                .expect("By temporary_authority_store invariant object exists.");

            if !object.is_read_only() {
                // Only objects that can be mutated have locks.
                write_batch = write_batch
                    .insert_batch(&self.order_lock, [(output_ref, None)].iter().cloned())
                    .map_err(|_| FastPayError::StorageError)?;
            }

            // Write the new object
            write_batch = write_batch
                .insert_batch(&self.objects, [(output_ref.0, object)].iter().cloned())
                .map_err(|_| FastPayError::StorageError)?;
        }

        // Atomic write of all locks & other data
        write_batch.write().map_err(|_| FastPayError::StorageError)
    }
}
