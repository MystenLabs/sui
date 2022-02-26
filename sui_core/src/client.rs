// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{authority_aggregator::AuthorityAggregator, authority_client::AuthorityAPI};
use async_trait::async_trait;
use futures::future;
use itertools::Itertools;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use sui_framework::build_move_package_to_bytes;
use sui_types::crypto::Signature;
use sui_types::{
    base_types::*,
    coin,
    committee::Committee,
    error::SuiError,
    fp_ensure,
    messages::*,
    object::{ObjectRead, Owner},
    SUI_FRAMEWORK_ADDRESS,
};
use typed_store::rocks::open_cf;
use typed_store::Map;

use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    pin::Pin,
};

use self::client_responses::{MergeCoinResponse, SplitCoinResponse};

/// a Trait object for `signature::Signer` that is:
/// - Pin, i.e. confined to one place in memory (we don't want to copy private keys).
/// - Sync, i.e. can be safely shared between threads.
///
/// Typically instantiated with Box::pin(keypair) where keypair is a `KeyPair`
///
pub type StableSyncSigner = Pin<Box<dyn signature::Signer<Signature> + Send + Sync>>;

pub mod client_responses;
pub mod client_store;

pub type AsyncResult<'a, T, E> = future::BoxFuture<'a, Result<T, E>>;

pub struct ClientAddressManager<A> {
    committee: Committee,
    authority_clients: BTreeMap<AuthorityName, A>,
    authorities: AuthorityAggregator<A>,
    store: client_store::ClientAddressManagerStore,
    address_states: BTreeMap<SuiAddress, ClientState<A>>,
}
impl<A> ClientAddressManager<A>
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
            store: client_store::ClientAddressManagerStore::open(path),
            authority_clients: authority_clients.clone(),
            authorities: AuthorityAggregator::new(committee.clone(), authority_clients),
            address_states: BTreeMap::new(),
            committee,
        }
    }

    /// Get (if exists) or create a new managed address state
    pub fn get_or_create_state_mut(
        &mut self,
        address: SuiAddress,
        secret: StableSyncSigner,
    ) -> Result<&mut ClientState<A>, SuiError> {
        #[allow(clippy::map_entry)]
        // the fallible store creation complicates the use of the entry API
        if !self.address_states.contains_key(&address) {
            // Load the records if available
            let single_store = match self.store.get_managed_address(address)? {
                Some(store) => store,
                None => self.store.manage_new_address(address)?,
            };
            self.address_states.insert(
                address,
                ClientState::new_for_manager(
                    address,
                    secret,
                    self.committee.clone(),
                    self.authority_clients.clone(),
                    single_store,
                ),
            );
        }
        // unwrap-safe as we just populated the entry
        Ok(self.address_states.get_mut(&address).unwrap())
    }

    /// Get all the states
    pub fn get_managed_address_states(&self) -> &BTreeMap<SuiAddress, ClientState<A>> {
        &self.address_states
    }

    /// Get the object info
    pub async fn get_object_info(&self, object_id: ObjectID) -> Result<ObjectRead, anyhow::Error> {
        self.authorities.get_object_info_execute(object_id).await
    }

    /// Get the current owner of the given object
    pub async fn get_object_owner(&self, object_id: ObjectID) -> Result<Owner, anyhow::Error> {
        let obj_read = self.authorities.get_object_info_execute(object_id).await?;
        Ok(obj_read.object()?.owner)
    }
}

pub struct ClientState<A> {
    /// Our Sui address.
    address: SuiAddress,
    /// Our signature key.
    secret: StableSyncSigner,
    /// Authority entry point.
    authorities: AuthorityAggregator<A>,
    /// Persistent store for client
    store: client_store::ClientSingleAddressStore,
}

// Operations are considered successful when they successfully reach a quorum of authorities.
#[async_trait]
pub trait Client {
    /// Send object to a Sui account.
    async fn transfer_object(
        &mut self,
        object_id: ObjectID,
        gas_payment: ObjectID,
        recipient: SuiAddress,
    ) -> Result<(CertifiedTransaction, TransactionEffects), anyhow::Error>;

