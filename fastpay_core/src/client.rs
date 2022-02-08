// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use crate::{authority_aggregator::AuthorityAggregator, authority_client::AuthorityAPI};
use async_trait::async_trait;
use fastx_framework::build_move_package_to_bytes;
use fastx_types::{
    base_types::*, committee::Committee, error::FastPayError, fp_ensure, messages::*,
};
use futures::future;
use itertools::Itertools;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use typed_store::rocks::open_cf;
use typed_store::Map;

use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    pin::Pin,
};

/// a Trait object for `signature::Signer` that is:
/// - Pin, i.e. confined to one place in memory (we don't want to copy private keys).
/// - Sync, i.e. can be safely shared between threads.
///
/// Typically instantiated with Box::pin(keypair) where keypair is a `KeyPair`
///
pub type StableSyncSigner = Pin<Box<dyn signature::Signer<ed25519_dalek::Signature> + Send + Sync>>;

pub mod client_store;
use self::client_store::ClientStore;

#[cfg(test)]
use fastx_types::FASTX_FRAMEWORK_ADDRESS;

pub type AsyncResult<'a, T, E> = future::BoxFuture<'a, Result<T, E>>;

pub struct ClientState<AuthorityAPI> {
    /// Our FastPay address.
    address: FastPayAddress,
    /// Our signature key.
    secret: StableSyncSigner,
    /// Authority entry point.
    authorities: AuthorityAggregator<AuthorityAPI>,
    /// Persistent store for client
    store: ClientStore,
}

pub enum ObjectRead {
    NotExists,
    Exists(ObjectRef, Object),
    Deleted(ObjectRef),
}

// Operations are considered successful when they successfully reach a quorum of authorities.
#[async_trait]
pub trait Client {
    /// Send object to a FastX account.
    async fn transfer_object(
        &mut self,
        object_id: ObjectID,
        gas_payment: ObjectID,
        recipient: FastPayAddress,
    ) -> Result<(CertifiedOrder, OrderEffects), anyhow::Error>;

    /// Try to complete all pending orders once. Return if any fails
    async fn try_complete_pending_orders(&mut self) -> Result<(), FastPayError>;

    /// Synchronise client state with a random authorities, updates all object_ids and certificates, request only goes out to one authority.
    /// this method doesn't guarantee data correctness, client will have to handle potential byzantine authority
    async fn sync_client_state(&mut self) -> Result<(), anyhow::Error>;

    /// Call move functions in the module in the given package, with args supplied
    async fn move_call(
        &mut self,
        package_object_ref: ObjectRef,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        gas_object_ref: ObjectRef,
        object_arguments: Vec<ObjectRef>,
        pure_arguments: Vec<Vec<u8>>,
        gas_budget: u64,
    ) -> Result<(CertifiedOrder, OrderEffects), anyhow::Error>;

    /// Publish Move modules
    async fn publish(
        &mut self,
        package_source_files_path: String,
        gas_object_ref: ObjectRef,
    ) -> Result<(CertifiedOrder, OrderEffects), anyhow::Error>;

    /// Get the object information
    async fn get_object_info(
        &mut self,
        object_info_req: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, anyhow::Error>;

    /// Get all object we own.
    async fn get_owned_objects(&self) -> Vec<ObjectID>;

    async fn download_owned_objects_not_in_db(&self) -> Result<BTreeSet<ObjectRef>, FastPayError>;
}

impl<A> ClientState<A> {
    /// It is recommended that one call sync and download_owned_objects
    /// right after constructor to fetch missing info form authorities
    /// TODO: client should manage multiple addresses instead of each addr having DBs
    /// https://github.com/MystenLabs/fastnft/issues/332
    pub fn new(
        path: PathBuf,
        address: FastPayAddress,
        secret: StableSyncSigner,
        committee: Committee,
        authority_clients: BTreeMap<AuthorityName, A>,
        certificates: BTreeMap<TransactionDigest, CertifiedOrder>,
        object_refs: BTreeMap<ObjectID, ObjectRef>,
    ) -> Result<Self, FastPayError> {
        let client_state = ClientState {
            address,
            secret,
            authorities: AuthorityAggregator::new(committee, authority_clients),
            store: ClientStore::new(path),
        };

        // Backfill the DB
        client_state.store.populate(object_refs, certificates)?;
        Ok(client_state)
    }

