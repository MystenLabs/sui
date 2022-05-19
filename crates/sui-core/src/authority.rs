// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoints::FragmentInternalError;
use crate::{
    authority_batch::{BroadcastReceiver, BroadcastSender},
    checkpoints::CheckpointStore,
    epoch::EpochInfoLocals,
    execution_engine,
    gateway_types::TransactionEffectsResponse,
    transaction_input_checker,
};
use anyhow::anyhow;
use async_trait::async_trait;
use itertools::Itertools;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::ModuleCache;
use move_core_types::{
    account_address::AccountAddress,
    ident_str,
    language_storage::{ModuleId, StructTag},
    resolver::{ModuleResolver, ResourceResolver},
};
use move_vm_runtime::{move_vm::MoveVM, native_functions::NativeFunctionTable};
use narwhal_executor::ExecutionStateError;
use narwhal_executor::{ExecutionIndices, ExecutionState};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use prometheus_exporter::prometheus::{
    register_histogram, register_int_counter, Histogram, IntCounter,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};
use sui_adapter::adapter;
use sui_config::genesis::Genesis;
use sui_storage::IndexStore;
use sui_types::{
    base_types::*,
    batch::{TxSequenceNumber, UpdateItem},
    committee::Committee,
    crypto::AuthoritySignature,
    error::{SuiError, SuiResult},
    fp_bail, fp_ensure,
    gas::SuiGasStatus,
    messages::*,
    object::{Data, Object, ObjectFormatOptions, ObjectRead},
    storage::{BackingPackageStore, DeleteKind, Storage},
    MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS,
};
use tracing::{debug, error, instrument};
use typed_store::Map;

#[cfg(test)]
#[path = "unit_tests/authority_tests.rs"]
pub mod authority_tests;

#[cfg(test)]
#[path = "unit_tests/batch_transaction_tests.rs"]
mod batch_transaction_tests;

#[cfg(test)]
#[path = "unit_tests/move_integration_tests.rs"]
pub mod move_integration_tests;

#[cfg(test)]
#[path = "unit_tests/gas_tests.rs"]
mod gas_tests;

mod temporary_store;
pub use temporary_store::AuthorityTemporaryStore;

mod authority_store;
pub use authority_store::{AuthorityStore, GatewayStore, ReplicaStore, SuiDataStore};
use sui_types::object::Owner;

use self::authority_store::{
    generate_genesis_system_object, store_package_and_init_modules_for_genesis,
};

pub mod authority_notifier;

pub const MAX_ITEMS_LIMIT: u64 = 100_000;
const BROADCAST_CAPACITY: usize = 10_000;
const MAX_TX_RANGE_SIZE: u64 = 4096;

/// Prometheus metrics which can be displayed in Grafana, queried and alerted on
pub struct AuthorityMetrics {
    tx_orders: IntCounter,
    total_certs: IntCounter,
    total_effects: IntCounter,
    total_events: IntCounter,
    signature_errors: IntCounter,
    pub shared_obj_tx: IntCounter,
    tx_already_processed: IntCounter,
    num_input_objs: Histogram,
    num_shared_objects: Histogram,
    batch_size: Histogram,

    pub gossip_queued_count: IntCounter,
    pub gossip_sync_count: IntCounter,
    pub gossip_task_success_count: IntCounter,
    pub gossip_task_error_count: IntCounter,
}

// Override default Prom buckets for positive numbers in 0-50k range
const POSITIVE_INT_BUCKETS: &[f64] = &[
    1., 2., 5., 10., 20., 50., 100., 200., 500., 1000., 2000., 5000., 10000., 20000., 50000.,
];

