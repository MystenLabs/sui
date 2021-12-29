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
    sync::Arc,
    sync::Mutex,
};
use std::path::Path;

#[cfg(test)]
#[path = "unit_tests/authority_tests.rs"]
mod authority_tests;

mod temporary_store;
use temporary_store::AuthorityTemporaryStore;

mod authority_store;
use authority_store::AuthorityStore;

pub struct AuthorityState {
    // Fixed size, static, identity of the authority
    /// The name of this authority.
    pub name: AuthorityName,
    /// Committee of this FastPay instance.
    pub committee: Committee,
    /// The signature key of the authority.
    pub secret: KeyPair,

    /// Move native functions that are available to invoke
    native_functions: NativeFunctionTable,
    /// The database
    _database: Arc<Mutex<AuthorityStore>>,
}

/// The authority state encapsulates all state, drives execution, and ensures safety.
///
/// Note the authority operations can be accesessed through a read reaf (&) and do not
/// require &mut. Internally a database is syncronized through a mutex lock.
///
/// Repeating commands should produce no changes and return no error.
impl AuthorityState {
    /// Initiate a new order.
    pub async fn handle_order(&self, order: Order) -> Result<AccountInfoResponse, FastPayError> {
        // Check the sender's signature.
        order.check_signature()?;

        let input_objects = order.input_objects();
        let mut mutable_objects = Vec::with_capacity(input_objects.len());

        // There is at least one input
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
                FastPayError::UnexpectedSequenceNumber {
                    object_id,
                    expected_sequence: object.next_sequence_number,
                    received_sequence: sequence_number
                }
            );

            // If this is an immutable object, checks end here.
            if object.is_read_only() {
                continue;
            }

            // Additional checks for mutable objects
            // Check the transaction sender is also the object owner
            fp_ensure!(
                order.sender() == &object.owner,
                FastPayError::IncorrectSigner
            );

            /* The call to self.set_order_lock checks the lock is not conflicting,
            and returns ConflictingOrder in case there is a lock on a different
            existing order */

            mutable_objects.push((object_id, sequence_number));
        }

        // There is at least one mutable input.
        fp_ensure!(
            !mutable_objects.is_empty(),
            FastPayError::InsufficientObjectNumber
        );

        // TODO(https://github.com/MystenLabs/fastnft/issues/45): check that c.gas_payment exists + that its value is > gas_budget
        // Note: the above code already checks that the gas object exists because it is included in the
        //       input_objects() list. So need to check it contains some gas.

        let object_id = *order.object_id();
        let signed_order = SignedOrder::new(order, self.name, &self.secret);

        // Check and write locks, to signed order, into the database
        self.set_order_lock(&mutable_objects, signed_order).await?;

        // TODO: what should we return here?
        let info = self.make_object_info(object_id).await?;
        Ok(info)
    }

    /// Confirm a transfer.
    pub async fn handle_confirmation_order(
        &self,
        confirmation_order: ConfirmationOrder,
    ) -> Result<AccountInfoResponse, FastPayError> {
        let certificate = confirmation_order.certificate;
        let order = certificate.order.clone();
        let mut object_id = *order.object_id();
        // Check the certificate and retrieve the transfer data.
        certificate.check(&self.committee)?;

        let mut inputs = vec![];
        for (input_object_id, input_seq) in order.input_objects() {
            // If we have a certificate on the confirmation order it means that the input
            // object exists on other honest authorities, but we do not have it.
            let input_object = self
                .object_state(&input_object_id)
                .await
                .map_err(|_| FastPayError::ObjectNotFound)?;

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

        // Update the database in an atomic manner
        self.update_state(temporary_store, certificate).await?;

        let info = self.make_object_info(object_id).await?;
        Ok(info)
    }

    pub async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, FastPayError> {
        let mut response = self.make_object_info(request.object_id).await?;
        if let Some(seq) = request.request_sequence_number {
            // Get the Transaction Digest that created the object
            let transaction_digest = self
                .parent(&(request.object_id, seq.increment()?))
                .await
                .ok_or(FastPayError::CertificateNotfound)?;
            // Get the cert from the transaction digest
            response.requested_certificate = Some(
                self.read_certificate(&transaction_digest)
                    .await?
                    .ok_or(FastPayError::CertificateNotfound)?
                    .clone(),
            );
        }
        Ok(response)
    }
}

impl AuthorityState {
    pub fn new<P: AsRef<Path>>(
        committee: Committee,
        name: AuthorityName,
        secret: KeyPair,
        path: P,
    ) -> Self {
        AuthorityState {
            committee,
            name,
            secret,
            // objects: Arc::new(Mutex::new(BTreeMap::new())),
            native_functions: NativeFunctionTable::new(),
            _database: Arc::new(Mutex::new(AuthorityStore::open(path))),
        }
    }

    async fn object_state(&self, object_id: &ObjectID) -> Result<Object, FastPayError> {
        self._database.lock().unwrap().object_state(object_id)
    }

    pub async fn insert_object(&self, object: Object) {
        self._database
            .lock()
            .unwrap()
            .insert_object(object)
            .expect("TODO: propagate the error")
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
            .or::<FastPayError>(Ok(None))?;

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
    pub async fn init_order_lock(&self, object_ref: ObjectRef) {
        self._database
            .lock()
            .unwrap()
            .init_order_lock(object_ref)
            .expect("TODO: propagate the error")
    }

    /// Set the order lock to a specific transaction
    pub async fn set_order_lock(
        &self,
        mutable_input_objects: &[ObjectRef],
        signed_order: SignedOrder,
    ) -> Result<(), FastPayError> {
        self._database
            .lock()
            .unwrap()
            .set_order_lock(mutable_input_objects, signed_order)
    }

    async fn update_state(
        &self,
        temporary_store: AuthorityTemporaryStore,
        certificate: CertifiedOrder,
    ) -> Result<(), FastPayError> {
        self._database
            .lock()
            .unwrap()
            .update_state(temporary_store, certificate)
    }

    /// Get a read reference to an object/seq lock
    pub async fn get_order_lock(
        &self,
        object_ref: &ObjectRef,
    ) -> Result<Option<SignedOrder>, FastPayError> {
        self._database.lock().unwrap().get_order_lock(object_ref)
    }

    // Helper functions to manage certificates

    /// Read from the DB of certificates
    pub async fn read_certificate(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<CertifiedOrder>, FastPayError> {
        self._database.lock().unwrap().read_certificate(digest)
    }

    pub async fn parent(&self, object_ref: &ObjectRef) -> Option<TransactionDigest> {
        self._database
            .lock()
            .unwrap()
            .parent(object_ref)
            .expect("TODO: propagate the error")
    }
}