    pub fn address(&self) -> FastPayAddress {
        self.address
    }

    pub fn next_sequence_number(
        &self,
        object_id: &ObjectID,
    ) -> Result<SequenceNumber, FastPayError> {
        if self.store.object_sequence_numbers.contains_key(object_id)? {
            Ok(self
                .store
                .object_sequence_numbers
                .get(object_id)?
                .expect("Unable to get sequence number"))
        } else {
            Err(FastPayError::ObjectNotFound {
                object_id: *object_id,
            })
        }
    }
    pub fn object_ref(&self, object_id: ObjectID) -> Result<ObjectRef, FastPayError> {
        self.store
            .object_refs
            .get(&object_id)?
            .ok_or(FastPayError::ObjectNotFound { object_id })
    }

    pub fn object_refs(&self) -> BTreeMap<ObjectID, ObjectRef> {
        self.store.object_refs.iter().collect()
    }

    /// Need to remove unwraps. Found this tricky due to iterator requirements of downloader and not being able to exit from closure to top fn
    /// https://github.com/MystenLabs/fastnft/issues/307
    pub fn certificates(&self, object_id: &ObjectID) -> impl Iterator<Item = CertifiedOrder> + '_ {
        self.store
            .object_certs
            .get(object_id)
            .unwrap()
            .into_iter()
            .flat_map(|cert_digests| {
                self.store
                    .certificates
                    .multi_get(&cert_digests[..])
                    .unwrap()
                    .into_iter()
                    .flatten()
            })
    }

    pub fn all_certificates(&self) -> BTreeMap<TransactionDigest, CertifiedOrder> {
        self.store.certificates.iter().collect()
    }

    pub fn insert_object_info(
        &mut self,
        object_ref: &ObjectRef,
        parent_tx_digest: &TransactionDigest,
    ) -> Result<(), FastPayError> {
        let (object_id, seq, _) = object_ref;
        let mut tx_digests = self.store.object_certs.get(object_id)?.unwrap_or_default();
        tx_digests.push(*parent_tx_digest);

        // Multi table atomic insert using batches
        let batch = self
            .store
            .object_sequence_numbers
            .batch()
            .insert_batch(
                &self.store.object_sequence_numbers,
                std::iter::once((object_id, seq)),
            )?
            .insert_batch(
                &self.store.object_certs,
                std::iter::once((object_id, &tx_digests.to_vec())),
            )?
            .insert_batch(
                &self.store.object_refs,
                std::iter::once((object_id, object_ref)),
            )?;
        // Execute atomic write of opers
        batch.write()?;
        Ok(())
    }

    pub fn remove_object_info(&mut self, object_id: &ObjectID) -> Result<(), FastPayError> {
        // Multi table atomic delete using batches
        let batch = self
            .store
            .object_sequence_numbers
            .batch()
            .delete_batch(
                &self.store.object_sequence_numbers,
                std::iter::once(object_id),
            )?
            .delete_batch(&self.store.object_certs, std::iter::once(object_id))?
            .delete_batch(&self.store.object_refs, std::iter::once(object_id))?;
        // Execute atomic write of opers
        batch.write()?;
        Ok(())
    }

    #[cfg(test)]
    pub fn store(&self) -> &ClientStore {
        &self.store
    }

    #[cfg(test)]
    pub fn secret(&self) -> &dyn signature::Signer<ed25519_dalek::Signature> {
        &*self.secret
    }
}