    /// Try to complete all pending transactions once. Return if any fails
    async fn try_complete_pending_transactions(&mut self) -> Result<(), SuiError>;

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
    ) -> Result<(CertifiedTransaction, TransactionEffects), anyhow::Error>;

    /// Publish Move modules
    async fn publish(
        &mut self,
        package_source_files_path: String,
        gas_object_ref: ObjectRef,
        gas_budget: u64,
    ) -> Result<(CertifiedTransaction, TransactionEffects), anyhow::Error>;

    /// Split the coin object (identified by `coin_object_ref`) into
    /// multiple new coins. The amount of each new coin is specified in
    /// `split_amounts`. Remaining balance is kept in the original
    /// coin object.
    /// Note that the order of the new coins in SplitCoinResponse will
    /// not be the same as the order of `split_amounts`.
    async fn split_coin(
        &mut self,
        coin_object_ref: ObjectRef,
        split_amounts: Vec<u64>,
        gas_payment: ObjectRef,
        gas_budget: u64,
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
        primary_coin: ObjectRef,
        coin_to_merge: ObjectRef,
        gas_payment: ObjectRef,
        gas_budget: u64,
    ) -> Result<MergeCoinResponse, anyhow::Error>;

    /// Get the object information
    /// TODO: move this out to AddressManager
    async fn get_object_info(&self, object_id: ObjectID) -> Result<ObjectRead, anyhow::Error>;

    /// Get all IDs of objects we own from local cache.
    fn get_owned_objects(&self) -> Vec<ObjectID>;

    /// Fetch objects from authorities
    async fn download_owned_objects_not_in_db(&self) -> Result<BTreeSet<ObjectRef>, SuiError>;
}

impl<A> ClientState<A> {
    /// It is recommended that one call sync and download_owned_objects
    /// right after constructor to fetch missing info form authorities
    /// TODO: client should manage multiple addresses instead of each addr having DBs
    /// https://github.com/MystenLabs/fastnft/issues/332
    #[cfg(test)]
    pub fn new(
        path: PathBuf,
        address: SuiAddress,
        secret: StableSyncSigner,
        committee: Committee,
        authority_clients: BTreeMap<AuthorityName, A>,
    ) -> Self {
        ClientState {
            address,
            secret,
            authorities: AuthorityAggregator::new(committee, authority_clients),
            store: client_store::ClientSingleAddressStore::new(path),
        }
    }

    pub fn new_for_manager(
        address: SuiAddress,
        secret: StableSyncSigner,
        committee: Committee,
        authority_clients: BTreeMap<AuthorityName, A>,
        store: client_store::ClientSingleAddressStore,
    ) -> Self {
        ClientState {
            address,
            secret,
            authorities: AuthorityAggregator::new(committee, authority_clients),
            store,
        }
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

    pub fn object_refs(&self) -> impl Iterator<Item = (ObjectID, ObjectRef)> + '_ {
        self.store.object_refs.iter()
    }
    pub fn certificates(
        &self,
        object_id: &ObjectID,
    ) -> Result<Vec<CertifiedTransaction>, SuiError> {
        let tx_digests =
            self.store
                .object_certs
                .get(object_id)?
                .ok_or(SuiError::ObjectNotFound {
                    object_id: *object_id,
                })?;

        // we need to check we get one certificate per digest, or we lost one
        self.store
            .certificates
            .multi_get(&tx_digests)?
            .into_iter()
            .zip(tx_digests)
            .map(|(opt_cert, tx_digest)| {
                opt_cert.ok_or(SuiError::CertificateNotfound {
                    certificate_digest: tx_digest,
                })
            })
            .collect::<Result<Vec<_>, _>>()
    }

