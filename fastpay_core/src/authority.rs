// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use fastx_adapter::{adapter, genesis};
use fastx_types::{
    base_types::*,
    committee::Committee,
    error::FastPayError,
    fp_bail, fp_ensure,
    gas::{calculate_object_transfer_cost, check_gas_requirement, deduct_gas},
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
    collections::{BTreeMap, HashMap, HashSet},
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
    pub async fn handle_order(&self, order: Order) -> Result<OrderInfoResponse, FastPayError> {
        // Check the sender's signature.
        order.check_signature()?;
        let transaction_digest = order.digest();

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
        let transfer_object_id = if let OrderKind::Transfer(t) = &order.kind {
            Some(&t.object_ref.0)
        } else {
            None
        };

        let ids: Vec<_> = input_objects.iter().map(|(id, _, _)| *id).collect();
        // Get a copy of the object.
        // TODO: We only need to read the read_only and owner field of the object,
        //      it's a bit wasteful to copy the entire object.
        let objects = self.get_objects(&ids[..]).await?;
        for (object_ref, object) in input_objects.into_iter().zip(objects) {
            let (object_id, sequence_number, _object_digest) = object_ref;

            fp_ensure!(
                sequence_number <= SequenceNumber::max(),
                FastPayError::InvalidSequenceNumber
            );

            let object = object.ok_or(FastPayError::ObjectNotFound)?;

            // TODO(https://github.com/MystenLabs/fastnft/issues/123): This hack substitutes the real
            // object digest instead of using the one passed in by the client. We need to fix clients and
            // then use the digest provided, by deleting this line.
            let object_digest = object.digest();

            // Check that the seq number is the same
            fp_ensure!(
                object.version() == sequence_number,
                FastPayError::UnexpectedSequenceNumber {
                    object_id,
                    expected_sequence: object.version(),
                    received_sequence: sequence_number
                }
            );

            if object.is_read_only() {
                // For a tranfer order, the object to be transferred
                // must not be read only.
                fp_ensure!(
                    Some(&object_id) != transfer_object_id,
                    FastPayError::TransferImmutableError
                );
                // Gas object must not be immutable.
                fp_ensure!(
                    &object_id != order.gas_payment_object_id(),
                    FastPayError::InsufficientGas {
                        error: "Gas object should not be immutable".to_string()
                    }
                );
                // Checks for read-only objects end here.
                continue;
            }

            // Additional checks for mutable objects
            // Check the transaction sender is also the object owner
            fp_ensure!(
                order.sender() == &object.owner,
                FastPayError::IncorrectSigner
            );

            if &object_id == order.gas_payment_object_id() {
                check_gas_requirement(&order, &object)?;
            }

            /* The call to self.set_order_lock checks the lock is not conflicting,
            and returns ConflictingOrder in case there is a lock on a different
            existing order */

            mutable_objects.push((object_id, sequence_number, object_digest));
        }

        // We have checked that there is a mutable gas object.
        debug_assert!(!mutable_objects.is_empty());

        let signed_order = SignedOrder::new(order, self.name, &self.secret);

        // Check and write locks, to signed order, into the database
        self.set_order_lock(&mutable_objects, signed_order).await?;

        // Return the signed Order or maybe a cert.
        self.make_order_info(&transaction_digest).await
    }

    /// Confirm a transfer.
    pub async fn handle_confirmation_order(
        &self,
        confirmation_order: ConfirmationOrder,
    ) -> Result<OrderInfoResponse, FastPayError> {
        let certificate = confirmation_order.certificate;
        let order = certificate.order.clone();
        let transaction_digest = order.digest();

        // Check the certificate and retrieve the transfer data.
        certificate.check(&self.committee)?;

        let input_objects = order.input_objects();
        let ids: Vec<_> = input_objects.iter().map(|(id, _, _)| *id).collect();
        // Get a copy of the object.
        // TODO: We only need to read the read_only and owner field of the object,
        //      it's a bit wasteful to copy the entire object.
        let objects = self.get_objects(&ids[..]).await?;

        let mut inputs = vec![];
        let mut owner_index = HashMap::new();
        for (object_ref, object) in input_objects.into_iter().zip(objects) {
            let (input_object_id, input_seq, _input_digest) = object_ref;

            // If we have a certificate on the confirmation order it means that the input
            // object exists on other honest authorities, but we do not have it.
            let input_object = object.ok_or(FastPayError::ObjectNotFound)?;

            let input_sequence_number = input_object.version();

            // Check that the current object is exactly the right version.
            if input_sequence_number < input_seq {
                fp_bail!(FastPayError::MissingEalierConfirmations {
                    current_sequence_number: input_sequence_number
                });
            }
            if input_sequence_number > input_seq {
                // Transfer was already confirmed.
                return self.make_order_info(&transaction_digest).await;
            }

            if !input_object.is_read_only() {
                owner_index.insert(input_object_id, input_object.owner);
            }

            inputs.push(input_object.clone());
        }

        // Insert into the certificates map
        let transaction_digest = certificate.order.digest();
        let mut tx_ctx = TxContext::new(transaction_digest);

        // Order-specific logic
        let mut temporary_store = AuthorityTemporaryStore::new(self, &inputs);
        let status = match order.kind {
            OrderKind::Transfer(t) => {
                debug_assert!(
                    inputs.len() == 2,
                    "Expecting two inputs: gas and object to be transferred"
                );
                // unwraps here are safe because we built `inputs`
                let mut gas_object = inputs.pop().unwrap();
                deduct_gas(
                    &mut gas_object,
                    calculate_object_transfer_cost(&inputs[0]) as i128,
                )?;
                temporary_store.write_object(gas_object);

                let mut output_object = inputs.pop().unwrap();
                output_object.transfer(match t.recipient {
                    Address::Primary(_) => FastPayAddress::default(),
                    Address::FastPay(addr) => addr,
                })?;
                temporary_store.write_object(output_object);
                Ok(())
            }
            OrderKind::Call(c) => {
                // unwraps here are safe because we built `inputs`
                let gas_object = inputs.pop().unwrap();
                let package = inputs.pop().unwrap();
                adapter::execute(
                    &mut temporary_store,
                    self.native_functions.clone(),
                    package,
                    &c.module,
                    &c.function,
                    c.type_arguments,
                    inputs,
                    c.pure_arguments,
                    c.gas_budget,
                    gas_object,
                    tx_ctx,
                )
            }

            OrderKind::Publish(m) => {
                let gas_object = inputs.pop().unwrap();
                adapter::publish(
                    &mut temporary_store,
                    m.modules,
                    m.sender,
                    &mut tx_ctx,
                    gas_object,
                )
            }
        };

        // Make a list of all object that are either deleted or have changed owner, along with their old owner.
        // This is used to update the owner index.
        let drop_index_entries = temporary_store
            .deleted()
            .iter()
            .map(|(id, _, _)| (owner_index[id], *id))
            .chain(temporary_store.written().iter().filter_map(|(id, _, _)| {
                let owner = owner_index.get(id);
                if owner.is_some() && *owner.unwrap() != temporary_store.objects()[id].owner {
                    Some((owner_index[id], *id))
                } else {
                    None
                }
            }))
            .collect();

        // Update the database in an atomic manner
        let to_signed_effects = temporary_store.to_signed_effects(
            &self.name,
            &self.secret,
            &transaction_digest,
            status,
        );
        self.update_state(
            temporary_store,
            drop_index_entries,
            certificate,
            to_signed_effects,
        )
        .await // Returns the OrderInfoResponse
    }

    pub async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, FastPayError> {
        self.make_account_info(request.account)
    }

    pub async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, FastPayError> {
        if let Some(seq) = request.request_sequence_number {
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
        }
    }
}

