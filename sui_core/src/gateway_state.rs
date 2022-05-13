// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use futures::future;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;
use sui_adapter::adapter::resolve_and_type_check;
use tracing::{debug, error, Instrument};

use sui_types::{
    base_types::*,
    coin,
    committee::Committee,
    error::{SuiError, SuiResult},
    fp_ensure,
    messages::*,
    object::{Object, ObjectRead},
    SUI_FRAMEWORK_ADDRESS,
};

use crate::sui_json::{resolve_move_function_args, SuiJsonCallArg, SuiJsonValue};
use crate::transaction_input_checker;
use crate::{
    authority::GatewayStore, authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
};

use self::gateway_responses::*;

#[cfg(test)]
#[path = "unit_tests/gateway_state_tests.rs"]
mod gateway_state_tests;

pub mod gateway_responses;

pub type AsyncResult<'a, T, E> = future::BoxFuture<'a, Result<T, E>>;

pub type GatewayClient = Box<dyn GatewayAPI + Sync + Send>;

pub type GatewayTxSeqNumber = u64;

const MAX_TX_RANGE_SIZE: u64 = 4096;
/// Number of times to retry failed TX
const MAX_NUM_TX_RETRIES: usize = 5;

pub struct GatewayState<A> {
    authorities: AuthorityAggregator<A>,
    store: Arc<GatewayStore>,
    /// Every transaction committed in authorities (and hence also committed in the Gateway)
    /// will have a unique sequence number. This number is specific to this gateway,
    /// and hence will not be compatible with authorities or other gateways.
    /// It's useful if we need some kind of ordering for transactions
    /// from a gateway.
    next_tx_seq_number: AtomicU64,
}

impl<A> GatewayState<A> {
    /// Create a new manager which stores its managed addresses at `path`
    pub fn new(
        path: PathBuf,
        committee: Committee,
        authority_clients: BTreeMap<AuthorityName, A>,
    ) -> SuiResult<Self> {
        Self::new_with_authorities(path, AuthorityAggregator::new(committee, authority_clients))
    }

    pub fn new_with_authorities(
        path: PathBuf,
        authorities: AuthorityAggregator<A>,
    ) -> SuiResult<Self> {
        let store = Arc::new(GatewayStore::open(path, None));
        let next_tx_seq_number = AtomicU64::new(store.next_sequence_number()?);
        Ok(Self {
            store,
            authorities,
            next_tx_seq_number,
        })
    }

    // Given a list of inputs from a transaction, fetch the objects
    // from the db.
    async fn read_objects_from_store(
        &self,
        input_objects: &[InputObjectKind],
    ) -> SuiResult<Vec<Option<Object>>> {
        let ids: Vec<_> = input_objects.iter().map(|kind| kind.object_id()).collect();
        let objects = self.store.get_objects(&ids[..])?;
        Ok(objects)
    }

    #[cfg(test)]
    pub fn get_authorities(&self) -> &AuthorityAggregator<A> {
        &self.authorities
    }

    #[cfg(test)]
    pub fn store(&self) -> &Arc<GatewayStore> {
        &self.store
    }
}

// Operations are considered successful when they successfully reach a quorum of authorities.
#[async_trait]
pub trait GatewayAPI {
    async fn execute_transaction(
        &self,
        tx: Transaction,
    ) -> Result<TransactionResponse, anyhow::Error>;

