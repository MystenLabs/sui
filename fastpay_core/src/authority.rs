// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use crate::{base_types::*, committee::Committee, error::FastPayError, messages::*};
use std::{collections::BTreeMap, convert::TryInto};

#[cfg(test)]
#[path = "unit_tests/authority_tests.rs"]
mod authority_tests;

// Refactor: eventually a transaction will have a (unique) digest. For the moment we only
// have transfer transactions so we index them by the object/seq they mutate.
type TransactionDigest = (ObjectID, SequenceNumber);

#[derive(Eq, PartialEq, Debug, Clone)]
pub struct ObjectState {
    /// The object identifier
    pub id: ObjectID,
    /// The authenticator that unlocks this object (eg. public key, or other).
    pub owner: FastPayAddress,
    /// Sequence number tracking spending actions.
    pub next_sequence_number: SequenceNumber,
    /// The contents of the Object. Right now just a blob.
    pub contents: Vec<u8>,
}

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
    objects: BTreeMap<ObjectID, ObjectState>,

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
    order_lock: BTreeMap<(ObjectID, SequenceNumber), Option<SignedTransferOrder>>,
    /// Certificates that have been accepted.
    certificates: BTreeMap<TransactionDigest, CertifiedTransferOrder>,
    /// An index mapping object IDs to the Transaction Digest that created them.
    /// This is used by synchronization logic to sync authorities.
    pub parent_sync: BTreeMap<(ObjectID, SequenceNumber), TransactionDigest>,
}

/// Interface provided by each (shard of an) authority.
/// All commands return either the current account info or an error.
/// Repeating commands produces no changes and returns no error.
pub trait Authority {
    /// Initiate a new transfer to a FastPay or Primary account.
    fn handle_transfer_order(
        &mut self,
        order: TransferOrder,
    ) -> Result<AccountInfoResponse, FastPayError>;

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
    fn handle_transfer_order(
        &mut self,
        order: TransferOrder,
    ) -> Result<AccountInfoResponse, FastPayError> {
        // Check the sender's signature and retrieve the transfer data.
        fp_ensure!(
            self.in_shard(&order.transfer.object_id),
            FastPayError::WrongShard
        );
        order.check_signature()?;
        let transfer = &order.transfer;
        let object_id = transfer.object_id;
        fp_ensure!(
            transfer.sequence_number <= SequenceNumber::max(),
            FastPayError::InvalidSequenceNumber
        );

        // Get a ref to the object concerned by the transaction
        let object = self
            .objects
            .get(&object_id)
            .ok_or(FastPayError::ObjectNotFound)?;

        // Check the transaction sender is also the object owner
        fp_ensure!(
            order.transfer.sender == object.owner,
            FastPayError::IncorrectSigner
        );

        // Check that this is the first, or same as the first order we sign.
        if let Some(pending_confirmation) = self.get_order_lock(&object.to_object_reference())? {
            fp_ensure!(
                &pending_confirmation.value.transfer == transfer,
                FastPayError::PreviousTransferMustBeConfirmedFirst {
                    pending_confirmation: pending_confirmation.value.clone()
                }
            );
            // This exact transfer order was already signed. Return the previous value.
            return self.make_object_info(object_id);
        }
        fp_ensure!(
            object.next_sequence_number == transfer.sequence_number,
            FastPayError::UnexpectedSequenceNumber
        );

        let signed_order = SignedTransferOrder::new(order, self.name, &self.secret);

        // Add to the order_lock structure.
        // Note: Safety requires this be persisted before any client-visible response is generated.
        self.set_order_lock(signed_order)?;

        let info = self.make_object_info(object_id)?;
        Ok(info)
    }

