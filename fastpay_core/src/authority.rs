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
use std::{collections::BTreeMap, convert::TryInto, sync::Arc, sync::Mutex};

#[cfg(test)]
#[path = "unit_tests/authority_tests.rs"]
mod authority_tests;

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
    order_lock: BTreeMap<(ObjectID, SequenceNumber), Option<SignedOrder>>,
    /// Certificates that have been accepted.
    certificates: BTreeMap<TransactionDigest, CertifiedOrder>,
    /// An index mapping object IDs to the Transaction Digest that created them.
    /// This is used by synchronization logic to sync authorities.
    parent_sync: BTreeMap<(ObjectID, SequenceNumber), TransactionDigest>,
}

/// Interface provided by each (shard of an) authority.
/// All commands return either the current account info or an error.
/// Repeating commands produces no changes and returns no error.
pub trait Authority {
    /// Initiate a new order to a FastPay or Primary account.
    fn handle_order(&mut self, order: Order) -> Result<AccountInfoResponse, FastPayError>;

    /// Confirm a transfer to a FastPay or Primary account.
    fn handle_confirmation_order(
        &mut self,
        order: ConfirmationOrder,
    ) -> Result<AccountInfoResponse, FastPayError>;

    /// Handle information requests for this account.
    fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, FastPayError>;
}

impl Authority for AuthorityState {
    /// Initiate a new transfer.
    fn handle_order(&mut self, order: Order) -> Result<AccountInfoResponse, FastPayError> {
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

        for object_ref in input_objects {
            let (object_id, sequence_number) = object_ref;

            fp_ensure!(
                sequence_number <= SequenceNumber::max(),
                FastPayError::InvalidSequenceNumber
            );

            // Get a ref to the object concerned by the transaction
            let object = self
                .object_state(&object_id)
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

            // Check that this is the first, or same as the first order we sign.
            if let Some(pending_confirmation) = self.get_order_lock(&object_ref)? {
                fp_ensure!(
                    pending_confirmation.order.kind == order.kind,
                    FastPayError::PreviousTransferMustBeConfirmedFirst {
                        pending_confirmation: pending_confirmation.order.clone()
                    }
                );
                // This exact transfer order was already signed. Return the previous value.
                return self.make_object_info(object_id);
            }

            mutable_objects.push((object_id, sequence_number));
        }

        // TODO(https://github.com/MystenLabs/fastnft/issues/45): check that c.gas_payment exists + that its value is > gas_budget
        // Note: the above code already checks that the gas object exists because it is included in the
        //       input_objects() list. So need to check it contains some gas.

        let object_id = *order.object_id();
        let signed_order = SignedOrder::new(order, self.name, &self.secret);

        // This is the critical section that requires a write lock on the lock DB.
        self.set_order_lock(&mutable_objects, signed_order)?;

        let info = self.make_object_info(object_id)?;
        Ok(info)
    }