impl AuthorityMetrics {
    pub fn new() -> AuthorityMetrics {
        Self {
            tx_orders: register_int_counter!(
                "total_transaction_orders",
                "Total number of transaction orders"
            )
            .unwrap(),
            total_certs: register_int_counter!(
                "total_transaction_certificates",
                "Total number of transaction certificates handled"
            )
            .unwrap(),
            // total_effects == total transactions finished
            total_effects: register_int_counter!(
                "total_transaction_effects",
                "Total number of transaction effects produced"
            )
            .unwrap(),
            total_events: register_int_counter!("total_events", "Total number of events produced")
                .unwrap(),
            signature_errors: register_int_counter!(
                "total_signature_errors",
                "Number of transaction signature errors"
            )
            .unwrap(),
            shared_obj_tx: register_int_counter!(
                "num_shared_obj_tx",
                "Number of transactions involving shared objects"
            )
            .unwrap(),
            tx_already_processed: register_int_counter!(
                "num_tx_already_processed",
                "Number of transaction orders already processed previously"
            )
            .unwrap(),
            num_input_objs: register_histogram!(
                "num_input_objects",
                "Distribution of number of input TX objects per TX",
                POSITIVE_INT_BUCKETS.to_vec()
            )
            .unwrap(),
            num_shared_objects: register_histogram!(
                "num_shared_objects",
                "Number of shared input objects per TX",
                POSITIVE_INT_BUCKETS.to_vec()
            )
            .unwrap(),
            batch_size: register_histogram!(
                "batch_size",
                "Distribution of size of transaction batch",
                POSITIVE_INT_BUCKETS.to_vec()
            )
            .unwrap(),
            gossip_queued_count: register_int_counter!(
                "gossip_queued_count",
                "Number of digests queued from gossip peers",
            )
            .unwrap(),
            gossip_sync_count: register_int_counter!(
                "gossip_sync_count",
                "Number of certificates downloaded from gossip peers"
            )
            .unwrap(),
            gossip_task_success_count: register_int_counter!(
                "gossip_task_success_count",
                "Number of gossip tasks that completed successfully"
            )
            .unwrap(),
            gossip_task_error_count: register_int_counter!(
                "gossip_task_error_count",
                "Number of gossip tasks that completed with errors"
            )
            .unwrap(),
        }
    }
}

impl Default for AuthorityMetrics {
    fn default() -> Self {
        Self::new()
    }
}

// One cannot register a metric multiple times.  We protect initialization with lazy_static
// for cases such as local tests or "sui start" which starts multiple authorities in one process.
pub static METRICS: Lazy<AuthorityMetrics> = Lazy::new(AuthorityMetrics::new);

/// a Trait object for `signature::Signer` that is:
/// - Pin, i.e. confined to one place in memory (we don't want to copy private keys).
/// - Sync, i.e. can be safely shared between threads.
///
/// Typically instantiated with Box::pin(keypair) where keypair is a `KeyPair`
///
pub type StableSyncAuthoritySigner =
    Pin<Arc<dyn signature::Signer<AuthoritySignature> + Send + Sync>>;

pub struct AuthorityState {
    // Fixed size, static, identity of the authority
    /// The name of this authority.
    pub name: AuthorityName,
    /// The signature key of the authority.
    pub secret: StableSyncAuthoritySigner,

    /// Committee of this Sui instance.
    pub committee: Committee,
    /// A global lock to halt all transaction/cert processing.
    #[allow(dead_code)]
    halted: AtomicBool,

    /// Move native functions that are available to invoke
    _native_functions: NativeFunctionTable,
    move_vm: Arc<MoveVM>,

    /// The database
    pub(crate) database: Arc<AuthorityStore>, // TODO: remove pub

    indexes: Option<Arc<IndexStore>>,

    /// The checkpoint store
    pub(crate) checkpoints: Option<Arc<Mutex<CheckpointStore>>>,

    // Structures needed for handling batching and notifications.
    /// The sender to notify of new transactions
    /// and create batches for this authority.
    /// Keep as None if there is no need for this.
    pub(crate) batch_channels: BroadcastSender, // TODO: remove pub

    // The Transaction notifier ticketing engine.
    pub(crate) batch_notifier: Arc<authority_notifier::TransactionNotifier>, // TODO: remove pub

    /// Ensures there can only be a single consensus client is updating the state.
    pub consensus_guardrail: AtomicUsize,

    pub metrics: &'static AuthorityMetrics,
}

/// The authority state encapsulates all state, drives execution, and ensures safety.
///
/// Note the authority operations can be accessed through a read ref (&) and do not
/// require &mut. Internally a database is synchronized through a mutex lock.
///
/// Repeating valid commands should produce no changes and return no error.
impl AuthorityState {
    /// Get a broadcast receiver for updates
    pub fn subscribe_batch(&self) -> BroadcastReceiver {
        self.batch_channels.subscribe()
    }

    async fn handle_transaction_impl(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let transaction_digest = *transaction.digest();
        // Ensure an idempotent answer.
        if self.database.transaction_exists(&transaction_digest)? {
            self.metrics.tx_already_processed.inc();
            let transaction_info = self.make_transaction_info(&transaction_digest).await?;
            return Ok(transaction_info);
        }

        let (_gas_status, all_objects) = transaction_input_checker::check_transaction_input(
            &self.database,
            &transaction,
            &self.metrics.shared_obj_tx,
        )
        .await?;

        let owned_objects = transaction_input_checker::filter_owned_objects(&all_objects);

        let signed_transaction =
            SignedTransaction::new(self.committee.epoch, transaction, self.name, &*self.secret);

        // Check and write locks, to signed transaction, into the database
        // The call to self.set_transaction_lock checks the lock is not conflicting,
        // and returns ConflictingTransaction error in case there is a lock on a different
        // existing transaction.
        self.set_transaction_lock(&owned_objects, signed_transaction)
            .await?;

        // Return the signed Transaction or maybe a cert.
        self.make_transaction_info(&transaction_digest).await
    }

