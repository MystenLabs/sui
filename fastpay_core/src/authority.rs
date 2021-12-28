// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use fastx_adapter::adapter;
use fastx_types::{
    base_types::*,
    committee::Committee,
    error::FastPayError,
    fp_bail, fp_ensure,
    messages::*,
    object::{Data, Object},
    storage::Storage,
};
use move_core_types::{
    account_address::AccountAddress,
    language_storage::{ModuleId, StructTag},
    resolver::{ModuleResolver, ResourceResolver},
};
use move_vm_runtime::native_functions::NativeFunctionTable;
use std::{
    collections::{BTreeMap, HashSet},
    convert::TryInto,
    sync::Arc,
    sync::Mutex,
};

#[cfg(test)]
#[path = "unit_tests/authority_tests.rs"]
mod authority_tests;

mod temporary_store;
use temporary_store::AuthorityTemporaryStore;

pub struct AuthorityState {
    // Fixed size, static, identity of the authority and shard
    /// The name of this authority.
    pub name: AuthorityName,
    /// Committee of this FastPay instance.
    pub committee: Committee,
    /// The signature key of the authority.
    pub secret: KeyPair,
    /// The sharding ID of this authority shard. 0 if one shard.
    pub shard_id: ShardId,
    /// The number of shards. 1 if single shard.
    pub number_of_shards: u32,

    // The variable length dynamic state of the authority shard
    /// States of fastnft objects
    ///
    /// This is the placeholder data representation for the actual database
    /// of objects that we will eventually have in a persistent store. Since
    /// this database will have to be used by many refs of the authority, and
    /// others it should be useable as a &ref for both reads and writes/deletes.
    /// Right now we do this through placing it in an Arc/Mutex, but eventually
    /// we will architect this to ensure perf.
    objects: Arc<Mutex<BTreeMap<ObjectID, Object>>>,

    /* Order lock states and invariants

    Each object in `objects` needs to have an entry in `order_locks`
    that is initially initialized to None. This indicates we have not
    seen any valid transactions mutating this object. Once we see the
    first valid transaction the lock is set to Some(SignedOrder). We
    will never change this lock to a different value of back to None.

    Eventually, a certificate may be seen with a transaction that consumes
    an object. In that case we can delete the key-value of the lock and
    archive them. This reduces the amount of memory consumed by locks. It
    also means that if an object has a lock entry it is 'active' ie transaction
    may use it as an input. If not, a transaction can be rejected.

    */
    /// Order lock map maps object versions to the first next transaction seen
    order_lock: BTreeMap<ObjectRef, Option<SignedOrder>>,
    /// Certificates that have been accepted.
    certificates: BTreeMap<TransactionDigest, CertifiedOrder>,
    /// An index mapping object IDs to the Transaction Digest that created them.
    /// This is used by synchronization logic to sync authorities.
    parent_sync: BTreeMap<ObjectRef, TransactionDigest>,

    /// Move native functions that are available to invoke
    native_functions: NativeFunctionTable,
}

/// Interface provided by each (shard of an) authority.
/// All commands return either the current account info or an error.
/// Repeating commands produces no changes and returns no error.

impl AuthorityState {
    /// Initiate a new transfer.
    pub async fn handle_order(
        &mut self,
        order: Order,
    ) -> Result<AccountInfoResponse, FastPayError> {
        // Check the sender's signature and retrieve the transfer data.
        fp_ensure!(self.in_shard(order.object_id()), FastPayError::WrongShard);
        order.check_signature()?;

        // We first do all the checks that can be done in parallel with read only access to
        // the object database and lock database.
        let input_objects = order.input_objects();
        let mut mutable_objects = Vec::with_capacity(input_objects.len());

        // Ensure at least one object is input to be mutated.
        fp_ensure!(
            !input_objects.is_empty(),
            FastPayError::InsufficientObjectNumber
        );
        // Ensure that there are no duplicate inputs
        let mut used = HashSet::new();
        if !input_objects.iter().all(|o| used.insert(o)) {
            return Err(FastPayError::DuplicateObjectRefInput);
        }

        for object_ref in input_objects {
            let (object_id, sequence_number) = object_ref;

            fp_ensure!(
                sequence_number <= SequenceNumber::max(),
                FastPayError::InvalidSequenceNumber
            );

            // Get a copy of the object.
            // TODO: We only need to read the read_only and owner field of the object,
            //      it's a bit wasteful to copy the entire object.
            let object = self
                .object_state(&object_id)
                .await
                .map_err(|_| FastPayError::ObjectNotFound)?;

            // Check that the seq number is the same
            fp_ensure!(
                object.next_sequence_number == sequence_number,
                FastPayError::UnexpectedSequenceNumber
            );

            // If this is an immutable object, we do no more checks
            // and check no locks.
            if object.is_read_only() {
                continue;
            }

            // Additional checks for mutable objects

            // Check the transaction sender is also the object owner
            fp_ensure!(
                order.sender() == &object.owner,
                FastPayError::IncorrectSigner
            );

            mutable_objects.push((object_id, sequence_number));
        }

        // TODO(https://github.com/MystenLabs/fastnft/issues/45): check that c.gas_payment exists + that its value is > gas_budget
        // Note: the above code already checks that the gas object exists because it is included in the
        //       input_objects() list. So need to check it contains some gas.

        let object_id = *order.object_id();
        let signed_order = SignedOrder::new(order, self.name, &self.secret);

        // This is the critical section that requires a write lock on the lock DB.
        self.set_order_lock(&mutable_objects, signed_order).await?;

        let info = self.make_object_info(object_id).await?;
        Ok(info)
    }

