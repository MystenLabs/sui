// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use fastx_adapter::adapter;
use fastx_types::{
    base_types::*,
    committee::Committee,
    error::FastPayError,
    fp_bail, fp_ensure,
    gas::check_gas_requirement,
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
};

#[cfg(test)]
#[path = "unit_tests/authority_tests.rs"]
mod authority_tests;

mod temporary_store;
use temporary_store::AuthorityTemporaryStore;

mod authority_store;
pub use authority_store::AuthorityStore;

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
    _database: Arc<AuthorityStore>,
}

/// The authority state encapsulates all state, drives execution, and ensures safety.
///
/// Note the authority operations can be accesessed through a read reaf (&) and do not
/// require &mut. Internally a database is syncronized through a mutex lock.
///
/// Repeating commands should produce no changes and return no error.
impl AuthorityState {
    /// Initiate a new order.
    pub async fn handle_order(&self, order: Order) -> Result<ObjectInfoResponse, FastPayError> {
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

        let input_object_cnt = input_objects.len();
        for (idx, object_ref) in input_objects.into_iter().enumerate() {
            let (object_id, sequence_number, _object_digest) = object_ref;

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

            // TODO(https://github.com/MystenLabs/fastnft/issues/123): This hack substitutes the real
            // object digest instead of using the one passed in by the client. We need to fix clients and
            // then use the digest provided, by deleting this line.
            let object_digest = object.digest();

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

            // The last object in input_objects is always the gas payment object.
            // TODO: Add get_gas_object_ref() to Order once Transfer order also has gas object.
            if idx + 1 == input_object_cnt {
                check_gas_requirement(&order, &object)?;
            }

            /* The call to self.set_order_lock checks the lock is not conflicting,
            and returns ConflictingOrder in case there is a lock on a different
            existing order */

            mutable_objects.push((object_id, sequence_number, object_digest));
        }

        // There is at least one mutable input.
        fp_ensure!(
            !mutable_objects.is_empty(),
            FastPayError::InsufficientObjectNumber
        );

        let object_id = *order.object_id();
        let signed_order = SignedOrder::new(order, self.name, &self.secret);

        // Check and write locks, to signed order, into the database
        self.set_order_lock(&mutable_objects, signed_order).await?;

        // TODO: what should we return here?
        let info = self.make_object_info(object_id, None).await?;
        Ok(info)
    }

