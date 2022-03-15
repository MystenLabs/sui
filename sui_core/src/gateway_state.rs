// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{authority_aggregator::AuthorityAggregator, authority_client::AuthorityAPI};
use async_trait::async_trait;
use futures::future;
use itertools::Itertools;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use move_core_types::value::MoveStructLayout;
use sui_types::crypto::Signature;
use sui_types::error::SuiResult;
use sui_types::{
    base_types::*,
    coin,
    committee::Committee,
    error::SuiError,
    fp_ensure,
    messages::*,
    object::{Object, ObjectRead, Owner},
    SUI_FRAMEWORK_ADDRESS,
};
use typed_store::rocks::open_cf;
use typed_store::Map;

use std::collections::btree_map::Entry;
use std::path::PathBuf;
use std::time::Duration;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    pin::Pin,
};

use self::gateway_responses::*;

/// A trait for supplying transaction signature asynchronously.
///
/// The transaction data can be validated inside [`Self::sign`] function before signing,
/// return `signature::Error` if the transaction data is incorrect or unexpected.
///
/// # Example
/// ```
/// use signature::Error;
/// use sui_core::gateway_state::AsyncTransactionSigner;
/// use sui_types::base_types::SuiAddress;
/// use sui_types::crypto::Signature;
/// use sui_types::messages::TransactionData;
/// use async_trait::async_trait;
///
/// struct ExampleTransactionSigner{
///     signer: dyn signature::Signer<Signature> + Sync + Send
/// }
/// #[async_trait]
/// impl AsyncTransactionSigner for ExampleTransactionSigner {
///     async fn sign(&self, address: &SuiAddress, data: TransactionData) -> Result<Signature, Error> {
///         // 1. Validate the transaction data
///
///         // 2. Create a signature if the transaction data is valid
///         let signature = Signature::new(&data, &self.signer);
///         // 3. return the signature
///         Ok(signature)
///     }
/// }
/// ```
/// This trait is typically use with [`StableSyncTransactionSigner`] to supply signature to the [`GatewayAPI`] trait
#[async_trait]
pub trait AsyncTransactionSigner {
    async fn sign(
        &self,
        address: &SuiAddress,
        data: TransactionData,
    ) -> Result<Signature, signature::Error>;
}

/// a Trait object for [`AsyncTransactionSigner`] that is:
/// - Pin, i.e. confined to one place in memory.
/// - Sync, i.e. can be safely shared between threads.
///
/// Typically instantiated with Box::pin(tx_signer) where tx_signer is a [`AsyncTransactionSigner`]
pub type StableSyncTransactionSigner = Pin<Box<dyn AsyncTransactionSigner + Send + Sync>>;

pub mod gateway_responses;
pub mod gateway_store;

pub type AsyncResult<'a, T, E> = future::BoxFuture<'a, Result<T, E>>;

pub type GatewayClient = Box<dyn GatewayAPI + Sync + Send>;

pub struct GatewayState<A> {
    authorities: AuthorityAggregator<A>,
    store: gateway_store::GatewayStore,
    address_states: BTreeMap<SuiAddress, AccountState>,
}

impl<A> GatewayState<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    /// Create a new manager which stores its managed addresses at `path`
    pub fn new(
        path: PathBuf,
        committee: Committee,
        authority_clients: BTreeMap<AuthorityName, A>,
    ) -> Self {
        Self {
            store: gateway_store::GatewayStore::open(path),
            authorities: AuthorityAggregator::new(committee, authority_clients),
            address_states: BTreeMap::new(),
        }
    }

    fn get_or_create_account(&mut self, address: SuiAddress) -> SuiResult<&AccountState> {
        let account_state = match self.address_states.entry(address) {
            Entry::Vacant(e) => {
                let single_store = match self.store.get_managed_address(address)? {
                    Some(store) => store,
                    None => self.store.manage_new_address(address)?,
                };
                let state = AccountState::new_for_manager(address, single_store);
                e.insert(state)
            }
            Entry::Occupied(o) => o.into_mut(),
        };
        Ok(account_state)
    }

    /// Get all the states
    #[cfg(test)]
    pub fn get_managed_address_states(&self) -> &BTreeMap<SuiAddress, AccountState> {
        &self.address_states
    }

    /// Get the object info
    async fn get_object_info(&self, object_id: ObjectID) -> Result<ObjectRead, anyhow::Error> {
        self.authorities.get_object_info_execute(object_id).await
    }

    #[cfg(test)]
    pub fn get_authorities(&self) -> &AuthorityAggregator<A> {
        &self.authorities
    }
}