impl<A> ClientState<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    #[cfg(test)]
    pub fn authorities(&self) -> &AuthorityAggregator<A> {
        &self.authorities
    }

    #[cfg(test)]
    pub async fn get_framework_object_ref(&mut self) -> Result<ObjectRef, anyhow::Error> {
        let info = self
            .get_object_info(ObjectInfoRequest {
                object_id: FASTX_FRAMEWORK_ADDRESS,
                request_sequence_number: None,
            })
            .await?;
        let reference = info
            .object_and_lock
            .ok_or(FastPayError::ObjectNotFound {
                object_id: FASTX_FRAMEWORK_ADDRESS,
            })?
            .object
            .to_object_reference();
        Ok(reference)
    }

    async fn execute_transaction_inner(
        &mut self,
        order: &Order,
    ) -> Result<(CertifiedOrder, OrderEffects), anyhow::Error> {
        let (new_certificate, effects) = self.authorities.execute_transaction(order).await?;

        // Update local data using new order response.
        self.update_objects_from_order_info(new_certificate.clone(), effects.clone())
            .await?;

        Ok((new_certificate, effects))
    }

    /// Execute (or retry) an order and execute the Confirmation Order.
    /// Update local object states using newly created certificate and ObjectInfoResponse from the Confirmation step.
    /// This functions locks all the input objects if possible, and unlocks at the end of confirmation or if an error occurs
    /// TODO: define other situations where we can unlock objects after authority error
    /// https://github.com/MystenLabs/fastnft/issues/346
    async fn execute_transaction(
        &mut self,
        order: Order,
    ) -> Result<(CertifiedOrder, OrderEffects), anyhow::Error> {
        for object_kind in &order.input_objects() {
            let object_id = object_kind.object_id();
            let next_sequence_number = self.next_sequence_number(&object_id).unwrap_or_default();
            fp_ensure!(
                object_kind.version() >= next_sequence_number,
                FastPayError::UnexpectedSequenceNumber {
                    object_id,
                    expected_sequence: next_sequence_number,
                }
                .into()
            );
        }
        // Lock the objects in this order
        self.lock_pending_order_objects(&order)?;

        // We can escape this function without unlocking. This could be dangerous
        let result = self.execute_transaction_inner(&order).await;

        // How do we handle errors on authority which lock objects?
        // Currently VM crash can keep objects locked, but we would like to avoid this.
        // TODO: https://github.com/MystenLabs/fastnft/issues/349
        // https://github.com/MystenLabs/fastnft/issues/211
        // https://github.com/MystenLabs/fastnft/issues/346

        self.unlock_pending_order_objects(&order)?;
        result
    }

    /// This function verifies that the objects in the specfied order are locked by the given order
    /// We use this to ensure that an order can indeed unclock or lock certain objects in order
    /// This means either exactly all the objects are owned by this order, or by no order
    /// The caller has to explicitly find which objects are locked
    /// TODO: always return true for immutable objects https://github.com/MystenLabs/fastnft/issues/305
    /// TODO: this function can fail. Need to handle it https://github.com/MystenLabs/fastnft/issues/383
    fn can_lock_or_unlock(&self, order: &Order) -> Result<bool, FastPayError> {
        let iter_matches = self.store.pending_orders.multi_get(
            &order
                .input_objects()
                .iter()
                .map(|q| q.object_id())
                .collect_vec(),
        )?;
        for o in iter_matches {
            // If we find any order that isn't the given order, we cannot proceed
            if o.is_some() && o.unwrap() != *order {
                return Ok(false);
            }
        }
        // All the objects are either owned by this order or by no order
        Ok(true)
    }

    /// Locks the objects for the given order
    /// It is important to check that the object is not locked before locking again
    /// One should call can_lock_or_unlock before locking as this overwites the previous lock
    /// If the object is already locked, ensure it is unlocked by calling unlock_pending_order_objects
    /// Client runs sequentially right now so access to this is safe
    /// Double-locking can cause equivocation. TODO: https://github.com/MystenLabs/fastnft/issues/335
    pub fn lock_pending_order_objects(&self, order: &Order) -> Result<(), FastPayError> {
        if !self.can_lock_or_unlock(order)? {
            return Err(FastPayError::ConcurrentTransactionError);
        }
        self.store
            .pending_orders
            .multi_insert(
                order
                    .input_objects()
                    .iter()
                    .map(|e| (e.object_id(), order.clone())),
            )
            .map_err(|e| e.into())
    }
    /// Unlocks the objects for the given order
    /// Unlocking an already unlocked object, is a no-op and does not Err
    fn unlock_pending_order_objects(&self, order: &Order) -> Result<(), FastPayError> {
        if !self.can_lock_or_unlock(order)? {
            return Err(FastPayError::ConcurrentTransactionError);
        }
        self.store
            .pending_orders
            .multi_remove(order.input_objects().iter().map(|e| e.object_id()))
            .map_err(|e| e.into())
    }

    async fn update_objects_from_order_info(
        &mut self,
        cert: CertifiedOrder,
        effects: OrderEffects,
    ) -> Result<(CertifiedOrder, OrderEffects), FastPayError> {
        // The cert should be included in the response
        let parent_tx_digest = cert.order.digest();
        self.store.certificates.insert(&parent_tx_digest, &cert)?;

        let mut objs_to_download = Vec::new();

        for &(object_ref, owner) in effects.all_mutated() {
            let (object_id, seq, _) = object_ref;
            let old_seq = self
                .store
                .object_sequence_numbers
                .get(&object_id)?
                .unwrap_or_default();
            // only update if data is new
            if old_seq < seq {
                if owner.is_address(&self.address) {
                    self.insert_object_info(&object_ref, &parent_tx_digest)?;
                    objs_to_download.push(object_ref);
                } else {
                    self.remove_object_info(&object_id)?;
                }
            } else if old_seq == seq && owner.is_address(&self.address) {
                // ObjectRef can be 1 version behind because it's only updated after confirmation.
                self.store.object_refs.insert(&object_id, &object_ref)?;
            }
        }

        // TODO: decide what to do with failed object downloads
        // https://github.com/MystenLabs/fastnft/issues/331
        let _failed = self.download_objects_not_in_db(objs_to_download).await?;

        for (object_id, seq, _) in &effects.deleted {
            let old_seq = self
                .store
                .object_sequence_numbers
                .get(object_id)?
                .unwrap_or_default();
            if old_seq < *seq {
                self.remove_object_info(object_id)?;
            }
        }
        Ok((cert, effects))
    }

    /// Fetch the objects for the given list of ObjectRefs, which do not already exist in the db.
    /// How it works: this function finds all object refs that are not in the DB
    /// then it downloads them by calling download_objects_from_all_authorities.
    /// Afterwards it persists objects returned.
    /// Returns a set of the object ids which failed to download
    /// TODO: return failed download errors along with the object id
    async fn download_objects_not_in_db(
        &self,
        object_refs: Vec<ObjectRef>,
    ) -> Result<BTreeSet<ObjectRef>, FastPayError> {
        // Check the DB
        // This could be expensive. Might want to use object_ref table
        // We want items that are NOT in the table
        let fresh_object_refs = self
            .store
            .objects
            .multi_get(&object_refs)?
            .iter()
            .zip(object_refs)
            .filter_map(|(object, ref_)| match object {
                Some(_) => None,
                None => Some(ref_),
            })
            .collect::<BTreeSet<_>>();

        // Now that we have all the fresh ids, fetch from authorities.
        let mut receiver = self
            .authorities
            .fetch_objects_from_authorities(fresh_object_refs.clone());

        let mut err_object_refs = fresh_object_refs.clone();
        // Receive from the downloader
        while let Some(resp) = receiver.recv().await {
            // Persists them to disk
            if let Ok(o) = resp {
                self.store.objects.insert(&o.to_object_reference(), &o)?;
                err_object_refs.remove(&o.to_object_reference());
            }
        }
        Ok(err_object_refs)
    }
}