    /// Confirm a transfer.
    fn handle_confirmation_order(
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
            let input_object = self.object_state(&input_object_id).map_err(|_| {
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
                return self.make_object_info(object_id);
            }

            inputs.push(input_object.clone());
        }

        // Note: State is mutated below and should be committed in an atomic way
        // to memory or persistent storage. Currently this is done in memory
        // through the calls to store being infallible.

        // Insert into the certificates map
        let transaction_digest = self.add_certificate(certificate);
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
                let sender = c.sender.to_address_hack();
                // TODO(https://github.com/MystenLabs/fastnft/issues/45): charge for gas
                adapter::execute(
                    &mut temporary_store,
                    &c.module,
                    &c.function,
                    sender,
                    c.object_arguments.clone(),
                    c.pure_arguments.clone(),
                    c.type_arguments.clone(),
                    Some(c.gas_budget),
                )
                .map_err(|_| FastPayError::MoveExecutionFailure)?;
            }
            OrderKind::Publish(m) => {
                // TODO(https://github.com/MystenLabs/fastnft/issues/45): charge for gas
                let sender = m.sender.to_address_hack();
                match adapter::publish(&mut temporary_store, m.modules, &sender, &mut tx_ctx) {
                    Ok(outputs) => {
                        // Fake the gas payment
                        let mut gas_object = temporary_store
                            .read_object(&object_id)
                            .expect("Checked existance at start of function.");
                        gas_object.next_sequence_number =
                            gas_object.next_sequence_number.increment()?;
                        temporary_store.write_object(gas_object);

                        // TODO: AccountInfoResponse should return all object ID outputs.
                        // but for now it only returns one, so use this hack
                        object_id = outputs[0].0;
                    }
                    Err(_e) => {
                        // TODO: return this error to the client
                    }
                }
            }
        };

        // Extract the new state from the execution
        let (mut objects, active_inputs, written, _deleted) = temporary_store.into_inner();

        // Note: State is mutated below and should be committed in an atomic way
        // to memory or persistent storage. Currently this is done in memory
        // through the calls to store being infallible.

        for input_ref in active_inputs {
            self.archive_order_lock(&input_ref);
        }

        // Insert each output object into the stores, index and make locks for it.
        for output_ref in written {
            // Index the certificate by the objects created
            self.add_parent_cert(output_ref, transaction_digest);

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

        let info = self.make_object_info(object_id)?;
        Ok(info)
    }

    fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, FastPayError> {
        fp_ensure!(self.in_shard(&request.object_id), FastPayError::WrongShard);
        let mut response = self.make_object_info(request.object_id)?;
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

    fn object_state(&self, object_id: &ObjectID) -> Result<Object, FastPayError> {
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

    fn make_object_info(&self, object_id: ObjectID) -> Result<AccountInfoResponse, FastPayError> {
        let object = self.object_state(&object_id)?;
        let lock = self
            .get_order_lock(&object.to_object_reference())
            .or(Ok(&None))?;

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

    /// Get a read reference to an object/seq lock
    pub fn get_order_lock(
        &self,
        object_ref: &ObjectRef,
    ) -> Result<&Option<SignedOrder>, FastPayError> {
        // The object / version must exist, and therefore lock initialized.
        self.order_lock
            .get(object_ref)
            .ok_or(FastPayError::OrderLockDoesNotExist)
    }

    /// Signals that the lock is no more needed, and can be archived / deleted.
    pub fn archive_order_lock(&mut self, object_ref: &ObjectRef) {
        // Note: for the moment just delete the local lock,
        // here we can log or write to longer term store.
        self.order_lock.remove(object_ref);
    }

    // Helper functions to manage certificates

    /// Add a certificate that has been processed
    pub fn add_certificate(&mut self, certificate: CertifiedOrder) -> TransactionDigest {
        let transaction_digest: TransactionDigest = certificate.order.digest();
        // re-inserting a certificate is not a safety issue since it must certify the same transaction.
        self.certificates.insert(transaction_digest, certificate);
        transaction_digest
    }

    /// Read from the DB of certificates
    pub fn read_certificate(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<&CertifiedOrder>, FastPayError> {
        Ok(self.certificates.get(digest))
    }

    /// Add object parent certificate relationship
    pub fn add_parent_cert(
        &mut self,
        object_ref: ObjectRef,
        transaction_digest: TransactionDigest,
    ) {
        self.parent_sync.insert(object_ref, transaction_digest);
    }
}

pub struct AuthorityTemporaryStore<'a> {
    authority_state: &'a AuthorityState,
    objects: BTreeMap<ObjectID, Object>,
    active_inputs: Vec<ObjectRef>, // Inputs that are not read only
    written: Vec<ObjectRef>,       // Objects written
    deleted: Vec<ObjectRef>,       // Objects actively deleted
}

impl<'a> AuthorityTemporaryStore<'a> {
    pub fn new(
        authority_state: &'a AuthorityState,
        _input_objects: &'_ [Object],
    ) -> AuthorityTemporaryStore<'a> {
        AuthorityTemporaryStore {
            authority_state,
            objects: _input_objects.iter().map(|v| (v.id(), v.clone())).collect(),
            active_inputs: _input_objects
                .iter()
                .filter(|v| !v.is_read_only())
                .map(|v| v.to_object_reference())
                .collect(),
            written: Vec::new(),
            deleted: Vec::new(),
        }
    }

    /// Break up the structure and return its internal stores (objects, active_inputs, written, deleted)
    pub fn into_inner(
        self,
    ) -> (
        BTreeMap<ObjectID, Object>,
        Vec<ObjectRef>,
        Vec<ObjectRef>,
        Vec<ObjectRef>,
    ) {
        #[cfg(debug_assertions)]
        {
            self.check_invariants();
        }
        (self.objects, self.active_inputs, self.written, self.deleted)
    }

    /// An internal check of the invariants (will only fire in debug)
    #[cfg(debug_assertions)]
    fn check_invariants(&self) {
        // Check uniqueness in the 'written' set
        debug_assert!(
            {
                use std::collections::HashSet;
                let mut used = HashSet::new();
                self.written.iter().all(move |elt| used.insert(elt.0))
            },
            "Duplicate object reference in self.written."
        );

        // Check uniqueness in the 'deleted' set
        debug_assert!(
            {
                use std::collections::HashSet;
                let mut used = HashSet::new();
                self.deleted.iter().all(move |elt| used.insert(elt.0))
            },
            "Duplicate object reference in self.deleted."
        );

        // Check not both deleted and written
        debug_assert!(
            {
                use std::collections::HashSet;
                let mut used = HashSet::new();
                self.written.iter().all(|elt| used.insert(elt.0));
                self.deleted.iter().all(move |elt| used.insert(elt.0))
            },
            "Object both written and deleted."
        );

        // Check all mutable inputs are either written or deleted
        debug_assert!(
            {
                use std::collections::HashSet;
                let mut used = HashSet::new();
                self.written.iter().all(|elt| used.insert(elt.0));
                self.deleted.iter().all(|elt| used.insert(elt.0));

                self.active_inputs.iter().all(|elt| !used.insert(elt.0))
            },
            "Mutable input neither written nor deleted."
        );
    }
}

impl<'a> Storage for AuthorityTemporaryStore<'a> {
    fn read_object(&self, id: &ObjectID) -> Option<Object> {
        match self.objects.get(id) {
            Some(x) => Some(x.clone()),
            None => self
                .authority_state
                .objects
                .lock()
                .unwrap()
                .get(id)
                .cloned(),
        }
    }

    /*
        Invariant: A key assumption of the write-delete logic
        is that an entry is not both added and deleted by the
        caller.
    */

    fn write_object(&mut self, object: Object) {
        // Check it is not read-only
        #[cfg(test)] // Movevm should ensure this
        if let Some(existing_object) = self.read_object(&object.id()) {
            if existing_object.is_read_only() {
                // This is an internal invariant violation. Move only allows us to
                // mutate objects if they are &mut so they cannot be read-only.
                panic!("Internal invariant violation: Mutating a read-only object.")
            }
        }

        self.written.push(object.to_object_reference());
        self.objects.insert(object.id(), object);
    }

    fn delete_object(&mut self, id: &ObjectID) {
        // Check it is not read-only
        #[cfg(test)] // Movevm should ensure this
        if let Some(object) = self.read_object(id) {
            if object.is_read_only() {
                // This is an internal invariant violation. Move only allows us to
                // mutate objects if they are &mut so they cannot be read-only.
                panic!("Internal invariant violation: Deleting a read-only object.")
            }
        }

        // If it exists remove it
        if let Some(removed) = self.objects.remove(id) {
            self.deleted.push(removed.to_object_reference());
        } else {
            panic!("Internal invariant: object must exist to be deleted.")
        }
    }
}

impl<'a> ModuleResolver for AuthorityTemporaryStore<'a> {
    type Error = FastPayError;
    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        match self
            .authority_state
            .objects
            .lock()
            .unwrap()
            .get(module_id.address())
        {
            Some(o) => match &o.data {
                Data::Module(c) => {
                    let mut bytes = Vec::new();
                    c.serialize(&mut bytes).expect("Invariant violation: serialization of well-formed module should never fail");
                    Ok(Some(bytes))
                }
                _ => Err(FastPayError::BadObjectType {
                    error: "Expected module object".to_string(),
                }),
            },
            None => Ok(None),
        }
    }
}

impl<'a> ResourceResolver for AuthorityTemporaryStore<'a> {
    type Error = FastPayError;

    fn get_resource(
        &self,
        address: &AccountAddress,
        struct_tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        let object = match self.read_object(address) {
            Some(x) => x,
            None => match self.authority_state.objects.lock().unwrap().get(address) {
                None => return Ok(None),
                Some(x) => {
                    if !x.is_read_only() {
                        fp_bail!(FastPayError::ExecutionInvariantViolation);
                    }
                    x.clone()
                }
            },
        };

        match &object.data {
            Data::Move(m) => {
                assert!(struct_tag == &m.type_, "Invariant violation: ill-typed object in storage or bad object request from caller\
");
                Ok(Some(m.contents.clone()))
            }
            other => unimplemented!(
                "Bad object lookup: expected Move object, but got {:?}",
                other
            ),
        }
    }
}