pub struct AccountState {
    /// Sui address of this account.
    address: SuiAddress,
    /// Persistent store for this account.
    store: gateway_store::AccountStore,
}

// Operations are considered successful when they successfully reach a quorum of authorities.
#[async_trait]
pub trait GatewayAPI {
    /// Send coin object to a Sui address.
    async fn transfer_coin(
        &mut self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas_payment: ObjectID,
        recipient: SuiAddress,
        tx_signer: StableSyncTransactionSigner,
    ) -> Result<(CertifiedTransaction, TransactionEffects), anyhow::Error>;

    /// Synchronise account state with a random authorities, updates all object_ids and certificates
    /// from account_addr, request only goes out to one authority.
    /// this method doesn't guarantee data correctness, caller will have to handle potential byzantine authority
    async fn sync_account_state(&mut self, account_addr: SuiAddress) -> Result<(), anyhow::Error>;

    /// Call move functions in the module in the given package, with args supplied
    async fn move_call(
        &mut self,
        signer: SuiAddress,
        package_object_ref: ObjectRef,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        gas_object_ref: ObjectRef,
        object_arguments: Vec<ObjectRef>,
        shared_object_arguments: Vec<ObjectID>,
        pure_arguments: Vec<Vec<u8>>,
        gas_budget: u64,
        tx_signer: StableSyncTransactionSigner,
    ) -> Result<(CertifiedTransaction, TransactionEffects), anyhow::Error>;

    /// Publish Move modules
    async fn publish(
        &mut self,
        signer: SuiAddress,
        package_bytes: Vec<Vec<u8>>,
        gas_object_ref: ObjectRef,
        gas_budget: u64,
        tx_signer: StableSyncTransactionSigner,
    ) -> Result<PublishResponse, anyhow::Error>;

    /// Split the coin object (identified by `coin_object_ref`) into
    /// multiple new coins. The amount of each new coin is specified in
    /// `split_amounts`. Remaining balance is kept in the original
    /// coin object.
    /// Note that the order of the new coins in SplitCoinResponse will
    /// not be the same as the order of `split_amounts`.
    async fn split_coin(
        &mut self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas_payment: ObjectID,
        gas_budget: u64,
        tx_signer: StableSyncTransactionSigner,
    ) -> Result<SplitCoinResponse, anyhow::Error>;

    /// Merge the `coin_to_merge` coin object into `primary_coin`.
    /// After this merge, the balance of `primary_coin` will become the
    /// sum of the two, while `coin_to_merge` will be deleted.
    ///
    /// Returns a pair:
    ///  (update primary coin object reference, updated gas payment object reference)
    ///
    /// TODO: Support merging a vector of coins.
    async fn merge_coins(
        &mut self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas_payment: ObjectID,
        gas_budget: u64,
        tx_signer: StableSyncTransactionSigner,
    ) -> Result<MergeCoinResponse, anyhow::Error>;

    /// Get the object information
    /// TODO: move this out to AddressManager
    async fn get_object_info(&self, object_id: ObjectID) -> Result<ObjectRead, anyhow::Error>;

    /// Get refs of all objects we own from local cache.
    fn get_owned_objects(&mut self, account_addr: SuiAddress) -> Vec<ObjectRef>;

