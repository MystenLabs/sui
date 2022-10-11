// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoints::ConsensusSender;
use crate::{
    authority_batch::{BroadcastReceiver, BroadcastSender},
    checkpoints::CheckpointStore,
    event_handler::EventHandler,
    execution_engine,
    metrics::start_timer,
    query_helpers::QueryHelpers,
    transaction_input_checker,
    transaction_streamer::TransactionStreamer,
};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use chrono::prelude::*;
use fastcrypto::traits::KeyPair;
use move_bytecode_utils::module_cache::SyncModuleCache;
use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};
use move_vm_runtime::{move_vm::MoveVM, native_functions::NativeFunctionTable};
use narwhal_config::{
    Committee as ConsensusCommittee, WorkerCache as ConsensusWorkerCache,
    WorkerId as ConsensusWorkerId,
};
use narwhal_executor::{ExecutionIndices, ExecutionState};
use parking_lot::Mutex;
use prometheus::{
    exponential_buckets, register_histogram_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, Histogram, IntCounter, IntGauge,
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::path::PathBuf;
use std::{
    collections::{HashMap, VecDeque},
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use sui_adapter::adapter;
use sui_config::genesis::Genesis;
use sui_json_rpc_types::{SuiEventEnvelope, SuiTransactionEffects};
use sui_simulator::nondeterministic;
use sui_storage::{
    event_store::{EventStore, EventStoreType, StoredEvent},
    write_ahead_log::{DBTxGuard, TxGuard, WriteAheadLog},
    IndexStore,
};
use sui_types::crypto::{AuthorityKeyPair, NetworkKeyPair};
use sui_types::temporary_store::InnerTemporaryStore;
use sui_types::{
    base_types::*,
    batch::{TxSequenceNumber, UpdateItem},
    committee::Committee,
    crypto::AuthoritySignature,
    error::{SuiError, SuiResult},
    fp_ensure,
    messages::*,
    object::{Object, ObjectFormatOptions, ObjectRead},
    storage::{BackingPackageStore, DeleteKind},
    MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_STATE_OBJECT_ID,
};
use tap::TapFallible;
use thiserror::Error;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc::Sender;
use tracing::Instrument;
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

pub use sui_types::temporary_store::TemporaryStore;

pub mod authority_store_tables;

mod authority_store;
use crate::authority::authority_store_tables::ExecutionIndicesWithHash;
use crate::consensus_adapter::ConsensusListenerMessage;
use crate::epoch::committee_store::CommitteeStore;
use crate::metrics::TaskUtilizationExt;
pub use authority_store::{
    AuthorityStore, GatewayStore, ResolverWrapper, SuiDataStore, UpdateType,
};
use sui_types::committee::EpochId;
use sui_types::messages_checkpoint::{
    CheckpointRequest, CheckpointRequestType, CheckpointResponse,
};
use sui_types::object::{Owner, PastObjectRead};
use sui_types::sui_system_state::SuiSystemState;

pub mod authority_notifier;

pub const MAX_ITEMS_LIMIT: u64 = 1_000;
const BROADCAST_CAPACITY: usize = 10_000;

pub(crate) const MAX_TX_RECOVERY_RETRY: u32 = 3;
type CertTxGuard<'a> = DBTxGuard<'a, CertifiedTransaction>;

pub type ReconfigConsensusMessage = (
    AuthorityKeyPair,
    NetworkKeyPair,
    ConsensusCommittee,
    Vec<(ConsensusWorkerId, NetworkKeyPair)>,
    ConsensusWorkerCache,
);

/// Prometheus metrics which can be displayed in Grafana, queried and alerted on
pub struct AuthorityMetrics {
    tx_orders: IntCounter,
    total_certs: IntCounter,
    total_cert_attempts: IntCounter,
    total_effects: IntCounter,
    signature_errors: IntCounter,
    pub shared_obj_tx: IntCounter,
    tx_already_processed: IntCounter,
    num_input_objs: Histogram,
    num_shared_objects: Histogram,
    batch_size: Histogram,

    prepare_certificate_latency: Histogram,
    commit_certificate_latency: Histogram,
    handle_transaction_latency: Histogram,
    handle_certificate_latency: Histogram,
    handle_node_sync_certificate_latency: Histogram,

    total_consensus_txns: IntCounter,
    skipped_consensus_txns: IntCounter,
    handle_consensus_duration_mcs: IntCounter,
    verify_narwhal_transaction_duration_mcs: IntCounter,

    pub follower_items_streamed: IntCounter,
    pub follower_items_loaded: IntCounter,
    pub follower_connections: IntCounter,
    pub follower_connections_concurrent: IntGauge,
    pub follower_start_seq_num: Histogram,

    // TODO: consolidate these into GossipMetrics
    // (issue: https://github.com/MystenLabs/sui/issues/3926)
    pub gossip_queued_count: IntCounter,
    pub gossip_sync_count: IntCounter,
    pub gossip_task_success_count: IntCounter,
    pub gossip_task_error_count: IntCounter,

    /// Post processing metrics
    post_processing_total_events_emitted: IntCounter,
    post_processing_total_tx_indexed: IntCounter,
    post_processing_total_tx_added_to_streamer: IntCounter,
    post_processing_total_tx_had_event_processed: IntCounter,
    post_processing_total_tx_sent_to_post_processing: IntCounter,
    post_processing_latest_seq_seen: IntGauge,

    /// Batch service metrics
    pub(crate) batch_service_total_tx_broadcasted: IntCounter,
    pub(crate) batch_service_latest_seq_broadcasted: IntGauge,
}

// Override default Prom buckets for positive numbers in 0-50k range
const POSITIVE_INT_BUCKETS: &[f64] = &[
    1., 2., 5., 10., 20., 50., 100., 200., 500., 1000., 2000., 5000., 10000., 20000., 50000.,
];

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1., 2.5, 5., 10., 20., 30., 60., 90.,
];

impl AuthorityMetrics {
    pub fn new(registry: &prometheus::Registry) -> AuthorityMetrics {
        // buckets are: 100, 10k, 1M, 100M, 10B, 1T, 100T, 10Q
        // Safe to unwarp because the values are all valid.
        let follower_seq_num_buckets = exponential_buckets(100., 100., 8).unwrap();
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
            total_cert_attempts: register_int_counter_with_registry!(
                "total_handle_certificate_attempts",
                "Number of calls to handle_certificate",
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
            prepare_certificate_latency: register_histogram_with_registry!(
                "validator_prepare_certificate_latency",
                "Latency of preparing certificate",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            commit_certificate_latency: register_histogram_with_registry!(
                "validator_commit_certificate_latency",
                "Latency of committing certificate",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_transaction_latency: register_histogram_with_registry!(
                "validator_handle_transaction_latency",
                "Latency of committing certificate",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_certificate_latency: register_histogram_with_registry!(
                "validator_handle_certificate_latency",
                "Latency of handling certificate",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_node_sync_certificate_latency: register_histogram_with_registry!(
                "fullnode_handle_node_sync_certificate_latency",
                "Latency of fullnode handling certificate from node sync",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            total_consensus_txns: register_int_counter_with_registry!(
                "total_consensus_txns",
                "Total number of consensus transactions received from narwhal",
                registry,
            )
            .unwrap(),
            skipped_consensus_txns: register_int_counter_with_registry!(
                "skipped_consensus_txns",
                "Total number of consensus transactions skipped",
                registry,
            )
            .unwrap(),
            handle_consensus_duration_mcs: register_int_counter_with_registry!(
                "handle_consensus_duration_mcs",
                "Total duration of handle_consensus_transaction",
                registry,
            )
            .unwrap(),
            verify_narwhal_transaction_duration_mcs: register_int_counter_with_registry!(
                "verify_narwhal_transaction_duration_mcs",
                "Total duration of verify_narwhal_transaction",
                registry,
            )
            .unwrap(),
            follower_items_streamed: register_int_counter_with_registry!(
                "follower_items_streamed",
                "Number of transactions/signed batches streamed to followers",
                registry,
            )
            .unwrap(),
            follower_items_loaded: register_int_counter_with_registry!(
                "follower_items_loaded",
                "Number of transactions/signed batches loaded from db to be streamed to followers",
                registry,
            )
            .unwrap(),
            follower_connections: register_int_counter_with_registry!(
                "follower_connections",
                "Number of follower connections initiated",
                registry,
            )
            .unwrap(),
            follower_connections_concurrent: register_int_gauge_with_registry!(
                "follower_connections_concurrent",
                "Current number of concurrent follower connections",
                registry,
            )
            .unwrap(),
            follower_start_seq_num: register_histogram_with_registry!(
                "follower_start_seq_num",
                "The start seq number this validator receives from fullnodes node_sync/follower process",
                follower_seq_num_buckets,
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
            post_processing_total_events_emitted: register_int_counter_with_registry!(
                "post_processing_total_events_emitted",
                "Total number of events emitted in post processing",
                registry,
            )
            .unwrap(),
            post_processing_total_tx_indexed: register_int_counter_with_registry!(
                "post_processing_total_tx_indexed",
                "Total number of txes indexed in post processing",
                registry,
            )
            .unwrap(),
            post_processing_total_tx_added_to_streamer: register_int_counter_with_registry!(
                "post_processing_total_tx_added_to_streamer",
                "Total number of txes added to tx streamer in post processing",
                registry,
            )
            .unwrap(),
            post_processing_total_tx_had_event_processed: register_int_counter_with_registry!(
                "post_processing_total_tx_had_event_processed",
                "Total number of txes finished event processing in post processing",
                registry,
            )
            .unwrap(),
            post_processing_total_tx_sent_to_post_processing: register_int_counter_with_registry!(
                "post_processing_total_tx_sent_to_post_processing",
                "Total number of txes sent to post processing",
                registry,
            )
            .unwrap(),
            post_processing_latest_seq_seen: register_int_gauge_with_registry!(
                "post_processing_latest_seq_seen",
                "The latest seq number of tx that is seen in post processing",
                registry,
            )
            .unwrap(),
            batch_service_total_tx_broadcasted: register_int_counter_with_registry!(
                "batch_service_total_tx_broadcasted",
                "Total number of txes broadcasted in batch service",
                registry,
            )
            .unwrap(),
            batch_service_latest_seq_broadcasted: register_int_gauge_with_registry!(
                "batch_service_latest_seq_broadcasted",
                "The latest seq number of tx that is broadcasted in batch service",
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

pub struct AuthorityState {
    // Fixed size, static, identity of the authority
    /// The name of this authority.
    pub name: AuthorityName,
    /// The signature key of the authority.
    pub secret: StableSyncAuthoritySigner,

    // Epoch related information.
    /// Committee of this Sui instance.
    pub committee: ArcSwap<Committee>,

    /// Move native functions that are available to invoke
    pub(crate) _native_functions: NativeFunctionTable,
    pub(crate) move_vm: Arc<MoveVM>,

    /// The database
    pub(crate) database: Arc<AuthorityStore>, // TODO: remove pub

    indexes: Option<Arc<IndexStore>>,

    pub module_cache: Arc<SyncModuleCache<ResolverWrapper<AuthorityStore>>>, // TODO: use strategies (e.g. LRU?) to constraint memory usage

    pub event_handler: Option<Arc<EventHandler>>,
    pub transaction_streamer: Option<Arc<TransactionStreamer>>,

    /// The checkpoint store
    pub checkpoints: Arc<Mutex<CheckpointStore>>,

    committee_store: Arc<CommitteeStore>,

    // Structures needed for handling batching and notifications.
    /// The sender to notify of new transactions
    /// and create batches for this authority.
    /// Keep as None if there is no need for this.
    pub(crate) batch_channels: BroadcastSender, // TODO: remove pub

    // The Transaction notifier ticketing engine.
    pub(crate) batch_notifier: Arc<authority_notifier::TransactionNotifier>, // TODO: remove pub

    /// Ensures there can only be a single consensus client is updating the state.
    pub consensus_guardrail: AtomicUsize,

    pub metrics: Arc<AuthorityMetrics>,

    /// A channel to tell consensus to reconfigure.
    tx_reconfigure_consensus: Sender<ReconfigConsensusMessage>,
}

/// The authority state encapsulates all state, drives execution, and ensures safety.
///
/// Note the authority operations can be accessed through a read ref (&) and do not
/// require &mut. Internally a database is synchronized through a mutex lock.
///
/// Repeating valid commands should produce no changes and return no error.
impl AuthorityState {
    pub fn is_fullnode(&self) -> bool {
        !self.committee.load().authority_exists(&self.name)
    }

    /// Get a broadcast receiver for updates
    pub fn subscribe_batch(&self) -> BroadcastReceiver {
        self.batch_channels.subscribe()
    }

    pub fn epoch(&self) -> EpochId {
        self.committee.load().epoch
    }

    pub fn committee_store(&self) -> &Arc<CommitteeStore> {
        &self.committee_store
    }

    async fn handle_transaction_impl(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let transaction_digest = *transaction.digest();
        // Ensure an idempotent answer.
        // If a transaction was signed in a previous epoch, we should no longer reuse it.
        if self
            .database
            .transaction_exists(self.epoch(), &transaction_digest)?
        {
            self.metrics.tx_already_processed.inc();
            let transaction_info = self.make_transaction_info(&transaction_digest).await?;
            return Ok(transaction_info);
        }

        // Validators should never sign an external system transaction.
        fp_ensure!(
            !transaction.signed_data.data.kind.is_system_tx(),
            SuiError::InvalidSystemTransaction
        );

        if self.is_halted() {
            // TODO: Do we want to include the new validator set?
            return Err(SuiError::ValidatorHaltedAtEpochEnd);
        }

        let (_gas_status, input_objects) =
            transaction_input_checker::check_transaction_input(&self.database, &transaction)
                .await?;

        let owned_objects = input_objects.filter_owned_objects();

        let signed_transaction =
            SignedTransaction::new(self.epoch(), transaction, self.name, &*self.secret);

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
        let transaction_digest = *transaction.digest();
        debug!(tx_digest=?transaction_digest, "handle_transaction. Tx data: {:?}", transaction.signed_data.data);
        let _metrics_guard = start_timer(self.metrics.handle_transaction_latency.clone());

        self.metrics.tx_orders.inc();
        // Check the sender's signature.
        transaction.verify().map_err(|e| {
            self.metrics.signature_errors.inc();
            e
        })?;

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

    /// Execute a certificate that's known to have correct effects.
    /// For such certificate, we don't have to wait for consensus to set shared object
    /// locks because we already know the shared object versions based on the effects.
    /// This function can be called either by a fullnode after seeing a quorum of signed effects,
    /// or by a validator after seeing the certificate included by a certified checkpoint.
    /// TODO: down the road, we may want to execute a shared object tx on a validator when f+1
    /// validators have executed it.
    #[instrument(level = "trace", skip_all)]
    pub async fn handle_certificate_with_effects<S>(
        &self,
        certificate: &CertifiedTransaction,
        // NOTE: the caller of this (node_sync) must promise to wait until it
        // knows for sure this tx is finalized, namely, it has seen a
        // CertifiedTransactionEffects or at least f+1 identifical effects
        // digests matching this TransactionEffectsEnvelope, before calling
        // this function, in order to prevent a byzantine validator from
        // giving us incorrect effects.
        // TODO: allow CertifiedTransactionEffects only
        effects: &TransactionEffectsEnvelope<S>,
    ) -> SuiResult {
        let _metrics_guard = start_timer(self.metrics.handle_node_sync_certificate_latency.clone());
        let digest = *certificate.digest();
        debug!(?digest, "handle_certificate_with_effects");
        fp_ensure!(
            effects.effects.transaction_digest == digest,
            SuiError::ErrorWhileProcessingConfirmationTransaction {
                err: "effects/tx digest mismatch".to_string()
            }
        );

        let tx_guard = self.database.acquire_tx_guard(certificate).await?;

        if certificate.contains_shared_object() {
            self.database.acquire_shared_locks_from_effects(
                certificate,
                &effects.effects,
                &tx_guard,
            )?;
        }

        let resp = self
            .process_certificate(tx_guard, certificate, true)
            .await
            .tap_err(|e| debug!(?digest, "process_certificate failed: {}", e))?;

        let expected_effects_digest = effects.digest();
        let observed_effects_digest = resp.signed_effects.as_ref().map(|e| e.digest());
        if observed_effects_digest != Some(expected_effects_digest) {
            error!(
                ?expected_effects_digest,
                ?observed_effects_digest,
                ?effects.effects,
                ?resp.signed_effects,
                input_objects = ?certificate.signed_data.data.input_objects(),
                "Locally executed effects do not match canonical effects!");
        }
        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub async fn handle_certificate(
        &self,
        certificate: CertifiedTransaction,
    ) -> SuiResult<TransactionInfoResponse> {
        let _metrics_guard = start_timer(self.metrics.handle_certificate_latency.clone());
        self.handle_certificate_impl(certificate, false).await
    }

    #[instrument(level = "trace", skip_all)]
    pub async fn handle_certificate_bypass_validator_halt(
        &self,
        certificate: CertifiedTransaction,
    ) -> SuiResult<TransactionInfoResponse> {
        self.handle_certificate_impl(certificate, true).await
    }

    #[instrument(level = "trace", skip_all)]
    async fn handle_certificate_impl(
        &self,
        certificate: CertifiedTransaction,
        bypass_validator_halt: bool,
    ) -> SuiResult<TransactionInfoResponse> {
        self.metrics.total_cert_attempts.inc();
        if self.is_fullnode() {
            return Err(SuiError::GenericStorageError(
                "cannot execute cert without effects on fullnode".into(),
            ));
        }

        let tx_digest = certificate.digest();
        debug!(?tx_digest, "handle_confirmation_transaction");

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
        let span = tracing::debug_span!(
            "validator_acquire_tx_guard",
            ?tx_digest,
            tx_kind = certificate.signed_data.data.kind_as_str()
        );
        let tx_guard = self
            .database
            .acquire_tx_guard(&certificate)
            .instrument(span)
            .await?;

        self.process_certificate(tx_guard, &certificate, bypass_validator_halt)
            .await
            .tap_err(|e| debug!(?tx_digest, "process_certificate failed: {}", e))
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
                    Some(SuiError::SharedObjectLockNotSetError)
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
            SuiError::ObjectErrors {
                errors: lock_errors
            }
        );

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    async fn process_certificate(
        &self,
        tx_guard: CertTxGuard<'_>,
        certificate: &CertifiedTransaction,
        mut bypass_validator_halt: bool,
    ) -> SuiResult<TransactionInfoResponse> {
        let digest = *certificate.digest();
        // The cert could have been processed by a concurrent attempt of the same cert, so check if
        // the effects have already been written.
        if let Some(info) = self.get_tx_info_already_executed(&digest).await? {
            tx_guard.release();
            return Ok(info);
        }

        // We also bypass validator halt if this is the system transaction.
        // TODO: Shared object transactions should also bypass validator halt.
        bypass_validator_halt |= certificate.signed_data.data.kind.is_system_tx();

        if self.is_halted() && !bypass_validator_halt {
            tx_guard.release();
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
        let (inner_temporary_store, signed_effects) =
            match self.prepare_certificate(certificate, digest).await {
                Err(e) => {
                    debug!(name = ?self.name, ?digest, "Error preparing transaction: {}", e);
                    tx_guard.release();
                    return Err(e);
                }
                Ok(res) => res,
            };

        let input_object_count = inner_temporary_store.objects.len();
        let shared_object_count = signed_effects.effects.shared_objects.len();

        // If commit_certificate returns an error, tx_guard will be dropped and the certificate
        // will be persisted in the log for later recovery.
        self.commit_certificate(
            inner_temporary_store,
            certificate,
            &signed_effects,
            bypass_validator_halt,
        )
        .await
        .tap_err(|e| error!(?digest, "commit_certificate failed: {}", e))?;

        // commit_certificate finished, the tx is fully committed to the store.
        tx_guard.commit_tx();

        // Update metrics.
        self.metrics.total_effects.inc();
        self.metrics.total_certs.inc();

        if shared_object_count > 0 {
            self.metrics.shared_obj_tx.inc();
        }

        self.metrics
            .num_input_objs
            .observe(input_object_count as f64);
        self.metrics
            .num_shared_objects
            .observe(shared_object_count as f64);
        self.metrics
            .batch_size
            .observe(certificate.signed_data.data.kind.batch_size() as f64);

        Ok(TransactionInfoResponse {
            signed_transaction: self.database.get_transaction(&digest)?,
            certified_transaction: Some(certificate.clone()),
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
    #[instrument(level = "trace", skip_all)]
    async fn prepare_certificate(
        &self,
        certificate: &CertifiedTransaction,
        transaction_digest: TransactionDigest,
    ) -> SuiResult<(InnerTemporaryStore, SignedTransactionEffects)> {
        let _metrics_guard = start_timer(self.metrics.prepare_certificate_latency.clone());
        let (gas_status, input_objects) =
            transaction_input_checker::check_certificate_input(&self.database, certificate).await?;

        // At this point we need to check if any shared objects need locks,
        // and whether they have them.
        let shared_object_refs = input_objects.filter_shared_objects();
        if !shared_object_refs.is_empty() && !certificate.signed_data.data.kind.is_change_epoch_tx()
        {
            // If the transaction contains shared objects, we need to ensure they have been scheduled
            // for processing by the consensus protocol.
            // There is no need to go through consensus for system transactions that can
            // only be executed at a time when consensus is turned off.
            // TODO: Add some assert here to make sure consensus is indeed off with
            // is_change_epoch_tx.
            self.check_shared_locks(&transaction_digest, &shared_object_refs)
                .await?;
        }

        debug!(
            num_inputs = input_objects.len(),
            "Read inputs for transaction from DB"
        );

        let transaction_dependencies = input_objects.transaction_dependencies();
        let temporary_store =
            TemporaryStore::new(self.database.clone(), input_objects, transaction_digest);
        let (inner_temp_store, effects, _execution_error) =
            execution_engine::execute_transaction_to_effects(
                shared_object_refs,
                temporary_store,
                certificate.signed_data.data.clone(),
                transaction_digest,
                transaction_dependencies,
                &self.move_vm,
                &self._native_functions,
                gas_status,
                self.epoch(),
            );

        // TODO: Distribute gas charge and rebate, which can be retrieved from effects.
        let signed_effects = effects.to_sign_effects(self.epoch(), &self.name, &*self.secret);
        Ok((inner_temp_store, signed_effects))
    }

    pub async fn dry_run_transaction(
        &self,
        transaction: &Transaction,
        transaction_digest: TransactionDigest,
    ) -> Result<SuiTransactionEffects, anyhow::Error> {
        transaction.verify()?;
        let (gas_status, input_objects) =
            transaction_input_checker::check_transaction_input(&self.database, transaction).await?;
        let shared_object_refs = input_objects.filter_shared_objects();

        let transaction_dependencies = input_objects.transaction_dependencies();
        let temporary_store =
            TemporaryStore::new(self.database.clone(), input_objects, transaction_digest);
        let (_inner_temp_store, effects, _execution_error) =
            execution_engine::execute_transaction_to_effects(
                shared_object_refs,
                temporary_store,
                transaction.signed_data.data.clone(),
                transaction_digest,
                transaction_dependencies,
                &self.move_vm,
                &self._native_functions,
                gas_status,
                self.epoch(),
            );
        SuiTransactionEffects::try_from(effects, self.module_cache.as_ref())
    }

    pub fn is_tx_already_executed(&self, digest: &TransactionDigest) -> SuiResult<bool> {
        self.database.effects_exists(digest)
    }

    pub async fn get_tx_info_already_executed(
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

    #[instrument(level = "debug", skip_all, fields(seq = ?seq, tx_digest =? digest), err)]
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
            cert.signed_data
                .data
                .input_objects()?
                .iter()
                .map(|o| o.object_id()),
            effects
                .effects
                .all_mutated()
                .map(|(obj_ref, owner, _kind)| (*obj_ref, *owner)),
            cert.signed_data
                .data
                .move_calls()
                .iter()
                .map(|mc| (mc.package.0, mc.module.clone(), mc.function.clone())),
            seq,
            digest,
            timestamp_ms,
        )
    }

    #[instrument(level = "debug", skip_all, fields(seq=?seq, tx_digest=?digest), err)]
    async fn post_process_one_tx(
        &self,
        seq: TxSequenceNumber,
        digest: &TransactionDigest,
    ) -> SuiResult {
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
            let _ = self
                .index_tx(indexes.as_ref(), seq, digest, &cert, &effects, timestamp_ms)
                .tap_ok(|_| self.metrics.post_processing_total_tx_indexed.inc())
                .tap_err(
                    |e| warn!(tx_digest=?digest, "Post processing - Couldn't index tx: {}", e),
                );
        }

        // Stream transaction
        if let Some(transaction_streamer) = &self.transaction_streamer {
            transaction_streamer.enqueue((cert, effects.clone())).await;
            self.metrics
                .post_processing_total_tx_added_to_streamer
                .inc();
        }

        // Emit events
        if let Some(event_handler) = &self.event_handler {
            event_handler
                .process_events(&effects.effects, timestamp_ms, seq)
                .await
                .tap_ok(|_| self.metrics.post_processing_total_tx_had_event_processed.inc())
                .tap_err(|e| warn!(tx_digest=?digest, "Post processing - Couldn't process events for tx: {}", e))?;

            self.metrics
                .post_processing_total_events_emitted
                .inc_by(effects.effects.events.len() as u64);
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
                        self.metrics.post_processing_latest_seq_seen.set(seq as i64);
                        self.metrics
                            .post_processing_total_tx_sent_to_post_processing
                            .inc();
                        if let Err(e) = self.post_process_one_tx(seq, &digest).await {
                            warn!(?digest, "Couldn't process tx: {}", e);
                        }
                    }
                }
                Err(RecvError::Closed) => {
                    // This shall not happen.
                    error!("run_tx_post_processing_process receiver channel closed. This shall not happen.");
                    break;
                }
                // Today if post processing is too slow we will skip indexing some txes.
                // TODO: https://github.com/MystenLabs/sui/issues/5025
                // Automatically restart the task, which in combination with the todo above,
                // will process any skipped txes and then begin listening for new ones.
                Err(RecvError::Lagged(number_skipped)) => {
                    error!(
                        "run_tx_post_processing_process too slow, skipped {} txes",
                        number_skipped
                    );
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
            ObjectInfoRequestKind::PastObjectInfo(seq)
            | ObjectInfoRequestKind::PastObjectInfoDebug(seq, _) => {
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
                            // Unowned objects have no locks.
                            None
                        } else {
                            self.get_transaction_lock(&object.compute_object_reference())
                                .await?
                        };
                        let layout = match request_layout {
                            Some(format) => {
                                object.get_layout(format, self.module_cache.as_ref())?
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
            ObjectInfoRequestKind::PastObjectInfoDebug(seq, request_layout) => {
                match self.database.get_object_by_key(&request.object_id, seq) {
                    Ok(Some(object)) => {
                        let layout = match request_layout {
                            Some(format) => {
                                object.get_layout(format, self.module_cache.as_ref())?
                            }
                            None => None,
                        };

                        Some(ObjectResponse {
                            object,
                            lock: None,
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
        let length = std::cmp::min(request.length, MAX_ITEMS_LIMIT);

        // If we do not have a start, pick next sequence number that has
        // not yet been put into a batch.
        let start = match request.start {
            Some(start) => start,
            None => {
                self.last_batch()?
                    .expect("Authority is always initialized with a batch")
                    .data()
                    .next_sequence_number
            }
        };
        let end = start + length;

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
                    || dq_transactions[0].0 >= current_batch.data().next_sequence_number
                {
                    break;
                }

                let current_transaction = dq_transactions.pop_front().unwrap();
                items.push_back(UpdateItem::Transaction(current_transaction));
            }

            // Now send the batch
            last_batch_next_seq = current_batch.data().next_sequence_number;
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
        let mut checkpoint_store = self.checkpoints.lock();
        match &request.request_type {
            CheckpointRequestType::AuthenticatedCheckpoint(seq) => {
                checkpoint_store.handle_authenticated_checkpoint(seq, request.detail)
            }
            CheckpointRequestType::CheckpointProposal => {
                checkpoint_store.handle_proposal(request.detail)
            }
        }
    }

    pub fn handle_committee_info_request(
        &self,
        request: &CommitteeInfoRequest,
    ) -> SuiResult<CommitteeInfoResponse> {
        let (epoch, committee) = match request.epoch {
            Some(epoch) => (epoch, self.committee_store.get_committee(&epoch)?),
            None => {
                let committee = self.committee_store.get_latest_committee();
                (committee.epoch, Some(committee))
            }
        };
        Ok(CommitteeInfoResponse {
            epoch,
            committee_info: committee.map(|c| c.voting_rights),
        })
    }

    // TODO: This function takes both committee and genesis as parameter.
    // Technically genesis already contains committee information. Could consider merging them.
    pub async fn new(
        name: AuthorityName,
        secret: StableSyncAuthoritySigner,
        store: Arc<AuthorityStore>,
        committee_store: Arc<CommitteeStore>,
        indexes: Option<Arc<IndexStore>>,
        event_store: Option<Arc<EventStoreType>>,
        transaction_streamer: Option<Arc<TransactionStreamer>>,
        checkpoints: Arc<Mutex<CheckpointStore>>,
        genesis: &Genesis,
        prometheus_registry: &prometheus::Registry,
        tx_reconfigure_consensus: Sender<ReconfigConsensusMessage>,
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
            store
                .bulk_object_insert(&genesis.objects().iter().collect::<Vec<_>>())
                .await
                .expect("Cannot bulk insert genesis objects");
        }

        let committee = committee_store.get_latest_committee();

        let event_handler = event_store.map(|es| Arc::new(EventHandler::new(store.clone(), es)));

        let mut state = AuthorityState {
            name,
            secret,
            committee: ArcSwap::from(Arc::new(committee)),
            _native_functions: native_functions,
            move_vm,
            database: store.clone(),
            indexes,
            // `module_cache` uses a separate in-mem cache from `event_handler`
            // this is because they largely deal with different types of MoveStructs
            module_cache: Arc::new(SyncModuleCache::new(ResolverWrapper(store.clone()))),
            event_handler,
            transaction_streamer,
            checkpoints,
            committee_store,
            batch_channels: tx,
            batch_notifier: Arc::new(
                authority_notifier::TransactionNotifier::new(store.clone())
                    .expect("Notifier cannot start."),
            ),
            consensus_guardrail: AtomicUsize::new(0),
            metrics: Arc::new(AuthorityMetrics::new(prometheus_registry)),
            tx_reconfigure_consensus,
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

        // Ensure it is up-to-date with the latest batches.
        let next_expected_tx = state
            .checkpoints
            .lock()
            .next_transaction_sequence_expected();

        // Get all unprocessed checkpoints
        for (_seq, batch) in state
            .database
            .tables
            .batches
            .iter()
            .skip_to(&next_expected_tx)
            .expect("Seeking batches should never fail at this point")
        {
            let transactions: Vec<(TxSequenceNumber, ExecutionDigests)> = state
                .database
                .tables
                .executed_sequence
                .iter()
                .skip_to(&batch.data().initial_sequence_number)
                .expect("Should never fail to get an iterator")
                .take_while(|(seq, _tx)| *seq < batch.data().next_sequence_number)
                .collect();

            if batch.data().next_sequence_number > next_expected_tx {
                // Update the checkpointing mechanism
                state
                    .checkpoints
                    .lock()
                    .handle_internal_batch(batch.data().next_sequence_number, &transactions)
                    .expect("Should see no errors updating the checkpointing mechanism.");
            }
        }

        state
    }

    // TODO: Technically genesis_committee can be derived from genesis.
    pub async fn new_for_testing(
        genesis_committee: Committee,
        key: &AuthorityKeyPair,
        store_base_path: Option<PathBuf>,
        genesis: Option<&Genesis>,
        consensus_sender: Option<Box<dyn ConsensusSender>>,
        tx_reconfigure_consensus: Sender<ReconfigConsensusMessage>,
    ) -> Self {
        let secret = Arc::pin(key.copy());
        let path = match store_base_path {
            Some(path) => path,
            None => {
                let dir = std::env::temp_dir();
                let path = dir.join(format!("DB_{:?}", nondeterministic!(ObjectID::random())));
                std::fs::create_dir(&path).unwrap();
                path
            }
        };
        let default_genesis = Genesis::get_default_genesis();
        let genesis = match genesis {
            Some(genesis) => genesis,
            None => &default_genesis,
        };

        let store = Arc::new(AuthorityStore::open(&path.join("store"), None));
        let mut checkpoints = CheckpointStore::open(
            &path.join("checkpoints"),
            None,
            &genesis_committee,
            secret.public().into(),
            secret.clone(),
        )
        .expect("Should not fail to open local checkpoint DB");
        if let Some(consensus_sender) = consensus_sender {
            checkpoints
                .set_consensus(consensus_sender)
                .expect("No issues");
        }

        let epochs = Arc::new(CommitteeStore::new(
            path.join("epochs"),
            &genesis_committee,
            None,
        ));
        // add the object_basics module
        AuthorityState::new(
            secret.public().into(),
            secret.clone(),
            store,
            epochs,
            None,
            None,
            None,
            Arc::new(Mutex::new(checkpoints)),
            genesis,
            &prometheus::Registry::new(),
            tx_reconfigure_consensus,
        )
        .await
    }

    // Continually pop in-progress txes from the WAL and try to drive them to completion.
    pub async fn process_tx_recovery_log(&self, limit: Option<usize>) -> SuiResult {
        let mut limit = limit.unwrap_or(usize::MAX);
        while limit > 0 {
            limit -= 1;
            if let Some((cert, tx_guard)) = self.database.wal.read_one_recoverable_tx().await? {
                let digest = tx_guard.tx_id();
                debug!(?digest, "replaying failed cert from log");

                if tx_guard.retry_num() >= MAX_TX_RECOVERY_RETRY {
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

                if let Err(e) = self.process_certificate(tx_guard, &cert, false).await {
                    warn!(?digest, "Failed to process in-progress certificate: {}", e);
                }
            } else {
                break;
            }
        }

        Ok(())
    }

    pub fn checkpoints(&self) -> Arc<Mutex<CheckpointStore>> {
        self.checkpoints.clone()
    }

    pub(crate) fn update_committee(&self, new_committee: Committee) -> SuiResult {
        // TODO: It's likely safer to do the following operations atomically, in case this function
        // gets called from different threads. It cannot happen today, but worth the caution.
        fp_ensure!(
            self.epoch() + 1 == new_committee.epoch,
            SuiError::from("Invalid new epoch to sign and update")
        );

        self.committee_store.insert_new_committee(&new_committee)?;
        // TODO: Do we want to make it possible to subscribe to committee changes?
        self.committee.swap(Arc::new(new_committee));
        Ok(())
    }

    pub(crate) fn is_halted(&self) -> bool {
        self.batch_notifier.is_paused()
    }

    pub(crate) fn halt_validator(&self) {
        self.batch_notifier.pause();
    }

    pub(crate) fn unhalt_validator(&self) {
        self.batch_notifier.unpause();
    }

    pub fn db(&self) -> Arc<AuthorityStore> {
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
                            let layout = object.get_layout(
                                ObjectFormatOptions::default(),
                                self.module_cache.as_ref(),
                            )?;
                            Ok(ObjectRead::Exists(obj_ref, object, layout))
                        }
                    }
                } else {
                    Ok(ObjectRead::Deleted(obj_ref))
                }
            }
        }
    }

    /// This function aims to serve rpc reads on past objects and
    /// we don't expect it to be called for other purposes.
    /// Depending on the object pruning policies that will be enforced in the
    /// future there is no software-level guarantee/SLA to retrieve an object
    /// with an old version even if it exists/existed.
    pub async fn get_past_object_read(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> Result<PastObjectRead, SuiError> {
        // Firstly we see if the object ever exists by getting its latest data
        match self.database.get_latest_parent_entry(*object_id)? {
            None => Ok(PastObjectRead::ObjectNotExists(*object_id)),
            Some((obj_ref, _)) => {
                if version > obj_ref.1 {
                    return Ok(PastObjectRead::VersionTooHigh {
                        object_id: *object_id,
                        asked_version: version,
                        latest_version: obj_ref.1,
                    });
                }
                if version < obj_ref.1 {
                    // Read past objects
                    return Ok(match self.database.get_object_by_key(object_id, version)? {
                        None => PastObjectRead::VersionNotFound(*object_id, version),
                        Some(object) => {
                            let layout = object.get_layout(
                                ObjectFormatOptions::default(),
                                self.module_cache.as_ref(),
                            )?;
                            let obj_ref = object.compute_object_reference();
                            PastObjectRead::VersionFound(obj_ref, object, layout)
                        }
                    });
                }
                // version is equal to the latest seq number this node knows
                if obj_ref.2.is_alive() {
                    match self.database.get_object_by_key(object_id, obj_ref.1)? {
                        None => {
                            error!("Object with in parent_entry is missing from object store, datastore is inconsistent");
                            Err(SuiError::ObjectNotFound {
                                object_id: *object_id,
                            })
                        }
                        Some(object) => {
                            let layout = object.get_layout(
                                ObjectFormatOptions::default(),
                                self.module_cache.as_ref(),
                            )?;
                            Ok(PastObjectRead::VersionFound(obj_ref, object, layout))
                        }
                    }
                } else {
                    Ok(PastObjectRead::ObjectDeleted(obj_ref))
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
        QueryHelpers::get_transaction(&self.database, &digest)
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

    /// Returns at most `limit` events emitted in the given transaction,
    /// emitted within [start_time, end_time) in order of events emitted.
    /// `limit` is capped to EVENT_STORE_QUERY_MAX_LIMIT
    pub async fn get_events_by_transaction(
        &self,
        digest: TransactionDigest,
        limit: usize,
    ) -> Result<Vec<SuiEventEnvelope>, anyhow::Error> {
        let es = self.get_event_store().ok_or(SuiError::NoEventStore)?;
        let stored_events = es.events_by_transaction(digest, limit).await?;
        StoredEvent::into_event_envelopes(stored_events)
    }

    /// Returns at most `limit` events emitted in the given module,
    /// emitted within [start_time, end_time), sorted in in descending time.
    /// `limit` is capped to EVENT_STORE_QUERY_MAX_LIMIT
    pub async fn get_events_by_transaction_module(
        &self,
        module_id: &ModuleId,
        start_time: u64,
        end_time: u64,
        limit: usize,
    ) -> Result<Vec<SuiEventEnvelope>, anyhow::Error> {
        let es = self.get_event_store().ok_or(SuiError::NoEventStore)?;
        let stored_events = es
            .events_by_module_id(start_time, end_time, module_id, limit)
            .await?;
        StoredEvent::into_event_envelopes(stored_events)
    }

    /// Returns at most `limit` events with the given move event struct name, e.g.
    /// `0x2::devnet_nft::MintNFTEvent` or
    /// `0x2::SUI::test_foo<address, vector<u8>>` with type params,
    /// emitted within [start_time, end_time), sorted in in descending time.
    /// `limit` is capped to EVENT_STORE_QUERY_MAX_LIMIT
    pub async fn get_events_by_move_event_struct_name(
        &self,
        move_event_struct_name: &str,
        start_time: u64,
        end_time: u64,
        limit: usize,
    ) -> Result<Vec<SuiEventEnvelope>, anyhow::Error> {
        let es = self.get_event_store().ok_or(SuiError::NoEventStore)?;
        let stored_events = es
            .events_by_move_event_struct_name(start_time, end_time, move_event_struct_name, limit)
            .await?;
        StoredEvent::into_event_envelopes(stored_events)
    }

    /// Returns at most `limit` events associated with the given sender,
    /// emitted within [start_time, end_time), sorted in in descending time.
    /// `limit` is capped to EVENT_STORE_QUERY_MAX_LIMIT
    pub async fn get_events_by_sender(
        &self,
        sender: &SuiAddress,
        start_time: u64,
        end_time: u64,
        limit: usize,
    ) -> Result<Vec<SuiEventEnvelope>, anyhow::Error> {
        let es = self.get_event_store().ok_or(SuiError::NoEventStore)?;
        let stored_events = es
            .events_by_sender(start_time, end_time, sender, limit)
            .await?;
        StoredEvent::into_event_envelopes(stored_events)
    }

    /// Returns at most `limit` events associated with the given recipient,
    /// emitted within [start_time, end_time), sorted in in descending time.
    /// `limit` is capped to EVENT_STORE_QUERY_MAX_LIMIT
    pub async fn get_events_by_recipient(
        &self,
        recipient: &Owner,
        start_time: u64,
        end_time: u64,
        limit: usize,
    ) -> Result<Vec<SuiEventEnvelope>, anyhow::Error> {
        let es = self.get_event_store().ok_or(SuiError::NoEventStore)?;
        let stored_events = es
            .events_by_recipient(start_time, end_time, recipient, limit)
            .await?;
        StoredEvent::into_event_envelopes(stored_events)
    }

    /// Returns at most `limit` events associated with the given object,
    /// emitted within [start_time, end_time), sorted in in descending time.
    /// `limit` is capped to EVENT_STORE_QUERY_MAX_LIMIT
    pub async fn get_events_by_object(
        &self,
        object: &ObjectID,
        start_time: u64,
        end_time: u64,
        limit: usize,
    ) -> Result<Vec<SuiEventEnvelope>, anyhow::Error> {
        let es = self.get_event_store().ok_or(SuiError::NoEventStore)?;
        let stored_events = es
            .events_by_object(start_time, end_time, object, limit)
            .await?;
        StoredEvent::into_event_envelopes(stored_events)
    }

    /// Returns at most `limit` events emitted within [start_time, end_time),
    /// sorted in in descending time.
    /// `limit` is capped to EVENT_STORE_QUERY_MAX_LIMIT
    pub async fn get_events_by_timerange(
        &self,
        start_time: u64,
        end_time: u64,
        limit: usize,
    ) -> Result<Vec<SuiEventEnvelope>, anyhow::Error> {
        let es = self.get_event_store().ok_or(SuiError::NoEventStore)?;
        let stored_events = es.event_iterator(start_time, end_time, limit).await?;
        StoredEvent::into_event_envelopes(stored_events)
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
    #[instrument(level = "trace", skip_all)]
    pub async fn set_transaction_lock(
        &self,
        mutable_input_objects: &[ObjectRef],
        signed_transaction: SignedTransaction,
    ) -> Result<(), SuiError> {
        self.database
            .lock_and_write_transaction(self.epoch(), mutable_input_objects, signed_transaction)
            .await
    }

    /// Update state and signals that a new transactions has been processed
    /// to the batch maker service.
    #[instrument(level = "trace", skip_all)]
    pub(crate) async fn commit_certificate(
        &self,
        inner_temporary_store: InnerTemporaryStore,
        certificate: &CertifiedTransaction,
        signed_effects: &SignedTransactionEffects,
        bypass_validator_halt: bool,
    ) -> SuiResult {
        let _metrics_guard = start_timer(self.metrics.commit_certificate_latency.clone());

        let notifier_ticket = self.batch_notifier.ticket(bypass_validator_halt)?;
        let seq = notifier_ticket.seq();

        let digest = certificate.digest();
        let effects_digest = &signed_effects.digest();
        self.database
            .update_state(
                inner_temporary_store,
                certificate,
                seq,
                signed_effects,
                effects_digest,
            )
            .await
            .tap_ok(|_| {
                debug!(?digest, ?effects_digest, ?self.name, "commit_certificate finished");
            })?;
        // We only notify i.e. update low watermark once database changes are committed
        notifier_ticket.notify();
        Ok(())
    }

    /// Check whether a shared-object certificate has already been given shared-locks.
    pub async fn transaction_shared_locks_exist(
        &self,
        certificate: &CertifiedTransaction,
    ) -> SuiResult<bool> {
        let digest = certificate.digest();
        let shared_inputs = certificate.shared_input_objects();
        let shared_locks = self
            .database
            .get_assigned_object_versions(digest, shared_inputs)?;
        Ok(shared_locks[0].is_some())
    }

    /// Get a read reference to an object/seq lock
    pub async fn get_transaction_lock(
        &self,
        object_ref: &ObjectRef,
    ) -> Result<Option<SignedTransaction>, SuiError> {
        self.database
            .get_object_locking_transaction(object_ref)
            .await
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

    fn verify_narwhal_transaction(&self, certificate: &CertifiedTransaction) -> SuiResult {
        let _timer = self
            .metrics
            .verify_narwhal_transaction_duration_mcs
            .utilization_timer();
        // Ensure the input is a shared object certificate. Remember that Byzantine authorities
        // may input anything into consensus.
        fp_ensure!(
            certificate.contains_shared_object(),
            SuiError::NotASharedObjectTransaction
        );

        // Check the certificate. Remember that Byzantine authorities may input anything into
        // consensus.
        certificate.verify(&self.committee.load())
    }

    pub(crate) async fn handle_consensus_transaction(
        &self,
        // TODO [2533]: use this once integrating Narwhal reconfiguration
        _consensus_output: &narwhal_consensus::ConsensusOutput,
        consensus_index: ExecutionIndicesWithHash,
        transaction: ConsensusTransaction,
    ) -> Result<(), NarwhalHandlerError> {
        self.metrics.total_consensus_txns.inc();
        let _timer = self
            .metrics
            .handle_consensus_duration_mcs
            .utilization_timer();
        let tracking_id = transaction.get_tracking_id();
        match transaction.kind {
            ConsensusTransactionKind::UserTransaction(certificate) => {
                if self
                    .checkpoints
                    .lock()
                    .should_reject_consensus_transaction()
                {
                    return Err(NarwhalHandlerError::ValidatorHalted);
                }
                if self
                    .database
                    .consensus_message_processed(certificate.digest())
                    .await
                    .map_err(NarwhalHandlerError::NodeError)?
                {
                    debug!(
                        ?consensus_index,
                        ?tracking_id,
                        tx_digest = ?certificate.digest(),
                        "handle_consensus_transaction UserTransaction [skip]",
                    );
                    self.metrics.skipped_consensus_txns.inc();
                    return Ok(());
                }
                self.verify_narwhal_transaction(&certificate)
                    .map_err(NarwhalHandlerError::SkipNarwhalTransaction)?;

                debug!(
                    ?consensus_index,
                    ?tracking_id,
                    tx_digest = ?certificate.digest(),
                    "handle_consensus_transaction UserTransaction",
                );

                self.database
                    .persist_certificate_and_lock_shared_objects(*certificate, consensus_index)
                    // todo - potentially more errors from inside here needs to be mapped differently
                    .await
                    .map_err(NarwhalHandlerError::NodeError)?;

                Ok(())
            }
            ConsensusTransactionKind::Checkpoint(fragment) => {
                let cp_seq = fragment.proposer_sequence_number();
                debug!(
                    ?consensus_index,
                    ?cp_seq,
                    "handle_consensus_transaction Checkpoint. Proposer: {}, Other: {}",
                    fragment.proposer.authority(),
                    fragment.other.authority(),
                );

                let committee = self.committee.load();
                fragment
                    .verify(&committee)
                    .map_err(NarwhalHandlerError::SkipNarwhalTransaction)?;

                let mut checkpoint = self.checkpoints.lock();
                checkpoint
                    .handle_internal_fragment(
                        consensus_index.index,
                        *fragment,
                        self.database.clone(),
                        &self.committee.load(),
                    )
                    .map_err(NarwhalHandlerError::NodeError)?;

                // NOTE: The method `handle_internal_fragment` is idempotent, so we don't need
                // to persist the consensus index. If the validator crashes, this transaction
                // may be resent to the checkpoint logic that will simply ignore it.

                // TODO: At this point we should know whether we want to change epoch. If we do,
                // we should have (i) the new committee and (ii) the new keypair of this authority.
                // We then call:
                // ```
                //  self
                //      .tx_reconfigure_consensus
                //      .send((new_keypair, new_committee, new_worker_ids_and_keypairs, new_worker_cache))
                //      .await
                //      .expect("Failed to reconfigure consensus");
                // ```
                let _tx_reconfigure_consensus = &self.tx_reconfigure_consensus;

                Ok(())
            }
        }
    }
}

pub struct ConsensusHandler {
    state: Arc<AuthorityState>,
    sender: Sender<ConsensusListenerMessage>,
    hash: Mutex<u64>,
}

impl ConsensusHandler {
    pub fn new(state: Arc<AuthorityState>, sender: Sender<ConsensusListenerMessage>) -> Self {
        let hash = Mutex::new(0);
        Self {
            state,
            sender,
            hash,
        }
    }

    fn update_hash(&self, index: ExecutionIndices, v: &[u8]) -> ExecutionIndicesWithHash {
        let mut hash_guard = self
            .hash
            .try_lock()
            .expect("Should not have contention on ExecutionState::update_hash");
        let mut hasher = DefaultHasher::new();
        (*hash_guard).hash(&mut hasher);
        v.hash(&mut hasher);
        let hash = hasher.finish();
        *hash_guard = hash;
        // Log hash for every certificate
        if index.next_transaction_index == 1 && index.next_batch_index == 1 {
            debug!(
                "Integrity hash for consensus output at certificate {} is {:016x}",
                index.next_certificate_index, hash
            );
        }
        ExecutionIndicesWithHash { index, hash }
    }
}

#[async_trait]
impl ExecutionState for ConsensusHandler {
    /// This function will be called by Narwhal, after Narwhal sequenced this certificate.
    #[instrument(level = "trace", skip_all)]
    async fn handle_consensus_transaction(
        &self,
        // TODO [2533]: use this once integrating Narwhal reconfiguration
        consensus_output: &narwhal_consensus::ConsensusOutput,
        consensus_index: ExecutionIndices,
        serialized_transaction: Vec<u8>,
    ) {
        let consensus_index = self.update_hash(consensus_index, &serialized_transaction);
        let transaction =
            match bincode::deserialize::<ConsensusTransaction>(&serialized_transaction) {
                Ok(transaction) => transaction,
                Err(err) => {
                    warn!(
                        "Ignoring malformed transaction (failed to deserialize) from {}: {}",
                        consensus_output.certificate.header.author, err
                    );
                    return;
                }
            };
        match self
            .state
            .handle_consensus_transaction(consensus_output, consensus_index, transaction)
            .await
        {
            Ok(()) => {
                if self
                    .sender
                    .send(ConsensusListenerMessage::Processed(serialized_transaction))
                    .await
                    .is_err()
                {
                    warn!("Consensus handler outbound channel closed");
                }
            }
            Err(NarwhalHandlerError::SkipNarwhalTransaction(err)) => {
                warn!(
                    "Ignoring malformed transaction (failed to verify) from {}: {:?}",
                    consensus_output.certificate.header.author, err
                );
            }
            Err(NarwhalHandlerError::ValidatorHalted) => {
                debug!("Validator has stopped accepting consensus transactions, skipping");
            }
            Err(NarwhalHandlerError::NodeError(err)) => {
                Err(err).expect("Unrecoverable error in consensus handler")
            }
        }
    }

    async fn load_execution_indices(&self) -> ExecutionIndices {
        let index_with_hash = self
            .state
            .database
            .last_consensus_index()
            .expect("Failed to load consensus indices");
        *self
            .hash
            .try_lock()
            .expect("Should not have contention on ExecutionState::load_execution_indices") =
            index_with_hash.hash;
        index_with_hash.index
    }
}

// todo - consider deleting this struct
#[derive(Eq, PartialEq, Clone, Debug, Error)]
pub enum NarwhalHandlerError {
    /// Local node error after which it is not safe to continue consuming narwhal stream
    #[error("Local node error {}", 0)]
    NodeError(SuiError),
    /// Transaction from narwhal did not pass verification and should be rejected,
    /// narwhal can continue streaming next transactions to SUI
    #[error("Invalid transaction {}", 0)]
    SkipNarwhalTransaction(SuiError),
    #[error("Validator halted and no longer accepts sequenced transactions")]
    ValidatorHalted,
}