#[async_trait]
impl<A> Client for ClientState<A>
where
    A: AuthorityAPI + Send + Sync + Clone + 'static,
{
    async fn transfer_object(
        &mut self,
        object_id: ObjectID,
        gas_payment: ObjectID,
        recipient: FastPayAddress,
    ) -> Result<(CertifiedOrder, OrderEffects), anyhow::Error> {
        let object_ref = self
            .store
            .object_refs
            .get(&object_id)?
            .ok_or(FastPayError::ObjectNotFound { object_id })?;

        let gas_payment =
            self.store
                .object_refs
                .get(&gas_payment)?
                .ok_or(FastPayError::ObjectNotFound {
                    object_id: gas_payment,
                })?;

        let transfer = Transfer {
            object_ref,
            sender: self.address,
            recipient,
            gas_payment,
        };
        let order = Order::new_transfer(transfer, &*self.secret);
        let (certificate, effects) = self.execute_transaction(order).await?;
        self.authorities
            .process_certificate(certificate.clone(), Duration::from_secs(60))
            .await?;

        // remove object from local storage if the recipient is not us.
        if recipient != self.address {
            self.remove_object_info(&object_id)?;
        }

        Ok((certificate, effects))
    }

    async fn try_complete_pending_orders(&mut self) -> Result<(), FastPayError> {
        // Orders are idempotent so no need to prevent multiple executions
        let unique_pending_orders: HashSet<_> = self
            .store
            .pending_orders
            .iter()
            .map(|(_, ord)| ord)
            .collect();
        // Need some kind of timeout or max_trials here?
        // TODO: https://github.com/MystenLabs/fastnft/issues/330
        for order in unique_pending_orders {
            self.execute_transaction(order.clone()).await.map_err(|e| {
                FastPayError::ErrorWhileProcessingTransactionOrder { err: e.to_string() }
            })?;
        }
        Ok(())
    }

    async fn sync_client_state(&mut self) -> Result<(), anyhow::Error> {
        if !self.store.pending_orders.is_empty() {
            // Finish executing the previous orders
            self.try_complete_pending_orders().await?;
        }
        // update object_ids.
        self.store.object_sequence_numbers.clear()?;
        self.store.object_refs.clear()?;

        let (active_object_certs, _deleted_refs_certs) = self
            .authorities
            .sync_all_owned_objects(self.address, Duration::from_secs(60))
            .await?;

        for (object, option_cert) in active_object_certs {
            let object_ref = object.to_object_reference();
            let (object_id, sequence_number, _) = object_ref;
            self.store
                .object_sequence_numbers
                .insert(&object_id, &sequence_number)?;
            self.store.object_refs.insert(&object_id, &object_ref)?;
            if let Some(cert) = option_cert {
                self.store
                    .certificates
                    .insert(&cert.order.digest(), &cert)?;
            }
        }

        Ok(())
    }

    async fn move_call(
        &mut self,
        package_object_ref: ObjectRef,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        gas_object_ref: ObjectRef,
        object_arguments: Vec<ObjectRef>,
        pure_arguments: Vec<Vec<u8>>,
        gas_budget: u64,
    ) -> Result<(CertifiedOrder, OrderEffects), anyhow::Error> {
        let move_call_order = Order::new_move_call(
            self.address,
            package_object_ref,
            module,
            function,
            type_arguments,
            gas_object_ref,
            object_arguments,
            pure_arguments,
            gas_budget,
            &*self.secret,
        );
        self.execute_transaction(move_call_order).await
    }

    async fn publish(
        &mut self,
        package_source_files_path: String,
        gas_object_ref: ObjectRef,
    ) -> Result<(CertifiedOrder, OrderEffects), anyhow::Error> {
        // Try to compile the package at the given path
        let compiled_modules = build_move_package_to_bytes(Path::new(&package_source_files_path))?;
        let move_publish_order = Order::new_module(
            self.address,
            gas_object_ref,
            compiled_modules,
            &*self.secret,
        );
        self.execute_transaction(move_publish_order).await
    }

    async fn get_object_info(
        &mut self,
        object_info_req: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, anyhow::Error> {
        self.authorities
            .get_object_info_execute(object_info_req)
            .await
    }

    async fn get_owned_objects(&self) -> Vec<ObjectID> {
        self.store.object_sequence_numbers.keys().collect()
    }

    async fn download_owned_objects_not_in_db(&self) -> Result<BTreeSet<ObjectRef>, FastPayError> {
        let object_refs: Vec<ObjectRef> = self.store.object_refs.iter().map(|q| q.1).collect();
        self.download_objects_not_in_db(object_refs).await
    }
}