    /// Fetch objects from authorities
    async fn download_owned_objects_not_in_db(
        &mut self,
        account_addr: SuiAddress,
    ) -> Result<BTreeSet<ObjectRef>, SuiError>;
}

impl AccountState {
    /// It is recommended that one call sync and download_owned_objects
    /// right after constructor to fetch missing info form authorities
    #[cfg(test)]
    pub fn new(path: PathBuf, address: SuiAddress) -> Self {
        AccountState {
            address,
            store: gateway_store::AccountStore::new(path),
        }
    }

    pub fn new_for_manager(address: SuiAddress, store: gateway_store::AccountStore) -> Self {
        AccountState { address, store }
    }

    pub fn address(&self) -> SuiAddress {
        self.address
    }

    pub fn highest_known_version(&self, object_id: &ObjectID) -> Result<SequenceNumber, SuiError> {
        self.latest_object_ref(object_id)
            .map(|(_oid, seq_num, _digest)| seq_num)
    }
    pub fn latest_object_ref(&self, object_id: &ObjectID) -> Result<ObjectRef, SuiError> {
        self.store
            .object_refs
            .get(object_id)?
            .ok_or(SuiError::ObjectNotFound {
                object_id: *object_id,
            })
    }

    pub fn update_object_ref(&self, object_ref: &ObjectRef) -> SuiResult {
        self.store.object_refs.insert(&object_ref.0, object_ref)?;
        Ok(())
    }