    /// Send coin object to a Sui address.
    async fn transfer_coin(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas_payment: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> Result<TransactionData, anyhow::Error>;

    /// Synchronise account state with a random authorities, updates all object_ids
    /// from account_addr, request only goes out to one authority.
    /// this method doesn't guarantee data correctness, caller will have to handle potential byzantine authority
    async fn sync_account_state(&self, account_addr: SuiAddress) -> Result<(), anyhow::Error>;

    /// Call move functions in the module in the given package, with args supplied
    async fn move_call(
        &self,
        signer: SuiAddress,
        package_object_ref: ObjectRef,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        arguments: Vec<SuiJsonValue>,
        gas_object_ref: ObjectRef,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error>;

    /// Publish Move modules
    async fn publish(
        &self,
        signer: SuiAddress,
        package_bytes: Vec<Vec<u8>>,
        gas_object_ref: ObjectRef,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error>;

    /// Split the coin object (identified by `coin_object_ref`) into
    /// multiple new coins. The amount of each new coin is specified in
    /// `split_amounts`. Remaining balance is kept in the original
    /// coin object.
    /// Note that the order of the new coins in SplitCoinResponse will
    /// not be the same as the order of `split_amounts`.
    async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas_payment: ObjectID,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error>;

    /// Merge the `coin_to_merge` coin object into `primary_coin`.
    /// After this merge, the balance of `primary_coin` will become the
    /// sum of the two, while `coin_to_merge` will be deleted.
    ///
    /// Returns a pair:
    ///  (update primary coin object reference, updated gas payment object reference)
    ///
    /// TODO: Support merging a vector of coins.
    async fn merge_coins(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas_payment: ObjectID,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error>;

    /// Get the object information
    async fn get_object_info(&self, object_id: ObjectID) -> Result<ObjectRead, anyhow::Error>;

    /// Get refs of all objects we own from local cache.
    async fn get_owned_objects(
        &self,
        account_addr: SuiAddress,
    ) -> Result<Vec<ObjectRef>, anyhow::Error>;

    /// Get the total number of transactions ever happened in history.
    fn get_total_transaction_number(&self) -> Result<u64, anyhow::Error>;

    /// Return the list of transactions with sequence number in range [`start`, end).
    /// `start` is included, `end` is excluded.
    fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>, anyhow::Error>;

    /// Return the most recent `count` transactions.
    fn get_recent_transactions(
        &self,
        count: u64,
    ) -> Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>, anyhow::Error>;

    // return transaction details by digest
    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<TransactionEffectsResponse, anyhow::Error>;
}

impl<A> GatewayState<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // TODO: This is expensive and unnecessary.
    // We should make sure that the framework package exists in the gateway store and read it.
    // Or even better, we should cache the reference in GatewayState struct.
    pub async fn get_framework_object_ref(&self) -> Result<ObjectRef, anyhow::Error> {
        let info = self
            .get_object_info(ObjectID::from(SUI_FRAMEWORK_ADDRESS))
            .await?;
        Ok(info.reference()?)
    }

    async fn get_object(&self, object_id: &ObjectID) -> SuiResult<Object> {
        let object = self
            .store
            .get_object(object_id)?
            .ok_or(SuiError::ObjectNotFound {
                object_id: *object_id,
            })?;
        debug!(?object_id, ?object, "Fetched object from local store");
        Ok(object)
    }

    async fn set_transaction_lock(
        &self,
        mutable_input_objects: &[ObjectRef],
        transaction: Transaction,
    ) -> Result<(), SuiError> {
        debug!(
            ?mutable_input_objects,
            ?transaction,
            "Setting transaction lock"
        );
        self.store
            .set_transaction_lock(mutable_input_objects, transaction)
    }

    /// Make sure all objects in the input exist in the gateway store.
    /// If any object does not exist in the store, give it a chance
    /// to download from authorities.
    async fn sync_input_objects_with_authorities(
        &self,
        transaction: &Transaction,
    ) -> Result<(), anyhow::Error> {
        let input_objects = transaction.data.input_objects()?;
        let mut objects = self.read_objects_from_store(&input_objects).await?;
        for (object_opt, kind) in objects.iter_mut().zip(&input_objects) {
            if object_opt.is_none() {
                if let ObjectRead::Exists(_, object, _) =
                    self.get_object_info(kind.object_id()).await?
                {
                    *object_opt = Some(object);
                }
            }
        }
        debug!(?transaction, "Synced input objects with authorities");
        Ok(())
    }

    /// Execute (or retry) a transaction and execute the Confirmation Transaction.
    /// Update local object states using newly created certificate and ObjectInfoResponse from the Confirmation step.
    async fn execute_transaction_impl(
        &self,
        transaction: Transaction,
    ) -> Result<(CertifiedTransaction, TransactionEffects), anyhow::Error> {
        transaction.verify_signature()?;

        self.sync_input_objects_with_authorities(&transaction)
            .await?;

        let (_gas_status, all_objects) =
            transaction_input_checker::check_transaction_input(&self.store, &transaction).await?;

        let owned_objects = transaction_input_checker::filter_owned_objects(&all_objects);
        self.set_transaction_lock(&owned_objects, transaction.clone())
            .instrument(tracing::trace_span!("db_set_transaction_lock"))
            .await?;
        // If execute_transaction ever fails due to panic, we should fix the panic and make sure it doesn't.
        // If execute_transaction fails, we should retry the same transaction, and it will
        // properly unlock the objects used in this transaction. In the short term, we will ask the wallet to retry failed transactions.
        // In the long term, the Gateway should handle retries.
        // TODO: There is also one edge case:
        //   If one object in the transaction is out-of-dated on the Gateway (comparing to authorities), and application
        //   explicitly wants to use the out-of-dated version, all objects will be locked on the Gateway, but
        //   authorities will fail due to LockError. We will not be able to unlock these objects.
        //   One solution is to reset the transaction locks upon LockError.
        let tx_digest = transaction.digest();
        let span = tracing::debug_span!(
            "execute_transaction",
            ?tx_digest,
            tx_kind = transaction.data.kind_as_str()
        );
        let exec_result = self
            .authorities
            .execute_transaction(&transaction)
            .instrument(span)
            .await;
        if exec_result.is_err() {
            error!("{:?}", exec_result);
        }
        let (new_certificate, effects) = exec_result?;

        debug!(
            ?new_certificate,
            ?effects,
            "Transaction completed successfully"
        );

        // Download the latest content of every mutated object from the authorities.
        let mutated_object_refs: BTreeSet<_> = effects
            .mutated_and_created()
            .map(|(obj_ref, _)| *obj_ref)
            .collect();
        let mutated_objects = self
            .download_objects_from_authorities(mutated_object_refs)
            .await?;
        self.store.update_gateway_state(
            all_objects,
            mutated_objects,
            new_certificate.clone(),
            effects.clone().to_unsigned_effects(),
            self.next_tx_seq_number
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        )?;

        Ok((new_certificate, effects))
    }

    async fn download_object_from_authorities(&self, object_id: ObjectID) -> SuiResult<ObjectRead> {
        let result = self.authorities.get_object_info_execute(object_id).await?;
        if let ObjectRead::Exists(obj_ref, object, _) = &result {
            let local_object = self.store.get_object(&object_id)?;
            if local_object.is_none()
                || &local_object.unwrap().compute_object_reference() != obj_ref
            {
                self.store.insert_object_direct(*obj_ref, object)?;
            }
        }
        debug!(?result, "Downloaded object from authorities");

        Ok(result)
    }

    async fn download_objects_from_authorities(
        &self,
        // TODO: HashSet probably works here just fine.
        object_refs: BTreeSet<ObjectRef>,
    ) -> Result<HashMap<ObjectRef, Object>, SuiError> {
        let mut receiver = self
            .authorities
            .fetch_objects_from_authorities(object_refs.clone());

        let mut objects = HashMap::new();
        while let Some(resp) = receiver.recv().await {
            if let Ok(o) = resp {
                // TODO: Make fetch_objects_from_authorities also return object ref
                // to avoid recomputation here.
                objects.insert(o.compute_object_reference(), o);
            }
        }
        fp_ensure!(
            object_refs.len() == objects.len(),
            SuiError::InconsistentGatewayResult {
                error: "Failed to download some objects after transaction succeeded".to_owned(),
            }
        );
        debug!(?object_refs, "Downloaded objects from authorities");
        Ok(objects)
    }

    async fn create_publish_response(
        &self,
        certificate: CertifiedTransaction,
        effects: TransactionEffects,
    ) -> Result<TransactionResponse, anyhow::Error> {
        if let ExecutionStatus::Failure { gas_cost: _, error } = effects.status {
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
        // execute_transaction should have updated the local object store with the
        // latest objects.
        let mutated_objects = self.store.get_objects(
            &effects
                .mutated_and_created()
                .map(|((object_id, _, _), _)| *object_id)
                .collect::<Vec<_>>(),
        )?;
        let mut updated_gas = None;
        let mut package = None;
        let mut created_objects = vec![];
        for ((obj_ref, _), object) in effects.mutated_and_created().zip(mutated_objects) {
            let object = object.ok_or(SuiError::InconsistentGatewayResult {
                error: format!(
                    "Crated/Updated object doesn't exist in the store: {:?}",
                    obj_ref.0
                ),
            })?;
            if object.is_package() {
                fp_ensure!(
                    package.is_none(),
                    SuiError::InconsistentGatewayResult {
                        error: "More than one package created".to_owned(),
                    }
                    .into()
                );
                package = Some(*obj_ref);
            } else if obj_ref == &effects.gas_object.0 {
                fp_ensure!(
                    updated_gas.is_none(),
                    SuiError::InconsistentGatewayResult {
                        error: "More than one gas updated".to_owned(),
                    }
                    .into()
                );
                updated_gas = Some(object);
            } else {
                created_objects.push(object);
            }
        }
        let package = package.ok_or(SuiError::InconsistentGatewayResult {
            error: "No package created".to_owned(),
        })?;
        let updated_gas = updated_gas.ok_or(SuiError::InconsistentGatewayResult {
            error: "No gas updated".to_owned(),
        })?;

        debug!(
            ?package,
            ?created_objects,
            ?updated_gas,
            ?certificate,
            "Created Publish response"
        );

        Ok(TransactionResponse::PublishResponse(PublishResponse {
            certificate,
            package,
            created_objects,
            updated_gas,
        }))
    }

    async fn create_split_coin_response(
        &self,
        certificate: CertifiedTransaction,
        effects: TransactionEffects,
    ) -> Result<TransactionResponse, anyhow::Error> {
        let call = Self::try_get_move_call(&certificate)?;
        let signer = certificate.data.signer();
        let (gas_payment, _, _) = certificate.data.gas();
        let (coin_object_id, split_arg) = match call.arguments.as_slice() {
            [CallArg::ImmOrOwnedObject((id, _, _)), CallArg::Pure(arg)] => (id, arg),
            _ => {
                return Err(SuiError::InconsistentGatewayResult {
                    error: "Malformed transaction data".to_string(),
                }
                .into())
            }
        };
        let split_amounts: Vec<u64> = bcs::from_bytes(split_arg)?;

        if let ExecutionStatus::Failure { gas_cost: _, error } = effects.status {
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
        let updated_coin = self.get_object(coin_object_id).await?;
        let mut new_coins = Vec::with_capacity(created.len());
        for ((id, _, _), _) in created {
            new_coins.push(self.get_object(id).await?);
        }
        let updated_gas = self.get_object(&gas_payment).await?;

        debug!(
            ?updated_coin,
            ?new_coins,
            ?updated_gas,
            ?certificate,
            "Created Split Coin response"
        );

        Ok(TransactionResponse::SplitCoinResponse(SplitCoinResponse {
            certificate,
            updated_coin,
            new_coins,
            updated_gas,
        }))
    }

    async fn create_merge_coin_response(
        &self,
        certificate: CertifiedTransaction,
        effects: TransactionEffects,
    ) -> Result<TransactionResponse, anyhow::Error> {
        let call = Self::try_get_move_call(&certificate)?;
        let primary_coin = match call.arguments.first() {
            Some(CallArg::ImmOrOwnedObject((id, _, _))) => id,
            _ => {
                return Err(SuiError::InconsistentGatewayResult {
                    error: "Malformed transaction data".to_string(),
                }
                .into())
            }
        };
        let (gas_payment, _, _) = certificate.data.gas();

        if let ExecutionStatus::Failure { gas_cost: _, error } = effects.status {
            return Err(error.into());
        }
        fp_ensure!(
            effects.mutated.len() == 2, // coin and gas
            SuiError::InconsistentGatewayResult {
                error: "Unexpected split outcome".to_owned()
            }
            .into()
        );
        let updated_coin = self.get_object_info(*primary_coin).await?.into_object()?;
        let updated_gas = self.get_object_info(gas_payment).await?.into_object()?;

        debug!(
            ?updated_coin,
            ?updated_gas,
            ?certificate,
            "Created Merge Coin response"
        );

        Ok(TransactionResponse::MergeCoinResponse(MergeCoinResponse {
            certificate,
            updated_coin,
            updated_gas,
        }))
    }

    fn try_get_move_call(certificate: &CertifiedTransaction) -> Result<&MoveCall, anyhow::Error> {
        if let TransactionKind::Single(SingleTransactionKind::Call(ref call)) =
            certificate.data.kind
        {
            Ok(call)
        } else {
            Err(SuiError::InconsistentGatewayResult {
                error: "Malformed transaction data".to_string(),
            }
            .into())
        }
    }

    #[cfg(test)]
    pub fn highest_known_version(&self, object_id: &ObjectID) -> Result<SequenceNumber, SuiError> {
        self.latest_object_ref(object_id)
            .map(|(_oid, seq_num, _digest)| seq_num)
    }

    #[cfg(test)]
    pub fn latest_object_ref(&self, object_id: &ObjectID) -> Result<ObjectRef, SuiError> {
        self.store
            .get_latest_parent_entry(*object_id)?
            .map(|(obj_ref, _)| obj_ref)
            .ok_or(SuiError::ObjectNotFound {
                object_id: *object_id,
            })
    }
}

#[async_trait]
impl<A> GatewayAPI for GatewayState<A>
where
    A: AuthorityAPI + Send + Sync + Clone + 'static,
{
    async fn execute_transaction(
        &self,
        tx: Transaction,
    ) -> Result<TransactionResponse, anyhow::Error> {
        let tx_kind = tx.data.kind.clone();
        let tx_digest = tx.digest();

        debug!(?tx_digest, ?tx, "Received execute_transaction request");

        let span = tracing::debug_span!(
            "gateway_execute_transaction",
            ?tx_digest,
            tx_kind = tx.data.kind_as_str()
        );

        let mut res = self
            .execute_transaction_impl(tx.clone())
            .instrument(span.clone())
            .await;

        let mut remaining_retries = MAX_NUM_TX_RETRIES;
        while res.is_err() {
            if remaining_retries == 0 {
                error!(
                    num_retries = MAX_NUM_TX_RETRIES,
                    ?tx_digest,
                    ?tx,
                    "All transaction retries failed"
                );
                // Okay to unwrap since we checked that this is an error
                return Err(res.unwrap_err());
            }
            remaining_retries -= 1;

            debug!(
                remaining_retries,
                ?tx_digest,
                ?tx,
                "Retrying failed transaction"
            );

            res = self
                .execute_transaction_impl(tx.clone())
                .instrument(span.clone())
                .await;
        }

        // Okay to unwrap() since we checked that this is Ok
        let (certificate, effects) = res.unwrap();

        debug!(?tx, ?certificate, ?effects, "Transaction succeeded");
        // Create custom response base on the request type
        if let TransactionKind::Single(tx_kind) = tx_kind {
            match tx_kind {
                SingleTransactionKind::Publish(_) => {
                    return self.create_publish_response(certificate, effects).await
                }
                // Work out if the transaction is split coin or merge coin transaction
                SingleTransactionKind::Call(move_call) => {
                    if move_call.package == self.get_framework_object_ref().await?
                        && move_call.module.as_ref() == coin::COIN_MODULE_NAME
                    {
                        if move_call.function.as_ref() == coin::COIN_SPLIT_VEC_FUNC_NAME {
                            return self.create_split_coin_response(certificate, effects).await;
                        } else if move_call.function.as_ref() == coin::COIN_JOIN_FUNC_NAME {
                            return self.create_merge_coin_response(certificate, effects).await;
                        }
                    }
                }
                _ => {}
            }
        }
        return Ok(TransactionResponse::EffectResponse(
            TransactionEffectsResponse {
                certificate,
                effects,
            },
        ));
    }

    async fn transfer_coin(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas_payment: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> Result<TransactionData, anyhow::Error> {
        // TODO: We should be passing in object_ref directly instead of object_id.
        let object = self.get_object(&object_id).await?;
        let object_ref = object.compute_object_reference();
        let gas_payment = self.get_object(&gas_payment).await?;
        let gas_payment_ref = gas_payment.compute_object_reference();

        let data = TransactionData::new_transfer(
            recipient,
            object_ref,
            signer,
            gas_payment_ref,
            gas_budget,
        );

        Ok(data)
    }

    // TODO: Get rid of the sync API.
    // https://github.com/MystenLabs/sui/issues/1045
    async fn sync_account_state(&self, account_addr: SuiAddress) -> Result<(), anyhow::Error> {
        let (active_object_certs, _deleted_refs_certs) = self
            .authorities
            .sync_all_owned_objects(account_addr, Duration::from_secs(60))
            .await?;

        debug!(
            ?active_object_certs,
            deletec = ?_deleted_refs_certs,
            ?account_addr,
            "Syncing account states"
        );

        for (object, _option_layout, _option_cert) in active_object_certs {
            let res = self
                .store
                .insert_object_direct(object.compute_object_reference(), &object);
            match res {
                Ok(_) => {}
                Err(e) => {
                    error!(
                        "failed to insert obj {:?}. ref: {:?}. Error: {:?}",
                        &object,
                        object.compute_object_reference(),
                        e,
                    );
                    return Err(anyhow!(e));
                }
            }
        }

        Ok(())
    }

    async fn move_call(
        &self,
        signer: SuiAddress,
        package_object_ref: ObjectRef,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        arguments: Vec<SuiJsonValue>,
        gas_object_ref: ObjectRef,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        let package_obj = self.get_object(&package_object_ref.0).await?;
        let json_args =
            resolve_move_function_args(&package_obj, module.clone(), function.clone(), arguments)?;

        // Fetch all the objects needed for this call
        let mut objects = BTreeMap::new();
        let mut args = Vec::with_capacity(json_args.len());

        for json_arg in json_args {
            args.push(match json_arg {
                SuiJsonCallArg::Object(id) => {
                    let obj = self.get_object(&id).await?;
                    let arg = if obj.is_shared() {
                        CallArg::SharedObject(id)
                    } else {
                        CallArg::ImmOrOwnedObject(obj.compute_object_reference())
                    };
                    objects.insert(id, obj);
                    arg
                }
                SuiJsonCallArg::Pure(bytes) => CallArg::Pure(bytes),
            })
        }

        // Pass in the objects for a deeper check
        let compiled_module = package_obj
            .data
            .try_as_package()
            .ok_or_else(|| anyhow!("Cannot get package from object"))?
            .deserialize_module(&module)?;
        resolve_and_type_check(
            &objects,
            &compiled_module,
            &function,
            &type_arguments,
            args.clone(),
        )?;

        let data = TransactionData::new_move_call(
            signer,
            package_object_ref,
            module,
            function,
            type_arguments,
            gas_object_ref,
            args,
            gas_budget,
        );

        debug!(?data, "Created Move Call transaction data");
        Ok(data)
    }

    async fn publish(
        &self,
        signer: SuiAddress,
        package_bytes: Vec<Vec<u8>>,
        gas_object_ref: ObjectRef,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        let data = TransactionData::new_module(signer, gas_object_ref, package_bytes, gas_budget);
        Ok(data)
    }

    async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas_payment: ObjectID,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        // TODO: We should be passing in object_refs directly instead of object_ids.
        let coin_object = self.get_object(&coin_object_id).await?;
        let coin_object_ref = coin_object.compute_object_reference();
        let gas_payment = self.get_object(&gas_payment).await?;
        let gas_payment_ref = gas_payment.compute_object_reference();
        let coin_type = coin_object.get_move_template_type()?;

        let data = TransactionData::new_move_call(
            signer,
            self.get_framework_object_ref().await?,
            coin::COIN_MODULE_NAME.to_owned(),
            coin::COIN_SPLIT_VEC_FUNC_NAME.to_owned(),
            vec![coin_type],
            gas_payment_ref,
            vec![
                CallArg::ImmOrOwnedObject(coin_object_ref),
                CallArg::Pure(bcs::to_bytes(&split_amounts)?),
            ],
            gas_budget,
        );
        debug!(?data, "Created Split Coin transaction data");
        Ok(data)
    }

    async fn merge_coins(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas_payment: ObjectID,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        // TODO: We should be passing in object_refs directly instead of object_ids.
        let primary_coin = self.get_object(&primary_coin).await?;
        let primary_coin_ref = primary_coin.compute_object_reference();
        let coin_to_merge = self.get_object(&coin_to_merge).await?;
        let coin_to_merge_ref = coin_to_merge.compute_object_reference();
        let gas_payment = self.get_object(&gas_payment).await?;
        let gas_payment_ref = gas_payment.compute_object_reference();

        let coin_type = coin_to_merge.get_move_template_type()?;

        let data = TransactionData::new_move_call(
            signer,
            self.get_framework_object_ref().await?,
            coin::COIN_MODULE_NAME.to_owned(),
            coin::COIN_JOIN_FUNC_NAME.to_owned(),
            vec![coin_type],
            gas_payment_ref,
            vec![
                CallArg::ImmOrOwnedObject(primary_coin_ref),
                CallArg::ImmOrOwnedObject(coin_to_merge_ref),
            ],
            gas_budget,
        );
        debug!(?data, "Created Merge Coin transaction data");
        Ok(data)
    }

    async fn get_object_info(&self, object_id: ObjectID) -> Result<ObjectRead, anyhow::Error> {
        let result = self.download_object_from_authorities(object_id).await?;
        Ok(result)
    }

    async fn get_owned_objects(
        &self,
        account_addr: SuiAddress,
    ) -> Result<Vec<ObjectRef>, anyhow::Error> {
        Ok(self.store.get_account_objects(account_addr)?)
    }

    fn get_total_transaction_number(&self) -> Result<u64, anyhow::Error> {
        Ok(self.store.next_sequence_number()?)
    }

    fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>, anyhow::Error> {
        fp_ensure!(
            start <= end,
            SuiError::GatewayInvalidTxRangeQuery {
                error: format!(
                    "start must not exceed end, (start={}, end={}) given",
                    start, end
                ),
            }
            .into()
        );
        fp_ensure!(
            end - start <= MAX_TX_RANGE_SIZE,
            SuiError::GatewayInvalidTxRangeQuery {
                error: format!(
                    "Number of transactions queried must not exceed {}, {} queried",
                    MAX_TX_RANGE_SIZE,
                    end - start
                ),
            }
            .into()
        );
        let res = self.store.transactions_in_seq_range(start, end)?;
        debug!(?start, ?end, ?res, "Fetched transactions");
        Ok(res)
    }

    fn get_recent_transactions(
        &self,
        count: u64,
    ) -> Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>, anyhow::Error> {
        fp_ensure!(
            count <= MAX_TX_RANGE_SIZE,
            SuiError::GatewayInvalidTxRangeQuery {
                error: format!(
                    "Number of transactions queried must not exceed {}, {} queried",
                    MAX_TX_RANGE_SIZE, count
                ),
            }
            .into()
        );
        let end = self.get_total_transaction_number()?;
        let start = if end >= count { end - count } else { 0 };
        self.get_transactions_in_range(start, end)
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<TransactionEffectsResponse, anyhow::Error> {
        let opt = self.store.get_certified_transaction(&digest)?;
        match opt {
            Some(certificate) => Ok(TransactionEffectsResponse {
                certificate,
                effects: self.store.get_effects(&digest)?,
            }),
            None => Err(anyhow!(SuiError::TransactionNotFound { digest })),
        }
    }
}