    /// Confirm a transfer.
    pub async fn handle_confirmation_order(
        &self,
        confirmation_order: ConfirmationOrder,
    ) -> Result<ObjectInfoResponse, FastPayError> {
        let certificate = confirmation_order.certificate;
        let order = certificate.order.clone();
        let mut object_id = *order.object_id();
        // Check the certificate and retrieve the transfer data.
        certificate.check(&self.committee)?;

        let mut inputs = vec![];
        for (input_object_id, input_seq, _input_digest) in order.input_objects() {
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
                return self.make_object_info(object_id, None).await;
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
                    Address::Primary(_) => FastPayAddress::default(),
                    Address::FastPay(addr) => addr,
                });
                temporary_store.write_object(output_object);
            }
            OrderKind::Call(c) => {
                // unwraps here are safe because we built `inputs`
                let gas_object = inputs.pop().unwrap();
                let module = inputs.pop().unwrap();
                match adapter::execute(
                    &mut temporary_store,
                    self.native_functions.clone(),
                    module,
                    &c.function,
                    c.type_arguments,
                    inputs,
                    c.pure_arguments,
                    c.gas_budget,
                    gas_object,
                    tx_ctx,
                ) {
                    Ok(()) => {
                        // TODO(https://github.com/MystenLabs/fastnft/issues/63): AccountInfoResponse should return all object ID outputs.
                        // but for now it only returns one, so use this hack
                        object_id = temporary_store.written[0].0
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
            OrderKind::Publish(m) => {
                let gas_object = inputs.pop().unwrap();
                match adapter::publish(
                    &mut temporary_store,
                    m.modules,
                    m.sender,
                    &mut tx_ctx,
                    gas_object,
                ) {
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

        let info = self.make_object_info(object_id, None).await?;
        Ok(info)
    }

    pub async fn handle_info_request(
        &self,
        request: InfoRequest,
    ) -> Result<InfoResponse, FastPayError> {
        match request {
            InfoRequest::AccountInfoRequest(request) => self
                .make_account_info(request.account)
                .map(|info| info.into()),
            InfoRequest::ObjectInfoRequest(request) => {
                let response = if let Some(seq) = request.request_sequence_number {
                    // TODO(https://github.com/MystenLabs/fastnft/issues/123): Here we need to develop a strategy
                    // to provide back to the client the object digest for specific objects requested. Probably,
                    // we have to return the full ObjectRef and why not the actual full object here.
                    let obj = self
                        .object_state(&request.object_id)
                        .await
                        .map_err(|_| FastPayError::ObjectNotFound)?;

                    // Get the Transaction Digest that created the object
                    let transaction_digest = self
                        .parent(&(request.object_id, seq.increment()?, obj.digest()))
                        .await
                        .ok_or(FastPayError::CertificateNotfound)?;
                    // Get the cert from the transaction digest
                    let requested_certificate = Some(
                        self.read_certificate(&transaction_digest)
                            .await?
                            .ok_or(FastPayError::CertificateNotfound)?,
                    );
                    self.make_object_info(request.object_id, requested_certificate)
                        .await
                } else {
                    self.make_object_info(request.object_id, None).await
                };
                response.map(|info| info.into())
            }
        }
    }
}

impl AuthorityState {
    pub fn new(
        committee: Committee,
        name: AuthorityName,
        secret: KeyPair,
        store: Arc<AuthorityStore>,
    ) -> Self {
        AuthorityState {
            committee,
            name,
            secret,
            native_functions: NativeFunctionTable::new(),
            _database: store,
        }
    }

    async fn object_state(&self, object_id: &ObjectID) -> Result<Object, FastPayError> {
        self._database.object_state(object_id)
    }

    pub async fn insert_object(&self, object: Object) {
        self._database
            .insert_object(object)
            .expect("TODO: propagate the error")
    }

    /// Make an information summary of an object to help clients

    async fn make_object_info(
        &self,
        object_id: ObjectID,
        requested_certificate: Option<CertifiedOrder>,
    ) -> Result<ObjectInfoResponse, FastPayError> {
        let object = self.object_state(&object_id).await?;
        let lock = self
            .get_order_lock(&object.to_object_reference())
            .await
            .or::<FastPayError>(Ok(None))?;

        Ok(ObjectInfoResponse {
            object_id: object.id(),
            owner: object.owner,
            next_sequence_number: object.next_sequence_number,
            requested_certificate,
            pending_confirmation: lock,
            requested_received_transfers: Vec::new(),
        })
    }

    fn make_account_info(
        &self,
        account: FastPayAddress,
    ) -> Result<AccountInfoResponse, FastPayError> {
        self._database
            .get_account_objects(account)
            .map(|object_ids| AccountInfoResponse {
                object_ids,
                owner: account,
            })
    }

    // Helper function to manage order_locks

    /// Initialize an order lock for an object/sequence to None
    pub async fn init_order_lock(&self, object_ref: ObjectRef) {
        self._database
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
            .set_order_lock(mutable_input_objects, signed_order)
    }

    async fn update_state(
        &self,
        temporary_store: AuthorityTemporaryStore,
        certificate: CertifiedOrder,
    ) -> Result<(), FastPayError> {
        self._database.update_state(temporary_store, certificate)
    }

    /// Get a read reference to an object/seq lock
    pub async fn get_order_lock(
        &self,
        object_ref: &ObjectRef,
    ) -> Result<Option<SignedOrder>, FastPayError> {
        self._database.get_order_lock(object_ref)
    }

    // Helper functions to manage certificates

    /// Read from the DB of certificates
    pub async fn read_certificate(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<CertifiedOrder>, FastPayError> {
        self._database.read_certificate(digest)
    }

    pub async fn parent(&self, object_ref: &ObjectRef) -> Option<TransactionDigest> {
        self._database
            .parent(object_ref)
            .expect("TODO: propagate the error")
    }
}