    /// Initiate a new transaction.
    pub async fn handle_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        self.metrics.tx_orders.inc();
        // Check the sender's signature.
        transaction.verify_signature().map_err(|e| {
            self.metrics.signature_errors.inc();
            e
        })?;
        let transaction_digest = *transaction.digest();

        let response = self.handle_transaction_impl(transaction).await;
        match response {
            Ok(r) => Ok(r),
            // If we see an error, it is possible that a certificate has already been processed.
            // In that case, we could still return Ok to avoid showing confusing errors.
            Err(err) => {
                if self.database.effects_exists(&transaction_digest)? {
                    self.metrics.tx_already_processed.inc();
                    Ok(self.make_transaction_info(&transaction_digest).await?)
                } else {
                    Err(err)
                }
            }
        }
    }

    /// Confirm a transfer.
    pub async fn handle_confirmation_transaction(
        &self,
        confirmation_transaction: ConfirmationTransaction,
    ) -> SuiResult<TransactionInfoResponse> {
        self.metrics.total_certs.inc();
        let transaction_digest = *confirmation_transaction.certificate.digest();

        // Ensure an idempotent answer.
        if self.database.effects_exists(&transaction_digest)? {
            let info = self.make_transaction_info(&transaction_digest).await?;
            debug!("Transaction {transaction_digest:?} already executed");
            return Ok(info);
        }

        // Check the certificate and retrieve the transfer data.
        tracing::trace_span!("cert_check_signature")
            .in_scope(|| confirmation_transaction.certificate.verify(&self.committee))
            .map_err(|e| {
                self.metrics.signature_errors.inc();
                e
            })?;

        self.process_certificate(confirmation_transaction).await
    }

    #[instrument(level = "trace", skip_all)]
    async fn check_shared_locks(
        &self,
        transaction_digest: &TransactionDigest,
        // inputs: &[(InputObjectKind, Object)],
        shared_object_refs: &[ObjectRef],
    ) -> Result<(), SuiError> {
        debug!("Validating shared object sequence numbers from consensus...");

        // Internal consistency check
        debug_assert!(
            !shared_object_refs.is_empty(),
            "we just checked that there are share objects yet none found?"
        );

        let shared_locks: HashMap<_, _> = self
            .database
            .all_shared_locks(transaction_digest)?
            .into_iter()
            .collect();

        // Check whether the shared objects have already been assigned a sequence number by
        // the consensus. Bail if the transaction contains even one shared object that either:
        // (i) was not assigned a sequence number, or
        // (ii) has a different sequence number than the current one.

        let lock_errors: Vec<_> = shared_object_refs
            .iter()
            .filter_map(|(object_id, version, _)| {
                if !shared_locks.contains_key(object_id) {
                    Some(SuiError::SharedObjectLockNotSetObject)
                } else if shared_locks[object_id] != *version {
                    Some(SuiError::UnexpectedSequenceNumber {
                        object_id: *object_id,
                        // This sequence number is the one attributed by consensus.
                        expected_sequence: shared_locks[object_id],
                        // This sequence number is the one we currently have in the database.
                        given_sequence: *version,
                    })
                } else {
                    None
                }
            })
            .collect();

        fp_ensure!(
            lock_errors.is_empty(),
            // NOTE: the error message here will say 'Error acquiring lock' but what it means is
            // 'error checking lock'.
            SuiError::LockErrors {
                errors: lock_errors
            }
        );

        Ok(())
    }

    #[instrument(level = "debug", name = "process_cert_inner", skip_all)]
    async fn process_certificate(
        &self,
        confirmation_transaction: ConfirmationTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let certificate = confirmation_transaction.certificate;
        let transaction_digest = *certificate.digest();

        let (gas_status, objects_by_kind) = transaction_input_checker::check_transaction_input(
            &self.database,
            &certificate,
            &self.metrics.shared_obj_tx,
        )
        .await?;

        // At this point we need to check if any shared objects need locks,
        // and whether they have them.
        let shared_object_refs: Vec<_> = objects_by_kind
            .iter()
            .filter(|(kind, _)| matches!(kind, InputObjectKind::SharedMoveObject(_)))
            .map(|(_, obj)| obj.compute_object_reference())
            .sorted()
            .collect();
        if !shared_object_refs.is_empty() {
            // If the transaction contains shared objects, we need to ensure they have been scheduled
            // for processing by the consensus protocol.
            self.check_shared_locks(&transaction_digest, &shared_object_refs)
                .await?;
        }

        self.metrics
            .num_input_objs
            .observe(objects_by_kind.len() as f64);
        self.metrics
            .num_shared_objects
            .observe(shared_object_refs.len() as f64);
        self.metrics
            .batch_size
            .observe(certificate.data.kind.batch_size() as f64);
        debug!(
            num_inputs = objects_by_kind.len(),
            "Read inputs for transaction from DB"
        );

        let transaction_dependencies = objects_by_kind
            .iter()
            .map(|(_, obj)| obj.previous_transaction)
            .collect();
        let mut temporary_store = AuthorityTemporaryStore::new(
            self.database.clone(),
            objects_by_kind,
            transaction_digest,
        );
        let effects = execution_engine::execute_transaction_to_effects(
            shared_object_refs,
            &mut temporary_store,
            certificate.data.clone(),
            transaction_digest,
            transaction_dependencies,
            &self.move_vm,
            &self._native_functions,
            gas_status,
            self.committee.epoch,
        )?;

        self.metrics.total_effects.inc();
        self.metrics
            .total_events
            .inc_by(effects.events.len() as u64);

        // TODO: Distribute gas charge and rebate, which can be retrieved from effects.
        let signed_effects =
            effects.to_sign_effects(self.committee.epoch, &self.name, &*self.secret);

        // Update the database in an atomic manner
        self.update_state(temporary_store, &certificate, &signed_effects)
            .await?;

        Ok(TransactionInfoResponse {
            signed_transaction: self.database.get_transaction(&transaction_digest)?,
            certified_transaction: Some(certificate),
            signed_effects: Some(signed_effects),
        })
    }

    /// Check if we need to submit this transaction to consensus. We usually do, unless (i) we already
    /// processed the transaction and we can immediately return the effects, or (ii) we already locked
    /// all shared-objects of the transaction and can (re-)attempt execution.
    pub async fn try_skip_consensus(
        &self,
        certificate: CertifiedTransaction,
    ) -> Result<Option<TransactionInfoResponse>, SuiError> {
        // Ensure the input is a shared object certificate
        fp_ensure!(
            certificate.contains_shared_object(),
            SuiError::NotASharedObjectTransaction
        );

        // If we already executed this transaction, return the signed effects.
        let digest = certificate.digest();
        if self.database.effects_exists(digest)? {
            debug!("Shared-object transaction {digest:?} already executed");
            return self.make_transaction_info(digest).await.map(Some);
        }

        // If we already assigned locks to this transaction, we can try to execute it immediately.
        // This can happen to transaction previously submitted to consensus that failed execution
        // due to missing dependencies.
        if self.shared_locks_exist(&certificate).await? {
            // Attempt to execute the transaction. This will only succeed if the authority
            // already executed all its dependencies and if the locks are correctly attributed to
            // the transaction (ie. this transaction is the next to be executed).
            debug!("Shared-locks already assigned to {digest:?} - executing now");
            let confirmation = ConfirmationTransaction { certificate };
            return self.process_certificate(confirmation).await.map(Some);
        }

        // If we didn't already attributed shared locks to this transaction, it needs to go
        // through consensus.
        Ok(None)
    }

    pub async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        self.make_transaction_info(&request.transaction_digest)
            .await
    }

    pub async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, SuiError> {
        self.make_account_info(request.account)
    }

    pub async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        let ref_and_digest = match request.request_kind {
            ObjectInfoRequestKind::PastObjectInfo(seq) => {
                // Get the Transaction Digest that created the object
                self.get_parent_iterator(request.object_id, Some(seq))
                    .await?
                    .next()
            }
            ObjectInfoRequestKind::LatestObjectInfo(_) => {
                // Or get the latest object_reference and transaction entry.
                self.get_latest_parent_entry(request.object_id).await?
            }
        };

        let (requested_object_reference, parent_certificate) = match ref_and_digest {
            Some((object_ref, transaction_digest)) => (
                Some(object_ref),
                if transaction_digest == TransactionDigest::genesis() {
                    None
                } else {
                    // Get the cert from the transaction digest
                    Some(self.read_certificate(&transaction_digest).await?.ok_or(
                        SuiError::CertificateNotfound {
                            certificate_digest: transaction_digest,
                        },
                    )?)
                },
            ),
            None => (None, None),
        };

        // Return the latest version of the object and the current lock if any, if requested.
        let object_and_lock = match request.request_kind {
            ObjectInfoRequestKind::LatestObjectInfo(request_layout) => {
                match self.get_object(&request.object_id).await {
                    Ok(Some(object)) => {
                        let lock = if !object.is_owned() {
                            // Unowned obejcts have no locks.
                            None
                        } else {
                            self.get_transaction_lock(&object.compute_object_reference())
                                .await?
                        };
                        let layout = match request_layout {
                            Some(format) => {
                                let resolver = ModuleCache::new(&self);
                                object.get_layout(format, &resolver)?
                            }
                            None => None,
                        };

                        Some(ObjectResponse {
                            object,
                            lock,
                            layout,
                        })
                    }
                    Err(e) => return Err(e),
                    _ => None,
                }
            }
            ObjectInfoRequestKind::PastObjectInfo(_) => None,
        };

        Ok(ObjectInfoResponse {
            parent_certificate,
            requested_object_reference,
            object_and_lock,
        })
    }

    /// Handles a request for a batch info. It returns a sequence of
    /// [batches, transactions, batches, transactions] as UpdateItems, and a flag
    /// that if true indicates the request goes beyond the last batch in the
    /// database.
    pub async fn handle_batch_info_request(
        &self,
        request: BatchInfoRequest,
    ) -> Result<
        (
            VecDeque<UpdateItem>,
            // Should subscribe, computed start, computed end
            (bool, TxSequenceNumber, TxSequenceNumber),
        ),
        SuiError,
    > {
        // Ensure the range contains some elements and end > start
        if request.length == 0 {
            return Err(SuiError::InvalidSequenceRangeError);
        };

        // Ensure we are not doing too much work per request
        if request.length > MAX_ITEMS_LIMIT {
            return Err(SuiError::TooManyItemsError(MAX_ITEMS_LIMIT));
        }

        // If we do not have a start, pick the low watermark from the notifier.
        let start = match request.start {
            Some(start) => start,
            None => {
                self.last_batch()?
                    .expect("Authority is always initialized with a batch")
                    .batch
                    .next_sequence_number
            }
        };
        let end = start + request.length;

        let (batches, transactions) = self.database.batches_and_transactions(start, end)?;

        let mut dq_batches = std::collections::VecDeque::from(batches);
        let mut dq_transactions = std::collections::VecDeque::from(transactions);
        let mut items = VecDeque::with_capacity(dq_batches.len() + dq_transactions.len());
        let mut last_batch_next_seq = 0;

        // Send full historical data as [Batch - Transactions - Batch - Transactions - Batch].
        while let Some(current_batch) = dq_batches.pop_front() {
            // Get all transactions belonging to this batch and send them
            loop {
                // No more items or item too large for this batch
                if dq_transactions.is_empty()
                    || dq_transactions[0].0 >= current_batch.batch.next_sequence_number
                {
                    break;
                }

                let current_transaction = dq_transactions.pop_front().unwrap();
                items.push_back(UpdateItem::Transaction(current_transaction));
            }

            // Now send the batch
            last_batch_next_seq = current_batch.batch.next_sequence_number;
            items.push_back(UpdateItem::Batch(current_batch));
        }

        // whether we have sent everything requested, or need to start
        // live notifications.
        let should_subscribe = end > last_batch_next_seq;

        // If any transactions are left they must be outside a batch
        while let Some(current_transaction) = dq_transactions.pop_front() {
            // Remember the last sequence sent
            items.push_back(UpdateItem::Transaction(current_transaction));
        }

        Ok((items, (should_subscribe, start, end)))
    }

    pub async fn new(
        committee: Committee,
        name: AuthorityName,
        secret: StableSyncAuthoritySigner,
        store: Arc<AuthorityStore>,
        indexes: Option<Arc<IndexStore>>,
        checkpoints: Option<Arc<Mutex<CheckpointStore>>>,
        genesis: &Genesis,
    ) -> Self {
        let (tx, _rx) = tokio::sync::broadcast::channel(BROADCAST_CAPACITY);
        let native_functions =
            sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
        let move_vm = Arc::new(
            adapter::new_move_vm(native_functions.clone())
                .expect("We defined natives to not fail here"),
        );

        // Only initialize an empty database.
        if store
            .database_is_empty()
            .expect("Database read should not fail.")
        {
            let mut genesis_ctx = genesis.genesis_ctx().to_owned();
            for genesis_modules in genesis.modules() {
                store_package_and_init_modules_for_genesis(
                    &store,
                    &native_functions,
                    &mut genesis_ctx,
                    genesis_modules.to_owned(),
                )
                .await
                .expect("We expect publishing the Genesis packages to not fail");
            }
            store
                .bulk_object_insert(&genesis.objects().iter().collect::<Vec<_>>())
                .await
                .expect("Cannot bulk insert genesis objects");
            generate_genesis_system_object(&store, &move_vm, &committee, &mut genesis_ctx)
                .await
                .expect("Cannot generate genesis system object");

            store
                .insert_new_epoch_info(EpochInfoLocals {
                    committee,
                    validator_halted: false,
                })
                .expect("Cannot initialize the first epoch entry");
        }
        let current_epoch_info = store
            .get_last_epoch_info()
            .expect("Fail to load the current epoch info");

        let mut state = AuthorityState {
            name,
            secret,
            committee: current_epoch_info.committee,
            halted: AtomicBool::new(current_epoch_info.validator_halted),
            _native_functions: native_functions,
            move_vm,
            database: store.clone(),
            indexes,
            checkpoints,
            batch_channels: tx,
            batch_notifier: Arc::new(
                authority_notifier::TransactionNotifier::new(store.clone())
                    .expect("Notifier cannot start."),
            ),
            consensus_guardrail: AtomicUsize::new(0),
            metrics: &METRICS,
        };

        state
            .init_batches_from_database()
            .expect("Init batches failed!");

        // If a checkpoint store is present, ensure it is up-to-date with the latest
        // batches.
        if let Some(checkpoint) = &state.checkpoints {
            let next_expected_tx = checkpoint.lock().next_transaction_sequence_expected();

            // Get all unprocessed checkpoints
            for (_seq, batch) in state
                .database
                .batches
                .iter()
                .skip_to(&next_expected_tx)
                .expect("Seeking batches should never fail at this point")
            {
                let transactions: Vec<(TxSequenceNumber, TransactionDigest)> = state
                    .database
                    .executed_sequence
                    .iter()
                    .skip_to(&batch.batch.initial_sequence_number)
                    .expect("Should never fail to get an iterator")
                    .take_while(|(seq, _tx)| *seq < batch.batch.next_sequence_number)
                    .collect();

                if batch.batch.next_sequence_number > next_expected_tx {
                    // Update the checkpointing mechanism
                    checkpoint
                        .lock()
                        .handle_internal_batch(batch.batch.next_sequence_number, &transactions)
                        .expect("Should see no errors updating the checkpointing mechanism.");
                }
            }
        }

        state
    }

    pub(crate) fn checkpoints(&self) -> Option<Arc<Mutex<CheckpointStore>>> {
        self.checkpoints.clone()
    }

    pub(crate) fn db(&self) -> Arc<AuthorityStore> {
        self.database.clone()
    }

    async fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        self.database.get_object(object_id)
    }

    pub async fn get_framework_object_ref(&self) -> SuiResult<ObjectRef> {
        Ok(self
            .get_object(&SUI_FRAMEWORK_ADDRESS.into())
            .await?
            .expect("framework object should always exist")
            .compute_object_reference())
    }

    pub async fn get_object_info(&self, object_id: &ObjectID) -> Result<ObjectRead, SuiError> {
        match self.database.get_latest_parent_entry(*object_id)? {
            None => Ok(ObjectRead::NotExists(*object_id)),
            Some((obj_ref, _)) => {
                if obj_ref.2.is_alive() {
                    match self.database.get_object_version(object_id, obj_ref.1)? {
                        None => {
                            error!("Object with in parent_entry is missing from object store, datastore is inconsistent");
                            Err(SuiError::ObjectNotFound {
                                object_id: *object_id,
                            })
                        }
                        Some(object) => {
                            let resolver = ModuleCache::new(&self);
                            let layout =
                                object.get_layout(ObjectFormatOptions::default(), &resolver)?;
                            Ok(ObjectRead::Exists(obj_ref, object, layout))
                        }
                    }
                } else {
                    Ok(ObjectRead::Deleted(obj_ref))
                }
            }
        }
    }

    pub async fn get_owned_objects(&self, account_addr: SuiAddress) -> SuiResult<Vec<ObjectRef>> {
        self.database.get_account_objects(account_addr)
    }

    pub fn get_total_transaction_number(&self) -> Result<u64, anyhow::Error> {
        Ok(self.database.next_sequence_number()?)
    }

    pub fn get_transactions_in_range(
        &self,
        start: TxSequenceNumber,
        end: TxSequenceNumber,
    ) -> Result<Vec<(TxSequenceNumber, TransactionDigest)>, anyhow::Error> {
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
        let res = self.database.transactions_in_seq_range(start, end)?;
        debug!(?start, ?end, ?res, "Fetched transactions");
        Ok(res)
    }

    pub fn get_recent_transactions(
        &self,
        count: u64,
    ) -> Result<Vec<(TxSequenceNumber, TransactionDigest)>, anyhow::Error> {
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

    pub async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<TransactionEffectsResponse, anyhow::Error> {
        let opt = self.database.get_certified_transaction(&digest)?;
        match opt {
            Some(certificate) => Ok(TransactionEffectsResponse {
                certificate: certificate.try_into()?,
                effects: self.database.get_effects(&digest)?.into(),
            }),
            None => Err(anyhow!(SuiError::TransactionNotFound { digest })),
        }
    }

    fn get_indexes(&self) -> SuiResult<Arc<IndexStore>> {
        match &self.indexes {
            Some(i) => Ok(i.clone()),
            None => Err(SuiError::UnsupportedFeatureError {
                error: "extended object indexing is not enabled on this server".into(),
            }),
        }
    }

    pub async fn get_transactions_by_input_object(
        &self,
        object: ObjectID,
    ) -> Result<Vec<(TxSequenceNumber, TransactionDigest)>, anyhow::Error> {
        Ok(self
            .get_indexes()?
            .get_transactions_by_input_object(object)?)
    }

    pub async fn get_transactions_by_mutated_object(
        &self,
        object: ObjectID,
    ) -> Result<Vec<(TxSequenceNumber, TransactionDigest)>, anyhow::Error> {
        Ok(self
            .get_indexes()?
            .get_transactions_by_mutated_object(object)?)
    }

    pub async fn get_transactions_from_addr(
        &self,
        address: SuiAddress,
    ) -> Result<Vec<(TxSequenceNumber, TransactionDigest)>, anyhow::Error> {
        Ok(self.get_indexes()?.get_transactions_from_addr(address)?)
    }

    pub async fn get_transactions_to_addr(
        &self,
        address: SuiAddress,
    ) -> Result<Vec<(TxSequenceNumber, TransactionDigest)>, anyhow::Error> {
        Ok(self.get_indexes()?.get_transactions_to_addr(address)?)
    }

    pub async fn insert_genesis_object(&self, object: Object) {
        self.database
            .insert_genesis_object(object)
            .await
            .expect("Cannot insert genesis object")
    }

    pub async fn insert_genesis_objects_bulk_unsafe(&self, objects: &[&Object]) {
        self.database
            .bulk_object_insert(objects)
            .await
            .expect("Cannot bulk insert genesis objects")
    }

    /// Make an information response for a transaction
    pub(crate) async fn make_transaction_info(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        self.database
            .get_signed_transaction_info(transaction_digest)
    }

    fn make_account_info(&self, account: SuiAddress) -> Result<AccountInfoResponse, SuiError> {
        self.database
            .get_owner_objects(Owner::AddressOwner(account))
            .map(|object_ids| AccountInfoResponse {
                object_ids: object_ids.into_iter().map(|id| id.into()).collect(),
                owner: account,
            })
    }

    // Helper function to manage transaction_locks

    /// Set the transaction lock to a specific transaction
    #[instrument(name = "db_set_transaction_lock", level = "trace", skip_all)]
    pub async fn set_transaction_lock(
        &self,
        mutable_input_objects: &[ObjectRef],
        signed_transaction: SignedTransaction,
    ) -> Result<(), SuiError> {
        self.database
            .lock_and_write_transaction(mutable_input_objects, signed_transaction)
            .await
    }

    /// Update state and signals that a new transactions has been processed
    /// to the batch maker service.
    #[instrument(name = "db_update_state", level = "debug", skip_all)]
    async fn update_state(
        &self,
        temporary_store: AuthorityTemporaryStore<AuthorityStore>,
        certificate: &CertifiedTransaction,
        signed_effects: &SignedTransactionEffects,
    ) -> SuiResult {
        let notifier_ticket = self.batch_notifier.ticket()?;
        let seq = notifier_ticket.seq();

        if let Some(indexes) = &self.indexes {
            let inputs: Vec<_> = temporary_store.objects().iter().map(|(_, o)| o).collect();
            let outputs: Vec<_> = temporary_store
                .written()
                .iter()
                .map(|(_, (_, o))| o)
                .collect();
            if let Err(e) = indexes.index_tx(
                certificate.sender_address(),
                &inputs,
                &outputs,
                seq,
                certificate.digest(),
            ) {
                error!("Error indexing certificate: {}", e);
            }
        }

        self.database
            .update_state(temporary_store, certificate, signed_effects, Some(seq))
            .await

        // implicitly we drop the ticket here and that notifies the batch manager
    }

    /// Check whether a shared-object certificate has already been given shared-locks.
    async fn shared_locks_exist(&self, certificate: &CertifiedTransaction) -> SuiResult<bool> {
        let digest = certificate.digest();
        let shared_inputs = certificate.shared_input_objects();
        let shared_locks = self.database.sequenced(digest, shared_inputs)?;
        Ok(shared_locks[0].is_some())
    }

    /// Get a read reference to an object/seq lock
    pub async fn get_transaction_lock(
        &self,
        object_ref: &ObjectRef,
    ) -> Result<Option<SignedTransaction>, SuiError> {
        self.database.get_transaction_envelope(object_ref).await
    }

    // Helper functions to manage certificates

    /// Read from the DB of certificates
    pub async fn read_certificate(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<CertifiedTransaction>, SuiError> {
        self.database.read_certificate(digest)
    }

    pub async fn parent(&self, object_ref: &ObjectRef) -> Option<TransactionDigest> {
        self.database
            .parent(object_ref)
            .expect("TODO: propagate the error")
    }

    pub async fn get_objects(
        &self,
        _objects: &[ObjectID],
    ) -> Result<Vec<Option<Object>>, SuiError> {
        self.database.get_objects(_objects)
    }

    /// Returns all parents (object_ref and transaction digests) that match an object_id, at
    /// any object version, or optionally at a specific version.
    pub async fn get_parent_iterator(
        &self,
        object_id: ObjectID,
        seq: Option<SequenceNumber>,
    ) -> Result<impl Iterator<Item = (ObjectRef, TransactionDigest)> + '_, SuiError> {
        {
            self.database.get_parent_iterator(object_id, seq)
        }
    }

    pub async fn get_latest_parent_entry(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<(ObjectRef, TransactionDigest)>, SuiError> {
        self.database.get_latest_parent_entry(object_id)
    }
}

impl ModuleResolver for AuthorityState {
    type Error = SuiError;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.database.get_module(module_id)
    }
}