    /// Confirm a transfer.
    pub async fn handle_confirmation_order(
        &mut self,
        confirmation_order: ConfirmationOrder,
    ) -> Result<AccountInfoResponse, FastPayError> {
        let certificate = confirmation_order.certificate;
        let order = certificate.order.clone();
        let mut object_id = *order.object_id();
        // Check the certificate and retrieve the transfer data.
        fp_ensure!(self.in_shard(&object_id), FastPayError::WrongShard);
        certificate.check(&self.committee)?;

        let mut inputs = vec![];
        for (input_object_id, input_seq) in order.input_objects() {
            // If we have a certificate on the confirmation order it means that the input
            // object exists on other honest authorities, but we do not have it. The only
            // way this may happen is if we missed some updates.
            let input_object = self.object_state(&input_object_id).await.map_err(|_| {
                FastPayError::MissingEalierConfirmations {
                    current_sequence_number: SequenceNumber::from(0),
                }
            })?;

            let input_sequence_number = input_object.next_sequence_number;

            // Check that the current object is exactly the right version.
            if input_sequence_number < input_seq {
                fp_bail!(FastPayError::MissingEalierConfirmations {
                    current_sequence_number: input_sequence_number
                });
            }
            if input_sequence_number > input_seq {
                // Transfer was already confirmed.
                return self.make_object_info(object_id).await;
            }

            inputs.push(input_object.clone());
        }

        // Note: State is mutated below and should be committed in an atomic way
        // to memory or persistent storage. Currently this is done in memory
        // through the calls to store being infallible.

        // Insert into the certificates map
        let transaction_digest = certificate.order.digest();
        let mut tx_ctx = TxContext::new(transaction_digest);

        // Order-specific logic
        //
        // TODO: think very carefully what to do in case we throw an Err here.
        let mut temporary_store = AuthorityTemporaryStore::new(self, &inputs);
        match order.kind {
            OrderKind::Transfer(t) => {
                let mut output_object = inputs[0].clone();
                output_object.next_sequence_number =
                    output_object.next_sequence_number.increment()?;

                output_object.transfer(match t.recipient {
                    Address::Primary(_) => PublicKeyBytes([0; 32]),
                    Address::FastPay(addr) => addr,
                });
                temporary_store.write_object(output_object);
            }
            OrderKind::Call(c) => {
                // unwraps here are safe because we built `inputs`
                // TODO(https://github.com/MystenLabs/fastnft/issues/45): charge for gas
                let mut gas_object = inputs.pop().unwrap();
                let module = inputs.pop().unwrap();
                // Fake the gas payment
                gas_object.next_sequence_number = gas_object.next_sequence_number.increment()?;
                temporary_store.write_object(gas_object);
                match adapter::execute(
                    &mut temporary_store,
                    self.native_functions.clone(),
                    module,
                    &c.function,
                    c.type_arguments,
                    inputs,
                    c.pure_arguments,
                    Some(c.gas_budget),
                    tx_ctx,
                ) {
                    Ok(()) => {
                        // TODO(https://github.com/MystenLabs/fastnft/issues/63): AccountInfoResponse should return all object ID outputs.
                        // but for now it only returns one, so use this hack
                        object_id = if temporary_store.written.len() > 1 {
                            temporary_store.written[1].0
                        } else {
                            c.gas_payment.0
                        }
                    }
                    Err(_e) => {
                        // TODO(https://github.com/MystenLabs/fastnft/issues/63): return this error to the client
                        object_id = c.gas_payment.0;
                    }
                }
            }
            OrderKind::Publish(m) => {
                // Fake the gas payment
                let mut gas_object = temporary_store
                    .read_object(&object_id)
                    .expect("Checked existence at start of function.");
                gas_object.next_sequence_number = gas_object.next_sequence_number.increment()?;
                temporary_store.write_object(gas_object);
                // TODO(https://github.com/MystenLabs/fastnft/issues/45): charge for gas
                let sender = m.sender.to_address_hack();
                match adapter::publish(&mut temporary_store, m.modules, &sender, &mut tx_ctx) {
                    Ok(outputs) => {
                        // TODO(https://github.com/MystenLabs/fastnft/issues/63): AccountInfoResponse should return all object ID outputs.
                        // but for now it only returns one, so use this hack
                        object_id = outputs[0].0;
                    }
                    Err(_e) => {
                        // TODO(https://github.com/MystenLabs/fastnft/issues/63): return this error to the client
                        object_id = m.gas_payment.0;
                    }
                }
            }
        };

        // Note: State is mutated below and should be committed in an atomic way
        // to memory or persistent storage. Currently this is done in memory
        // through the calls to store being infallible.
        self.update_state(temporary_store, certificate).await?;

        let info = self.make_object_info(object_id).await?;
        Ok(info)
    }