impl AuthorityState {
    pub async fn new_with_genesis_modules(
        committee: Committee,
        name: AuthorityName,
        secret: KeyPair,
        store: Arc<AuthorityStore>,
    ) -> Self {
        let (genesis_modules, native_functions) = genesis::clone_genesis_data();
        let state = AuthorityState {
            committee,
            name,
            secret,
            native_functions,
            _database: store,
        };

        for genesis_module in genesis_modules {
            #[cfg(debug_assertions)]
            genesis_module.data.try_as_package().unwrap();

            state
                .init_order_lock(genesis_module.to_object_reference())
                .await;
            state.insert_object(genesis_module).await;
        }
        state
    }

    #[cfg(test)]
    pub fn new_without_genesis_for_testing(
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

    /// Make an information response for an order
    async fn make_order_info(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> Result<OrderInfoResponse, FastPayError> {
        self._database.get_order_info(transaction_digest)
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
            next_sequence_number: object.version(),
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
        expired_object_owners: Vec<(FastPayAddress, ObjectID)>,
        certificate: CertifiedOrder,
        signed_effects: SignedOrderEffects,
    ) -> Result<OrderInfoResponse, FastPayError> {
        self._database.update_state(
            temporary_store,
            expired_object_owners,
            certificate,
            signed_effects,
        )
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

    pub async fn get_objects(
        &self,
        _objects: &[ObjectID],
    ) -> Result<Vec<Option<Object>>, FastPayError> {
        self._database.get_objects(_objects)
    }

    /// Returns all parents (object_ref and transaction digests) that match an object_id, at
    /// any object version, or optionally at a specific version.
    pub async fn get_parent_iterator(
        &self,
        object_id: ObjectID,
        seq: Option<SequenceNumber>,
    ) -> Result<Vec<(ObjectRef, TransactionDigest)>, FastPayError> {
        {
            self._database.get_parent_iterator(object_id, seq)
        }
    }
}