#[async_trait]
impl ExecutionState for AuthorityState {
    type Transaction = ConsensusTransaction;
    type Error = SuiError;

    async fn handle_consensus_transaction(
        &self,
        consensus_index: ExecutionIndices,
        transaction: Self::Transaction,
    ) -> Result<Vec<u8>, Self::Error> {
        match transaction {
            ConsensusTransaction::UserTransaction(certificate) => {
                // Ensure the input is a shared object certificate. Remember that Byzantine authorities
                // may input anything into consensus.
                fp_ensure!(
                    certificate.contains_shared_object(),
                    SuiError::NotASharedObjectTransaction
                );

                // If we already executed this transaction, return the signed effects.
                let digest = certificate.digest();
                if self.database.effects_exists(digest)? {
                    debug!(tx_digest =? digest, "Shared-object transaction already executed");
                    let info = self.make_transaction_info(digest).await?;
                    return Ok(bincode::serialize(&info).expect("Failed to serialize tx info"));
                }

                // If we didn't already assigned shared-locks to this transaction, we do it now.
                if !self.shared_locks_exist(&certificate).await? {
                    // Check the certificate. Remember that Byzantine authorities may input anything into
                    // consensus.
                    certificate.verify(&self.committee)?;

                    // Persist the certificate since we are about to lock one or more shared object.
                    // We thus need to make sure someone (if not the client) can continue the protocol.
                    // Also atomically lock the shared objects for this particular transaction and
                    // increment the last consensus index. Note that a single process can ever call
                    // this function and that the last consensus index is also kept in memory. It is
                    // thus ok to only persist now (despite this function may have returned earlier).
                    // In the worst case, the synchronizer of the consensus client will catch up.
                    self.database.persist_certificate_and_lock_shared_objects(
                        *certificate,
                        consensus_index,
                    )?;
                }

                // TODO: This return time is not ideal.
                Ok(Vec::default())
            }
            ConsensusTransaction::Checkpoint(fragment) => {
                let seq = consensus_index;
                if let Some(checkpoint) = &self.checkpoints {
                    checkpoint
                        .lock()
                        .handle_internal_fragment(seq, *fragment)
                        .map_err(|e| SuiError::from(&e.to_string()[..]))?;

                    // NOTE: The method `handle_internal_fragment` is idempotent, so we don't need
                    // to persist the consensus index. If the validator crashes, this transaction
                    // may be resent to the checkpoint logic that will simply ignore it.
                }

                // TODO: This return time is not ideal. The authority submitting the checkpoint fragment
                // is not expecting any reply.
                Ok(Vec::default())
            }
        }
    }

    fn ask_consensus_write_lock(&self) -> bool {
        self.consensus_guardrail.fetch_add(1, Ordering::SeqCst) == 0
    }

    fn release_consensus_write_lock(&self) {
        self.consensus_guardrail.fetch_sub(0, Ordering::SeqCst);
    }

    async fn load_execution_indices(&self) -> Result<ExecutionIndices, Self::Error> {
        self.database.last_consensus_index()
    }
}

impl ExecutionStateError for FragmentInternalError {
    fn node_error(&self) -> bool {
        match self {
            // Those are errors caused by the client. Every authority processing this fragment will
            // deterministically trigger this error.
            Self::Error(..) => false,
            // Those are errors caused by the authority (eg. storage failure). It is not guaranteed
            // that other validators will also trigger it and they may not be deterministic.
            Self::Retry(..) => true,
        }
    }

    fn to_string(&self) -> String {
        match self {
            Self::Error(sui_error) => format!("Failed to process checkpoint fragment {sui_error}"),
            Self::Retry(fragment) => format!("Failed to sequence checkpoint fragment {fragment:?}"),
        }
    }
}