    pub fn insert_object_info(
        &mut self,
        object_ref: &ObjectRef,
        parent_tx_digest: &TransactionDigest,
    ) -> Result<(), SuiError> {
        let (object_id, _, _) = object_ref;
        let mut tx_digests = self.store.object_certs.get(object_id)?.unwrap_or_default();
        tx_digests.push(*parent_tx_digest);

        // Multi table atomic insert using batches
        let batch = self
            .store
            .object_refs
            .batch()
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

    pub fn remove_object_info(&mut self, object_id: &ObjectID) -> Result<(), SuiError> {
        // Multi table atomic delete using batches
        let batch = self
            .store
            .object_refs
            .batch()
            .delete_batch(&self.store.object_certs, std::iter::once(object_id))?
            .delete_batch(&self.store.object_refs, std::iter::once(object_id))?;
        // Execute atomic write of opers
        batch.write()?;
        Ok(())
    }

    #[cfg(test)]
    pub fn store(&self) -> &client_store::ClientSingleAddressStore {
        &self.store
    }

    #[cfg(test)]
    pub fn secret(&self) -> &dyn signature::Signer<Signature> {
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
    /// https://github.com/MystenLabs/fastnft/issues/346
    async fn execute_transaction(
        &mut self,
        transaction: Transaction,
    ) -> Result<(CertifiedTransaction, TransactionEffects), anyhow::Error> {
        for object_kind in &transaction.input_objects() {
            let object_id = object_kind.object_id();
            let next_sequence_number = self.highest_known_version(&object_id).unwrap_or_default();
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
        self.lock_pending_transaction_objects(&transaction)?;

        // We can escape this function without unlocking. This could be dangerous
        let result = self.execute_transaction_inner(&transaction).await;

        // How do we handle errors on authority which lock objects?
        // Currently VM crash can keep objects locked, but we would like to avoid this.
        // TODO: https://github.com/MystenLabs/fastnft/issues/349
        // https://github.com/MystenLabs/fastnft/issues/211
        // https://github.com/MystenLabs/fastnft/issues/346

        self.unlock_pending_transaction_objects(&transaction)?;
        result
    }

    /// This function verifies that the objects in the specfied transaction are locked by the given transaction
    /// We use this to ensure that a transaction can indeed unlock or lock certain objects in the transaction
    /// This means either exactly all the objects are owned by this transaction, or by no transaction
    /// The caller has to explicitly find which objects are locked
    /// TODO: always return true for immutable objects https://github.com/MystenLabs/fastnft/issues/305
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
    /// One should call can_lock_or_unlock before locking as this overwites the previous lock
    /// If the object is already locked, ensure it is unlocked by calling unlock_pending_transaction_objects
    /// Client runs sequentially right now so access to this is safe
    /// Double-locking can cause equivocation. TODO: https://github.com/MystenLabs/fastnft/issues/335
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
    fn unlock_pending_transaction_objects(
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

    async fn update_objects_from_transaction_info(
        &mut self,
        cert: CertifiedTransaction,
        effects: TransactionEffects,
    ) -> Result<(CertifiedTransaction, TransactionEffects), SuiError> {
        // The cert should be included in the response
        let parent_tx_digest = cert.transaction.digest();
        self.store.certificates.insert(&parent_tx_digest, &cert)?;

        let mut objs_to_download = Vec::new();

        for &(object_ref, owner) in effects.mutated_and_created() {
            let (object_id, seq, _) = object_ref;
            let old_seq = self.highest_known_version(&object_id).unwrap_or_default();
            // only update if data is new
            if old_seq < seq {
                if owner == self.address {
                    self.insert_object_info(&object_ref, &parent_tx_digest)?;
                    objs_to_download.push(object_ref);
                } else {
                    self.remove_object_info(&object_id)?;
                }
            } else if old_seq == seq && owner == Owner::SingleOwner(self.address) {
                // ObjectRef can be 1 version behind because it's only updated after confirmation.
                self.store.object_refs.insert(&object_id, &object_ref)?;
            }
        }

        // TODO: decide what to do with failed object downloads
        // https://github.com/MystenLabs/fastnft/issues/331
        let _failed = self.download_objects_not_in_db(objs_to_download).await?;

        for (object_id, seq, _) in &effects.deleted {
            let old_seq = self.highest_known_version(object_id).unwrap_or_default();
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
    ) -> Result<BTreeSet<ObjectRef>, SuiError> {
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
        recipient: SuiAddress,
    ) -> Result<(CertifiedTransaction, TransactionEffects), anyhow::Error> {
        let object_ref = self
            .store
            .object_refs
            .get(&object_id)?
            .ok_or(SuiError::ObjectNotFound { object_id })?;

        let gas_payment =
            self.store
                .object_refs
                .get(&gas_payment)?
                .ok_or(SuiError::ObjectNotFound {
                    object_id: gas_payment,
                })?;

        let transaction = Transaction::new_transfer(
            recipient,
            object_ref,
            self.address,
            gas_payment,
            &*self.secret,
        );
        let (certificate, effects) = self.execute_transaction(transaction).await?;

        Ok((certificate, effects))
    }

    async fn try_complete_pending_transactions(&mut self) -> Result<(), SuiError> {
        // Transactions are idempotent so no need to prevent multiple executions
        let unique_pending_transactions: HashSet<_> = self
            .store
            .pending_transactions
            .iter()
            .map(|(_, ord)| ord)
            .collect();
        // Need some kind of timeout or max_trials here?
        // TODO: https://github.com/MystenLabs/fastnft/issues/330
        for transaction in unique_pending_transactions {
            self.execute_transaction(transaction.clone())
                .await
                .map_err(|e| SuiError::ErrorWhileProcessingTransactionTransaction {
                    err: e.to_string(),
                })?;
        }
        Ok(())
    }

    async fn sync_client_state(&mut self) -> Result<(), anyhow::Error> {
        if !self.store.pending_transactions.is_empty() {
            // Finish executing the previous transactions
            self.try_complete_pending_transactions().await?;
        }
        // update object_ids.
        self.store.object_refs.clear()?;

        let (active_object_certs, _deleted_refs_certs) = self
            .authorities
            .sync_all_owned_objects(self.address, Duration::from_secs(60))
            .await?;

        for (object, option_layout, option_cert) in active_object_certs {
            let object_ref = object.to_object_reference();
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
    ) -> Result<(CertifiedTransaction, TransactionEffects), anyhow::Error> {
        let move_call_transaction = Transaction::new_move_call(
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
        self.execute_transaction(move_call_transaction).await
    }

    async fn publish(
        &mut self,
        package_source_files_path: String,
        gas_object_ref: ObjectRef,
        gas_budget: u64,
    ) -> Result<(CertifiedTransaction, TransactionEffects), anyhow::Error> {
        // Try to compile the package at the given path
        let compiled_modules = build_move_package_to_bytes(Path::new(&package_source_files_path))?;
        let move_publish_transaction = Transaction::new_module(
            self.address,
            gas_object_ref,
            compiled_modules,
            gas_budget,
            &*self.secret,
        );
        self.execute_transaction(move_publish_transaction).await
    }

    async fn split_coin(
        &mut self,
        coin_object_ref: ObjectRef,
        split_amounts: Vec<u64>,
        gas_payment: ObjectRef,
        gas_budget: u64,
    ) -> Result<SplitCoinResponse, anyhow::Error> {
        let coin_type = self
            .get_object_info(coin_object_ref.0)
            .await?
            .object()?
            .get_move_template_type()?;

        let move_call_transaction = Transaction::new_move_call(
            self.address,
            self.get_framework_object_ref().await?,
            coin::COIN_MODULE_NAME.to_owned(),
            coin::COIN_SPLIT_VEC_FUNC_NAME.to_owned(),
            vec![coin_type],
            gas_payment,
            vec![coin_object_ref],
            vec![bcs::to_bytes(&split_amounts)?],
            gas_budget,
            &*self.secret,
        );
        let (certificate, effects) = self.execute_transaction(move_call_transaction).await?;
        if let ExecutionStatus::Failure { gas_used: _, error } = effects.status {
            return Err(error.into());
        }
        let created = &effects.created;
        fp_ensure!(
            effects.mutated.len() == 2     // coin and gas
               && created.len() == split_amounts.len()
               && created.iter().all(|(_, owner)| owner == &Owner::SingleOwner(self.address)),
            SuiError::IncorrectGasSplit.into()
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
        primary_coin: ObjectRef,
        coin_to_merge: ObjectRef,
        gas_payment: ObjectRef,
        gas_budget: u64,
    ) -> Result<MergeCoinResponse, anyhow::Error> {
        let coin_type = self
            .get_object_info(primary_coin.0)
            .await?
            .object()?
            .get_move_template_type()?;

        let move_call_transaction = Transaction::new_move_call(
            self.address,
            self.get_framework_object_ref().await?,
            coin::COIN_MODULE_NAME.to_owned(),
            coin::COIN_JOIN_FUNC_NAME.to_owned(),
            vec![coin_type],
            gas_payment,
            vec![primary_coin, coin_to_merge],
            vec![],
            gas_budget,
            &*self.secret,
        );
        let (certificate, effects) = self.execute_transaction(move_call_transaction).await?;
        if let ExecutionStatus::Failure { gas_used: _, error } = effects.status {
            return Err(error.into());
        }
        fp_ensure!(
            effects.mutated.len() == 2, // coin and gas
            SuiError::IncorrectGasMerge.into()
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

    fn get_owned_objects(&self) -> Vec<ObjectID> {
        self.store.object_refs.keys().collect()
    }

    async fn download_owned_objects_not_in_db(&self) -> Result<BTreeSet<ObjectRef>, SuiError> {
        let object_refs: Vec<ObjectRef> = self.store.object_refs.iter().map(|q| q.1).collect();
        self.download_objects_not_in_db(object_refs).await
    }
}