    pub async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, FastPayError> {
        fp_ensure!(self.in_shard(&request.object_id), FastPayError::WrongShard);
        let mut response = self.make_object_info(request.object_id).await?;
        if let Some(seq) = request.request_sequence_number {
            // Get the Transaction Digest that created the object
            let transaction_digest = self
                .parent_sync
                .get(&(request.object_id, seq.increment()?))
                .ok_or(FastPayError::CertificateNotfound)?;
            // Get the cert from the transaction digest
            response.requested_certificate = Some(
                self.read_certificate(transaction_digest)?
                    .ok_or(FastPayError::CertificateNotfound)?
                    .clone(),
            );
        }
        Ok(response)
    }
}

impl AuthorityState {
    pub fn new(committee: Committee, name: AuthorityName, secret: KeyPair) -> Self {
        AuthorityState {
            committee,
            name,
            secret,
            objects: Arc::new(Mutex::new(BTreeMap::new())),
            order_lock: BTreeMap::new(),
            shard_id: 0,
            number_of_shards: 1,
            certificates: BTreeMap::new(),
            parent_sync: BTreeMap::new(),
            native_functions: NativeFunctionTable::new(),
        }
    }

    pub fn new_shard(
        committee: Committee,
        name: AuthorityName,
        secret: KeyPair,
        shard_id: u32,
        number_of_shards: u32,
    ) -> Self {
        AuthorityState {
            committee,
            name,
            secret,
            objects: Arc::new(Mutex::new(BTreeMap::new())),
            order_lock: BTreeMap::new(),
            shard_id,
            number_of_shards,
            certificates: BTreeMap::new(),
            parent_sync: BTreeMap::new(),
            native_functions: NativeFunctionTable::new(),
        }
    }

    pub fn in_shard(&self, object_id: &ObjectID) -> bool {
        self.which_shard(object_id) == self.shard_id
    }

    pub fn get_shard(num_shards: u32, object_id: &ObjectID) -> u32 {
        const LAST_INTEGER_INDEX: usize = std::mem::size_of::<ObjectID>() - 4;
        u32::from_le_bytes(object_id[LAST_INTEGER_INDEX..].try_into().expect("4 bytes"))
            % num_shards
    }

    pub fn which_shard(&self, object_id: &ObjectID) -> u32 {
        Self::get_shard(self.number_of_shards, object_id)
    }

    async fn object_state(&self, object_id: &ObjectID) -> Result<Object, FastPayError> {
        self.objects
            .lock()
            .unwrap()
            .get(object_id)
            .cloned()
            .ok_or(FastPayError::UnknownSenderAccount)
    }

    pub fn insert_object(&self, object: Object) {
        self.objects.lock().unwrap().insert(object.id(), object);
    }

    #[cfg(test)]
    pub fn accounts_mut(&self) -> &Arc<Mutex<BTreeMap<ObjectID, Object>>> {
        &self.objects
    }

    /// Make an information summary of an object to help clients

    async fn make_object_info(
        &self,
        object_id: ObjectID,
    ) -> Result<AccountInfoResponse, FastPayError> {
        let object = self.object_state(&object_id).await?;
        let lock = self
            .get_order_lock(&object.to_object_reference())
            .await
            .or::<FastPayError>(Ok(&None))?;

        Ok(AccountInfoResponse {
            object_id: object.id(),
            owner: object.owner,
            next_sequence_number: object.next_sequence_number,
            pending_confirmation: lock.clone(),
            requested_certificate: None,
            requested_received_transfers: Vec::new(),
        })
    }

