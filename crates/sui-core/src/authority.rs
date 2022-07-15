// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoints::FragmentInternalError;
use crate::{
    authority_batch::{BroadcastReceiver, BroadcastSender},
    checkpoints::CheckpointStore,
    epoch::EpochInfoLocals,
    event_handler::EventHandler,
    execution_engine,
    query_helpers::QueryHelpers,
    transaction_input_checker,
};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use chrono::prelude::*;
use move_bytecode_utils::module_cache::SyncModuleCache;
use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};
use move_vm_runtime::{move_vm::MoveVM, native_functions::NativeFunctionTable};
use narwhal_executor::ExecutionStateError;
use narwhal_executor::{ExecutionIndices, ExecutionState};
use parking_lot::Mutex;
use prometheus::{
    register_histogram_with_registry, register_int_counter_with_registry, Histogram, IntCounter,
};
use std::ops::Deref;
use std::{
    collections::{HashMap, VecDeque},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
};
use sui_adapter::adapter;
use sui_config::genesis::Genesis;
use sui_storage::{
    event_store::{EventStore, EventStoreType, StoredEvent},
    write_ahead_log::{DBTxGuard, TxGuard, WriteAheadLog},
    IndexStore,
};

use sui_types::{
    base_types::*,
    batch::{TxSequenceNumber, UpdateItem},
    committee::Committee,
    crypto::AuthoritySignature,
    error::{SuiError, SuiResult},
    fp_ensure,
    messages::*,
    object::{Object, ObjectFormatOptions, ObjectRead},
    storage::{BackingPackageStore, DeleteKind, Storage},
    MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_STATE_OBJECT_ID,
};
use tokio::sync::broadcast::error::RecvError;
use tracing::{debug, error, instrument, warn};
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

pub use sui_adapter::temporary_store::TemporaryStore;

mod authority_store;
pub use authority_store::{
    AuthorityStore, GatewayStore, ResolverWrapper, SuiDataStore, UpdateType,
};
use sui_types::messages_checkpoint::{
    CheckpointRequest, CheckpointRequestType, CheckpointResponse,
};
use sui_types::object::Owner;
use sui_types::sui_system_state::SuiSystemState;

pub mod authority_notifier;

pub const MAX_ITEMS_LIMIT: u64 = 100_000;
const BROADCAST_CAPACITY: usize = 10_000;

