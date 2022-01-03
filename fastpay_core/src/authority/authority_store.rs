use super::*;

use rocksdb::Options;
use std::path::Path;
use std::sync::Mutex;
use typed_store::rocks::{open_cf, DBMap};
use typed_store::traits::Map;

pub struct AuthorityStore {
    objects: DBMap<ObjectID, Object>,
    order_lock: DBMap<ObjectRef, Option<TransactionDigest>>,
    signed_orders: DBMap<TransactionDigest, SignedOrder>,
    certificates: DBMap<TransactionDigest, CertifiedOrder>,
    parent_sync: DBMap<ObjectRef, TransactionDigest>,
    check_and_write_lock: Mutex<()>,
}

impl AuthorityStore {
    /// Open an authority store by directory path
    pub fn open<P: AsRef<Path>>(path: P, db_options: Option<Options>) -> AuthorityStore {
        let db = open_cf(
            &path,
            db_options,
            &[
                "objects",
                "order_lock",
                "signed_orders",
                "certificates",
                "parent_sync",
            ],
        )
        .expect("Cannot open DB.");
        AuthorityStore {
            objects: DBMap::reopen(&db, Some("objects")).expect("Cannot open CF."),
            order_lock: DBMap::reopen(&db, Some("order_lock")).expect("Cannot open CF."),
            signed_orders: DBMap::reopen(&db, Some("signed_orders")).expect("Cannot open CF."),
            certificates: DBMap::reopen(&db, Some("certificates")).expect("Cannot open CF."),
            parent_sync: DBMap::reopen(&db, Some("parent_sync")).expect("Cannot open CF."),
            check_and_write_lock: Mutex::new(()),
        }
    }

    // Methods to read the store

    pub fn get_account_objects(
        &self,
        account: FastPayAddress,
    ) -> Result<Vec<ObjectRef>, FastPayError> {
        Ok(self
            .objects
            .iter()
            .filter(|(_, object)| object.owner == account)
            .map(|(id, object)| ObjectRef::from((id, object.next_sequence_number)))
            .collect())
    }

    /// Read an object and return it, or Err(ObjectNotFound) if the object was not found.
    pub fn object_state(&self, object_id: &ObjectID) -> Result<Object, FastPayError> {
        self.objects
            .get(object_id)
            .map_err(|_| FastPayError::StorageError)?
            .ok_or(FastPayError::ObjectNotFound)
    }

    /// Read a lock or returns Err(OrderLockDoesNotExist) if the lock does not exist.
    pub fn get_order_lock(
        &self,
        object_ref: &ObjectRef,
    ) -> Result<Option<SignedOrder>, FastPayError> {
        let order_option = self
            .order_lock
            .get(object_ref)
            .map_err(|_| FastPayError::StorageError)?
            .ok_or(FastPayError::OrderLockDoesNotExist)?;

        match order_option {
            Some(tx_digest) => Ok(Some(
                self.signed_orders
                    .get(&tx_digest)
                    .map_err(|_| FastPayError::StorageError)?
                    .expect("Stored a lock without storing order?"),
            )),
            None => Ok(None),
        }
    }