    // Helper function to manage order_locks

    /// Initialize an order lock for an object/sequence to None
    pub fn init_order_lock(&mut self, object_ref: ObjectRef) {
        self.order_lock.entry(object_ref).or_insert(None);
        // If the lock exists, we do not modify it or reset it.
    }

    /// Set the order lock to a specific transaction
    pub async fn set_order_lock(
        &mut self,
        mutable_input_objects: &[ObjectRef],
        signed_order: SignedOrder,
    ) -> Result<(), FastPayError> {
        // This is the only function that writes as part of the handle_order flow
        // and the only that therefore needs an exclusive write lock on the lock
        // database. Inconsistent / delayed reads of the lock database do not result in safety
        // violations since at the end this function also re-checks that the lock
        // is not set and returns an Err if it is.
        //
        // Note that the writes are not atomic: we may actually set locks for a
        // few objects before returning an Err as a result of trying to overwrite one.
        // This is a liveness issue for equivocating clients and therefore not an issue
        // we are trying to resolve.

        for obj_ref in mutable_input_objects {
            // The object / version must exist, and therefore lock initialized.
            let lock = self
                .order_lock
                .get_mut(obj_ref)
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
            lock.replace(signed_order.clone());
        }
        Ok(())
    }

    async fn update_state(
        &mut self,
        temporary_store: AuthorityTemporaryStore,
        certificate: CertifiedOrder,
    ) -> Result<(), FastPayError> {
        // Extract the new state from the execution
        let (mut objects, active_inputs, written, _deleted) = temporary_store.into_inner();

        // Archive the old lock.
        for input_ref in active_inputs {
            let old_lock = self.order_lock.remove(&input_ref);
            fp_ensure!(old_lock.is_some(), FastPayError::OrderLockDoesNotExist);
        }

        // Store the certificate indexed by transaction digest
        let transaction_digest: TransactionDigest = certificate.order.digest();
        self.certificates.insert(transaction_digest, certificate);

        for deleted_ref in _deleted {
            // Remove the object
            self.objects.lock().unwrap().remove(&deleted_ref.0);
        }

        // Insert each output object into the stores, index and make locks for it.
        for output_ref in written {
            // Index the certificate by the objects created
            self.parent_sync.insert(output_ref, transaction_digest);

            // Add new object, init locks and remove old ones
            let object = objects
                .remove(&output_ref.0)
                .expect("By temporary_authority_store invariant object exists.");

            if !object.is_read_only() {
                // Only objects that can be mutated have locks.
                self.init_order_lock(output_ref);
            }

            self.insert_object(object);
        }
        Ok(())
    }

    /// Get a read reference to an object/seq lock
    pub async fn get_order_lock(
        &self,
        object_ref: &ObjectRef,
    ) -> Result<&Option<SignedOrder>, FastPayError> {
        // The object / version must exist, and therefore lock initialized.
        self.order_lock
            .get(object_ref)
            .ok_or(FastPayError::OrderLockDoesNotExist)
    }

    // Helper functions to manage certificates

    /// Read from the DB of certificates
    pub fn read_certificate(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<&CertifiedOrder>, FastPayError> {
        Ok(self.certificates.get(digest))
    }
}

pub struct AuthorityStore {
    objects: DBMap<ObjectID, Object>,
    order_lock: DBMap<ObjectRef, Option<SignedOrder>>,
    certificates: DBMap<TransactionDigest, CertifiedOrder>,
    parent_sync: DBMap<ObjectRef, TransactionDigest>,
}

use std::path::Path;
use store::rocks::{open_cf, DBMap};
use store::traits::Map;

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
        write_batch = write_batch.insert_batch(
            &self.certificates,
            [(transaction_digest, certificate)].iter().cloned())
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
            write_batch = write_batch.insert_batch(
                &self.parent_sync,
                [(output_ref, transaction_digest)].iter().cloned())
                .map_err(|_| FastPayError::StorageError)?;
            

            // Add new object, init locks and remove old ones
            let object = objects
                .remove(&output_ref.0)
                .expect("By temporary_authority_store invariant object exists.");

            if !object.is_read_only() {
                // Only objects that can be mutated have locks.
                write_batch = write_batch.insert_batch(
                    &self.order_lock,
                    [(output_ref, None)].iter().cloned())
                    .map_err(|_| FastPayError::StorageError)?;
            }

            // Write the new object
            write_batch = write_batch.insert_batch(
                &self.objects,
                [(output_ref.0, object)].iter().cloned())
                .map_err(|_| FastPayError::StorageError)?;
        }

        // Atomic write of all locks & other data
        write_batch.write().map_err(|_| FastPayError::StorageError)
    }
}