const MAX_TX_RECOVERY_RETRY: u32 = 3;
type CertTxGuard<'a> = DBTxGuard<'a, CertifiedTransaction>;

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
    pub fn new(registry: &prometheus::Registry) -> AuthorityMetrics {
        Self {
            tx_orders: register_int_counter_with_registry!(
                "total_transaction_orders",
                "Total number of transaction orders",
                registry,
            )
            .unwrap(),
            total_certs: register_int_counter_with_registry!(
                "total_transaction_certificates",
                "Total number of transaction certificates handled",
                registry,
            )
            .unwrap(),
            // total_effects == total transactions finished
            total_effects: register_int_counter_with_registry!(
                "total_transaction_effects",
                "Total number of transaction effects produced",
                registry,
            )
            .unwrap(),
            total_events: register_int_counter_with_registry!(
                "total_events",
                "Total number of events produced",
                registry,
            )
            .unwrap(),
            signature_errors: register_int_counter_with_registry!(
                "total_signature_errors",
                "Number of transaction signature errors",
                registry,
            )
            .unwrap(),
            shared_obj_tx: register_int_counter_with_registry!(
                "num_shared_obj_tx",
                "Number of transactions involving shared objects",
                registry,
            )
            .unwrap(),
            tx_already_processed: register_int_counter_with_registry!(
                "num_tx_already_processed",
                "Number of transaction orders already processed previously",
                registry,
            )
            .unwrap(),
            num_input_objs: register_histogram_with_registry!(
                "num_input_objects",
                "Distribution of number of input TX objects per TX",
                POSITIVE_INT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            num_shared_objects: register_histogram_with_registry!(
                "num_shared_objects",
                "Number of shared input objects per TX",
                POSITIVE_INT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            batch_size: register_histogram_with_registry!(
                "batch_size",
                "Distribution of size of transaction batch",
                POSITIVE_INT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            gossip_queued_count: register_int_counter_with_registry!(
                "gossip_queued_count",
                "Number of digests queued from gossip peers",
                registry,
            )
            .unwrap(),
            gossip_sync_count: register_int_counter_with_registry!(
                "gossip_sync_count",
                "Number of certificates downloaded from gossip peers",
                registry,
            )
            .unwrap(),
            gossip_task_success_count: register_int_counter_with_registry!(
                "gossip_task_success_count",
                "Number of gossip tasks that completed successfully",
                registry,
            )
            .unwrap(),
            gossip_task_error_count: register_int_counter_with_registry!(
                "gossip_task_error_count",
                "Number of gossip tasks that completed with errors",
                registry,
            )
            .unwrap(),
        }
    }
}

/// a Trait object for `signature::Signer` that is:
/// - Pin, i.e. confined to one place in memory (we don't want to copy private keys).
/// - Sync, i.e. can be safely shared between threads.
///
/// Typically instantiated with Box::pin(keypair) where keypair is a `KeyPair`
///
pub type StableSyncAuthoritySigner =
    Pin<Arc<dyn signature::Signer<AuthoritySignature> + Send + Sync>>;

const DEFAULT_QUERY_LIMIT: usize = 1000;

pub struct AuthorityState {
    // Fixed size, static, identity of the authority
    /// The name of this authority.
    pub name: AuthorityName,
    /// The signature key of the authority.
    pub secret: StableSyncAuthoritySigner,

    /// Committee of this Sui instance.
    pub committee: ArcSwap<Committee>,
    /// A global lock to halt all transaction/cert processing.
    #[allow(dead_code)]
    pub(crate) halted: AtomicBool,

    /// Move native functions that are available to invoke
    pub(crate) _native_functions: NativeFunctionTable,
    pub(crate) move_vm: Arc<MoveVM>,

    /// The database
    pub(crate) database: Arc<AuthorityStore>, // TODO: remove pub

    indexes: Option<Arc<IndexStore>>,

    pub module_cache: SyncModuleCache<ResolverWrapper<AuthorityStore>>, // TODO: use strategies (e.g. LRU?) to constraint memory usage

    pub event_handler: Option<Arc<EventHandler>>,

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

    pub metrics: AuthorityMetrics,

    // Cache the latest checkpoint number to avoid expensive locking to access checkpoint store
    latest_checkpoint_num: AtomicU64,
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

        // Validators should never sign an external system transaction.
        fp_ensure!(
            !transaction.data.kind.is_system_tx(),
            SuiError::InvalidSystemTransaction
        );

        if self.halted.load(Ordering::SeqCst) {
            // TODO: Do we want to include the new validator set?
            return Err(SuiError::ValidatorHaltedAtEpochEnd);
        }

        let (_gas_status, input_objects) = transaction_input_checker::check_transaction_input(
            &self.database,
            &transaction,
            &self.metrics.shared_obj_tx,
        )
        .await?;

        let owned_objects = input_objects.filter_owned_objects();

        let signed_transaction = SignedTransaction::new(
            self.committee.load().epoch,
            transaction,
            self.name,
            &*self.secret,
        );

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
        transaction.verify().map_err(|e| {
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

    /// We cannot use handle_certificate in fullnode to execute a certificate because there is no
    /// consensus engine to assign locks for shared objects. Hence we need special handling here.
    pub async fn handle_node_sync_certificate(
        &self,
        certificate: CertifiedTransaction,
        // Signed effects is signed by only one validator, it is not a
        // CertifiedTransactionEffects. The caller of this (node_sync) must promise to
        // wait until it has seen at least f+1 identifical effects digests matching this
        // SignedTransactionEffects before calling this function, in order to prevent a
        // byzantine validator from giving us incorrect effects.
        signed_effects: SignedTransactionEffects,
    ) -> SuiResult {
        let digest = *certificate.digest();
        debug!(?digest, "handle_node_sync_transaction");
        fp_ensure!(
            signed_effects.effects.transaction_digest == digest,
            SuiError::ErrorWhileProcessingConfirmationTransaction {
                err: "effects/tx digest mismatch".to_string()
            }
        );

        let tx_guard = self.acquire_tx_guard(&certificate).await?;

        if certificate.contains_shared_object() {
            self.database
                .acquire_shared_locks_from_effects(&certificate, &signed_effects.effects)?;
        }

        let resp = self.process_certificate(tx_guard, certificate).await?;

        let expected_effects_digest = signed_effects.effects.digest();
        let observed_effects_digest = resp.signed_effects.as_ref().map(|e| e.effects.digest());
        if observed_effects_digest != Some(expected_effects_digest) {
            error!(?expected_effects_digest, ?observed_effects_digest, ?signed_effects, ?resp.signed_effects, "Locally executed effects do not match canonical effects!");
        }
        Ok(())
    }

    pub async fn handle_certificate(
        &self,
        certificate: CertifiedTransaction,
    ) -> SuiResult<TransactionInfoResponse> {
        let digest = certificate.digest();
        debug!(?digest, "handle_confirmation_transaction");

        // This acquires a lock on the tx digest to prevent multiple concurrent executions of the
        // same tx. While we don't need this for safety (tx sequencing is ultimately atomic), it is
        // very common to receive the same tx multiple times simultaneously due to gossip, so we
        // may as well hold the lock and save the cpu time for other requests.
        //
        // Note that this lock has some false contention (since it uses a MutexTable), so you can't
        // assume that different txes can execute concurrently. This is probably the fastest way
        // to do this, since the false contention can be made arbitrarily low (no cost for 1.0 -
        // epsilon of txes) while solutions without false contention have slightly higher cost
        // for every tx.
        let tx_guard = self.acquire_tx_guard(&certificate).await?;

        self.process_certificate(tx_guard, certificate).await
    }

    async fn acquire_tx_guard<'a, 'b>(
        &'a self,
        cert: &'b CertifiedTransaction,
    ) -> SuiResult<CertTxGuard<'a>> {
        let digest = cert.digest();
        match self.database.wal.begin_tx(digest, cert).await? {
            Some(g) => Ok(g),
            None => {
                // If the tx previously errored out without committing, we return an
                // error now as well. We could retry the transaction on behalf of
                // the client right now, but:
                //
                // a) This keeps the normal and recovery paths separated.
                // b) If a client finds a way to create a tx that always fails here,
                //    allowing them to retry it on command could be a DoS channel.
                let err = "previous attempt of transaction resulted in an error - \
                          transaction will be retried offline"
                    .to_owned();
                debug!(?digest, "{}", err);
                Err(SuiError::ErrorWhileProcessingConfirmationTransaction { err })
            }
        }
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
        tx_guard: CertTxGuard<'_>,
        certificate: CertifiedTransaction,
    ) -> SuiResult<TransactionInfoResponse> {
        self.metrics.total_certs.inc();
        let transaction_digest = *certificate.digest();
        // The cert could have been processed by a concurrent attempt of the same cert, so check if
        // the effects have already been written.
        if let Some(info) = self.check_tx_already_executed(&transaction_digest).await? {
            tx_guard.release();
            return Ok(info);
        }

        if self.halted.load(Ordering::SeqCst) && !certificate.data.kind.is_system_tx() {
            tx_guard.release();
            // TODO: Do we want to include the new validator set?
            return Err(SuiError::ValidatorHaltedAtEpochEnd);
        }

        // Check the certificate signatures.
        let committee = &self.committee.load();
        tracing::trace_span!("cert_check_signature")
            .in_scope(|| certificate.verify(committee))
            .map_err(|e| {
                self.metrics.signature_errors.inc();
                e
            })?;

        // Errors originating from prepare_certificate may be transient (failure to read locks) or
        // non-transient (transaction input is invalid, move vm errors). However, all errors from
        // this function occur before we have written anything to the db, so we commit the tx
        // guard and rely on the client to retry the tx (if it was transient).
        let (temporary_store, signed_effects) = match self
            .prepare_certificate(&certificate, transaction_digest)
            .await
        {
            Err(e) => {
                debug!(name = ?self.name, digest = ?transaction_digest, "Error preparing transaction: {}", e);
                tx_guard.release();
                return Err(e);
            }
            Ok(res) => res,
        };

        // If commit_certificate returns an error, tx_guard will be dropped and the certificate
        // will be persisted in the log for later recovery.
        self.commit_certificate(temporary_store, &certificate, &signed_effects)
            .await?;

        // commit_certificate finished, the tx is fully committed to the store.
        tx_guard.commit_tx();

        Ok(TransactionInfoResponse {
            signed_transaction: self.database.get_transaction(&transaction_digest)?,
            certified_transaction: Some(certificate),
            signed_effects: Some(signed_effects),
        })
    }

    /// prepare_certificate validates the transaction input, and executes the certificate,
    /// returning effects, output objects, events, etc.
    ///
    /// It reads state from the db (both owned and shared locks), but it has no side effects.
    ///
    /// It can be generally understood that a failure of prepare_certificate indicates a
    /// non-transient error, e.g. the transaction input is somehow invalid, the correct
    /// locks are not held, etc. However, this is not entirely true, as a transient db read error
    /// may also cause this function to fail.
    #[instrument(level = "debug", name = "prepare_certificate", skip_all)]
    async fn prepare_certificate(
        &self,
        certificate: &CertifiedTransaction,
        transaction_digest: TransactionDigest,
    ) -> SuiResult<(
        TemporaryStore<Arc<AuthorityStore>>,
        SignedTransactionEffects,
    )> {
        let (gas_status, input_objects) = transaction_input_checker::check_transaction_input(
            &self.database,
            certificate,
            &self.metrics.shared_obj_tx,
        )
        .await?;

        // At this point we need to check if any shared objects need locks,
        // and whether they have them.
        let shared_object_refs = input_objects.filter_shared_objects();
        if !shared_object_refs.is_empty() && !certificate.data.kind.is_system_tx() {
            // If the transaction contains shared objects, we need to ensure they have been scheduled
            // for processing by the consensus protocol.
            // There is no need to go through consensus for system transactions that can
            // only be executed at a time when consensus is turned off.
            // TODO: Add some assert here to make sure consensus is indeed off with is_system_tx.
            self.check_shared_locks(&transaction_digest, &shared_object_refs)
                .await?;
        }

        self.metrics
            .num_input_objs
            .observe(input_objects.len() as f64);
        self.metrics
            .num_shared_objects
            .observe(shared_object_refs.len() as f64);
        self.metrics
            .batch_size
            .observe(certificate.data.kind.batch_size() as f64);
        debug!(
            num_inputs = input_objects.len(),
            "Read inputs for transaction from DB"
        );

        let transaction_dependencies = input_objects.transaction_dependencies();
        let mut temporary_store =
            TemporaryStore::new(self.database.clone(), input_objects, transaction_digest);
        let (effects, _execution_error) = execution_engine::execute_transaction_to_effects(
            shared_object_refs,
            &mut temporary_store,
            certificate.data.clone(),
            transaction_digest,
            transaction_dependencies,
            &self.move_vm,
            &self._native_functions,
            gas_status,
            self.committee.load().epoch,
        );

        self.metrics.total_effects.inc();
        self.metrics
            .total_events
            .inc_by(effects.events.len() as u64);

        // TODO: Distribute gas charge and rebate, which can be retrieved from effects.
        let signed_effects =
            effects.to_sign_effects(self.committee.load().epoch, &self.name, &*self.secret);

        Ok((temporary_store, signed_effects))
    }

    pub async fn check_tx_already_executed(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<Option<TransactionInfoResponse>> {
        if self.database.effects_exists(digest)? {
            debug!("Transaction {digest:?} already executed");
            Ok(Some(self.make_transaction_info(digest).await?))
        } else {
            Ok(None)
        }
    }

    fn index_tx(
        &self,
        indexes: &IndexStore,
        seq: TxSequenceNumber,
        digest: &TransactionDigest,
        cert: &CertifiedTransaction,
        effects: &SignedTransactionEffects,
        timestamp_ms: u64,
    ) -> SuiResult {
        indexes.index_tx(
            cert.sender_address(),
            cert.data.input_objects()?.iter().map(|o| o.object_id()),
            effects.effects.mutated_and_created(),
            cert.data
                .move_calls()?
                .iter()
                .map(|mc| (mc.package.0, mc.module.clone(), mc.function.clone())),
            seq,
            digest,
            timestamp_ms,
        )
    }

    async fn process_one_tx(&self, seq: TxSequenceNumber, digest: &TransactionDigest) -> SuiResult {
        // Load cert and effects.
        let info = self.make_transaction_info(digest).await?;
        let (cert, effects) = match info {
            TransactionInfoResponse {
                certified_transaction: Some(cert),
                signed_effects: Some(effects),
                ..
            } => (cert, effects),
            _ => {
                return Err(SuiError::CertificateNotfound {
                    certificate_digest: *digest,
                })
            }
        };

        let timestamp_ms = Self::unixtime_now_ms();

        // Index tx
        if let Some(indexes) = &self.indexes {
            if let Err(e) =
                self.index_tx(indexes.as_ref(), seq, digest, &cert, &effects, timestamp_ms)
            {
                warn!(?digest, "Couldn't index tx: {}", e);
            }
        }

        // Emit events
        if let Some(event_handler) = &self.event_handler {
            let checkpoint_num = self.latest_checkpoint_num.load(Ordering::Relaxed);
            event_handler
                .process_events(&effects.effects, timestamp_ms, seq, checkpoint_num)
                .await?;
        }

        Ok(())
    }

    // TODO: This should persist the last successfully-processed sequence to disk, and upon
    // starting up, look for any sequences in the store since then and process them.
    pub async fn run_tx_post_processing_process(&self) -> SuiResult {
        let mut subscriber = self.subscribe_batch();

        loop {
            match subscriber.recv().await {
                Ok(item) => {
                    if let UpdateItem::Transaction((
                        seq,
                        ExecutionDigests {
                            transaction: digest,
                            ..
                        },
                    )) = item
                    {
                        if let Err(e) = self.process_one_tx(seq, &digest).await {
                            warn!(?digest, "Couldn't process tx: {}", e);
                        }
                    }
                }

                // For both the error cases, we exit the loop which ends this task.
                // TODO: Automatically restart the task, which in combination with the todo above,
                // will process any skipped txes and then begin listening for new ones.
                Err(RecvError::Closed) => {
                    // The service closed the channel.
                    error!("run_tx_post_processing_process receiver channel closed");
                    break;
                }
                Err(RecvError::Lagged(number_skipped)) => {
                    error!(
                        "run_tx_post_processing_process too slow, skipped {} txes",
                        number_skipped
                    );
                    break;
                }
            }
        }

        Ok(())
    }

    pub fn unixtime_now_ms() -> u64 {
        let ts_ms = Utc::now().timestamp_millis();
        u64::try_from(ts_ms).expect("Travelling in time machine")
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
                        let lock = if !object.is_owned_or_quasi_shared() {
                            // Unowned obejcts have no locks.
                            None
                        } else {
                            self.get_transaction_lock(&object.compute_object_reference())
                                .await?
                        };
                        let layout = match request_layout {
                            Some(format) => object.get_layout(format, &self.module_cache)?,
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

        // If we do not have a start, pick next sequence number that has
        // not yet been put into a batch.
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

    pub fn handle_checkpoint_request(
        &self,
        request: &CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        let mut checkpoint_store = self
            .checkpoints
            .as_ref()
            .ok_or(SuiError::UnsupportedFeatureError {
                error: "Checkpoint not supported".to_owned(),
            })?
            .lock();
        match &request.request_type {
            CheckpointRequestType::LatestCheckpointProposal => {
                checkpoint_store.handle_latest_proposal(request)
            }
            CheckpointRequestType::PastCheckpoint(seq) => {
                checkpoint_store.handle_past_checkpoint(request.detail, *seq)
            }
        }
    }

    // TODO: This function takes both committee and genesis as parameter.
    // Technically genesis already contains committee information. Could consider merging them.
    pub async fn new(
        committee: Committee,
        name: AuthorityName,
        secret: StableSyncAuthoritySigner,
        store: Arc<AuthorityStore>,
        indexes: Option<Arc<IndexStore>>,
        event_store: Option<Arc<EventStoreType>>,
        checkpoints: Option<Arc<Mutex<CheckpointStore>>>,
        genesis: &Genesis,
        prometheus_registry: &prometheus::Registry,
    ) -> Self {
        let (tx, _rx) = tokio::sync::broadcast::channel(BROADCAST_CAPACITY);
        let native_functions =
            sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
        let move_vm = Arc::new(
            adapter::new_move_vm(native_functions.clone())
                .expect("We defined natives to not fail here"),
        );
        // TODO: update this function to not take genesis, committee if store already exists
        // Only initialize an empty database.
        if store
            .database_is_empty()
            .expect("Database read should not fail.")
        {
            store
                .bulk_object_insert(&genesis.objects().iter().collect::<Vec<_>>())
                .await
                .expect("Cannot bulk insert genesis objects");

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

        let event_handler = event_store.map(|es| Arc::new(EventHandler::new(store.clone(), es)));

        let mut state = AuthorityState {
            name,
            secret,
            committee: ArcSwap::from(Arc::new(current_epoch_info.committee)),
            halted: AtomicBool::new(current_epoch_info.validator_halted),
            _native_functions: native_functions,
            move_vm,
            database: store.clone(),
            indexes,
            // `module_cache` uses a separate in-mem cache from `event_handler`
            // this is because they largely deal with different types of MoveStructs
            module_cache: SyncModuleCache::new(ResolverWrapper(store.clone())),
            event_handler,
            checkpoints,
            batch_channels: tx,
            batch_notifier: Arc::new(
                authority_notifier::TransactionNotifier::new(store.clone())
                    .expect("Notifier cannot start."),
            ),
            consensus_guardrail: AtomicUsize::new(0),
            metrics: AuthorityMetrics::new(prometheus_registry),
            latest_checkpoint_num: AtomicU64::new(0),
        };

        // Process tx recovery log first, so that the batch and checkpoint recovery (below)
        // don't observe partially-committed txes.
        state
            .process_tx_recovery_log(None)
            .await
            .expect("Could not fully process recovery log at startup!");

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
                let transactions: Vec<(TxSequenceNumber, ExecutionDigests)> = state
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

    // Continually pop in-progress txes from the WAL and try to drive them to completion.
    async fn process_tx_recovery_log(&self, limit: Option<usize>) -> SuiResult {
        let mut limit = limit.unwrap_or(usize::max_value());
        while limit > 0 {
            limit -= 1;
            if let Some(tx_guard) = self.database.wal.read_one_recoverable_tx().await {
                let digest = tx_guard.tx_id();

                let (cert, retry_count) = self.database.wal.get_tx_data(&tx_guard)?;

                if retry_count >= MAX_TX_RECOVERY_RETRY {
                    // This tx will be only partially executed, however the store will be in a safe
                    // state. We will simply never reach eventual consistency for this TX.
                    // TODO: Should we revert the tx entirely? I'm not sure the effort is
                    // warranted, since the only way this can happen is if we are repeatedly
                    // failing to write to the db, in which case a revert probably won't succeed
                    // either.
                    error!(
                        ?digest,
                        "Abandoning in-progress TX after {} retries.", MAX_TX_RECOVERY_RETRY
                    );
                    // prevent the tx from going back into the recovery list again.
                    tx_guard.release();
                    continue;
                }

                if let Err(e) = self.process_certificate(tx_guard, cert).await {
                    warn!(?digest, "Failed to process in-progress certificate: {}", e);
                }
            } else {
                break;
            }
        }

        Ok(())
    }

    pub fn checkpoints(&self) -> Option<Arc<Mutex<CheckpointStore>>> {
        self.checkpoints.clone()
    }

    pub(crate) fn insert_new_epoch_info(&self, new_committee: &Committee) -> SuiResult {
        let current_epoch_info = self.database.get_last_epoch_info()?;
        fp_ensure!(
            current_epoch_info.committee.epoch <= new_committee.epoch,
            SuiError::InconsistentEpochState {
                error: "Trying to insert an old epoch entry".to_owned()
            }
        );
        self.database.insert_new_epoch_info(EpochInfoLocals {
            committee: new_committee.clone(),
            validator_halted: true,
        })?;
        self.committee.store(Arc::new(new_committee.clone()));
        Ok(())
    }

    pub(crate) fn unhalt_validator(&self) -> SuiResult {
        let epoch_info = self.database.get_last_epoch_info()?;
        assert_eq!(
            &epoch_info.committee,
            self.committee.load().clone().deref(),
            "About to being new epoch, however current committee differs from epoch store"
        );
        self.database.insert_new_epoch_info(EpochInfoLocals {
            committee: self.clone_committee(),
            validator_halted: false,
        })?;
        self.halted.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub(crate) fn db(&self) -> Arc<AuthorityStore> {
        self.database.clone()
    }

    pub fn clone_committee(&self) -> Committee {
        self.committee.load().clone().deref().clone()
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

    pub async fn get_sui_system_state_object(&self) -> SuiResult<SuiSystemState> {
        self.database.get_sui_system_state_object()
    }

    pub async fn get_object_read(&self, object_id: &ObjectID) -> Result<ObjectRead, SuiError> {
        match self.database.get_latest_parent_entry(*object_id)? {
            None => Ok(ObjectRead::NotExists(*object_id)),
            Some((obj_ref, _)) => {
                if obj_ref.2.is_alive() {
                    match self.database.get_object_by_key(object_id, obj_ref.1)? {
                        None => {
                            error!("Object with in parent_entry is missing from object store, datastore is inconsistent");
                            Err(SuiError::ObjectNotFound {
                                object_id: *object_id,
                            })
                        }
                        Some(object) => {
                            let layout = object
                                .get_layout(ObjectFormatOptions::default(), &self.module_cache)?;
                            Ok(ObjectRead::Exists(obj_ref, object, layout))
                        }
                    }
                } else {
                    Ok(ObjectRead::Deleted(obj_ref))
                }
            }
        }
    }

    pub fn get_owner_objects(&self, owner: Owner) -> SuiResult<Vec<ObjectInfo>> {
        self.database.get_owner_objects(owner)
    }

    pub fn get_total_transaction_number(&self) -> Result<u64, anyhow::Error> {
        QueryHelpers::get_total_transaction_number(&self.database)
    }

    pub fn get_transactions_in_range(
        &self,
        start: TxSequenceNumber,
        end: TxSequenceNumber,
    ) -> Result<Vec<(TxSequenceNumber, TransactionDigest)>, anyhow::Error> {
        QueryHelpers::get_transactions_in_range(&self.database, start, end)
    }

    pub fn get_recent_transactions(
        &self,
        count: u64,
    ) -> Result<Vec<(TxSequenceNumber, TransactionDigest)>, anyhow::Error> {
        QueryHelpers::get_recent_transactions(&self.database, count)
    }

    pub async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<(CertifiedTransaction, TransactionEffects), anyhow::Error> {
        QueryHelpers::get_transaction(&self.database, digest)
    }

    fn get_indexes(&self) -> SuiResult<Arc<IndexStore>> {
        match &self.indexes {
            Some(i) => Ok(i.clone()),
            None => Err(SuiError::UnsupportedFeatureError {
                error: "extended object indexing is not enabled on this server".into(),
            }),
        }
    }

    pub async fn get_transactions_by_move_function(
        &self,
        package: ObjectID,
        module: Option<String>,
        function: Option<String>,
    ) -> Result<Vec<(TxSequenceNumber, TransactionDigest)>, anyhow::Error> {
        Ok(self
            .get_indexes()?
            .get_transactions_by_move_function(package, module, function)?)
    }

    pub async fn get_timestamp_ms(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<u64>, anyhow::Error> {
        Ok(self.get_indexes()?.get_timestamp_ms(digest)?)
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

    /// Returns a full handle to the event store, including inserts... so be careful!
    fn get_event_store(&self) -> Option<Arc<EventStoreType>> {
        self.event_handler
            .as_ref()
            .map(|handler| handler.event_store.clone())
    }

    /// Returns a set of events corresponding to a given transaction, in order events were emitted
    pub async fn get_events_for_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<Vec<StoredEvent>, SuiError> {
        let es = self.get_event_store().ok_or(SuiError::NoEventStore)?;
        es.events_for_transaction(digest).await
    }

    /// Returns a whole set of events for a range of time
    pub async fn get_events_for_timerange(
        &self,
        start_time: u64,
        end_time: u64,
        limit: Option<usize>,
    ) -> Result<Vec<StoredEvent>, SuiError> {
        let es = self.get_event_store().ok_or(SuiError::NoEventStore)?;
        es.event_iterator(start_time, end_time, limit.unwrap_or(DEFAULT_QUERY_LIMIT))
            .await
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
    async fn make_transaction_info(
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
    #[instrument(name = "commit_certificate", level = "debug", skip_all)]
    pub(crate) async fn commit_certificate(
        &self,
        temporary_store: TemporaryStore<Arc<AuthorityStore>>,
        certificate: &CertifiedTransaction,
        signed_effects: &SignedTransactionEffects,
    ) -> SuiResult {
        if self.halted.load(Ordering::SeqCst) && !certificate.data.kind.is_system_tx() {
            // TODO: Here we should allow consensus transaction to continue.
            // TODO: Do we want to include the new validator set?
            return Err(SuiError::ValidatorHaltedAtEpochEnd);
        }

        let notifier_ticket = self.batch_notifier.ticket()?;
        let seq = notifier_ticket.seq();

        let effects_digest = &signed_effects.effects.digest();
        let res = self
            .database
            .update_state(
                temporary_store,
                certificate,
                seq,
                signed_effects,
                effects_digest,
            )
            .await;

        let digest = certificate.digest();

        debug!(?digest, ?effects_digest, "commit_certificate finished");

        res

        // implicitly we drop the ticket here and that notifies the batch manager
    }

    /// Check whether a shared-object certificate has already been given shared-locks.
    pub async fn transaction_shared_locks_exist(
        &self,
        certificate: &CertifiedTransaction,
    ) -> SuiResult<bool> {
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

#[async_trait]
impl ExecutionState for AuthorityState {
    type Transaction = ConsensusTransaction;
    type Error = SuiError;

    /// This function will be called by Narwhal, after Narwhal sequenced this certificate.
    #[instrument(name = "handle_consensus_transaction", level = "debug", skip_all)]
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

                // Check if we already assigned locks to the shared objects.
                let shared_locks = self.transaction_shared_locks_exist(&certificate).await?;

                // If we already executed this transaction, return the signed effects.
                // This is not an optimization, and is critical for safety. It is to ensure that
                // we don't end up re-assigning shared object locks after they are unlocked when
                // the transaction was committed.
                let digest = certificate.digest();
                if let Some(response) = self.check_tx_already_executed(digest).await? {
                    return Ok(bincode::serialize(&response).expect("Failed to serialize tx info"));
                }

                // If we didn't already assigned shared-locks to this transaction, we do it now.
                if !shared_locks {
                    // Check the certificate. Remember that Byzantine authorities may input anything into
                    // consensus.
                    certificate.verify(&self.committee.load())?;

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
                    let mut checkpoint = checkpoint.lock();
                    checkpoint
                        .handle_internal_fragment(
                            seq,
                            *fragment,
                            &self.committee.load(),
                            self.database.clone(),
                        )
                        .map_err(|e| SuiError::from(&e.to_string()[..]))?;

                    // NOTE: The method `handle_internal_fragment` is idempotent, so we don't need
                    // to persist the consensus index. If the validator crashes, this transaction
                    // may be resent to the checkpoint logic that will simply ignore it.

                    // Cache the next checkpoint number if it changes
                    self.latest_checkpoint_num
                        .store(checkpoint.next_checkpoint(), Ordering::Relaxed);
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
