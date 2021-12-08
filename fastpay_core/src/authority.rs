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
use std::{collections::BTreeMap, convert::TryInto};

#[cfg(test)]
#[path = "unit_tests/authority_tests.rs"]
mod authority_tests;

// Refactor: eventually a transaction will have a (unique) digest. For the moment we only
// have transfer transactions so we index them by the object/seq they mutate.
pub(crate) type TransactionDigest = (ObjectID, SequenceNumber);

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
    objects: BTreeMap<ObjectID, Object>,

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
    pub parent_sync: BTreeMap<(ObjectID, SequenceNumber), TransactionDigest>,
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
        for object_ref in &input_objects {
            let (object_id, sequence_number) = object_ref;

            fp_ensure!(
                *sequence_number <= SequenceNumber::max(),
                FastPayError::InvalidSequenceNumber
            );

            // Get a ref to the object concerned by the transaction
            let object = self
                .objects
                .get(object_id)
                .ok_or(FastPayError::ObjectNotFound)?;

            // Check that the seq number is the same
            fp_ensure!(
                object.next_sequence_number == order.sequence_number(),
                FastPayError::UnexpectedSequenceNumber
            );

            // Check the transaction sender is also the object owner
            fp_ensure!(
                order.sender() == &object.owner,
                FastPayError::IncorrectSigner
            );

            // Check that this is the first, or same as the first order we sign.
            if let Some(pending_confirmation) = self.get_order_lock(object_ref)? {
                fp_ensure!(
                    pending_confirmation.order.kind == order.kind,
                    FastPayError::PreviousTransferMustBeConfirmedFirst {
                        pending_confirmation: pending_confirmation.order.clone()
                    }
                );
                // This exact transfer order was already signed. Return the previous value.
                return self.make_object_info(*object_id);
            }
        }

        // TODO(https://github.com/MystenLabs/fastnft/issues/45): check that c.gas_payment exists + that its value is > gas_budget

        let object_id = *order.object_id();
        let signed_order = SignedOrder::new(order, self.name, &self.secret);

        // This is the critical section that requires a write lock on the lock DB.
        self.set_order_lock(signed_order)?;

        let info = self.make_object_info(object_id)?;
        Ok(info)
    }

    /// Confirm a transfer.
    fn handle_confirmation_order(
        &mut self,
        confirmation_order: ConfirmationOrder,
    ) -> Result<AccountInfoResponse, FastPayError> {
        let certificate = confirmation_order.certificate;
        let order = &certificate.order;
        let object_id = *order.object_id();
        // Check the certificate and retrieve the transfer data.
        fp_ensure!(self.in_shard(&object_id), FastPayError::WrongShard);
        certificate.check(&self.committee)?;

        // If we have a certificate on the confirmation order it means that the input
        // object exists on other honest authorities, but we do not have it. The only
        // way this may happen is if we missed some updates.
        let input_object =
            self.objects
                .get(&object_id)
                .ok_or(FastPayError::MissingEalierConfirmations {
                    current_sequence_number: SequenceNumber::from(0),
                })?;

        let input_sequence_number = input_object.next_sequence_number;

        // Check that the current object is exactly the right version.
        if input_sequence_number < order.sequence_number() {
            fp_bail!(FastPayError::MissingEalierConfirmations {
                current_sequence_number: input_sequence_number
            });
        }
        if input_sequence_number > order.sequence_number() {
            // Transfer was already confirmed.
            return self.make_object_info(object_id);
        }

        // Here we implement the semantics of a transfer transaction, by which
        // the owner changes. Down the line here we do general smart contract
        // execution.

        let mut output_object = input_object.clone();
        let output_sequence_number = input_sequence_number.increment()?;
        output_object.next_sequence_number = output_sequence_number;

        let input_ref = input_object.to_object_reference();
        let output_ref = output_object.to_object_reference();

        // Order-specific logic
        match &order.kind {
            OrderKind::Transfer(t) => {
                output_object.transfer(match t.recipient {
                    Address::Primary(_) => PublicKeyBytes([0; 32]),
                    Address::FastPay(addr) => addr,
                });
            }
            OrderKind::Call(c) => {
                let sender = c.sender.to_address_hack();
                // TODO(https://github.com/MystenLabs/fastnft/issues/45): charge for gas
                // TODO(https://github.com/MystenLabs/fastnft/issues/30): read value of c.object_arguments +
                // pass objects directly to the VM instead of passing ObjectRef's
                adapter::execute(
                    self,
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
            OrderKind::Publish(_) => {
                unimplemented!("invoke the FastX adapter to publish modules")
            }
        }

        // Note: State is mutated below and should be committed in an atomic way
        // to memory or persistent storage. Currently this is done in memory
        // through the calls to store being infallible.

        // Insert into the certificates map
        let transaction_digest = self.add_certificate(certificate);

        // Index the certificate by the objects created
        self.add_parent_cert(output_ref, transaction_digest);

        // Add new object, init locks and remove old ones
        self.insert_object(output_object);
        self.archive_order_lock(&input_ref);
        self.init_order_lock(output_ref);

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
            objects: BTreeMap::new(),
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
            objects: BTreeMap::new(),
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

    fn object_state(&self, object_id: &ObjectID) -> Result<&Object, FastPayError> {
        self.objects
            .get(object_id)
            .ok_or(FastPayError::UnknownSenderAccount)
    }

    pub fn insert_object(&mut self, object: Object) {
        self.objects.insert(object.id(), object);
    }

    #[cfg(test)]
    pub fn accounts_mut(&mut self) -> &mut BTreeMap<ObjectID, Object> {
        &mut self.objects
    }

    /// Make an information summary of an object to help clients

    fn make_object_info(&self, object_id: ObjectID) -> Result<AccountInfoResponse, FastPayError> {
        let object = self.object_state(&object_id)?;
        let lock = self.get_order_lock(&object.to_object_reference())?;

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
    pub fn set_order_lock(&mut self, signed_order: SignedOrder) -> Result<(), FastPayError> {
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

        let input_objects = signed_order.order.input_objects();
        for (object_id, seq) in input_objects {
            // The object / version must exist, and therefore lock initialized.
            let lock = self
                .order_lock
                .get_mut(&(object_id, seq))
                .ok_or(FastPayError::OrderLockDoesNotExist)?;
            if let Some(_existing_signed_order) = lock {
                if _existing_signed_order.order == signed_order.order {
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

impl Storage for AuthorityState {
    fn read_object(&self, id: &ObjectID) -> Option<Object> {
        self.objects.get(id).cloned()
    }

    // TODO: buffer changes to storage + flush buffer after commit()
    fn write_object(&mut self, object: Object) {
        self.insert_object(object)
    }

    // TODO: buffer changes to storage + flush buffer after commit()
    fn delete_object(&mut self, id: &ObjectID) {
        self.objects.remove(id);
    }
}

impl ModuleResolver for AuthorityState {
    type Error = FastPayError;
    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        match self.objects.get(module_id.address()) {
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

impl ResourceResolver for AuthorityState {
    type Error = FastPayError;

    fn get_resource(
        &self,
        address: &AccountAddress,
        struct_tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        match self.objects.get(address) {
            Some(o) => match &o.data {
                Data::Move(m) => {
                    assert!(struct_tag == &m.type_, "Invariant violation: ill-typed object in storage or bad object resquest from caller\
");
                    Ok(Some(m.contents.clone()))
                }
                other => unimplemented!(
                    "Bad object lookup: expected Move object, but got {:?}",
                    other
                ),
            },
            _ => Ok(None),
        }
    }
}
