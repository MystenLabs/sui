// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::join_all;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use async_trait::async_trait;
use futures::future;
use move_binary_format::access::ModuleAccess;
use move_bytecode_utils::module_cache::SyncModuleCache;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::{StructTag, TypeTag};
use prometheus::{
    register_histogram_with_registry, register_int_counter_with_registry, Histogram, IntCounter,
    Registry,
};
use sui_adapter::adapter::resolve_and_type_check;
use sui_config::gateway::GatewayConfig;
use sui_types::gas_coin::GasCoin;
use sui_types::object::{Data, ObjectFormatOptions, Owner};
use sui_types::SUI_SYSTEM_STATE_OBJECT_ID;
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
use tracing::{debug, error, trace, Instrument};

use crate::authority::ResolverWrapper;
use crate::authority_aggregator::AuthAggMetrics;
use crate::authority_client::{
    make_authority_clients, NetworkAuthorityClient, NetworkAuthorityClientMetrics,
};
use crate::safe_client::SafeClientMetrics;
use crate::transaction_input_checker;
use crate::validator_info::make_committee;
use crate::{
    authority::GatewayStore, authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI, query_helpers::QueryHelpers,
};
use sui_json::{resolve_move_function_args, SuiJsonCallArg, SuiJsonValue};
use sui_json_rpc_types::{
    GetObjectDataResponse, GetRawObjectDataResponse, MoveCallParams, RPCTransactionRequestParams,
    SuiData, SuiObject, SuiObjectInfo, SuiParsedMergeCoinResponse, SuiParsedPublishResponse,
    SuiParsedSplitCoinResponse, SuiParsedTransactionResponse, SuiTransactionEffects,
    SuiTransactionResponse, SuiTypeTag, TransferObjectParams,
};
use sui_types::error::SuiError::ObjectLockConflict;

use crate::epoch::committee_store::CommitteeStore;
use sui_config::genesis::Genesis;
use tap::TapFallible;

pub type AsyncResult<'a, T, E> = future::BoxFuture<'a, Result<T, E>>;

pub type GatewayClient = Arc<dyn GatewayAPI + Sync + Send>;

pub type TxSeqNumber = u64;

/// Number of times to retry failed TX
const MAX_NUM_TX_RETRIES: usize = 5;

/// Prometheus metrics which can be displayed in Grafana, queried and alerted on
#[derive(Clone)]
pub struct GatewayMetrics {
    total_tx_processed: IntCounter,
    total_tx_errored: IntCounter,
    num_tx_publish: IntCounter,
    num_tx_movecall: IntCounter,
    num_tx_splitcoin: IntCounter,
    num_tx_splitcoin_equal: IntCounter,
    num_tx_mergecoin: IntCounter,
    total_tx_retries: IntCounter,
    shared_obj_tx: IntCounter,
    pub total_tx_certificates: IntCounter,
    pub transaction_latency: Histogram,
}

impl GatewayMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_tx_processed: register_int_counter_with_registry!(
                "total_tx_processed",
                "Total number of transaction certificates processed in Gateway",
                registry,
            )
            .unwrap(),
            total_tx_errored: register_int_counter_with_registry!(
                "total_tx_errored",
                "Total number of transactions which errored out",
                registry,
            )
            .unwrap(),
            // total_effects == total transactions finished
            num_tx_publish: register_int_counter_with_registry!(
                "num_tx_publish",
                "Number of publish transactions",
                registry,
            )
            .unwrap(),
            num_tx_movecall: register_int_counter_with_registry!(
                "num_tx_movecall",
                "Number of MOVE call transactions",
                registry,
            )
            .unwrap(),
            num_tx_splitcoin: register_int_counter_with_registry!(
                "num_tx_splitcoin",
                "Number of split coin transactions",
                registry,
            )
            .unwrap(),
            num_tx_splitcoin_equal: register_int_counter_with_registry!(
                "num_tx_splitcoin_equal",
                "Number of equal-size split coin transactions",
                registry,
            )
            .unwrap(),
            num_tx_mergecoin: register_int_counter_with_registry!(
                "num_tx_mergecoin",
                "Number of merge coin transactions",
                registry,
            )
            .unwrap(),
            total_tx_certificates: register_int_counter_with_registry!(
                "total_tx_certificates",
                "Total number of certificates made from validators",
                registry,
            )
            .unwrap(),
            total_tx_retries: register_int_counter_with_registry!(
                "total_tx_retries",
                "Total number of retries for transactions",
                registry,
            )
            .unwrap(),
            shared_obj_tx: register_int_counter_with_registry!(
                "gateway_shared_obj_tx",
                "Number of transactions involving shared objects",
                registry,
            )
            .unwrap(),
            transaction_latency: register_histogram_with_registry!(
                "transaction_latency",
                "Latency of execute_transaction_impl",
                registry,
            )
            .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = Registry::new();
        Self::new(&registry)
    }
}

pub struct GatewayState<A> {
    authorities: AuthorityAggregator<A>,
    store: Arc<GatewayStore>,
    /// Every transaction committed in authorities (and hence also committed in the Gateway)
    /// will have a unique sequence number. This number is specific to this gateway,
    /// and hence will not be compatible with authorities or other gateways.
    /// It's useful if we need some kind of ordering for transactions
    /// from a gateway.
    next_tx_seq_number: AtomicU64,
    metrics: GatewayMetrics,
    module_cache: SyncModuleCache<ResolverWrapper<GatewayStore>>,
}

impl<A> GatewayState<A> {
    /// Create a new manager which stores its managed addresses at `path`
    pub async fn new(
        base_path: &Path,
        committee: Committee,
        authority_clients: BTreeMap<AuthorityName, A>,
        prometheus_registry: &Registry,
        network_metrics: Arc<NetworkAuthorityClientMetrics>,
    ) -> SuiResult<Self> {
        let gateway_metrics = GatewayMetrics::new(prometheus_registry);
        let auth_agg_metrics = AuthAggMetrics::new(prometheus_registry);
        let safe_client_metrics = Arc::new(SafeClientMetrics::new(prometheus_registry));
        let gateway_store = Arc::new(
            GatewayStore::open(
                &base_path.join("store"),
                None,
                &Genesis::get_default_genesis(),
            )
            .await?,
        );
        let committee_store = Arc::new(CommitteeStore::new(
            base_path.join("epochs"),
            &committee,
            None,
        ));
        Self::new_with_authorities(
            gateway_store,
            AuthorityAggregator::new(
                committee,
                committee_store,
                authority_clients,
                auth_agg_metrics,
                safe_client_metrics,
                network_metrics,
            ),
            gateway_metrics,
        )
    }