    pub fn object_refs(&self) -> impl Iterator<Item = (ObjectID, ObjectRef)> + '_ {
        self.store.object_refs.iter()
    }

    /// Returns all object references that are in `object_refs` but not in the store.
    pub fn object_refs_not_in_store(
        &self,
        object_refs: &[ObjectRef],
    ) -> SuiResult<BTreeSet<ObjectRef>> {
        let result = self
            .store
            .objects
            .multi_get(object_refs)?
            .iter()
            .zip(object_refs)
            .filter_map(|(object, ref_)| match object {
                Some(_) => None,
                None => Some(*ref_),
            })
            .collect::<BTreeSet<_>>();
        Ok(result)
    }

    pub fn clear_object_refs(&self) -> SuiResult {
        self.store.object_refs.clear()?;
        Ok(())
    }

    pub fn insert_object(&self, object: Object) -> SuiResult {
        self.store
            .objects
            .insert(&object.compute_object_reference(), &object)?;
        Ok(())
    }

    pub fn insert_active_object_cert(
        &self,
        object: Object,
        option_layout: Option<MoveStructLayout>,
        option_cert: Option<CertifiedTransaction>,
    ) -> SuiResult {
        let object_ref = object.compute_object_reference();
        let (object_id, _seqnum, _) = object_ref;

        self.store.object_refs.insert(&object_id, &object_ref)?;
        if let Some(cert) = option_cert {
            self.store
                .certificates
                .insert(&cert.transaction.digest(), &cert)?;
        }
        // Save the object layout, if any
        if let Some(layout) = option_layout {
            if let Some(type_) = object.type_() {
                // TODO: sanity check to add: if we're overwriting an old layout, it should be the same as the new one
                self.store.object_layouts.insert(type_, &layout)?;
            }
        }
        Ok(())
    }

    pub fn insert_certificate(
        &self,
        tx_digest: &TransactionDigest,
        cert: &CertifiedTransaction,
    ) -> SuiResult {
        self.store.certificates.insert(tx_digest, cert)?;
        Ok(())
    }

    pub fn insert_object_info(
        &self,
        object_ref: &ObjectRef,
        parent_tx_digest: &TransactionDigest,
    ) -> Result<(), SuiError> {
        let (object_id, _, _) = object_ref;
        // Multi table atomic insert using batches
        let batch = self
            .store
            .object_refs
            .batch()
            .insert_batch(
                &self.store.object_certs,
                std::iter::once((object_ref, parent_tx_digest)),
            )?
            .insert_batch(
                &self.store.object_refs,
                std::iter::once((object_id, object_ref)),
            )?;
        // Execute atomic write of opers
        batch.write()?;
        Ok(())
    }

    pub fn remove_object_info(&self, object_id: &ObjectID) -> Result<(), SuiError> {
        let min_for_id = (*object_id, SequenceNumber::MIN, ObjectDigest::MIN);
        let max_for_id = (*object_id, SequenceNumber::MAX, ObjectDigest::MAX);

        // Multi table atomic delete using batches
        let batch = self
            .store
            .object_refs
            .batch()
            .delete_range(&self.store.object_certs, &min_for_id, &max_for_id)?
            .delete_batch(&self.store.object_refs, std::iter::once(object_id))?;
        // Execute atomic write of opers
        batch.write()?;
        Ok(())
    }

    pub fn get_owned_objects(&self) -> Vec<ObjectID> {
        self.store.object_refs.keys().collect()
    }

    #[cfg(test)]
    pub fn store(&self) -> &gateway_store::AccountStore {
        &self.store
    }

    pub fn get_unique_pending_transactions(&self) -> HashSet<Transaction> {
        self.store
            .pending_transactions
            .iter()
            .map(|(_, ord)| ord)
            .collect()
    }

    /// This function verifies that the objects in the specified transaction are locked by the given transaction
    /// We use this to ensure that a transaction can indeed unlock or lock certain objects in the transaction
    /// This means either exactly all the objects are owned by this transaction, or by no transaction
    /// The caller has to explicitly find which objects are locked
    /// TODO: always return true for immutable objects https://github.com/MystenLabs/sui/issues/305
    fn can_lock_or_unlock(&self, transaction: &Transaction) -> Result<bool, SuiError> {
        let iter_matches = self.store.pending_transactions.multi_get(
            &transaction
                .input_objects()
                .iter()
                .map(|q| q.object_id())
                .collect_vec(),
        )?;
        if iter_matches.into_iter().any(|match_for_transaction| {
            matches!(match_for_transaction,
                // If we find any transaction that isn't the given transaction, we cannot proceed
                Some(o) if o != *transaction)
        }) {
            return Ok(false);
        }
        // All the objects are either owned by this transaction or by no transaction
        Ok(true)
    }

    /// Locks the objects for the given transaction
    /// It is important to check that the object is not locked before locking again
    /// One should call can_lock_or_unlock before locking as this overwrites the previous lock
    /// If the object is already locked, ensure it is unlocked by calling unlock_pending_transaction_objects
    /// Client runs sequentially right now so access to this is safe
    /// Double-locking can cause equivocation. TODO: https://github.com/MystenLabs/sui/issues/335
    pub fn lock_pending_transaction_objects(
        &self,
        transaction: &Transaction,
    ) -> Result<(), SuiError> {
        if !self.can_lock_or_unlock(transaction)? {
            return Err(SuiError::ConcurrentTransactionError);
        }
        self.store
            .pending_transactions
            .multi_insert(
                transaction
                    .input_objects()
                    .iter()
                    .map(|e| (e.object_id(), transaction.clone())),
            )
            .map_err(|e| e.into())
    }

    /// Unlocks the objects for the given transaction
    /// Unlocking an already unlocked object, is a no-op and does not Err
    pub fn unlock_pending_transaction_objects(
        &self,
        transaction: &Transaction,
    ) -> Result<(), SuiError> {
        if !self.can_lock_or_unlock(transaction)? {
            return Err(SuiError::ConcurrentTransactionError);
        }
        self.store
            .pending_transactions
            .multi_remove(transaction.input_objects().iter().map(|e| e.object_id()))
            .map_err(|e| e.into())
    }
}