    /// Confirm a transfer.
    fn handle_confirmation_order(
        &mut self,
        confirmation_order: ConfirmationOrder,
    ) -> Result<AccountInfoResponse, FastPayError> {
        let certificate = confirmation_order.transfer_certificate;
        // Check the certificate and retrieve the transfer data.
        fp_ensure!(
            self.in_shard(&certificate.value.transfer.object_id),
            FastPayError::WrongShard
        );
        certificate.check(&self.committee)?;
        let transfer = certificate.value.transfer.clone();

        // If we have a certificate on the confirmation order it means that the input
        // object exists on other honest authorities, but we do not have it. The only
        // way this may happen is if we missed some updates.
        let input_object = self.objects.get(&transfer.object_id).ok_or(
            FastPayError::MissingEalierConfirmations {
                current_sequence_number: SequenceNumber::from(0),
            },
        )?;

        let input_sequence_number = input_object.next_sequence_number;

        // Check that the current object is exactly the right version.
        if input_sequence_number < transfer.sequence_number {
            fp_bail!(FastPayError::MissingEalierConfirmations {
                current_sequence_number: input_sequence_number
            });
        }
        if input_sequence_number > transfer.sequence_number {
            // Transfer was already confirmed.
            return self.make_object_info(transfer.object_id);
        }

        // Here we implement the semantics of a transfer transaction, by which
        // the owner changes. Down the line here we do general smart contract
        // execution.

        let mut output_object = input_object.clone();
        let output_sequence_number = input_sequence_number.increment()?;
        output_object.next_sequence_number = output_sequence_number;
        output_object.owner = match transfer.recipient {
            Address::Primary(_) => PublicKeyBytes([0; 32]),
            Address::FastPay(addr) => addr,
        };

        // Note: State is mutated below and should be committed in an atomic way
        // to memory or persistent storage. Currently this is done in memory
        // through the calls to store being infallible.

        let input_ref = input_object.to_object_reference();
        let output_ref = output_object.to_object_reference();

        // Insert into the certificates map
        let transaction_digest = self.add_certificate(certificate);

        // Index the certificate by the objects created
        self.add_parent_cert(output_ref, transaction_digest);

        // Add new object, init locks and remove old ones
        self.insert_object(output_object);
        self.archive_order_lock(&input_ref);
        self.init_order_lock(output_ref);

        let info = self.make_object_info(transfer.object_id)?;
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

impl Default for ObjectState {
    fn default() -> Self {
        Self {
            id: [0; 20],
            contents: Vec::new(),
            owner: PublicKeyBytes([0; 32]),
            next_sequence_number: SequenceNumber::new(),
        }
    }
}

impl ObjectState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn to_object_reference(&self) -> ObjectRef {
        (self.id, self.next_sequence_number)
    }

    #[cfg(test)]
    pub fn new_with_balance(contents: Vec<u8>, _received_log: Vec<CertifiedTransferOrder>) -> Self {
        Self {
            id: [0; 20],
            owner: PublicKeyBytes([0; 32]),
            contents,
            next_sequence_number: SequenceNumber::new(),
        }
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

    fn object_state(&self, object_id: &ObjectID) -> Result<&ObjectState, FastPayError> {
        self.objects
            .get(object_id)
            .ok_or(FastPayError::UnknownSenderAccount)
    }

    pub fn insert_object(&mut self, object: ObjectState) {
        self.objects.insert(object.id, object);
    }

    #[cfg(test)]
    pub fn accounts_mut(&mut self) -> &mut BTreeMap<ObjectID, ObjectState> {
        &mut self.objects
    }

    /// Make an information summary of an object to help clients

    fn make_object_info(&self, object_id: ObjectID) -> Result<AccountInfoResponse, FastPayError> {
        let object = self.object_state(&object_id)?;
        let lock = self.get_order_lock(&object.to_object_reference())?;

        Ok(AccountInfoResponse {
            object_id: object.id,
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
        signed_order: SignedTransferOrder,
    ) -> Result<(), FastPayError> {
        let object_id = signed_order.value.transfer.object_id;
        let seq = signed_order.value.transfer.sequence_number;

        // The object / version must exist, and therefore lock initialized.
        let lock = self
            .order_lock
            .get_mut(&(object_id, seq))
            .ok_or(FastPayError::OrderLockDoesNotExist)?;
        if let Some(_existing_signed_order) = lock {
            if _existing_signed_order.value.transfer == signed_order.value.transfer {
                // For some reason we are re-inserting the same order. Not optimal but correct.
                return Ok(());
            } else {
                // We are trying to set the lock to a different order, this is unsafe.
                return Err(FastPayError::OrderLockReset);
            }
        }

        // The lock is None, so we replace it with the given order.
        lock.replace(signed_order);
        Ok(())
    }

    /// Get a read reference to an object/seq lock
    pub fn get_order_lock(
        &self,
        object_ref: &ObjectRef,
    ) -> Result<&Option<SignedTransferOrder>, FastPayError> {
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
    pub fn add_certificate(&mut self, certificate: CertifiedTransferOrder) -> TransactionDigest {
        let transaction_digest: TransactionDigest = (
            certificate.value.transfer.object_id,
            certificate.value.transfer.sequence_number,
        );

        // re-inserting a certificate is not a safety issue since it must certify the same transaction.
        self.certificates.insert(transaction_digest, certificate);
        transaction_digest
    }

    /// Read from the DB of certificates
    pub fn read_certificate(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<&CertifiedTransferOrder>, FastPayError> {
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