    pub fn new_with_authorities(
        gateway_store: Arc<GatewayStore>,
        authorities: AuthorityAggregator<A>,
        metrics: GatewayMetrics,
    ) -> SuiResult<Self> {
        let next_tx_seq_number = AtomicU64::new(gateway_store.next_sequence_number()?);
        Ok(Self {
            store: gateway_store.clone(),
            authorities,
            next_tx_seq_number,
            metrics,
            module_cache: SyncModuleCache::new(ResolverWrapper(gateway_store)),
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

impl GatewayState<NetworkAuthorityClient> {
    pub async fn create_client(
        config: &GatewayConfig,
        prometheus_registry: Option<&Registry>,
    ) -> Result<GatewayClient, anyhow::Error> {
        let committee = make_committee(config.epoch, &config.validator_set)?;
        let default_registry = Registry::new();
        let prometheus_registry = prometheus_registry.unwrap_or(&default_registry);
        let network_metrics = Arc::new(NetworkAuthorityClientMetrics::new(prometheus_registry));
        let authority_clients = make_authority_clients(
            &config.validator_set,
            config.send_timeout,
            config.recv_timeout,
            network_metrics.clone(),
        );

        Ok(Arc::new(
            GatewayState::new(
                &config.db_folder_path,
                committee,
                authority_clients,
                prometheus_registry,
                network_metrics,
            )
            .await?,
        ))
    }
}

// Operations are considered successful when they successfully reach a quorum of authorities.
#[async_trait]
pub trait GatewayAPI {
    async fn execute_transaction(
        &self,
        tx: Transaction,
    ) -> Result<SuiTransactionResponse, anyhow::Error>;

    /// Send an object to a Sui address. The object's type must allow public transfers
    async fn public_transfer_object(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> Result<TransactionData, anyhow::Error>;

    /// Send SUI coin object to a Sui address. The SUI object is also used as the gas object.
    async fn transfer_sui(
        &self,
        signer: SuiAddress,
        sui_object_id: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
        amount: Option<u64>,
    ) -> Result<TransactionData, anyhow::Error>;

    /// Send SUI coins to a list of addresses, following a list of amounts.
    async fn pay(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error>;

    /// Send SUI coins to a list of addresses, following a list of amounts.
    /// only for SUI coin and does not require a separate gas coin object.
    /// Specifically, what pay_sui does are:
    /// 1. debit each input_coin to create new coin following the order of
    /// amounts and assign it to the corresponding recipient.
    /// 2. accumulate all residual SUI from input coins left and deposit all SUI to the first
    /// input coin, then use the first input coin as the gas coin object.
    /// 3. the balance of the first input coin after tx is sum(input_coins) - sum(amounts) - actual_gas_cost
    /// 4. all other input coints other than the first one are deleted.
    async fn pay_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error>;

    /// Send all SUI coins to one recipient.
    /// only for SUI coin and does not require a separate gas coin object either.
    /// Specifically, what pay_all_sui does are:
    /// 1. accumulate all SUI from input coins and deposit all SUI to the first input coin
    /// 2. transfer the updated first coin to the recipient and also use this first coin as
    /// gas coin object.
    /// 3. the balance of the first input coin after tx is sum(input_coins) - actual_gas_cost.
    /// 4. all other input coins other than the first are deleted.
    async fn pay_all_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipient: SuiAddress,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error>;

    /// Synchronise account state with a random authorities, updates all object_ids
    /// from account_addr, request only goes out to one authority.
    /// this method doesn't guarantee data correctness, caller will have to handle potential byzantine authority
    async fn sync_account_state(&self, account_addr: SuiAddress) -> Result<(), anyhow::Error>;

    /// Call move functions in the module in the given package, with args supplied
    async fn move_call(
        &self,
        signer: SuiAddress,
        package_object_id: ObjectID,
        module: String,
        function: String,
        type_arguments: Vec<SuiTypeTag>,
        arguments: Vec<SuiJsonValue>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error>;

    /// Publish Move modules
    async fn publish(
        &self,
        signer: SuiAddress,
        package_bytes: Vec<Vec<u8>>,
        gas: Option<ObjectID>,
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
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error>;

    /// Split the coin object (identified by `coin_object_ref`) into
    /// multiple new coins of equal amounts. Any extra remainder is
    /// kept in the original coin object.
    async fn split_coin_equal(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_count: u64,
        gas: Option<ObjectID>,
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
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error>;

    /// Create a Batch Transaction that contains a vector of parameters needed to construct
    /// all the single transactions in it.
    /// Supported single transactions are TransferObject and MoveCall.
    async fn batch_transaction(
        &self,
        signer: SuiAddress,
        single_transaction_params: Vec<RPCTransactionRequestParams>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error>;

    /// Get the object data
    async fn get_object(&self, object_id: ObjectID)
        -> Result<GetObjectDataResponse, anyhow::Error>;

    /// Get the object data
    async fn get_raw_object(
        &self,
        object_id: ObjectID,
    ) -> Result<GetRawObjectDataResponse, anyhow::Error>;

    /// Get refs of all objects we own from local cache.
    async fn get_objects_owned_by_address(
        &self,
        account_addr: SuiAddress,
    ) -> Result<Vec<SuiObjectInfo>, anyhow::Error>;

    async fn get_objects_owned_by_object(
        &self,
        object_id: ObjectID,
    ) -> Result<Vec<SuiObjectInfo>, anyhow::Error>;

    /// Get the total number of transactions ever happened in history.
    fn get_total_transaction_number(&self) -> Result<u64, anyhow::Error>;

    /// Return the list of transactions with sequence number in range [`start`, end).
    /// `start` is included, `end` is excluded.
    fn get_transactions_in_range(
        &self,
        start: TxSeqNumber,
        end: TxSeqNumber,
    ) -> Result<Vec<TransactionDigest>, anyhow::Error>;

    /// return transaction details by digest
    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<SuiTransactionResponse, anyhow::Error>;
}

impl<A> GatewayState<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub async fn get_framework_object_ref(&self) -> Result<ObjectRef, anyhow::Error> {
        Ok(self
            .get_object_ref(&ObjectID::from(SUI_FRAMEWORK_ADDRESS))
            .await?)
    }

    /// This function now always fetch the latest state of the object from validators.
    /// We need to do so because it's possible for the state on the gateway to be out-of-dated.
    /// TODO: Once we move the gateway to the wallet SDK and serve the wallet locally,
    /// we should be able to speculate that the object state is up-to-date most of the time.
    /// And when it's out-of-dated in the rare case, we need to be able to understand the error
    /// returned from validators and update the object locally so that the wallet can retry.
    async fn get_object_internal(&self, object_id: &ObjectID) -> SuiResult<Object> {
        if let Ok(Some(o)) = self.store.get_object(object_id) {
            if o.is_immutable() {
                // If an object is immutable, it can never be mutated and hence is guaranteed to
                // be up-to-date. No need to download from validators.
                return Ok(o);
            }
        }
        let object = self
            .download_object_from_authorities(*object_id)
            .await?
            .into_object()?;
        let obj_ref = object.compute_object_reference();
        debug!(?object_id, ?obj_ref, "Fetched object from validators");
        Ok(object)
    }

    async fn get_sui_object<T: SuiData>(
        &self,
        object_id: &ObjectID,
    ) -> Result<SuiObject<T>, anyhow::Error> {
        let object = self.get_object_internal(object_id).await?;
        self.to_sui_object(object).await
    }

    async fn to_sui_object<T: SuiData>(
        &self,
        object: Object,
    ) -> Result<SuiObject<T>, anyhow::Error> {
        // we must load the package the defines the object's type
        // and the packages that are used in any interior fields
        // These modules are needed for get_layout
        if let Data::Move(move_object) = &object.data {
            self.load_object_transitive_deps(&move_object.type_).await?;
        }
        let layout = object.get_layout(ObjectFormatOptions::default(), &self.module_cache)?;
        SuiObject::<T>::try_from(object, layout)
    }

    // this function over-approximates
    // it loads all modules used in the type declaration
    // and then all of their dependencies.
    // To be exact, it would need to look at the field layout for each type used, but this will
    // be complicated with generics. The extra loading here is hopefully insignificant
    async fn load_object_transitive_deps(
        &self,
        struct_tag: &StructTag,
    ) -> Result<(), anyhow::Error> {
        fn used_packages(packages: &mut Vec<ObjectID>, type_: &TypeTag) {
            match type_ {
                TypeTag::Bool
                | TypeTag::U8
                | TypeTag::U16
                | TypeTag::U32
                | TypeTag::U64
                | TypeTag::U128
                | TypeTag::U256
                | TypeTag::Address
                | TypeTag::Signer => (),
                TypeTag::Vector(inner) => used_packages(packages, inner),
                TypeTag::Struct(StructTag {
                    address,
                    type_params,
                    ..
                }) => {
                    packages.push((*address).into());
                    for t in type_params {
                        used_packages(packages, t)
                    }
                }
            }
        }
        let StructTag {
            address,
            type_params,
            ..
        } = struct_tag;
        let mut queue = vec![(*address).into()];
        for t in type_params {
            used_packages(&mut queue, t)
        }

        let mut seen: HashSet<ObjectID> = HashSet::new();
        while let Some(cur) = queue.pop() {
            if seen.contains(&cur) {
                continue;
            }
            let obj = self.get_object_internal(&cur).await?;
            let package = match &obj.data {
                Data::Move(_) => {
                    debug_assert!(false, "{cur} should be a package, not a move object");
                    continue;
                }
                Data::Package(package) => package,
            };
            let modules = package
                .serialized_module_map()
                .keys()
                .map(|name| package.deserialize_module(&Identifier::new(name.clone()).unwrap()))
                .collect::<Result<Vec<_>, _>>()?;
            for module in modules {
                let self_package_idx = module
                    .module_handle_at(module.self_module_handle_idx)
                    .address;
                let self_package = *module.address_identifier_at(self_package_idx);
                seen.insert(self_package.into());
                for handle in &module.module_handles {
                    let dep_package = *module.address_identifier_at(handle.address);
                    queue.push(dep_package.into());
                }
            }
        }
        Ok(())
    }

    async fn get_object_ref(&self, object_id: &ObjectID) -> SuiResult<ObjectRef> {
        let object = self.get_object_internal(object_id).await?;
        Ok(object.compute_object_reference())
    }

    async fn set_transaction_lock(
        &self,
        mutable_input_objects: &[ObjectRef],
        transaction: VerifiedTransaction,
    ) -> Result<(), SuiError> {
        debug!(
            ?mutable_input_objects,
            ?transaction,
            "Setting transaction lock"
        );
        self.store
            .lock_and_write_transaction(
                self.authorities.committee.epoch,
                mutable_input_objects,
                transaction,
            )
            .await
    }

    /// Make sure all objects in the input exist in the gateway store.
    /// If any object does not exist in the store, give it a chance
    /// to download from authorities.
    async fn sync_input_objects_with_authorities(&self, transaction: &Transaction) -> SuiResult {
        let input_objects = transaction.data().data.input_objects()?;
        let mut objects = self.read_objects_from_store(&input_objects).await?;
        for (object_opt, kind) in objects.iter_mut().zip(&input_objects) {
            if object_opt.is_none() {
                if let ObjectRead::Exists(_, object, _) = self
                    .download_object_from_authorities(kind.object_id())
                    .await?
                {
                    *object_opt = Some(object);
                }
            }
        }
        debug!(?transaction, "Synced input objects with authorities");
        Ok(())
    }

    async fn execute_transaction_impl_inner(
        &self,
        input_objects: InputObjects,
        transaction: VerifiedTransaction,
    ) -> Result<(VerifiedCertificate, CertifiedTransactionEffects), anyhow::Error> {
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
            tx_digest = ?tx_digest,
            tx_kind = transaction.data().data.kind_as_str()
        );
        let exec_result = self
            .authorities
            .execute_transaction(&transaction)
            .instrument(span)
            .await;

        self.metrics.total_tx_processed.inc();
        if exec_result.is_err() {
            self.metrics.total_tx_errored.inc();
            error!("{:?}", exec_result);
        }
        let (new_certificate, effects) = exec_result?;

        debug!(
            tx_digest = ?tx_digest,
            effects = ?effects.data(),
            "Transaction completed successfully"
        );

        // Download the latest content of every mutated object from the authorities.
        let mut mutated_object_kinds = BTreeMap::new();
        let mut mutated_object_refs = BTreeSet::new();
        for (obj_ref, _, kind) in effects.data().all_mutated() {
            mutated_object_kinds.insert(obj_ref.0, kind);
            mutated_object_refs.insert(*obj_ref);
        }
        let mutated_objects = self
            .download_objects_from_authorities(mutated_object_refs)
            .await?;
        let mutated_objects_with_kind = mutated_objects
            .into_iter()
            .map(|(obj_ref, obj)| {
                let kind = mutated_object_kinds.get(&obj_ref.0).copied().unwrap();
                (obj_ref, (obj, kind))
            })
            .collect();
        let seq = self
            .next_tx_seq_number
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.store
            .update_gateway_state(
                input_objects,
                mutated_objects_with_kind,
                new_certificate.clone(),
                seq,
                effects.clone().into_unsigned(),
                effects.digest(),
            )
            .await?;

        Ok((new_certificate, effects))
    }

    /// Checks the transaction input and set locks.
    /// If success, returns the input objects and owned objects in the input.
    async fn prepare_transaction(
        &self,
        transaction: &VerifiedTransaction,
    ) -> SuiResult<(InputObjects, Vec<ObjectRef>)> {
        self.sync_input_objects_with_authorities(transaction)
            .await?;

        // Getting the latest system state for gas information
        // TODO: once we figure out a better way to sync system state and epoch information (like pubsub or epoch change callback)
        // we don't need to download every time to get latest information like gas_price
        self.download_object_from_authorities(SUI_SYSTEM_STATE_OBJECT_ID)
            .await?;

        let (_gas_status, input_objects) = transaction_input_checker::check_transaction_input(
            &self.store,
            &transaction.data().data,
        )
        .await?;

        let owned_objects = input_objects.filter_owned_objects();
        if let Err(err) = self
            .set_transaction_lock(&owned_objects, transaction.clone())
            .instrument(tracing::trace_span!("db_set_transaction_lock"))
            .await
        {
            // This is a temporary solution to get objects out of locked state.
            // When we failed to execute a transaction due to objects locked by a previous transaction,
            // we should first try to finish executing the previous transaction. If that failed,
            // we should just reset the locks.
            match err {
                ObjectLockConflict {
                    pending_transaction,
                    ..
                } => {
                    debug!(tx_digest=?pending_transaction, "Objects locked by a previous transaction, re-executing the previous transaction");
                    if let Err(err) = self.retry_pending_tx(pending_transaction).await {
                        debug!(
                            "Retrying pending tx failed: {:?}. Resetting the transaction lock",
                            err
                        );
                        self.store.reset_transaction_lock(&owned_objects).await?;
                    }
                    self.set_transaction_lock(&owned_objects, transaction.clone())
                        .instrument(tracing::trace_span!("db_set_transaction_lock"))
                        .await?;
                }
                _ => {
                    return Err(err);
                }
            }
        }
        Ok((input_objects, owned_objects))
    }

    /// Execute (or retry) a transaction and execute the Confirmation Transaction.
    /// Update local object states using newly created certificate and ObjectInfoResponse from the Confirmation step.
    async fn execute_transaction_impl(
        &self,
        transaction: VerifiedTransaction,
        is_last_retry: bool,
    ) -> Result<(VerifiedCertificate, CertifiedTransactionEffects), anyhow::Error> {
        let (input_objects, owned_objects) =
            self.prepare_transaction(&transaction)
                .await
                .map_err(|err| SuiError::GatewayTransactionPrepError {
                    error: ToString::to_string(&err),
                })?;

        let exec_result = self
            .execute_transaction_impl_inner(input_objects, transaction)
            .await
            .tap_ok(|(_, effects)| {
                if effects.data().shared_objects.len() > 1 {
                    self.metrics.shared_obj_tx.inc();
                }
            });

        if exec_result.is_err() && is_last_retry {
            // If we cannot successfully execute this transaction, even after all the retries,
            // we have to give up. Here we reset all transaction locks for each input object.
            self.store.reset_transaction_lock(&owned_objects).await?;
        }

        exec_result
    }

    async fn retry_pending_tx(&self, digest: TransactionDigest) -> Result<(), anyhow::Error> {
        let tx = self.store.get_transaction(&digest)?;
        match tx {
            Some(tx) => {
                self.execute_verified_transaction(tx).await?;
                Ok(())
            }
            None => {
                // It's possible that the tx has been executed already.
                if self.store.get_certified_transaction(&digest)?.is_some() {
                    Ok(())
                } else {
                    Err(SuiError::TransactionNotFound { digest }.into())
                }
            }
        }
    }

    async fn download_object_from_authorities(&self, object_id: ObjectID) -> SuiResult<ObjectRead> {
        let result = self.authorities.get_object_info_execute(object_id).await?;
        if let ObjectRead::Exists(obj_ref, object, _) = &result {
            let local_object = self.store.get_object(&object_id)?;
            let should_update = match local_object {
                None => true, // Local store doesn't have it.
                Some(local_obj) => {
                    let local_obj_ref = local_obj.compute_object_reference();
                    match local_obj_ref.1.cmp(&obj_ref.1) {
                        Ordering::Greater => false, // Local version is more up-to-date
                        Ordering::Less => true,
                        Ordering::Equal => {
                            if local_obj_ref.2 != obj_ref.2 {
                                error!(
                                    "Inconsistent object digest. Local store: {:?}, on-chain: {:?}",
                                    local_obj_ref, obj_ref
                                );
                                true
                            } else {
                                false
                            }
                        }
                    }
                }
            };
            if should_update {
                self.store.insert_object_direct(*obj_ref, object).await?;
            }
        }
        debug!("Downloaded object from authorities: {}", result);

        Ok(result)
    }

    async fn download_objects_from_authorities(
        &self,
        // TODO: HashSet probably works here just fine.
        object_refs: BTreeSet<ObjectRef>,
    ) -> Result<BTreeMap<ObjectRef, Object>, SuiError> {
        let mut receiver = self
            .authorities
            .fetch_objects_from_authorities(object_refs.clone());

        let mut objects = BTreeMap::new();
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

    async fn create_parsed_transaction_response(
        &self,
        tx_kind: TransactionKind,
        certificate: CertifiedTransaction,
        effects: TransactionEffects,
    ) -> Result<Option<SuiParsedTransactionResponse>, anyhow::Error> {
        if let TransactionKind::Single(tx_kind) = tx_kind {
            match tx_kind {
                SingleTransactionKind::Publish(_) => {
                    self.metrics.num_tx_publish.inc();
                    return Ok(Some(
                        self.create_publish_response(certificate, effects).await?,
                    ));
                }
                // Work out if the transaction is split coin or merge coin transaction
                SingleTransactionKind::Call(move_call) => {
                    self.metrics.num_tx_movecall.inc();
                    if move_call.package == self.get_framework_object_ref().await?
                        && move_call.module.as_ref() == coin::PAY_MODULE_NAME
                    {
                        if move_call.function.as_ref() == coin::PAY_SPLIT_VEC_FUNC_NAME {
                            self.metrics.num_tx_splitcoin.inc();
                            return Ok(Some(
                                self.create_split_coin_response(certificate, effects, false)
                                    .await?,
                            ));
                        } else if move_call.function.as_ref() == coin::PAY_SPLIT_N_FUNC_NAME {
                            self.metrics.num_tx_splitcoin_equal.inc();
                            return Ok(Some(
                                self.create_split_coin_response(certificate, effects, true)
                                    .await?,
                            ));
                        } else if move_call.function.as_ref() == coin::PAY_JOIN_FUNC_NAME {
                            self.metrics.num_tx_mergecoin.inc();
                            return Ok(Some(
                                self.create_merge_coin_response(certificate, effects)
                                    .await?,
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(None)
    }

    async fn create_publish_response(
        &self,
        certificate: CertifiedTransaction,
        effects: TransactionEffects,
    ) -> Result<SuiParsedTransactionResponse, anyhow::Error> {
        if let ExecutionStatus::Failure { error } = effects.status {
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
                .all_mutated()
                .map(|((object_id, _, _), _, _)| *object_id)
                .collect::<Vec<_>>(),
        )?;
        let mut updated_gas = None;
        let mut package = None;
        let mut created_objects = vec![];
        for ((obj_ref, _, _), object) in effects.all_mutated().zip(mutated_objects) {
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
                updated_gas = Some(self.to_sui_object(object).await?);
            } else {
                created_objects.push(self.to_sui_object(object).await?);
            }
        }
        let package = package
            .ok_or(SuiError::InconsistentGatewayResult {
                error: "No package created".to_owned(),
            })?
            .into();

        let updated_gas = updated_gas.ok_or(SuiError::InconsistentGatewayResult {
            error: "No gas updated".to_owned(),
        })?;

        debug!(
            ?package,
            ?created_objects,
            ?updated_gas,
            tx_digest = ?certificate.digest(),
            "Created Publish response"
        );

        Ok(SuiParsedTransactionResponse::Publish(
            SuiParsedPublishResponse {
                package,
                created_objects,
                updated_gas,
            },
        ))
    }

    async fn create_split_coin_response(
        &self,
        certificate: CertifiedTransaction,
        effects: TransactionEffects,
        equal_parts: bool,
    ) -> anyhow::Result<SuiParsedTransactionResponse> {
        let call = Self::try_get_move_call(&certificate)?;
        let signer = certificate.data().data.signer();
        let (gas_payment, _, _) = certificate.data().data.gas();
        let (coin_object_id, split_arg) = match call.arguments.as_slice() {
            [CallArg::Object(ObjectArg::ImmOrOwnedObject((id, _, _))), CallArg::Pure(arg)] => {
                (id, arg)
            }
            _ => {
                return Err(SuiError::InconsistentGatewayResult {
                    error: "Malformed transaction data".to_string(),
                }
                .into())
            }
        };

        if let ExecutionStatus::Failure { error } = effects.status {
            return Err(error.into());
        }
        let created = &effects.created;
        let saw_expected_count = if equal_parts {
            let split_count: u64 = bcs::from_bytes(split_arg)?;
            created.len() as u64 == split_count - 1
        } else {
            let split_amounts: Vec<u64> = bcs::from_bytes(split_arg)?;
            created.len() == split_amounts.len()
        };
        fp_ensure!(
            effects.mutated.len() == 2     // coin and gas
               && saw_expected_count
               && created.iter().all(|(_, owner)| owner == &signer),
            SuiError::InconsistentGatewayResult {
                error: "Unexpected split outcome".to_owned()
            }
            .into()
        );
        let updated_coin = self.get_sui_object(coin_object_id).await?;
        let mut new_coins = Vec::with_capacity(created.len());
        for ((id, _, _), _) in created {
            new_coins.push(self.get_sui_object(id).await?);
        }
        let updated_gas = self.get_sui_object(&gas_payment).await?;

        debug!(
            ?updated_coin,
            ?new_coins,
            ?updated_gas,
            ?certificate,
            "Created Split Coin response"
        );

        Ok(SuiParsedTransactionResponse::SplitCoin(
            SuiParsedSplitCoinResponse {
                updated_coin,
                new_coins,
                updated_gas,
            },
        ))
    }

    async fn create_merge_coin_response(
        &self,
        certificate: CertifiedTransaction,
        effects: TransactionEffects,
    ) -> Result<SuiParsedTransactionResponse, anyhow::Error> {
        let call = Self::try_get_move_call(&certificate)?;
        let primary_coin = match call.arguments.first() {
            Some(CallArg::Object(ObjectArg::ImmOrOwnedObject((id, _, _)))) => id,
            _ => {
                return Err(SuiError::InconsistentGatewayResult {
                    error: "Malformed transaction data".to_string(),
                }
                .into())
            }
        };
        let (gas_payment, _, _) = certificate.data().data.gas();

        if let ExecutionStatus::Failure { error } = effects.status {
            return Err(error.into());
        }
        fp_ensure!(
            effects.mutated.len() == 2, // coin and gas
            SuiError::InconsistentGatewayResult {
                error: "Unexpected split outcome".to_owned()
            }
            .into()
        );
        let updated_coin = self.get_object(*primary_coin).await?.into_object()?;
        let updated_gas = self.get_object(gas_payment).await?.into_object()?;

        debug!(
            ?updated_coin,
            ?updated_gas,
            ?certificate,
            "Created Merge Coin response"
        );

        Ok(SuiParsedTransactionResponse::MergeCoin(
            SuiParsedMergeCoinResponse {
                updated_coin,
                updated_gas,
            },
        ))
    }

    fn try_get_move_call(certificate: &CertifiedTransaction) -> Result<&MoveCall, anyhow::Error> {
        if let TransactionKind::Single(SingleTransactionKind::Call(ref call)) =
            certificate.data().data.kind
        {
            Ok(call)
        } else {
            Err(SuiError::InconsistentGatewayResult {
                error: "Malformed transaction data".to_string(),
            }
            .into())
        }
    }

    async fn choose_gas_for_address(
        &self,
        address: SuiAddress,
        budget: u64,
        gas: Option<ObjectID>,
        used_object_ids: BTreeSet<ObjectID>,
    ) -> Result<ObjectRef, anyhow::Error> {
        if let Some(id) = gas {
            Ok(self
                .get_object_internal(&id)
                .await?
                .compute_object_reference())
        } else {
            for (id, balance) in self.get_owned_coins(address).await.unwrap() {
                if balance >= budget && !used_object_ids.contains(&id.0) {
                    return Ok(id);
                }
            }
            Err(anyhow!(
                "No non-argument gas objects found with value >= budget {budget}"
            ))
        }
    }

    async fn get_owned_coins(
        &self,
        address: SuiAddress,
    ) -> Result<Vec<(ObjectRef, u64)>, anyhow::Error> {
        let mut coins = Vec::new();
        for info in self.store.get_owner_objects(Owner::AddressOwner(address))? {
            if info.type_.is_gas_coin() {
                let object = self.get_object_internal(&info.object_id).await?;
                let gas_coin = GasCoin::try_from(object.data.try_as_move().unwrap())?;
                coins.push((info.into(), gas_coin.value()));
            }
        }
        Ok(coins)
    }

    async fn create_public_transfer_object_transaction_kind(
        &self,
        params: TransferObjectParams,
        used_object_ids: &mut BTreeSet<ObjectID>,
    ) -> Result<SingleTransactionKind, anyhow::Error> {
        used_object_ids.insert(params.object_id);
        let object = self.get_object_internal(&params.object_id).await?;
        let object_ref = object.compute_object_reference();
        Ok(SingleTransactionKind::TransferObject(TransferObject {
            recipient: params.recipient,
            object_ref,
        }))
    }

    async fn get_object_arg(
        &self,
        id: ObjectID,
        objects: &mut BTreeMap<ObjectID, Object>,
    ) -> Result<ObjectArg, anyhow::Error> {
        let obj = self.get_object_internal(&id).await?;
        let arg = match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => ObjectArg::SharedObject {
                id: obj.id(),
                initial_shared_version,
            },
            Owner::AddressOwner(_) | Owner::ObjectOwner(_) | Owner::Immutable => {
                ObjectArg::ImmOrOwnedObject(obj.compute_object_reference())
            }
        };
        objects.insert(id, obj);
        Ok(arg)
    }

    async fn create_move_call_transaction_kind(
        &self,
        params: MoveCallParams,
        used_object_ids: &mut BTreeSet<ObjectID>,
    ) -> Result<SingleTransactionKind, anyhow::Error> {
        let MoveCallParams {
            module,
            function,
            package_object_id,
            type_arguments,
            arguments,
        } = params;
        let mut type_params = vec![];
        if !type_arguments.is_empty() {
            for t in type_arguments.clone() {
                type_params.push(t.try_into()?);
            }
        }
        let module = Identifier::new(module)?;
        let function = Identifier::new(function)?;
        let package_obj = self.get_object_internal(&package_object_id).await?;
        let package_obj_ref = package_obj.compute_object_reference();
        let json_args = resolve_move_function_args(
            package_obj.data.try_as_package().unwrap(),
            module.clone(),
            function.clone(),
            &type_params,
            arguments,
        )?;

        // Fetch all the objects needed for this call
        let mut objects = BTreeMap::new();
        let mut args = Vec::with_capacity(json_args.len());

        // TODO: duplicated code in transaction_builder.rs
        for json_arg in json_args {
            args.push(match json_arg {
                SuiJsonCallArg::Object(id) => {
                    CallArg::Object(self.get_object_arg(id, &mut objects).await?)
                }
                SuiJsonCallArg::Pure(bytes) => CallArg::Pure(bytes),
                SuiJsonCallArg::ObjVec(v) => {
                    let mut object_ids = vec![];
                    for id in v {
                        object_ids.push(self.get_object_arg(id, &mut objects).await?);
                    }
                    CallArg::ObjVec(object_ids)
                }
            })
        }

        // Pass in the objects for a deeper check
        let is_genesis = false;
        let type_arguments = type_arguments
            .into_iter()
            .map(|arg| arg.try_into())
            .collect::<Result<Vec<_>, _>>()?;
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
            is_genesis,
        )?;
        used_object_ids.extend(objects.keys());

        Ok(SingleTransactionKind::Call(MoveCall {
            package: package_obj_ref,
            module,
            function,
            type_arguments,
            arguments: args,
        }))
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
                version: None,
            })
    }

    fn execute_verified_transaction(
        &self,
        tx: VerifiedTransaction,
    ) -> future::BoxFuture<'_, Result<SuiTransactionResponse, anyhow::Error>> {
        async fn inner<A>(
            _self: &GatewayState<A>,
            tx: VerifiedTransaction,
        ) -> Result<SuiTransactionResponse, anyhow::Error>
        where
            A: AuthorityAPI + Send + Sync + 'static + Clone,
        {
            let tx_kind = tx.data().data.kind.clone();
            let tx_digest = tx.digest();

            debug!(tx_digest = ?tx_digest, "Received execute_transaction request");

            // Ensure idempotency.
            let (certificate, effects) =
                match QueryHelpers::get_transaction(&_self.store, tx_digest) {
                    Ok((cert, effects)) => (cert, effects),
                    _ => {
                        let span = tracing::debug_span!(
                            "gateway_execute_transaction",
                            ?tx_digest,
                            tx_kind = tx.data().data.kind_as_str()
                        );

                        // Use start_coarse_time() if the below turns out to have a perf impact
                        let timer = _self.metrics.transaction_latency.start_timer();
                        let mut res = _self
                            .execute_transaction_impl(tx.clone(), false)
                            .instrument(span.clone())
                            .await;
                        // NOTE: below only records latency if this completes.
                        timer.stop_and_record();

                        let mut remaining_retries = MAX_NUM_TX_RETRIES;
                        while res.is_err() {
                            if remaining_retries == 0 {
                                error!(
                                    num_retries = MAX_NUM_TX_RETRIES,
                                    ?tx_digest,
                                    "All transaction retries failed"
                                );
                                // Okay to unwrap since we checked that this is an error
                                return Err(res.unwrap_err());
                            }
                            remaining_retries -= 1;
                            _self.metrics.total_tx_retries.inc();

                            debug!(
                                remaining_retries,
                                ?tx_digest,
                                ?res,
                                "Retrying failed transaction"
                            );

                            res = _self
                                .execute_transaction_impl(tx.clone(), remaining_retries == 0)
                                .instrument(span.clone())
                                .await;
                        }

                        // Okay to unwrap() since we checked that this is Ok
                        let (certificate, effects) = res.unwrap();
                        let effects = effects.into_data();

                        debug!(?tx_digest, "Transaction succeeded");
                        (certificate, effects)
                    }
                };

            // Create custom response base on the request type
            let parsed_data = _self
                .create_parsed_transaction_response(
                    tx_kind,
                    certificate.clone().into(),
                    effects.clone(),
                )
                .await?;

            Ok(SuiTransactionResponse {
                certificate: certificate.try_into()?,
                effects: SuiTransactionEffects::try_from(effects, &_self.module_cache)?,
                timestamp_ms: None,
                parsed_data,
            })
        }
        Box::pin(inner(self, tx))
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
    ) -> Result<SuiTransactionResponse, anyhow::Error> {
        let tx = tx.verify()?;
        self.execute_verified_transaction(tx).await
    }

    async fn public_transfer_object(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> Result<TransactionData, anyhow::Error> {
        let mut used_object_ids = BTreeSet::new();
        let params = TransferObjectParams {
            recipient,
            object_id,
        };
        let kind = TransactionKind::Single(
            self.create_public_transfer_object_transaction_kind(params, &mut used_object_ids)
                .await?,
        );
        let gas_payment = self
            .choose_gas_for_address(signer, gas_budget, gas, used_object_ids)
            .await?;
        Ok(TransactionData::new(kind, signer, gas_payment, gas_budget))
    }

    async fn transfer_sui(
        &self,
        signer: SuiAddress,
        sui_object_id: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
        amount: Option<u64>,
    ) -> Result<TransactionData, anyhow::Error> {
        let object = self.get_object_internal(&sui_object_id).await?;
        let object_ref = object.compute_object_reference();
        let data =
            TransactionData::new_transfer_sui(recipient, signer, amount, object_ref, gas_budget);
        Ok(data)
    }

    async fn pay(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        let used_coins = BTreeSet::from_iter(input_coins.iter().cloned());
        if let Some(gas) = gas {
            if used_coins.contains(&gas) {
                return Err(anyhow!("Gas coin is in input coins of Pay transaction, use PaySui transaction instead!"));
            }
        }
        let gas = self
            .choose_gas_for_address(signer, gas_budget, gas, used_coins)
            .await?;
        let handles: Vec<_> = input_coins
            .iter()
            .map(|id| self.get_object_ref(id))
            .collect();
        let coins = join_all(handles)
            .await
            .into_iter()
            .map(|c| c.unwrap())
            .collect();
        let data = TransactionData::new_pay(signer, coins, recipients, amounts, gas, gas_budget);
        Ok(data)
    }

    async fn pay_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        fp_ensure!(!input_coins.is_empty(), SuiError::EmptyInputCoins.into());

        let handles: Vec<_> = input_coins
            .iter()
            .map(|id| self.get_object_ref(id))
            .collect();
        let coins: Vec<ObjectRef> = join_all(handles)
            .await
            .into_iter()
            .map(|c| c.unwrap())
            .collect();
        // [0] is safe because input_coins is non-empty and coins are of same length as input_coins.
        let gas_object = coins[0];
        Ok(TransactionData::new_pay_sui(
            signer, coins, recipients, amounts, gas_object, gas_budget,
        ))
    }

    async fn pay_all_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipient: SuiAddress,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        fp_ensure!(!input_coins.is_empty(), SuiError::EmptyInputCoins.into());

        let handles: Vec<_> = input_coins
            .iter()
            .map(|id| self.get_object_ref(id))
            .collect();
        let coins: Vec<ObjectRef> = join_all(handles)
            .await
            .into_iter()
            .map(|c| c.unwrap())
            .collect();
        // [0] is safe because input_coins is non-empty and coins are of same length as input_coins.
        let gas_object = coins[0];
        Ok(TransactionData::new_pay_all_sui(
            signer, coins, recipient, gas_object, gas_budget,
        ))
    }

    async fn batch_transaction(
        &self,
        signer: SuiAddress,
        single_transaction_params: Vec<RPCTransactionRequestParams>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        fp_ensure!(
            !single_transaction_params.is_empty(),
            SuiError::InvalidBatchTransaction {
                error: "Batch Transaction cannot be empty".to_owned(),
            }
            .into()
        );
        let mut all_tx_kind = vec![];
        let mut used_object_ids = BTreeSet::new();
        for param in single_transaction_params {
            let kind = match param {
                RPCTransactionRequestParams::TransferObjectRequestParams(t) => {
                    self.create_public_transfer_object_transaction_kind(t, &mut used_object_ids)
                        .await?
                }
                RPCTransactionRequestParams::MoveCallRequestParams(m) => {
                    self.create_move_call_transaction_kind(m, &mut used_object_ids)
                        .await?
                }
            };
            all_tx_kind.push(kind);
        }
        let gas = self
            .choose_gas_for_address(signer, gas_budget, gas, used_object_ids)
            .await?;
        Ok(TransactionData::new(
            TransactionKind::Batch(all_tx_kind),
            signer,
            gas,
            gas_budget,
        ))
    }

    // TODO: Get rid of the sync API.
    // https://github.com/MystenLabs/sui/issues/1045
    async fn sync_account_state(&self, account_addr: SuiAddress) -> Result<(), anyhow::Error> {
        debug!(
            ?account_addr,
            "Syncing account states from validators starts."
        );

        let (active_object_certs, _deleted_refs_certs) = self
            .authorities
            .sync_all_owned_objects(account_addr, Duration::from_secs(60))
            .await?;

        // This is quite spammy when there are a number of huge objects
        trace!(
            ?active_object_certs,
            deletec = ?_deleted_refs_certs,
            ?account_addr,
            "Syncing account states from validators ends."
        );

        for (object, _option_layout, _option_cert) in active_object_certs {
            self.store
                .insert_object_direct(object.compute_object_reference(), &object)
                .await?;
        }
        debug!(?account_addr, "Syncing account states ends.");

        Ok(())
    }

    async fn move_call(
        &self,
        signer: SuiAddress,
        package_object_id: ObjectID,
        module: String,
        function: String,
        type_arguments: Vec<SuiTypeTag>,
        arguments: Vec<SuiJsonValue>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        let params = MoveCallParams {
            package_object_id,
            module,
            function,
            type_arguments,
            arguments,
        };
        let mut used_object_ids = BTreeSet::new();
        let kind = TransactionKind::Single(
            self.create_move_call_transaction_kind(params, &mut used_object_ids)
                .await?,
        );
        let gas = self
            .choose_gas_for_address(signer, gas_budget, gas, used_object_ids)
            .await?;
        let data = TransactionData::new(kind, signer, gas, gas_budget);
        debug!(?data, "Created Move Call transaction data");
        Ok(data)
    }

    async fn publish(
        &self,
        signer: SuiAddress,
        package_bytes: Vec<Vec<u8>>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        let gas = self
            .choose_gas_for_address(signer, gas_budget, gas, BTreeSet::new())
            .await?;
        let data = TransactionData::new_module(signer, gas, package_bytes, gas_budget);
        Ok(data)
    }

    // TODO: consolidate this with Pay transactions
    async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        let gas = self
            .choose_gas_for_address(signer, gas_budget, gas, BTreeSet::from([coin_object_id]))
            .await?;
        let coin_object = self.get_object_internal(&coin_object_id).await?;
        let coin_object_ref = coin_object.compute_object_reference();
        let coin_type = coin_object.get_move_template_type()?;
        let data = TransactionData::new_move_call(
            signer,
            self.get_framework_object_ref().await?,
            coin::PAY_MODULE_NAME.to_owned(),
            coin::PAY_SPLIT_VEC_FUNC_NAME.to_owned(),
            vec![coin_type],
            gas,
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_object_ref)),
                CallArg::Pure(bcs::to_bytes(&split_amounts)?),
            ],
            gas_budget,
        );
        debug!(?data, "Created Split Coin transaction data");
        Ok(data)
    }

    // TODO: consolidate this with Pay transactions
    async fn split_coin_equal(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_count: u64,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        let gas = self
            .choose_gas_for_address(signer, gas_budget, gas, BTreeSet::from([coin_object_id]))
            .await?;
        let coin_object = self.get_object_internal(&coin_object_id).await?;
        let coin_object_ref = coin_object.compute_object_reference();
        let coin_type = coin_object.get_move_template_type()?;
        let data = TransactionData::new_move_call(
            signer,
            self.get_framework_object_ref().await?,
            coin::PAY_MODULE_NAME.to_owned(),
            coin::PAY_SPLIT_N_FUNC_NAME.to_owned(),
            vec![coin_type],
            gas,
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_object_ref)),
                CallArg::Pure(bcs::to_bytes(&split_count)?),
            ],
            gas_budget,
        );
        debug!(?data, "Created equal-size Split Coin transaction data");
        Ok(data)
    }

    // TODO: consolidate this with Pay transactions
    async fn merge_coins(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        let gas = self
            .choose_gas_for_address(
                signer,
                gas_budget,
                gas,
                BTreeSet::from([coin_to_merge, primary_coin]),
            )
            .await?;
        let primary_coin_ref = self.get_object_ref(&primary_coin).await?;
        let coin_to_merge = self.get_object_internal(&coin_to_merge).await?;
        let coin_to_merge_ref = coin_to_merge.compute_object_reference();

        let coin_type = coin_to_merge.get_move_template_type()?;
        let data = TransactionData::new_move_call(
            signer,
            self.get_framework_object_ref().await?,
            coin::PAY_MODULE_NAME.to_owned(),
            coin::PAY_JOIN_FUNC_NAME.to_owned(),
            vec![coin_type],
            gas,
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(primary_coin_ref)),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_to_merge_ref)),
            ],
            gas_budget,
        );
        debug!(?data, "Created Merge Coin transaction data");
        Ok(data)
    }

    async fn get_object(
        &self,
        object_id: ObjectID,
    ) -> Result<GetObjectDataResponse, anyhow::Error> {
        let result = self.download_object_from_authorities(object_id).await?;
        Ok(result.try_into()?)
    }

    async fn get_raw_object(
        &self,
        object_id: ObjectID,
    ) -> Result<GetRawObjectDataResponse, anyhow::Error> {
        let result = self.download_object_from_authorities(object_id).await?;
        Ok(result.try_into()?)
    }

    async fn get_objects_owned_by_address(
        &self,
        account_addr: SuiAddress,
    ) -> Result<Vec<SuiObjectInfo>, anyhow::Error> {
        let refs: Vec<SuiObjectInfo> = self
            .store
            .get_owner_objects(Owner::AddressOwner(account_addr))?
            .into_iter()
            .map(SuiObjectInfo::from)
            .collect();
        Ok(refs)
    }

    async fn get_objects_owned_by_object(
        &self,
        object_id: ObjectID,
    ) -> Result<Vec<SuiObjectInfo>, anyhow::Error> {
        let refs: Vec<SuiObjectInfo> = self
            .store
            .get_owner_objects(Owner::ObjectOwner(object_id.into()))?
            .into_iter()
            .map(SuiObjectInfo::from)
            .collect();
        Ok(refs)
    }

    fn get_total_transaction_number(&self) -> Result<u64, anyhow::Error> {
        QueryHelpers::get_total_transaction_number(&self.store)
    }

    fn get_transactions_in_range(
        &self,
        start: TxSeqNumber,
        end: TxSeqNumber,
    ) -> Result<Vec<TransactionDigest>, anyhow::Error> {
        Ok(
            QueryHelpers::get_transactions_in_range(&self.store, start, end)?
                .into_iter()
                .map(|(_, digest)| digest)
                .collect(),
        )
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<SuiTransactionResponse, anyhow::Error> {
        let (cert, effect) = QueryHelpers::get_transaction(&self.store, &digest)?;

        Ok(SuiTransactionResponse {
            certificate: cert.try_into()?,
            effects: SuiTransactionEffects::try_from(effect, &self.module_cache)?,
            timestamp_ms: None,
            parsed_data: None,
        })
    }
}