impl<A> GatewayState<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    #[cfg(test)]
    pub fn authorities(&self) -> &AuthorityAggregator<A> {
        &self.authorities
    }

    pub async fn get_framework_object_ref(&mut self) -> Result<ObjectRef, anyhow::Error> {
        let info = self
            .get_object_info(ObjectID::from(SUI_FRAMEWORK_ADDRESS))
            .await?;
        Ok(info.reference()?)
    }

    async fn execute_transaction_inner(
        &mut self,
        transaction: &Transaction,
    ) -> Result<(CertifiedTransaction, TransactionEffects), anyhow::Error> {
        let (new_certificate, effects) = self.authorities.execute_transaction(transaction).await?;

        // Update local data using new transaction response.
        self.update_objects_from_transaction_info(new_certificate.clone(), effects.clone())
            .await?;

        Ok((new_certificate, effects))
    }

    /// Execute (or retry) a transaction and execute the Confirmation Transaction.
    /// Update local object states using newly created certificate and ObjectInfoResponse from the Confirmation step.
    /// This functions locks all the input objects if possible, and unlocks at the end of confirmation or if an error occurs
    /// TODO: define other situations where we can unlock objects after authority error
    /// https://github.com/MystenLabs/sui/issues/346
    async fn execute_transaction(
        &mut self,
        transaction: Transaction,
    ) -> Result<(CertifiedTransaction, TransactionEffects), anyhow::Error> {
        let account = self.get_or_create_account(transaction.sender_address())?;
        for object_kind in &transaction.input_objects() {
            let object_id = object_kind.object_id();
            let next_sequence_number = account
                .highest_known_version(&object_id)
                .unwrap_or_default();
            fp_ensure!(
                object_kind.version() >= next_sequence_number,
                SuiError::UnexpectedSequenceNumber {
                    object_id,
                    expected_sequence: next_sequence_number,
                }
                .into()
            );
        }
        // Lock the objects in this transaction
        account.lock_pending_transaction_objects(&transaction)?;

        // We can escape this function without unlocking. This could be dangerous
        let result = self.execute_transaction_inner(&transaction).await;

        // How do we handle errors on authority which lock objects?
        // Currently VM crash can keep objects locked, but we would like to avoid this.
        // TODO: https://github.com/MystenLabs/sui/issues/349
        // https://github.com/MystenLabs/sui/issues/211
        // https://github.com/MystenLabs/sui/issues/346

        let account = self.get_or_create_account(transaction.sender_address())?;
        account.unlock_pending_transaction_objects(&transaction)?;
        result
    }

    async fn update_objects_from_transaction_info(
        &mut self,
        cert: CertifiedTransaction,
        effects: TransactionEffects,
    ) -> Result<(CertifiedTransaction, TransactionEffects), SuiError> {
        let address = cert.transaction.sender_address();
        let account = self.get_or_create_account(address)?;
        // The cert should be included in the response
        let parent_tx_digest = cert.transaction.digest();
        // TODO: certificates should ideally be inserted to the shared store.
        account.insert_certificate(&parent_tx_digest, &cert)?;

        let mut objs_to_download = Vec::new();

        for &(object_ref, owner) in effects.mutated_and_created() {
            let (object_id, seq, _) = object_ref;
            let old_seq = account
                .highest_known_version(&object_id)
                .unwrap_or_default();
            // only update if data is new
            if old_seq < seq {
                if owner == address {
                    account.insert_object_info(&object_ref, &parent_tx_digest)?;
                    objs_to_download.push(object_ref);
                } else {
                    account.remove_object_info(&object_id)?;
                    // TODO: Could potentially add this object_ref to the relevant account store
                }
            } else if old_seq == seq && owner == Owner::AddressOwner(address) {
                // TODO: Store objects owned by objects as well.
                // ObjectRef can be 1 version behind because it's only updated after confirmation.
                account.update_object_ref(&object_ref)?;
            }
        }

        for (object_id, seq, _) in &effects.deleted {
            let old_seq = account.highest_known_version(object_id).unwrap_or_default();
            if old_seq < *seq {
                account.remove_object_info(object_id)?;
            }
        }

        // TODO: decide what to do with failed object downloads
        // https://github.com/MystenLabs/sui/issues/331
        let _failed = self
            .download_objects_not_in_db(address, objs_to_download)
            .await?;
        Ok((cert, effects))
    }

    /// Fetch the objects for the given list of ObjectRefs, which do not already exist in the db.
    /// How it works: this function finds all object refs that are not in the DB
    /// then it downloads them by calling download_objects_from_all_authorities.
    /// Afterwards it persists objects returned.
    /// Returns a set of the object ids which failed to download
    /// TODO: return failed download errors along with the object id
    async fn download_objects_not_in_db(
        &mut self,
        account_addr: SuiAddress,
        object_refs: Vec<ObjectRef>,
    ) -> Result<BTreeSet<ObjectRef>, SuiError> {
        let account = self.get_or_create_account(account_addr)?;
        // Check the DB
        // This could be expensive. Might want to use object_ref table
        // We want items that are NOT in the table
        let fresh_object_refs = account.object_refs_not_in_store(&object_refs)?;

        // Now that we have all the fresh ids, fetch from authorities.
        let mut receiver = self
            .authorities
            .fetch_objects_from_authorities(fresh_object_refs.clone());

        let mut err_object_refs = fresh_object_refs;
        let account = self.get_or_create_account(account_addr)?;
        // Receive from the downloader
        while let Some(resp) = receiver.recv().await {
            // Persists them to disk
            if let Ok(o) = resp {
                err_object_refs.remove(&o.compute_object_reference());
                account.insert_object(o)?;
            }
        }
        Ok(err_object_refs)
    }

    /// Try to complete all pending transactions once in account_addr.
    /// Return if any fails
    async fn try_complete_pending_transactions(
        &mut self,
        account_addr: SuiAddress,
    ) -> Result<(), SuiError> {
        let account = self.get_or_create_account(account_addr)?;
        let unique_pending_transactions = account.get_unique_pending_transactions();
        // Transactions are idempotent so no need to prevent multiple executions
        // Need some kind of timeout or max_trials here?
        // TODO: https://github.com/MystenLabs/sui/issues/330
        for transaction in unique_pending_transactions {
            self.execute_transaction(transaction.clone())
                .await
                .map_err(|e| SuiError::ErrorWhileProcessingTransactionTransaction {
                    err: e.to_string(),
                })?;
        }
        Ok(())
    }
}