    /// Read a certificate and return an option with None if it does not exist.
    pub fn read_certificate(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<CertifiedOrder>, FastPayError> {
        self.certificates
            .get(digest)
            .map_err(|_| FastPayError::StorageError)
    }

    /// Read the transactionDigest that is the parent of an object reference
    /// (ie. the transaction that created an object at this version.)
    pub fn parent(
        &self,
        object_ref: &ObjectRef,
    ) -> Result<Option<TransactionDigest>, FastPayError> {
        self.parent_sync
            .get(object_ref)
            .map_err(|_| FastPayError::StorageError)
    }

    // Methods to mutate the store

    /// Insert an object
    pub fn insert_object(&self, object: Object) -> Result<(), FastPayError> {
        self.objects
            .insert(&object.id(), &object)
            .map_err(|_| FastPayError::StorageError)
    }

    /// Initialize a lock to an object reference to None, but keep it
    /// as it is if a value is already present.
    pub fn init_order_lock(&self, object_ref: ObjectRef) -> Result<(), FastPayError> {
        self.order_lock
            .get_or_insert(&object_ref, || None)
            .map_err(|_| FastPayError::StorageError)?;
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
    ) -> Result<(), FastPayError> {
        let tx_digest = signed_order.order.digest();
        let lock_batch = self
            .order_lock
            .batch()
            .insert_batch(
                &self.order_lock,
                mutable_input_objects
                    .iter()
                    .map(|obj_ref| (*obj_ref, Some(tx_digest))),
            )
            .map_err(|_| FastPayError::StorageError)?
            .insert_batch(
                &self.signed_orders,
                std::iter::once((tx_digest, signed_order)),
            )
            .map_err(|_| FastPayError::StorageError)?;

        // This is the critical region: testing the locks and writing the
        // new locks must be atomic, and not writes should happen in between.
        {
            // Aquire the lock to ensure no one else writes when we are in here.
            let _lock = self
                .check_and_write_lock
                .lock()
                .map_err(|_| FastPayError::StorageError)?;

            for obj_ref in mutable_input_objects {
                // The object / version must exist, and therefore lock initialized.
                let lock = self
                    .order_lock
                    .get(obj_ref)
                    .map_err(|_| FastPayError::StorageError)?
                    .ok_or(FastPayError::OrderLockDoesNotExist)?;

                if let Some(previous_tx_digest) = lock {
                    if previous_tx_digest != tx_digest {
                        let prev_order = self
                            .get_order_lock(obj_ref)
                            .map_err(|_| FastPayError::StorageError)?
                            .expect("If we have a lock we should have an order.");

                        // TODO: modify ConflictingOrder to only return the order digest here.
                        return Err(FastPayError::ConflictingOrder {
                            pending_confirmation: prev_order.order,
                        });
                    }
                }
            }

            // Atomic write of all locks
            lock_batch.write().map_err(|_| FastPayError::StorageError)

            // Implicit: drop(_lock);
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
    ) -> Result<(), FastPayError> {
        // TODO: There is a lot of cloning used -- eliminate it.

        // Extract the new state from the execution
        let (mut objects, active_inputs, written, _deleted) = temporary_store.into_inner();
        let mut write_batch = self.order_lock.batch();

        // Archive the old lock.
        write_batch = write_batch
            .delete_batch(&self.order_lock, active_inputs.iter().cloned())
            .map_err(|_| FastPayError::StorageError)?;

        // Store the certificate indexed by transaction digest
        let transaction_digest: TransactionDigest = certificate.order.digest();
        write_batch = write_batch
            .insert_batch(
                &self.certificates,
                std::iter::once((transaction_digest, certificate)),
            )
            .map_err(|_| FastPayError::StorageError)?;

        // Delete objects
        write_batch = write_batch
            .delete_batch(
                &self.objects,
                _deleted.iter().map(|deleted_ref| deleted_ref.0),
            )
            .map_err(|_| FastPayError::StorageError)?;

        // Index the certificate by the objects created
        write_batch = write_batch
            .insert_batch(
                &self.parent_sync,
                written
                    .iter()
                    .map(|output_ref| (*output_ref, transaction_digest)),
            )
            .map_err(|_| FastPayError::StorageError)?;

        // Create locks for new objects, if they are not immutable
        write_batch = write_batch
            .insert_batch(
                &self.order_lock,
                written
                    .iter()
                    .filter(|output_ref| !objects[&output_ref.0].is_read_only())
                    .map(|output_ref| (*output_ref, None)),
            )
            .map_err(|_| FastPayError::StorageError)?;

        // Insert each output object into the stores
        write_batch = write_batch
            .insert_batch(
                &self.objects,
                written.iter().map(|output_ref| {
                    (
                        output_ref.0,
                        objects
                            .remove(&output_ref.0)
                            .expect("By temporary_authority_store invariant object exists."),
                    )
                }),
            )
            .map_err(|_| FastPayError::StorageError)?;

        // This is the critical region: testing the locks and writing the
        // new locks must be atomic, and no writes should happen in between.
        {
            // Aquire the lock to ensure no one else writes when we are in here.
            let _lock = self
                .check_and_write_lock
                .lock()
                .map_err(|_| FastPayError::StorageError)?;

            // Check the locks are still active
            // TODO: maybe we could just check if the certificate is there instead?
            for input_ref in active_inputs {
                fp_ensure!(
                    self.order_lock
                        .contains_key(&input_ref)
                        .map_err(|_| FastPayError::StorageError)?,
                    FastPayError::OrderLockDoesNotExist
                );
            }

            // Atomic write of all locks & other data
            write_batch.write().map_err(|_| FastPayError::StorageError)

            // Implicit: drop(_lock);
        } // End of critical region
    }
}