#[async_trait]
impl<A> GatewayAPI for GatewayState<A>
where
    A: AuthorityAPI + Send + Sync + Clone + 'static,
{
    async fn transfer_coin(
        &mut self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas_payment: ObjectID,
        recipient: SuiAddress,
        tx_signer: StableSyncTransactionSigner,
    ) -> Result<(CertifiedTransaction, TransactionEffects), anyhow::Error> {
        let account = self.get_or_create_account(signer)?;
        let object_ref = account.latest_object_ref(&object_id)?;
        self.get_object_info(object_id)
            .await?
            .object()?
            .is_transfer_eligible()?;

        let account = self.get_or_create_account(signer)?;
        let gas_payment = account.latest_object_ref(&gas_payment)?;
        let data = TransactionData::new_transfer(recipient, object_ref, signer, gas_payment);
        let signature = tx_signer.sign(&signer, data.clone()).await?;
        let (certificate, effects) = self
            .execute_transaction(Transaction::new(data, signature))
            .await?;

        Ok((certificate, effects))
    }

    async fn sync_account_state(&mut self, account_addr: SuiAddress) -> Result<(), anyhow::Error> {
        self.try_complete_pending_transactions(account_addr).await?;

        let (active_object_certs, _deleted_refs_certs) = self
            .authorities
            .sync_all_owned_objects(account_addr, Duration::from_secs(60))
            .await?;

        let account = self.get_or_create_account(account_addr)?;
        account.clear_object_refs()?;
        for (object, option_layout, option_cert) in active_object_certs {
            account.insert_active_object_cert(object, option_layout, option_cert)?;
        }

        Ok(())
    }

    async fn move_call(
        &mut self,
        signer: SuiAddress,
        package_object_ref: ObjectRef,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        gas_object_ref: ObjectRef,
        object_arguments: Vec<ObjectRef>,
        shared_object_arguments: Vec<ObjectID>,
        pure_arguments: Vec<Vec<u8>>,
        gas_budget: u64,
        tx_signer: StableSyncTransactionSigner,
    ) -> Result<(CertifiedTransaction, TransactionEffects), anyhow::Error> {
        let data = TransactionData::new_move_call(
            signer,
            package_object_ref,
            module,
            function,
            type_arguments,
            gas_object_ref,
            object_arguments,
            shared_object_arguments,
            pure_arguments,
            gas_budget,
        );
        let signature = tx_signer.sign(&signer, data.clone()).await?;
        self.execute_transaction(Transaction::new(data, signature))
            .await
    }

    async fn publish(
        &mut self,
        signer: SuiAddress,
        package_bytes: Vec<Vec<u8>>,
        gas_object_ref: ObjectRef,
        gas_budget: u64,
        tx_signer: StableSyncTransactionSigner,
    ) -> Result<PublishResponse, anyhow::Error> {
        let data = TransactionData::new_module(signer, gas_object_ref, package_bytes, gas_budget);
        let signature = tx_signer.sign(&signer, data.clone()).await?;

        let (certificate, effects) = self
            .execute_transaction(Transaction::new(data, signature))
            .await?;
        if let ExecutionStatus::Failure { gas_used: _, error } = effects.status {
            return Err(error.into());
        }
        fp_ensure!(
            effects.mutated.len() == 1,
            SuiError::InconsistentGatewayResult {
                error: format!(
                    "Expecting only one object mutated (the gas), seeing {} mutated",
                    effects.mutated.len()
                ),
            }
            .into()
        );
        let updated_gas = self
            .get_object_info(gas_object_ref.0)
            .await?
            .into_object()?;
        let mut package = None;
        let mut created_objects = vec![];
        for (obj_ref, _) in effects.created {
            let object = self.get_object_info(obj_ref.0).await?.into_object()?;
            if object.is_package() {
                fp_ensure!(
                    package.is_none(),
                    SuiError::InconsistentGatewayResult {
                        error: "More than one package created".to_owned(),
                    }
                    .into()
                );
                package = Some(obj_ref);
            } else {
                created_objects.push(object);
            }
        }
        let package = package.ok_or(SuiError::InconsistentGatewayResult {
            error: "No package created".to_owned(),
        })?;
        Ok(PublishResponse {
            certificate,
            package,
            created_objects,
            updated_gas,
        })
    }

    async fn split_coin(
        &mut self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas_payment: ObjectID,
        gas_budget: u64,
        tx_signer: StableSyncTransactionSigner,
    ) -> Result<SplitCoinResponse, anyhow::Error> {
        let account = self.get_or_create_account(signer)?;
        let coin_object_ref = account.latest_object_ref(&coin_object_id)?;
        let gas_payment = account.latest_object_ref(&gas_payment)?;
        let coin_type = self
            .get_object_info(coin_object_ref.0)
            .await?
            .object()?
            .get_move_template_type()?;

        let data = TransactionData::new_move_call(
            signer,
            self.get_framework_object_ref().await?,
            coin::COIN_MODULE_NAME.to_owned(),
            coin::COIN_SPLIT_VEC_FUNC_NAME.to_owned(),
            vec![coin_type],
            gas_payment,
            vec![coin_object_ref],
            vec![],
            vec![bcs::to_bytes(&split_amounts)?],
            gas_budget,
        );

        let signature = tx_signer.sign(&signer, data.clone()).await?;

        let (certificate, effects) = self
            .execute_transaction(Transaction::new(data, signature))
            .await?;
        if let ExecutionStatus::Failure { gas_used: _, error } = effects.status {
            return Err(error.into());
        }
        let created = &effects.created;
        fp_ensure!(
            effects.mutated.len() == 2     // coin and gas
               && created.len() == split_amounts.len()
               && created.iter().all(|(_, owner)| owner == &signer),
            SuiError::InconsistentGatewayResult {
                error: "Unexpected split outcome".to_owned()
            }
            .into()
        );
        let updated_coin = self
            .get_object_info(coin_object_ref.0)
            .await?
            .into_object()?;
        let mut new_coins = Vec::with_capacity(created.len());
        for ((id, _, _), _) in created {
            new_coins.push(self.get_object_info(*id).await?.into_object()?);
        }
        let updated_gas = self.get_object_info(gas_payment.0).await?.into_object()?;
        Ok(SplitCoinResponse {
            certificate,
            updated_coin,
            new_coins,
            updated_gas,
        })
    }

    async fn merge_coins(
        &mut self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas_payment: ObjectID,
        gas_budget: u64,
        tx_signer: StableSyncTransactionSigner,
    ) -> Result<MergeCoinResponse, anyhow::Error> {
        let account = self.get_or_create_account(signer)?;
        let primary_coin = account.latest_object_ref(&primary_coin)?;
        let coin_to_merge = account.latest_object_ref(&coin_to_merge)?;
        let gas_payment = account.latest_object_ref(&gas_payment)?;

        let coin_type = self
            .get_object_info(primary_coin.0)
            .await?
            .object()?
            .get_move_template_type()?;

        let data = TransactionData::new_move_call(
            signer,
            self.get_framework_object_ref().await?,
            coin::COIN_MODULE_NAME.to_owned(),
            coin::COIN_JOIN_FUNC_NAME.to_owned(),
            vec![coin_type],
            gas_payment,
            vec![primary_coin, coin_to_merge],
            vec![],
            vec![],
            gas_budget,
        );
        let signature = tx_signer.sign(&signer, data.clone()).await?;
        let (certificate, effects) = self
            .execute_transaction(Transaction::new(data, signature))
            .await?;
        if let ExecutionStatus::Failure { gas_used: _, error } = effects.status {
            return Err(error.into());
        }
        fp_ensure!(
            effects.mutated.len() == 2, // coin and gas
            SuiError::InconsistentGatewayResult {
                error: "Unexpected split outcome".to_owned()
            }
            .into()
        );
        let updated_coin = self.get_object_info(primary_coin.0).await?.into_object()?;
        let updated_gas = self.get_object_info(gas_payment.0).await?.into_object()?;
        Ok(MergeCoinResponse {
            certificate,
            updated_coin,
            updated_gas,
        })
    }

    async fn get_object_info(&self, object_id: ObjectID) -> Result<ObjectRead, anyhow::Error> {
        self.authorities.get_object_info_execute(object_id).await
    }

    fn get_owned_objects(&mut self, account_addr: SuiAddress) -> Vec<ObjectRef> {
        // Returns empty vec![] if the account cannot be found.
        self.get_or_create_account(account_addr)
            .map(|acc| acc.object_refs().map(|(_, r)| r).collect())
            .unwrap_or_default()
    }

    async fn download_owned_objects_not_in_db(
        &mut self,
        account_addr: SuiAddress,
    ) -> Result<BTreeSet<ObjectRef>, SuiError> {
        let object_refs: Vec<ObjectRef> = self.get_owned_objects(account_addr);
        self.download_objects_not_in_db(account_addr, object_refs)
            .await
    }
}
