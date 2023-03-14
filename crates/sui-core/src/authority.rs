// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;
use std::{collections::HashMap, fs, pin::Pin, sync::Arc};

use anyhow::anyhow;
use arc_swap::{ArcSwap, Guard};
use chrono::prelude::*;
use fastcrypto::encoding::Base58;
use fastcrypto::encoding::Encoding;
use fastcrypto::traits::KeyPair;
use itertools::Itertools;
use move_binary_format::compatibility::Compatibility;
use move_binary_format::CompiledModule;
use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::ModuleId;
use parking_lot::Mutex;
use prometheus::{
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_with_registry, Histogram, IntCounter,
    IntCounterVec, IntGauge, Registry,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tap::TapFallible;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::oneshot;
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tracing::{debug, error, error_span, info, instrument, trace, warn, Instrument};

pub use authority_notify_read::EffectsNotifyRead;
pub use authority_store::{AuthorityStore, ResolverWrapper, UpdateType};
use mysten_metrics::spawn_monitored_task;
use narwhal_config::{
    Committee as ConsensusCommittee, WorkerCache as ConsensusWorkerCache,
    WorkerId as ConsensusWorkerId,
};
use shared_crypto::intent::{Intent, IntentScope};
use sui_adapter::execution_engine;
use sui_adapter::{adapter, execution_mode};
use sui_config::genesis::Genesis;
use sui_config::node::{AuthorityStorePruningConfig, DBCheckpointConfig};
use sui_json_rpc_types::{
    DevInspectResults, DryRunTransactionResponse, SuiEvent, SuiEventEnvelope, SuiMoveValue,
    SuiTransactionEvents,
};
use sui_macros::{fail_point, nondeterministic};
use sui_protocol_config::{ProtocolConfig, SupportedProtocolVersions};
use sui_storage::indexes::{ObjectIndexChanges, MAX_GET_OWNED_OBJECT_SIZE};
use sui_storage::write_ahead_log::WriteAheadLog;
use sui_storage::{
    event_store::EventStoreType,
    write_ahead_log::{DBTxGuard, TxGuard},
    IndexStore,
};
use sui_types::committee::{EpochId, ProtocolVersion};
use sui_types::crypto::AuthoritySignInfo;
use sui_types::crypto::{sha3_hash, AuthorityKeyPair, NetworkKeyPair, Signer};
use sui_types::digests::TransactionEventsDigest;
use sui_types::dynamic_field::{DynamicFieldInfo, DynamicFieldName, DynamicFieldType, Field};
use sui_types::error::UserInputError;
use sui_types::event::{Event, EventID};
use sui_types::gas::{GasCostSummary, GasPrice, SuiCostTable, SuiGasStatus};
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointContentsDigest, CheckpointDigest, CheckpointSequenceNumber,
    CheckpointSummary, CheckpointTimestamp, VerifiedCheckpoint,
};
use sui_types::messages_checkpoint::{CheckpointRequest, CheckpointResponse};
use sui_types::move_package::MovePackage;
use sui_types::object::{MoveObject, Owner, PastObjectRead, OBJECT_START_VERSION};
use sui_types::parse_sui_struct_tag;
use sui_types::query::{EventQuery, TransactionFilter};
use sui_types::storage::{ObjectKey, WriteKind};
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::temporary_store::InnerTemporaryStore;
pub use sui_types::temporary_store::TemporaryStore;
use sui_types::MOVE_STDLIB_OBJECT_ID;
use sui_types::SUI_FRAMEWORK_OBJECT_ID;
use sui_types::{
    base_types::*,
    committee::Committee,
    crypto::AuthoritySignature,
    error::{SuiError, SuiResult},
    fp_ensure,
    messages::*,
    object::{Object, ObjectFormatOptions, ObjectRead},
    SUI_FRAMEWORK_ADDRESS,
};
use typed_store::Map;

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
pub use crate::authority::authority_per_epoch_store::{
    VerifiedCertificateCache, VerifiedCertificateCacheMetrics,
};
use crate::authority::authority_per_epoch_store_pruner::AuthorityPerEpochStorePruner;
use crate::authority::authority_store::{ExecutionLockReadGuard, InputKey, ObjectLockStatus};
use crate::authority::authority_store_pruner::AuthorityStorePruner;
use crate::authority::epoch_start_configuration::EpochStartConfigTrait;
use crate::authority::epoch_start_configuration::EpochStartConfiguration;
use crate::checkpoints::CheckpointStore;
use crate::epoch::committee_store::CommitteeStore;
use crate::epoch::epoch_metrics::EpochMetrics;
use crate::execution_driver::execution_process;
use crate::module_cache_metrics::ResolverMetrics;
use crate::stake_aggregator::StakeAggregator;
use crate::{
    event_handler::EventHandler, transaction_input_checker, transaction_manager::TransactionManager,
};

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

#[cfg(test)]
#[path = "unit_tests/batch_verification_tests.rs"]
mod batch_verification_tests;

pub mod authority_per_epoch_store;
pub mod authority_per_epoch_store_pruner;

pub mod authority_store_pruner;
pub mod authority_store_tables;
pub mod authority_store_types;
pub mod epoch_start_configuration;

pub(crate) mod authority_notify_read;
pub(crate) mod authority_store;

pub(crate) const MAX_TX_RECOVERY_RETRY: u32 = 3;

// Reject a transaction if the number of certificates pending execution is above this threshold.
// 20000 = 10k TPS * 2s resident time in transaction manager.
pub(crate) const MAX_EXECUTION_QUEUE_LENGTH: usize = 20_000;

// Reject a transaction if the number of pending transactions depending on the object
// is above the threshold.
pub(crate) const MAX_PER_OBJECT_EXECUTION_QUEUE_LENGTH: usize = 1000;

type CertTxGuard<'a> =
    DBTxGuard<'a, TrustedExecutableTransaction, (InnerTemporaryStore, TransactionEffects)>;

pub type ReconfigConsensusMessage = (
    AuthorityKeyPair,
    NetworkKeyPair,
    ConsensusCommittee,
    Vec<(ConsensusWorkerId, NetworkKeyPair)>,
    ConsensusWorkerCache,
);

pub type VerifiedTransactionBatch = Vec<(
    VerifiedTransaction,
    TransactionEffects,
    TransactionEvents,
    Option<(EpochId, CheckpointSequenceNumber)>,
)>;

/// Prometheus metrics which can be displayed in Grafana, queried and alerted on
pub struct AuthorityMetrics {
    tx_orders: IntCounter,
    total_certs: IntCounter,
    total_cert_attempts: IntCounter,
    total_effects: IntCounter,
    pub shared_obj_tx: IntCounter,
    tx_already_processed: IntCounter,
    num_input_objs: Histogram,
    num_shared_objects: Histogram,
    batch_size: Histogram,

    handle_transaction_latency: Histogram,
    execute_certificate_latency: Histogram,
    execute_certificate_with_effects_latency: Histogram,
    internal_execution_latency: Histogram,
    prepare_certificate_latency: Histogram,
    commit_certificate_latency: Histogram,
    db_checkpoint_latency: Histogram,

    pub(crate) transaction_manager_num_enqueued_certificates: IntCounterVec,
    pub(crate) transaction_manager_num_missing_objects: IntGauge,
    pub(crate) transaction_manager_num_pending_certificates: IntGauge,
    pub(crate) transaction_manager_num_executing_certificates: IntGauge,
    pub(crate) transaction_manager_num_ready: IntGauge,

    pub(crate) execution_driver_executed_transactions: IntCounter,

    pub(crate) skipped_consensus_txns: IntCounter,

    /// Post processing metrics
    post_processing_total_events_emitted: IntCounter,
    post_processing_total_tx_indexed: IntCounter,
    post_processing_total_tx_had_event_processed: IntCounter,

    pending_notify_read: IntGauge,

    /// Consensus handler metrics
    pub consensus_handler_processed_batches: IntCounter,
    pub consensus_handler_processed_bytes: IntCounter,
    pub consensus_handler_processed: IntCounterVec,
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
            handle_transaction_latency: register_histogram_with_registry!(
                "authority_state_handle_transaction_latency",
                "Latency of handling transactions",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            execute_certificate_latency: register_histogram_with_registry!(
                "authority_state_execute_certificate_latency",
                "Latency of executing certificates, including waiting for inputs",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            execute_certificate_with_effects_latency: register_histogram_with_registry!(
                "authority_state_execute_certificate_with_effects_latency",
                "Latency of executing certificates with effects, including waiting for inputs",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            internal_execution_latency: register_histogram_with_registry!(
                "authority_state_internal_execution_latency",
                "Latency of actual certificate executions",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            prepare_certificate_latency: register_histogram_with_registry!(
                "authority_state_prepare_certificate_latency",
                "Latency of executing certificates, before committing the results",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            commit_certificate_latency: register_histogram_with_registry!(
                "authority_state_commit_certificate_latency",
                "Latency of committing certificate execution results",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            db_checkpoint_latency: register_histogram_with_registry!(
                "db_checkpoint_latency",
                "Latency of checkpointing dbs",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            transaction_manager_num_enqueued_certificates: register_int_counter_vec_with_registry!(
                "transaction_manager_num_enqueued_certificates",
                "Current number of certificates enqueued to TransactionManager",
                &["result"],
                registry,
            )
            .unwrap(),
            transaction_manager_num_missing_objects: register_int_gauge_with_registry!(
                "transaction_manager_num_missing_objects",
                "Current number of missing objects in TransactionManager",
                registry,
            )
            .unwrap(),
            transaction_manager_num_pending_certificates: register_int_gauge_with_registry!(
                "transaction_manager_num_pending_certificates",
                "Number of certificates pending in TransactionManager, with at least 1 missing input object",
                registry,
            )
            .unwrap(),
            transaction_manager_num_executing_certificates: register_int_gauge_with_registry!(
                "transaction_manager_num_executing_certificates",
                "Number of executing certificates, including queued and actually running certificates",
                registry,
            )
            .unwrap(),
            transaction_manager_num_ready: register_int_gauge_with_registry!(
                "transaction_manager_num_ready",
                "Number of ready transactions in TransactionManager",
                registry,
            )
            .unwrap(),
            execution_driver_executed_transactions: register_int_counter_with_registry!(
                "execution_driver_executed_transactions",
                "Cumulative number of transaction executed by execution driver",
                registry,
            )
            .unwrap(),
            skipped_consensus_txns: register_int_counter_with_registry!(
                "skipped_consensus_txns",
                "Total number of consensus transactions skipped",
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
            post_processing_total_tx_had_event_processed: register_int_counter_with_registry!(
                "post_processing_total_tx_had_event_processed",
                "Total number of txes finished event processing in post processing",
                registry,
            )
            .unwrap(),
            pending_notify_read: register_int_gauge_with_registry!(
                "pending_notify_read",
                "Pending notify read requests",
                registry,
            )
                .unwrap(),
            consensus_handler_processed_batches: register_int_counter_with_registry!(
                "consensus_handler_processed_batches",
                "Number of batches processed by consensus_handler",
                registry
            ).unwrap(),
            consensus_handler_processed_bytes: register_int_counter_with_registry!(
                "consensus_handler_processed_bytes",
                "Number of bytes processed by consensus_handler",
                registry
            ).unwrap(),
            consensus_handler_processed: register_int_counter_vec_with_registry!("consensus_handler_processed", "Number of transactions processed by consensus handler", &["class"], registry)
                .unwrap()
        }
    }
}

/// a Trait object for `Signer` that is:
/// - Pin, i.e. confined to one place in memory (we don't want to copy private keys).
/// - Sync, i.e. can be safely shared between threads.
///
/// Typically instantiated with Box::pin(keypair) where keypair is a `KeyPair`
///
pub type StableSyncAuthoritySigner = Pin<Arc<dyn Signer<AuthoritySignature> + Send + Sync>>;

pub struct AuthorityState {
    // Fixed size, static, identity of the authority
    /// The name of this authority.
    pub name: AuthorityName,
    /// The signature key of the authority.
    pub secret: StableSyncAuthoritySigner,

    /// The database
    pub database: Arc<AuthorityStore>, // TODO: remove pub

    epoch_store: ArcSwap<AuthorityPerEpochStore>,

    indexes: Option<Arc<IndexStore>>,

    pub event_handler: Option<Arc<EventHandler>>,
    pub(crate) checkpoint_store: Arc<CheckpointStore>,

    committee_store: Arc<CommitteeStore>,

    /// Manages pending certificates and their missing input objects.
    transaction_manager: Arc<TransactionManager>,

    /// Shuts down the execution task. Used only in testing.
    #[allow(unused)]
    tx_execution_shutdown: Mutex<Option<oneshot::Sender<()>>>,

    pub metrics: Arc<AuthorityMetrics>,
    _objects_pruner: AuthorityStorePruner,
    _authority_per_epoch_pruner: AuthorityPerEpochStorePruner,

    /// Take db checkpoints af different dbs
    db_checkpoint_config: DBCheckpointConfig,
}

/// The authority state encapsulates all state, drives execution, and ensures safety.
///
/// Note the authority operations can be accessed through a read ref (&) and do not
/// require &mut. Internally a database is synchronized through a mutex lock.
///
/// Repeating valid commands should produce no changes and return no error.
impl AuthorityState {
    pub fn is_validator(&self, epoch_store: &AuthorityPerEpochStore) -> bool {
        epoch_store.committee().authority_exists(&self.name)
    }

    pub fn is_fullnode(&self, epoch_store: &AuthorityPerEpochStore) -> bool {
        !self.is_validator(epoch_store)
    }

    pub fn committee_store(&self) -> &Arc<CommitteeStore> {
        &self.committee_store
    }

    pub fn clone_committee_store(&self) -> Arc<CommitteeStore> {
        self.committee_store.clone()
    }

    /// This is a private method and should be kept that way. It doesn't check whether
    /// the provided transaction is a system transaction, and hence can only be called internally.
    async fn handle_transaction_impl(
        &self,
        transaction: VerifiedTransaction,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<VerifiedSignedTransaction> {
        let execution_queue_len = self.transaction_manager.execution_queue_len();
        if execution_queue_len >= MAX_EXECUTION_QUEUE_LENGTH {
            return Err(SuiError::TooManyTransactionsPendingExecution {
                queue_len: execution_queue_len,
                threshold: MAX_EXECUTION_QUEUE_LENGTH,
            });
        }
        let (_gas_status, input_objects) = transaction_input_checker::check_transaction_input(
            &self.database,
            epoch_store.as_ref(),
            &transaction.data().intent_message.value,
        )
        .await?;

        for (object_id, queue_len) in self.transaction_manager.objects_queue_len(
            input_objects
                .mutable_inputs()
                .into_iter()
                .map(|r| r.0)
                .collect(),
        ) {
            // When this occurs, most likely transactions piled up on a shared object.
            if queue_len >= MAX_PER_OBJECT_EXECUTION_QUEUE_LENGTH {
                return Err(SuiError::TooManyTransactionsPendingOnObject {
                    object_id,
                    queue_len,
                    threshold: MAX_PER_OBJECT_EXECUTION_QUEUE_LENGTH,
                });
            }
        }

        let owned_objects = input_objects.filter_owned_objects();

        let signed_transaction = VerifiedSignedTransaction::new(
            epoch_store.epoch(),
            transaction,
            self.name,
            &*self.secret,
        );

        // Check and write locks, to signed transaction, into the database
        // The call to self.set_transaction_lock checks the lock is not conflicting,
        // and returns ConflictingTransaction error in case there is a lock on a different
        // existing transaction.
        self.set_transaction_lock(&owned_objects, signed_transaction.clone(), epoch_store)
            .await?;

        Ok(signed_transaction)
    }

    /// Initiate a new transaction.
    pub async fn handle_transaction(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        transaction: VerifiedTransaction,
    ) -> Result<HandleTransactionResponse, SuiError> {
        let tx_digest = *transaction.digest();
        debug!(
            "handle_transaction with transaction data: {:?}",
            &transaction.data().intent_message.value
        );

        // Ensure an idempotent answer. This is checked before the system_tx check so that
        // a validator is able to return the signed system tx if it was already signed locally.
        if let Some((_, status)) = self.get_transaction_status(&tx_digest, epoch_store)? {
            return Ok(HandleTransactionResponse { status });
        }
        // CRITICAL! Validators should never sign an external system transaction.
        fp_ensure!(
            !transaction.is_system_tx(),
            SuiError::InvalidSystemTransaction
        );

        let _metrics_guard = self.metrics.handle_transaction_latency.start_timer();

        self.metrics.tx_orders.inc();

        // The should_accept_user_certs check here is best effort, because
        // between a validator signs a tx and a cert is formed, the validator
        // could close the window.
        if !epoch_store
            .get_reconfig_state_read_lock_guard()
            .should_accept_user_certs()
        {
            return Err(SuiError::ValidatorHaltedAtEpochEnd);
        }

        // Checks to see if the transaction has expired
        if match &transaction.inner().data().transaction_data().expiration() {
            TransactionExpiration::None => false,
            TransactionExpiration::Epoch(epoch) => *epoch < epoch_store.epoch(),
        } {
            return Err(SuiError::TransactionExpired);
        }

        let signed = self.handle_transaction_impl(transaction, epoch_store).await;
        match signed {
            Ok(s) => Ok(HandleTransactionResponse {
                status: TransactionStatus::Signed(s.into_inner().into_sig()),
            }),
            // It happens frequently that while we are checking the validity of the transaction, it
            // has just been executed.
            // In that case, we could still return Ok to avoid showing confusing errors.
            Err(err) => Ok(HandleTransactionResponse {
                status: self
                    .get_transaction_status(&tx_digest, epoch_store)?
                    .ok_or(err)?
                    .1,
            }),
        }
    }

    /// Executes a transaction that's known to have correct effects.
    /// For such transaction, we don't have to wait for consensus to set shared object
    /// locks because we already know the shared object versions based on the effects.
    /// This function can be called by a fullnode only.
    #[instrument(level = "trace", skip_all)]
    pub async fn fullnode_execute_certificate_with_effects(
        &self,
        transaction: &VerifiedExecutableTransaction,
        // NOTE: the caller of this must promise to wait until it
        // knows for sure this tx is finalized, namely, it has seen a
        // CertifiedTransactionEffects or at least f+1 identifical effects
        // digests matching this TransactionEffectsEnvelope, before calling
        // this function, in order to prevent a byzantine validator from
        // giving us incorrect effects.
        effects: &VerifiedCertifiedTransactionEffects,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        assert!(self.is_fullnode(epoch_store));
        let _metrics_guard = self
            .metrics
            .execute_certificate_with_effects_latency
            .start_timer();
        let digest = *transaction.digest();
        debug!("execute_certificate_with_effects");
        fp_ensure!(
            *effects.data().transaction_digest() == digest,
            SuiError::ErrorWhileProcessingCertificate {
                err: "effects/tx digest mismatch".to_string()
            }
        );

        if transaction.contains_shared_object() {
            epoch_store
                .acquire_shared_locks_from_effects(transaction, effects.data(), &self.database)
                .await?;
        }

        let expected_effects_digest = effects.digest();

        self.transaction_manager
            .enqueue(vec![transaction.clone()], epoch_store)?;

        let observed_effects = self
            .database
            .notify_read_executed_effects(vec![digest])
            .instrument(tracing::debug_span!(
                "notify_read_effects_in_execute_certificate_with_effects"
            ))
            .await?
            .pop()
            .expect("notify_read_effects should return exactly 1 element");

        let observed_effects_digest = observed_effects.digest();
        if &observed_effects_digest != expected_effects_digest {
            panic!(
                "Locally executed effects do not match canonical effects! expected_effects_digest={:?} observed_effects_digest={:?} expected_effects={:?} observed_effects={:?} input_objects={:?}",
                expected_effects_digest, observed_effects_digest, effects.data(), observed_effects, transaction.data().transaction_data().input_objects()
            );
        }
        Ok(())
    }

    /// Executes a certificate for its effects.
    #[instrument(level = "trace", skip_all)]
    pub async fn execute_certificate(
        &self,
        certificate: &VerifiedCertificate,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<VerifiedSignedTransactionEffects> {
        let _metrics_guard = self.metrics.execute_certificate_latency.start_timer();
        debug!("execute_certificate");

        self.metrics.total_cert_attempts.inc();

        if !certificate.contains_shared_object() {
            // Shared object transactions need to be sequenced by Narwhal before enqueueing
            // for execution.
            // They are done in AuthorityPerEpochStore::handle_consensus_transaction(),
            // which will enqueue this certificate for execution.
            // For owned object transactions, we can enqueue the certificate for execution immediately.
            self.enqueue_certificates_for_execution(vec![certificate.clone()], epoch_store)?;
        }

        let effects = self.notify_read_effects(certificate).await?;
        self.sign_effects(effects, epoch_store)
    }

    /// Internal logic to execute a certificate.
    ///
    /// Guarantees that
    /// - If input objects are available, return no permanent failure.
    /// - Execution and output commit are atomic. i.e. outputs are only written to storage,
    /// on successful execution; crashed execution has no observable effect and can be retried.
    ///
    /// It is caller's responsibility to ensure input objects are available and locks are set.
    /// If this cannot be satisfied by the caller, execute_certificate() should be called instead.
    ///
    /// Should only be called within sui-core.
    #[instrument(level = "trace", skip_all)]
    pub async fn try_execute_immediately(
        &self,
        certificate: &VerifiedExecutableTransaction,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<TransactionEffects> {
        let _metrics_guard = self.metrics.internal_execution_latency.start_timer();
        let tx_digest = *certificate.digest();
        debug!("execute_certificate_internal");

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
        let tx_guard = epoch_store.acquire_tx_guard(certificate).await?;

        self.process_certificate(tx_guard, certificate, epoch_store)
            .await
            .tap_err(|e| debug!(?tx_digest, "process_certificate failed: {e}"))
    }

    /// Test only wrapper for `try_execute_immediately()` above, useful for checking errors if the
    /// pre-conditions are not satisfied, and executing change epoch transactions.
    pub async fn try_execute_for_test(
        &self,
        certificate: &VerifiedCertificate,
    ) -> SuiResult<VerifiedSignedTransactionEffects> {
        let epoch_store = self.epoch_store_for_testing();
        let effects = self
            .try_execute_immediately(
                &VerifiedExecutableTransaction::new_from_certificate(certificate.clone()),
                &epoch_store,
            )
            .await?;
        self.sign_effects(effects, &epoch_store)
    }

    pub async fn notify_read_effects(
        &self,
        certificate: &VerifiedCertificate,
    ) -> SuiResult<TransactionEffects> {
        let tx_digest = *certificate.digest();
        Ok(self
            .database
            .notify_read_executed_effects(vec![tx_digest])
            .await?
            .pop()
            .expect("notify_read_effects should return exactly 1 element"))
    }

    async fn check_owned_locks(&self, owned_object_refs: &[ObjectRef]) -> SuiResult {
        self.database
            .check_owned_object_locks_exist(owned_object_refs)
    }

    #[instrument(level = "trace", skip_all)]
    pub(crate) async fn process_certificate(
        &self,
        tx_guard: CertTxGuard<'_>,
        certificate: &VerifiedExecutableTransaction,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<TransactionEffects> {
        let digest = *certificate.digest();
        // The cert could have been processed by a concurrent attempt of the same cert, so check if
        // the effects have already been written.
        if let Some(effects) = self.database.get_executed_effects(&digest)? {
            tx_guard.release();
            return Ok(effects);
        }
        let execution_guard = self
            .database
            .execution_lock_for_executable_transaction(certificate)
            .await;
        // Any caller that verifies the signatures on the certificate will have already checked the
        // epoch. But paths that don't verify sigs (e.g. execution from checkpoint, reading from db)
        // present the possibility of an epoch mismatch. If this cert is not finalzied in previous
        // epoch, then it's invalid.
        let execution_guard = match execution_guard {
            Ok(execution_guard) => execution_guard,
            Err(err) => {
                tx_guard.release();
                return Err(err);
            }
        };
        // Since we obtain a reference to the epoch store before taking the execution lock, it's
        // possible that reconfiguration has happened and they no longer match.
        if *execution_guard != epoch_store.epoch() {
            tx_guard.release();
            debug!("The epoch of the execution_guard doesn't match the epoch store");
            return Err(SuiError::WrongEpoch {
                expected_epoch: epoch_store.epoch(),
                actual_epoch: *execution_guard,
            });
        }

        // first check to see if we have already executed and committed the tx
        // to the WAL
        if let Some((inner_temporary_storage, effects)) =
            epoch_store.wal().get_execution_output(&digest)?
        {
            self.commit_cert_and_notify(
                certificate,
                inner_temporary_storage,
                &effects,
                tx_guard,
                execution_guard,
                epoch_store,
            )
            .await?;
            return Ok(effects);
        }

        // Errors originating from prepare_certificate may be transient (failure to read locks) or
        // non-transient (transaction input is invalid, move vm errors). However, all errors from
        // this function occur before we have written anything to the db, so we commit the tx
        // guard and rely on the client to retry the tx (if it was transient).
        let (inner_temporary_store, effects) = match self
            .prepare_certificate(&execution_guard, certificate, epoch_store)
            .await
        {
            Err(e) => {
                debug!(name = ?self.name, ?digest, "Error preparing transaction: {e}");
                tx_guard.release();
                return Err(e);
            }
            Ok(res) => res,
        };

        // Write tx output to WAL as first commit phase. In second phase
        // we write from WAL to permanent storage. The purpose of this scheme
        // is to allow for retrying phase 2 from phase 1 in the case where we
        // fail mid-write. We prefer this over making the write to permanent
        // storage atomic as this allows for sharding storage across nodes, which
        // would be more difficult in the alternative.
        epoch_store
            .wal()
            .write_execution_output(&digest, (inner_temporary_store.clone(), effects.clone()))?;

        // Insert an await in between write_execution_output and commit so that tests can observe
        // and test the interruption.
        #[cfg(any(test, msim))]
        tokio::task::yield_now().await;

        self.commit_cert_and_notify(
            certificate,
            inner_temporary_store,
            &effects,
            tx_guard,
            execution_guard,
            epoch_store,
        )
        .await?;
        Ok(effects)
    }

    async fn commit_cert_and_notify(
        &self,
        certificate: &VerifiedExecutableTransaction,
        inner_temporary_store: InnerTemporaryStore,
        effects: &TransactionEffects,
        tx_guard: CertTxGuard<'_>,
        _execution_guard: ExecutionLockReadGuard<'_>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        let input_object_count = inner_temporary_store.objects.len();
        let shared_object_count = effects.shared_objects().len();

        // If commit_certificate returns an error, tx_guard will be dropped and the certificate
        // will be persisted in the log for later recovery.
        let output_keys: Vec<_> = inner_temporary_store
            .written
            .iter()
            .map(|(_, ((id, seq, _), obj, _))| InputKey(*id, (!obj.is_package()).then_some(*seq)))
            .collect();

        let events = inner_temporary_store.events.clone();

        self.commit_certificate(inner_temporary_store, certificate, effects, epoch_store)
            .await?;

        // Notifies transaction manager about available input objects. This allows the transaction
        // manager to schedule ready transactions.
        //
        // REQUIRED: this must be called after commit_certificate() (above), to ensure
        // TransactionManager can receive the notifications for objects that it did not find
        // in the objects table.
        //
        // REQUIRED: this must be called before tx_guard.commit_tx() (below), to ensure
        // TransactionManager can get the notifications after the node crashes and restarts.
        self.transaction_manager
            .objects_available(output_keys, epoch_store);

        // commit_certificate finished, the tx is fully committed to the store.
        tx_guard.commit_tx();

        // index certificate
        let _ = self
            .post_process_one_tx(certificate, effects, &events, epoch_store)
            .await
            .tap_err(|e| error!("tx post processing failed: {e}"));

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
        self.metrics.batch_size.observe(
            certificate
                .data()
                .intent_message
                .value
                .kind()
                .num_commands() as f64,
        );

        Ok(())
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
        _execution_guard: &ExecutionLockReadGuard<'_>,
        certificate: &VerifiedExecutableTransaction,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<(InnerTemporaryStore, TransactionEffects)> {
        let _metrics_guard = self.metrics.prepare_certificate_latency.start_timer();

        // check_certificate_input also checks shared object locks when loading the shared objects.
        let (gas_status, input_objects) = transaction_input_checker::check_certificate_input(
            &self.database,
            epoch_store,
            certificate,
        )
        .await?;

        let owned_object_refs = input_objects.filter_owned_objects();
        self.check_owned_locks(&owned_object_refs).await?;

        let shared_object_refs = input_objects.filter_shared_objects();
        let transaction_dependencies = input_objects.transaction_dependencies();
        let temporary_store = TemporaryStore::new(
            self.database.clone(),
            input_objects,
            *certificate.digest(),
            epoch_store.protocol_config(),
        );
        let transaction_data = &certificate.data().intent_message.value;
        let (kind, signer, gas) = transaction_data.execution_parts();
        let (inner_temp_store, effects, _execution_error) =
            execution_engine::execute_transaction_to_effects::<execution_mode::Normal, _>(
                shared_object_refs,
                temporary_store,
                kind,
                signer,
                &gas,
                *certificate.digest(),
                transaction_dependencies,
                epoch_store.move_vm(),
                gas_status,
                &epoch_store.epoch_start_config().epoch_data(),
                epoch_store.protocol_config(),
            );

        Ok((inner_temp_store, effects))
    }

    /// Notifies TransactionManager about an executed certificate.
    pub fn certificate_executed(
        &self,
        digest: &TransactionDigest,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        self.transaction_manager
            .certificate_executed(digest, epoch_store)
    }

    pub async fn dry_exec_transaction(
        &self,
        transaction: TransactionData,
        transaction_digest: TransactionDigest,
    ) -> Result<DryRunTransactionResponse, anyhow::Error> {
        let epoch_store = self.load_epoch_store_one_call_per_task();
        if !self.is_fullnode(&epoch_store) {
            return Err(anyhow!("dry-exec is only supported on fullnodes"));
        }

        // make a gas object if one was not provided
        let mut gas_object_refs = transaction.gas().to_vec();
        let (gas_status, input_objects) = if transaction.gas().is_empty() {
            let sender = transaction.sender();
            let protocol_config = epoch_store.protocol_config();
            let max_tx_gas = protocol_config.max_tx_gas();
            let gas_object_id = ObjectID::random();
            let gas_object = Object::new_move(
                MoveObject::new_gas_coin(OBJECT_START_VERSION, gas_object_id, max_tx_gas),
                Owner::AddressOwner(sender),
                TransactionDigest::genesis(),
            );
            let gas_object_ref = gas_object.compute_object_reference();
            gas_object_refs = vec![gas_object_ref];
            transaction_input_checker::check_transaction_input_with_given_gas(
                &self.database,
                epoch_store.as_ref(),
                &transaction,
                gas_object,
            )
            .await?
        } else {
            transaction_input_checker::check_transaction_input(
                &self.database,
                epoch_store.as_ref(),
                &transaction,
            )
            .await?
        };

        let shared_object_refs = input_objects.filter_shared_objects();

        let transaction_dependencies = input_objects.transaction_dependencies();
        let temporary_store = TemporaryStore::new(
            self.database.clone(),
            input_objects,
            transaction_digest,
            epoch_store.protocol_config(),
        );
        let (kind, signer, _) = transaction.execution_parts();
        let move_vm = Arc::new(
            adapter::new_move_vm(
                epoch_store.native_functions().clone(),
                epoch_store.protocol_config(),
            )
            .expect("We defined natives to not fail here"),
        );
        let (inner_temp_store, effects, _execution_error) =
            execution_engine::execute_transaction_to_effects::<execution_mode::Normal, _>(
                shared_object_refs,
                temporary_store,
                kind,
                signer,
                &gas_object_refs,
                transaction_digest,
                transaction_dependencies,
                &move_vm,
                gas_status,
                &epoch_store.epoch_start_config().epoch_data(),
                epoch_store.protocol_config(),
            );
        Ok(DryRunTransactionResponse {
            effects: effects.try_into()?,
            events: SuiTransactionEvents::try_from(
                inner_temp_store.events,
                epoch_store.module_cache().as_ref(),
            )?,
        })
    }

    /// The object ID for gas can be any object ID, even for an uncreated object
    pub async fn dev_inspect_transaction(
        &self,
        sender: SuiAddress,
        transaction_kind: TransactionKind,
        gas_price: Option<u64>,
    ) -> Result<DevInspectResults, anyhow::Error> {
        let epoch_store = self.load_epoch_store_one_call_per_task();
        if !self.is_fullnode(&epoch_store) {
            return Err(anyhow!("dev-inspect is only supported on fullnodes"));
        }

        transaction_kind.check_version_supported(epoch_store.protocol_config())?;

        let gas_price = gas_price.unwrap_or_else(|| epoch_store.reference_gas_price());

        let protocol_config = epoch_store.protocol_config();

        let max_tx_gas = protocol_config.max_tx_gas();
        let storage_gas_price = protocol_config.storage_gas_price();

        let gas_object_id = ObjectID::random();
        // give the gas object 2x the max gas to have coin balance to play with during execution
        let gas_object = Object::new_move(
            MoveObject::new_gas_coin(SequenceNumber::new(), gas_object_id, max_tx_gas * 2),
            Owner::AddressOwner(sender),
            TransactionDigest::genesis(),
        );
        let (gas_object_ref, input_objects) = transaction_input_checker::check_dev_inspect_input(
            &self.database,
            protocol_config,
            &transaction_kind,
            gas_object,
        )
        .await?;
        let shared_object_refs = input_objects.filter_shared_objects();

        // TODO should we error instead for 0?
        let gas_price = std::cmp::max(gas_price, 1);
        let gas_budget = max_tx_gas;
        let data = TransactionData::new(
            transaction_kind,
            sender,
            gas_object_ref,
            gas_price,
            gas_budget,
        );
        let transaction_digest = TransactionDigest::new(sha3_hash(&data));
        let transaction_kind = data.into_kind();
        let transaction_dependencies = input_objects.transaction_dependencies();
        let temporary_store = TemporaryStore::new(
            self.database.clone(),
            input_objects,
            transaction_digest,
            protocol_config,
        );
        let mut gas_status = SuiGasStatus::new_with_budget(
            max_tx_gas,
            GasPrice::from(gas_price),
            storage_gas_price.into(),
            SuiCostTable::new(protocol_config),
        );
        gas_status.charge_min_tx_gas()?;
        let move_vm = Arc::new(
            adapter::new_move_vm(
                epoch_store.native_functions().clone(),
                epoch_store.protocol_config(),
            )
            .expect("We defined natives to not fail here"),
        );
        let (inner_temp_store, effects, execution_result) =
            execution_engine::execute_transaction_to_effects::<execution_mode::DevInspect, _>(
                shared_object_refs,
                temporary_store,
                transaction_kind,
                sender,
                &[gas_object_ref],
                transaction_digest,
                transaction_dependencies,
                &move_vm,
                gas_status,
                &epoch_store.epoch_start_config().epoch_data(),
                protocol_config,
            );
        DevInspectResults::new(
            effects,
            inner_temp_store.events,
            execution_result,
            epoch_store.module_cache().as_ref(),
        )
    }

    pub fn is_tx_already_executed(&self, digest: &TransactionDigest) -> SuiResult<bool> {
        self.database.is_tx_already_executed(digest)
    }

    #[instrument(level = "debug", skip_all, err)]
    fn index_tx(
        &self,
        indexes: &IndexStore,
        digest: &TransactionDigest,
        // TODO: index_tx really just need the transaction data here.
        cert: &VerifiedExecutableTransaction,
        effects: &TransactionEffects,
        events: &TransactionEvents,
        timestamp_ms: u64,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<u64> {
        let changes = self
            .process_object_index(effects, epoch_store)
            .tap_err(|e| warn!("{e}"))?;

        indexes.index_tx(
            cert.data().intent_message.value.sender(),
            cert.data()
                .intent_message
                .value
                .input_objects()?
                .iter()
                .map(|o| o.object_id()),
            effects
                .all_mutated()
                .into_iter()
                .map(|(obj_ref, owner, _kind)| (*obj_ref, *owner)),
            cert.data()
                .intent_message
                .value
                .move_calls()
                .into_iter()
                .map(|(package, module, function)| {
                    (*package, module.to_owned(), function.to_owned())
                }),
            events,
            changes,
            digest,
            timestamp_ms,
        )
    }

    fn process_object_index(
        &self,
        effects: &TransactionEffects,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> Result<ObjectIndexChanges, SuiError> {
        let modified_at_version = effects
            .modified_at_versions()
            .iter()
            .cloned()
            .collect::<HashMap<_, _>>();

        let mut deleted_owners = vec![];
        let mut deleted_dynamic_fields = vec![];
        for (id, _, _) in effects.deleted() {
            let old_version = modified_at_version.get(id).unwrap();

            match self.get_owner_at_version(id, *old_version)? {
                Owner::AddressOwner(addr) => deleted_owners.push((addr, *id)),
                Owner::ObjectOwner(object_id) => {
                    deleted_dynamic_fields.push((ObjectID::from(object_id), *id))
                }
                _ => {}
            }
        }

        let mut new_owners = vec![];
        let mut new_dynamic_fields = vec![];

        for (oref, owner, kind) in effects.all_mutated() {
            let id = &oref.0;
            // For mutated objects, retrieve old owner and delete old index if there is a owner change.
            if let WriteKind::Mutate = kind {
                let Some(old_version) = modified_at_version.get(id) else{
                        error!("Error processing object owner index for tx [{:?}], cannot find modified at version for mutated object [{id}].", effects.transaction_digest());
                        continue;
                    };
                let Some(old_object) = self.database.get_object_by_key(id, *old_version)? else {
                        error!("Error processing object owner index for tx [{:?}], cannot find object [{id}] at version [{old_version}].", effects.transaction_digest());
                        continue;
                    };
                if &old_object.owner != owner {
                    match old_object.owner {
                        Owner::AddressOwner(addr) => {
                            deleted_owners.push((addr, *id));
                        }
                        Owner::ObjectOwner(object_id) => {
                            deleted_dynamic_fields.push((ObjectID::from(object_id), *id))
                        }
                        _ => {}
                    }
                }
            }

            match owner {
                Owner::AddressOwner(addr) => {
                    // TODO: We can remove the object fetching after we added ObjectType to TransactionEffects
                    let Some(o) = self.database.get_object_by_key(id, oref.1)? else{
                        continue;
                    };

                    let type_ = o
                        .type_()
                        .map(|type_| ObjectType::Struct(type_.clone()))
                        .unwrap_or(ObjectType::Package);

                    new_owners.push((
                        (*addr, *id),
                        ObjectInfo {
                            object_id: *id,
                            version: oref.1,
                            digest: oref.2,
                            type_,
                            owner: *owner,
                            previous_transaction: *effects.transaction_digest(),
                        },
                    ));
                }
                Owner::ObjectOwner(owner) => {
                    let Some(o) = self.database.get_object_by_key(&oref.0, oref.1)? else{
                        continue;
                    };
                    let Some(df_info) = self.try_create_dynamic_field_info(&o, epoch_store)? else{
                        // Skip indexing for non dynamic field objects.
                        continue;
                    };
                    new_dynamic_fields.push(((ObjectID::from(*owner), *id), df_info))
                }
                _ => {}
            }
        }

        Ok(ObjectIndexChanges {
            deleted_owners,
            deleted_dynamic_fields,
            new_owners,
            new_dynamic_fields,
        })
    }

    fn try_create_dynamic_field_info(
        &self,
        o: &Object,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<Option<DynamicFieldInfo>> {
        // Skip if not a move object
        let Some(move_object) =  o.data.try_as_move().cloned() else {
            return Ok(None);
        };
        // We only index dynamic field objects
        if !move_object.type_().is_dynamic_field() {
            return Ok(None);
        }
        let move_struct = move_object.to_move_struct_with_resolver(
            ObjectFormatOptions::default(),
            epoch_store.module_cache().as_ref(),
        )?;

        let (name_value, type_, object_id) =
            DynamicFieldInfo::parse_move_object(&move_struct).tap_err(|e| warn!("{e}"))?;

        let name_type = move_object.type_().try_extract_field_name(&type_)?;

        let bcs_name = bcs::to_bytes(&name_value.clone().undecorate()).map_err(|e| {
            SuiError::ObjectSerializationError {
                error: format!("{e}"),
            }
        })?;

        let name = DynamicFieldName {
            type_: name_type,
            value: SuiMoveValue::from(name_value).to_json_value(),
        };

        Ok(Some(match type_ {
            DynamicFieldType::DynamicObject => {
                // Find the actual object from storage using the object id obtained from the wrapper.
                let Some(object) = self.database.find_object_lt_or_eq_version(object_id, o.version()) else{
                    return Err(UserInputError::ObjectNotFound {
                        object_id,
                        version: Some(o.version()),
                    }.into())
                };
                let version = object.version();
                let digest = object.digest();
                let object_type = object.data.type_().unwrap();

                DynamicFieldInfo {
                    name,
                    bcs_name,
                    type_,
                    object_type: object_type.to_string(),
                    object_id,
                    version,
                    digest,
                }
            }
            DynamicFieldType::DynamicField { .. } => DynamicFieldInfo {
                name,
                bcs_name,
                type_,
                object_type: move_object.into_type().into_type_params()[1].to_string(),
                object_id: o.id(),
                version: o.version(),
                digest: o.digest(),
            },
        }))
    }

    #[instrument(level = "debug", skip_all, err)]
    async fn post_process_one_tx(
        &self,
        certificate: &VerifiedExecutableTransaction,
        effects: &TransactionEffects,
        events: &TransactionEvents,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        if self.indexes.is_none() && self.event_handler.is_none() {
            return Ok(());
        }

        let tx_digest = certificate.digest();
        let timestamp_ms = Self::unixtime_now_ms();

        // Index tx
        if let Some(indexes) = &self.indexes {
            let res = self
                .index_tx(
                    indexes.as_ref(),
                    tx_digest,
                    certificate,
                    effects,
                    events,
                    timestamp_ms,
                    epoch_store,
                )
                .tap_ok(|_| self.metrics.post_processing_total_tx_indexed.inc())
                .tap_err(|e| error!(?tx_digest, "Post processing - Couldn't index tx: {e}"));

            // Emit events
            if let (Some(event_handler), Ok(seq)) = (&self.event_handler, res) {
                event_handler
                    .process_events(
                        effects,
                        events,
                        timestamp_ms,
                        seq,
                        epoch_store.module_cache().as_ref(),
                    )
                    .await
                    .tap_ok(|_| {
                        self.metrics
                            .post_processing_total_tx_had_event_processed
                            .inc()
                    })
                    .tap_err(|e| {
                        warn!(
                            ?tx_digest,
                            "Post processing - Couldn't process events for tx: {}", e
                        )
                    })?;

                self.metrics
                    .post_processing_total_events_emitted
                    .inc_by(events.data.len() as u64);
            }
        };

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
        let epoch_store = self.load_epoch_store_one_call_per_task();
        let (transaction, status) = self
            .get_transaction_status(&request.transaction_digest, &epoch_store)?
            .ok_or(SuiError::TransactionNotFound {
                digest: request.transaction_digest,
            })?;
        Ok(TransactionInfoResponse {
            transaction,
            status,
        })
    }

    pub async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        let epoch_store = self.load_epoch_store_one_call_per_task();

        let requested_object_seq = match request.request_kind {
            ObjectInfoRequestKind::LatestObjectInfo => {
                let (_, seq, _) = self
                    .get_latest_parent_entry(request.object_id)
                    .await?
                    .ok_or_else(|| {
                        SuiError::from(UserInputError::ObjectNotFound {
                            object_id: request.object_id,
                            version: None,
                        })
                    })?
                    .0;
                seq
            }
            ObjectInfoRequestKind::PastObjectInfoDebug(seq) => seq,
        };

        let object = self
            .database
            .get_object_by_key(&request.object_id, requested_object_seq)?
            .ok_or_else(|| {
                SuiError::from(UserInputError::ObjectNotFound {
                    object_id: request.object_id,
                    version: Some(requested_object_seq),
                })
            })?;

        let layout = match request.object_format_options {
            Some(format) => object.get_layout(format, epoch_store.module_cache().as_ref())?,
            None => None,
        };

        let lock = if !object.is_address_owned() {
            // Only address owned objects have locks.
            None
        } else {
            self.get_transaction_lock(&object.compute_object_reference(), &epoch_store)
                .await?
                .map(|s| s.into_inner())
        };

        Ok(ObjectInfoResponse {
            object,
            layout,
            lock_for_debugging: lock,
        })
    }

    pub fn handle_checkpoint_request(
        &self,
        request: &CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        let summary = match request.sequence_number {
            Some(seq) => self
                .checkpoint_store
                .get_checkpoint_by_sequence_number(seq)?,
            None => self.checkpoint_store.get_latest_certified_checkpoint(),
        }
        .map(|v| v.into_inner());
        let contents = match &summary {
            Some(s) => self
                .checkpoint_store
                .get_checkpoint_contents(&s.content_digest)?,
            None => None,
        };
        Ok(CheckpointResponse {
            checkpoint: summary,
            contents,
        })
    }

    fn check_protocol_version(
        supported_protocol_versions: SupportedProtocolVersions,
        current_version: ProtocolVersion,
    ) {
        info!("current protocol version is now {:?}", current_version);
        info!("supported versions are: {:?}", supported_protocol_versions);
        if !supported_protocol_versions.is_version_supported(current_version) {
            let msg = format!(
                "Unsupported protocol version. The network is at {:?}, but this SuiNode only supports: {:?}. Shutting down.",
                current_version, supported_protocol_versions,
            );

            error!("{}", msg);
            eprintln!("{}", msg);

            #[cfg(not(msim))]
            std::process::exit(1);

            #[cfg(msim)]
            sui_simulator::task::shutdown_current_node();
        }
    }
    // TODO: This function takes both committee and genesis as parameter.
    // Technically genesis already contains committee information. Could consider merging them.
    #[allow(clippy::disallowed_methods)] // allow unbounded_channel()
    pub async fn new(
        name: AuthorityName,
        secret: StableSyncAuthoritySigner,
        supported_protocol_versions: SupportedProtocolVersions,
        store: Arc<AuthorityStore>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        committee_store: Arc<CommitteeStore>,
        indexes: Option<Arc<IndexStore>>,
        event_store: Option<Arc<EventStoreType>>,
        checkpoint_store: Arc<CheckpointStore>,
        prometheus_registry: &Registry,
        pruning_config: AuthorityStorePruningConfig,
        genesis_objects: &[Object],
        epoch_duration_ms: u64,
        db_checkpoint_config: &DBCheckpointConfig,
    ) -> Arc<Self> {
        Self::check_protocol_version(supported_protocol_versions, epoch_store.protocol_version());

        let event_handler = event_store.map(|es| {
            let handler = EventHandler::new(es);
            handler.regular_cleanup_task();
            Arc::new(handler)
        });
        let metrics = Arc::new(AuthorityMetrics::new(prometheus_registry));
        let (tx_ready_certificates, rx_ready_certificates) = unbounded_channel();
        let transaction_manager = Arc::new(TransactionManager::new(
            store.clone(),
            &epoch_store,
            tx_ready_certificates,
            metrics.clone(),
        ));
        let (tx_execution_shutdown, rx_execution_shutdown) = oneshot::channel();

        let _authority_per_epoch_pruner =
            AuthorityPerEpochStorePruner::new(epoch_store.get_parent_path(), &pruning_config);
        let _objects_pruner = AuthorityStorePruner::new(
            store.perpetual_tables.clone(),
            checkpoint_store.clone(),
            store.objects_lock_table.clone(),
            pruning_config,
            epoch_duration_ms,
        );
        let state = Arc::new(AuthorityState {
            name,
            secret,
            epoch_store: ArcSwap::new(epoch_store.clone()),
            database: store.clone(),
            indexes,
            event_handler,
            checkpoint_store,
            committee_store,
            transaction_manager,
            tx_execution_shutdown: Mutex::new(Some(tx_execution_shutdown)),
            metrics,
            _objects_pruner,
            _authority_per_epoch_pruner,
            db_checkpoint_config: db_checkpoint_config.clone(),
        });

        // Process tx recovery log first, so that checkpoint recovery (below)
        // doesn't observe partially-committed txes.
        state
            .process_tx_recovery_log(None, &epoch_store)
            .await
            .expect("Could not fully process recovery log at startup!");

        // Start a task to execute ready certificates.
        let authority_state = Arc::downgrade(&state);
        spawn_monitored_task!(execution_process(
            authority_state,
            rx_ready_certificates,
            rx_execution_shutdown
        ));

        state
            .create_owner_index_if_empty(genesis_objects, &epoch_store)
            .expect("Error indexing genesis objects.");

        state
    }

    // TODO: Technically genesis_committee can be derived from genesis.
    pub async fn new_for_testing(
        genesis_committee: Committee,
        key: &AuthorityKeyPair,
        store_base_path: Option<PathBuf>,
        genesis: &Genesis,
    ) -> Arc<Self> {
        let secret = Arc::pin(key.copy());
        let name: AuthorityName = secret.public().into();
        let path = match store_base_path {
            Some(path) => path,
            None => {
                let dir = std::env::temp_dir();
                let path = dir.join(format!("DB_{:?}", nondeterministic!(ObjectID::random())));
                std::fs::create_dir(&path).unwrap();
                path
            }
        };

        // unwrap ok - for testing only.
        let store = Arc::new(
            AuthorityStore::open_with_committee_for_testing(
                &path.join("store"),
                None,
                &genesis_committee,
                genesis,
                0,
            )
            .await
            .unwrap(),
        );
        let registry = Registry::new();
        let cache_metrics = Arc::new(ResolverMetrics::new(&registry));
        let verified_cert_cache_metrics = VerifiedCertificateCacheMetrics::new(&registry);
        let epoch_store = AuthorityPerEpochStore::new(
            name,
            Arc::new(genesis_committee.clone()),
            &path.join("store"),
            None,
            EpochMetrics::new(&registry),
            EpochStartConfiguration::new_for_testing(),
            store.clone(),
            cache_metrics,
            verified_cert_cache_metrics,
        );

        let epochs = Arc::new(CommitteeStore::new(
            path.join("epochs"),
            &genesis_committee,
            None,
        ));

        let checkpoint_store = CheckpointStore::new(&path.join("checkpoints"));
        let index_store = Some(Arc::new(IndexStore::new(path.join("indexes"))));

        let state = AuthorityState::new(
            secret.public().into(),
            secret.clone(),
            SupportedProtocolVersions::SYSTEM_DEFAULT,
            store,
            epoch_store,
            epochs,
            index_store,
            None,
            checkpoint_store,
            &registry,
            AuthorityStorePruningConfig::default(),
            genesis.objects(),
            10000,
            &DBCheckpointConfig::default(),
        )
        .await;

        let epoch_store = state.epoch_store_for_testing();
        state
            .create_owner_index_if_empty(genesis.objects(), &epoch_store)
            .unwrap();

        state
    }

    pub fn transaction_manager(&self) -> &Arc<TransactionManager> {
        &self.transaction_manager
    }

    /// Adds certificates to the pending certificate store and transaction manager for ordered execution.
    pub fn enqueue_certificates_for_execution(
        &self,
        certs: Vec<VerifiedCertificate>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<()> {
        let executable_txns: Vec<_> = certs
            .clone()
            .into_iter()
            .map(VerifiedExecutableTransaction::new_from_certificate)
            .map(VerifiedExecutableTransaction::serializable)
            .collect();
        epoch_store.insert_pending_execution(&executable_txns)?;
        self.transaction_manager
            .enqueue_certificates(certs, epoch_store)
    }

    // Continually pop in-progress txes from the WAL and try to drive them to completion.
    pub async fn process_tx_recovery_log(
        &self,
        limit: Option<usize>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        let mut limit = limit.unwrap_or(usize::MAX);
        while limit > 0 {
            limit -= 1;
            if let Some((cert, tx_guard)) = epoch_store.wal().read_one_recoverable_tx().await? {
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

                if let Err(e) = self
                    .process_certificate(tx_guard, &cert.into(), epoch_store)
                    .instrument(error_span!("process_tx_recovery_log", tx_digest = ?digest))
                    .await
                {
                    warn!(?digest, "Failed to process in-progress certificate: {e}");
                }
            } else {
                break;
            }
        }

        Ok(())
    }

    fn create_owner_index_if_empty(
        &self,
        genesis_objects: &[Object],
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        let Some(index_store) = &self.indexes else{
            return Ok(())
        };
        if !index_store.is_empty() {
            return Ok(());
        }

        let mut new_owners = vec![];
        let mut new_dynamic_fields = vec![];
        for o in genesis_objects.iter() {
            match o.owner {
                Owner::AddressOwner(addr) => new_owners.push((
                    (addr, o.id()),
                    ObjectInfo::new(&o.compute_object_reference(), o),
                )),
                Owner::ObjectOwner(object_id) => {
                    let id = o.id();
                    let Some(info) = self.try_create_dynamic_field_info(o, epoch_store)? else{
                        continue;
                    };
                    new_dynamic_fields.push(((ObjectID::from(object_id), id), info));
                }
                _ => {}
            }
        }

        index_store.insert_genesis_objects(ObjectIndexChanges {
            deleted_owners: vec![],
            deleted_dynamic_fields: vec![],
            new_owners,
            new_dynamic_fields,
        })
    }

    pub async fn reconfigure(
        &self,
        cur_epoch_store: &AuthorityPerEpochStore,
        supported_protocol_versions: SupportedProtocolVersions,
        new_committee: Committee,
        epoch_start_configuration: EpochStartConfiguration,
    ) -> SuiResult<Arc<AuthorityPerEpochStore>> {
        Self::check_protocol_version(
            supported_protocol_versions,
            epoch_start_configuration
                .epoch_start_state()
                .protocol_version(),
        );

        self.committee_store.insert_new_committee(&new_committee)?;
        let db = self.db();
        let mut execution_lock = db.execution_lock_for_reconfiguration().await;
        self.revert_uncommitted_epoch_transactions(cur_epoch_store)
            .await?;
        if let Some(checkpoint_path) = &self.db_checkpoint_config.checkpoint_path {
            if self
                .db_checkpoint_config
                .perform_db_checkpoints_at_epoch_end
            {
                let current_epoch = cur_epoch_store.epoch();
                let epoch_checkpoint_path =
                    checkpoint_path.join(format!("epoch_{}", current_epoch));
                self.checkpoint_all_dbs(&epoch_checkpoint_path, cur_epoch_store)?;
            }
        }
        let new_epoch = new_committee.epoch;
        let new_epoch_store = self
            .reopen_epoch_db(cur_epoch_store, new_committee, epoch_start_configuration)
            .await?;
        assert_eq!(new_epoch_store.epoch(), new_epoch);
        self.transaction_manager.reconfigure(new_epoch);
        *execution_lock = new_epoch;
        // drop execution_lock after epoch store was updated
        // see also assert in AuthorityState::process_certificate
        // on the epoch store and execution lock epoch match
        Ok(new_epoch_store)
    }

    pub fn db(&self) -> Arc<AuthorityStore> {
        self.database.clone()
    }

    pub fn current_epoch_for_testing(&self) -> EpochId {
        self.epoch_store_for_testing().epoch()
    }

    pub fn checkpoint_all_dbs(
        &self,
        checkpoint_path: &Path,
        cur_epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult {
        let _metrics_guard = self.metrics.db_checkpoint_latency.start_timer();
        let current_epoch = cur_epoch_store.epoch();

        if checkpoint_path.exists() {
            info!("Skipping db checkpoint as it already exists for epoch: {current_epoch}");
            return Ok(());
        }

        let checkpoint_path_tmp = checkpoint_path.with_extension("tmp");
        let store_checkpoint_path_tmp = checkpoint_path_tmp.join("store");

        if checkpoint_path_tmp.exists() {
            fs::remove_dir_all(&checkpoint_path_tmp)
                .map_err(|e| SuiError::FileIOError(e.to_string()))?;
        }

        fs::create_dir_all(&checkpoint_path_tmp)
            .map_err(|e| SuiError::FileIOError(e.to_string()))?;
        fs::create_dir(&store_checkpoint_path_tmp)
            .map_err(|e| SuiError::FileIOError(e.to_string()))?;

        self.database
            .perpetual_tables
            .checkpoint_db(&store_checkpoint_path_tmp.join("perpetual"))?;
        self.committee_store
            .checkpoint_db(&checkpoint_path_tmp.join("epochs"))?;
        self.checkpoint_store
            .checkpoint_db(&checkpoint_path_tmp.join("checkpoints"))?;

        fs::rename(checkpoint_path_tmp, checkpoint_path)
            .map_err(|e| SuiError::FileIOError(e.to_string()))?;
        Ok(())
    }

    /// Load the current epoch store. This can change during reconfiguration. To ensure that
    /// we never end up accessing different epoch stores in a single task, we need to make sure
    /// that this is called once per task. Each call needs to be carefully audited to ensure it is
    /// the case. This also means we should minimize the number of call-sites. Only call it when
    /// there is no way to obtain it from somewhere else.
    pub fn load_epoch_store_one_call_per_task(&self) -> Guard<Arc<AuthorityPerEpochStore>> {
        self.epoch_store.load()
    }

    // Load the epoch store, should be used in tests only.
    pub fn epoch_store_for_testing(&self) -> Guard<Arc<AuthorityPerEpochStore>> {
        self.load_epoch_store_one_call_per_task()
    }

    pub fn clone_committee_for_testing(&self) -> Committee {
        self.epoch_store_for_testing().committee().clone()
    }

    pub(crate) async fn get_object(
        &self,
        object_id: &ObjectID,
    ) -> Result<Option<Object>, SuiError> {
        self.database.get_object(object_id)
    }

    pub async fn get_framework_object_ref(&self) -> SuiResult<ObjectRef> {
        Ok(self
            .get_object(&SUI_FRAMEWORK_ADDRESS.into())
            .await?
            .expect("framework object should always exist")
            .compute_object_reference())
    }

    /// This function should be called once and exactly once during reconfiguration.
    /// Instead of this function use AuthorityEpochStore::epoch_start_configuration() to access this object everywhere
    /// besides when we are reading fields for the current epoch
    pub fn get_sui_system_state_object_during_reconfig(&self) -> SuiResult<SuiSystemState> {
        self.database.get_sui_system_state_object()
    }

    // This function is only used for testing.
    #[cfg(test)]
    pub fn get_sui_system_state_object_for_testing(&self) -> SuiResult<SuiSystemState> {
        self.database.get_sui_system_state_object()
    }

    pub fn get_transaction_checkpoint_sequence(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<Option<(EpochId, CheckpointSequenceNumber)>> {
        self.database.get_transaction_checkpoint(digest)
    }

    pub fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> SuiResult<Option<VerifiedCheckpoint>> {
        Ok(self
            .checkpoint_store
            .get_checkpoint_by_sequence_number(sequence_number)?)
    }

    pub fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<Option<VerifiedCheckpoint>> {
        let checkpoint = self.database.get_transaction_checkpoint(digest)?;
        let Some((_, checkpoint)) = checkpoint else { return Ok(None); };
        let checkpoint = self
            .checkpoint_store
            .get_checkpoint_by_sequence_number(checkpoint)?;
        Ok(checkpoint)
    }

    pub async fn get_object_read(&self, object_id: &ObjectID) -> Result<ObjectRead, SuiError> {
        match self.database.get_latest_parent_entry(*object_id)? {
            None => Ok(ObjectRead::NotExists(*object_id)),
            Some((obj_ref, _)) => {
                if obj_ref.2.is_alive() {
                    match self.database.get_object_by_key(object_id, obj_ref.1)? {
                        None => {
                            error!("Object with in parent_entry is missing from object store, datastore is inconsistent");
                            Err(UserInputError::ObjectNotFound {
                                object_id: *object_id,
                                version: Some(obj_ref.1),
                            }
                            .into())
                        }
                        Some(object) => {
                            let layout = object.get_layout(
                                ObjectFormatOptions::default(),
                                // threading the epoch_store through this API does not
                                // seem possible, so we just read it from the state (self) and fetch
                                // the module cache out of it.
                                // Notice that no matter what module cache we get things
                                // should work
                                self.load_epoch_store_one_call_per_task()
                                    .module_cache()
                                    .as_ref(),
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

    pub async fn get_move_object<T>(&self, object_id: &ObjectID) -> SuiResult<T>
    where
        T: DeserializeOwned,
    {
        let o = self.get_object_read(object_id).await?.into_object()?;
        if let Some(move_object) = o.data.try_as_move() {
            Ok(bcs::from_bytes(move_object.contents()).map_err(|e| {
                SuiError::ObjectDeserializationError {
                    error: format!("{e}"),
                }
            })?)
        } else {
            Err(SuiError::ObjectDeserializationError {
                error: format!("Provided object : [{object_id}] is not a Move object."),
            })
        }
    }

    /// This function read the dynamic fields of a Table and return the deserialized value for the key.
    pub async fn read_table_value<K, V>(&self, table: ObjectID, key: &K) -> Option<V>
    where
        K: DeserializeOwned + Serialize,
        V: DeserializeOwned,
    {
        let key_bcs = bcs::to_bytes(key).ok()?;
        let df = self
            .get_dynamic_fields_iterator(table, None)
            .ok()?
            .find(|df| key_bcs == df.bcs_name)?;
        let field: Field<K, V> = self.get_move_object(&df.object_id).await.ok()?;
        Some(field.value)
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
                                // threading the epoch_store through this API does not
                                // seem possible, so we just read it from the state (self) and fetch
                                // the module cache out of it.
                                // Notice that no matter what module cache we get things
                                // should work
                                self.epoch_store.load().module_cache().as_ref(),
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
                            Err(UserInputError::ObjectNotFound {
                                object_id: *object_id,
                                version: Some(obj_ref.1),
                            }
                            .into())
                        }
                        Some(object) => {
                            let layout = object.get_layout(
                                ObjectFormatOptions::default(),
                                // threading the epoch_store through this API does not
                                // seem possible, so we just read it from the state (self) and fetch
                                // the module cache out of it.
                                // Notice that no matter what module cache we get things
                                // should work
                                self.epoch_store.load().module_cache().as_ref(),
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

    fn get_owner_at_version(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> Result<Owner, SuiError> {
        self.database
            .get_object_by_key(object_id, version)?
            .ok_or_else(|| {
                SuiError::from(UserInputError::ObjectNotFound {
                    object_id: *object_id,
                    version: Some(version),
                })
            })
            .map(|o| o.owner)
    }

    pub fn get_owner_objects(&self, owner: SuiAddress) -> SuiResult<Vec<ObjectInfo>> {
        if let Some(indexes) = &self.indexes {
            indexes.get_owner_objects(owner)
        } else {
            Err(SuiError::IndexStoreNotAvailable)
        }
    }

    pub fn get_owner_objects_iterator(
        &self,
        owner: SuiAddress,
    ) -> SuiResult<impl Iterator<Item = ObjectInfo> + '_> {
        if let Some(indexes) = &self.indexes {
            indexes.get_owner_objects_iterator(owner, ObjectID::ZERO, MAX_GET_OWNED_OBJECT_SIZE)
        } else {
            Err(SuiError::IndexStoreNotAvailable)
        }
    }

    pub async fn get_move_objects<T>(
        &self,
        owner: SuiAddress,
        type_: MoveObjectType,
    ) -> SuiResult<Vec<T>>
    where
        T: DeserializeOwned,
    {
        let object_ids = self
            .get_owner_objects_iterator(owner)?
            .filter(|o| match &o.type_ {
                ObjectType::Struct(s) => Self::matches_type_fuzzy_generics(&type_, s),
                ObjectType::Package => false,
            })
            .map(|info| info.object_id);
        let mut move_objects = vec![];
        for id in object_ids {
            move_objects.push(self.get_move_object(&id).await?)
        }
        Ok(move_objects)
    }

    // TODO: should be in impl MoveType
    fn matches_type_fuzzy_generics(type_: &MoveObjectType, other_type: &MoveObjectType) -> bool {
        type_.address() == other_type.address()
                    && type_.module() == other_type.module()
                    && type_.name() == other_type.name()
                    // TODO: is_empty() looks like a bug here. I think the intention is to support "fuzzy matching" where `get_move_objects`
                    // leaves type_params unspecified, but I don't actually see any call sites taking advantage of this
                    && (type_.type_params().is_empty() || type_.type_params() == other_type.type_params())
    }

    pub fn get_dynamic_fields(
        &self,
        owner: ObjectID,
        // exclusive cursor if `Some`, otherwise start from the beginning
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> SuiResult<Vec<DynamicFieldInfo>> {
        Ok(self
            .get_dynamic_fields_iterator(owner, cursor)?
            .take(limit)
            .collect())
    }

    pub fn get_dynamic_fields_iterator(
        &self,
        owner: ObjectID,
        cursor: Option<ObjectID>,
    ) -> SuiResult<impl Iterator<Item = DynamicFieldInfo> + '_> {
        if let Some(indexes) = &self.indexes {
            indexes.get_dynamic_fields_iterator(owner, cursor)
        } else {
            Err(SuiError::IndexStoreNotAvailable)
        }
    }

    pub fn get_dynamic_field_object_id(
        &self,
        owner: ObjectID,
        name: &DynamicFieldName,
    ) -> SuiResult<Option<ObjectID>> {
        if let Some(indexes) = &self.indexes {
            indexes.get_dynamic_field_object_id(owner, name)
        } else {
            Err(SuiError::IndexStoreNotAvailable)
        }
    }

    pub fn get_total_transaction_number(&self) -> Result<u64, anyhow::Error> {
        Ok(self.get_indexes()?.next_sequence_number())
    }

    pub fn get_transactions_in_range_deprecated(
        &self,
        start: TxSequenceNumber,
        end: TxSequenceNumber,
    ) -> Result<Vec<(TxSequenceNumber, TransactionDigest)>, anyhow::Error> {
        self.get_indexes()?
            .get_transactions_in_range_deprecated(start, end)
    }

    pub fn get_recent_transactions(
        &self,
        count: u64,
    ) -> Result<Vec<(TxSequenceNumber, TransactionDigest)>, anyhow::Error> {
        self.get_indexes()?.get_recent_transactions(count)
    }

    pub async fn get_executed_transaction_and_effects(
        &self,
        digest: TransactionDigest,
    ) -> Result<(VerifiedTransaction, TransactionEffects), anyhow::Error> {
        let transaction = self.database.get_transaction(&digest)?;
        let effects = self.database.get_executed_effects(&digest)?;
        match (transaction, effects) {
            (Some(transaction), Some(effects)) => Ok((transaction, effects)),
            _ => Err(anyhow!(SuiError::TransactionNotFound { digest })),
        }
    }

    pub async fn get_executed_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<VerifiedTransaction, anyhow::Error> {
        let transaction = self.database.get_transaction(&digest)?;
        transaction.ok_or_else(|| anyhow!(SuiError::TransactionNotFound { digest }))
    }

    pub async fn get_executed_effects(
        &self,
        digest: TransactionDigest,
    ) -> Result<TransactionEffects, anyhow::Error> {
        let effects = self.database.get_executed_effects(&digest)?;
        effects.ok_or_else(|| anyhow!(SuiError::TransactionNotFound { digest }))
    }

    pub async fn multi_get_executed_transactions(
        &self,
        digests: &[TransactionDigest],
    ) -> Result<Vec<Option<VerifiedTransaction>>, anyhow::Error> {
        Ok(self.database.multi_get_transactions(digests)?)
    }

    pub async fn multi_get_executed_effects(
        &self,
        digests: &[TransactionDigest],
    ) -> Result<Vec<Option<TransactionEffects>>, anyhow::Error> {
        Ok(self.database.multi_get_executed_effects(digests)?)
    }

    pub async fn multi_get_transaction_checkpoint(
        &self,
        digests: &[TransactionDigest],
    ) -> Result<Vec<Option<(EpochId, CheckpointSequenceNumber)>>, anyhow::Error> {
        Ok(self.database.multi_get_transaction_checkpoint(digests)?)
    }

    pub fn multi_get_events(
        &self,
        digests: &[TransactionEventsDigest],
    ) -> Result<Vec<Option<TransactionEvents>>, anyhow::Error> {
        Ok(self.database.multi_get_events(digests)?)
    }

    pub fn multi_get_checkpoint_by_sequence_number(
        &self,
        sequence_numbers: &[CheckpointSequenceNumber],
    ) -> SuiResult<Vec<Option<VerifiedCheckpoint>>> {
        Ok(self
            .checkpoint_store
            .multi_get_checkpoint_by_sequence_number(sequence_numbers)?)
    }

    pub fn get_transaction_events(
        &self,
        digest: &TransactionEventsDigest,
    ) -> SuiResult<TransactionEvents> {
        self.database
            .get_events(digest)?
            .ok_or(SuiError::TransactionEventsNotFound { digest: *digest })
    }

    fn get_indexes(&self) -> SuiResult<Arc<IndexStore>> {
        match &self.indexes {
            Some(i) => Ok(i.clone()),
            None => Err(SuiError::UnsupportedFeatureError {
                error: "extended object indexing is not enabled on this server".into(),
            }),
        }
    }

    pub fn get_transactions(
        &self,
        filter: Option<TransactionFilter>,
        // exclusive cursor if `Some`, otherwise start from the beginning
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        reverse: bool,
    ) -> Result<Vec<TransactionDigest>, anyhow::Error> {
        self.get_indexes()?
            .get_transactions(filter, cursor, limit, reverse)
    }

    fn get_checkpoint_store(&self) -> Arc<CheckpointStore> {
        self.checkpoint_store.clone()
    }

    pub fn get_latest_checkpoint_sequence_number(
        &self,
    ) -> Result<CheckpointSequenceNumber, anyhow::Error> {
        self.get_checkpoint_store()
            .get_highest_executed_checkpoint_seq_number()?
            .ok_or_else(|| anyhow!("Latest checkpoint sequence number not found"))
    }

    pub fn get_checkpoint_summary_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<CheckpointSummary, anyhow::Error> {
        let verified_checkpoint = self
            .get_checkpoint_store()
            .get_checkpoint_by_sequence_number(sequence_number)?;
        match verified_checkpoint {
            Some(verified_checkpoint) => Ok(verified_checkpoint.into_inner().into_data()),
            None => Err(anyhow!(
                "Verified checkpoint not found for sequence number {}",
                sequence_number
            )),
        }
    }

    pub fn get_checkpoint_summary_by_digest(
        &self,
        digest: CheckpointDigest,
    ) -> Result<CheckpointSummary, anyhow::Error> {
        let verified_checkpoint = self
            .get_checkpoint_store()
            .get_checkpoint_by_digest(&digest)?;
        match verified_checkpoint {
            Some(verified_checkpoint) => Ok(verified_checkpoint.into_inner().into_data()),
            None => Err(anyhow!(
                "Verified checkpoint not found for digest: {}",
                Base58::encode(digest)
            )),
        }
    }

    pub fn get_checkpoint_contents(
        &self,
        digest: CheckpointContentsDigest,
    ) -> Result<CheckpointContents, anyhow::Error> {
        self.get_checkpoint_store()
            .get_checkpoint_contents(&digest)?
            .ok_or_else(|| anyhow!("Checkpoint contents not found for digest: {:?}", digest))
    }

    pub fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<CheckpointContents, anyhow::Error> {
        let verified_checkpoint = self
            .get_checkpoint_store()
            .get_checkpoint_by_sequence_number(sequence_number)?;
        match verified_checkpoint {
            Some(verified_checkpoint) => {
                let content_digest = verified_checkpoint.into_inner().content_digest;
                self.get_checkpoint_contents(content_digest)
            }
            None => Err(anyhow!(
                "Verified checkpoint not found for sequence number {}",
                sequence_number
            )),
        }
    }

    pub async fn get_timestamp_ms(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<u64>, anyhow::Error> {
        Ok(self.get_indexes()?.get_timestamp_ms(digest)?)
    }

    pub async fn query_events(
        &self,
        query: EventQuery,
        // exclusive cursor if `Some`, otherwise start from the beginning
        cursor: Option<EventID>,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<(EventID, SuiEventEnvelope)>, anyhow::Error> {
        let index_store = self.get_indexes()?;

        //Get the tx_num from tx_digest
        let (tx_num, event_num) = if let Some(cursor) = cursor.as_ref() {
            let tx_seq = index_store.get_transaction_seq(&cursor.tx_digest)?.ok_or(
                SuiError::TransactionNotFound {
                    digest: cursor.tx_digest,
                },
            )?;
            (tx_seq, cursor.event_seq as usize)
        } else if descending {
            (u64::MAX, usize::MAX)
        } else {
            (0, 0)
        };

        let limit = limit + 1;
        let mut event_keys = match query {
            EventQuery::All => index_store.all_events(tx_num, event_num, limit, descending)?,
            EventQuery::Transaction(digest) => {
                index_store.events_by_transaction(&digest, tx_num, event_num, limit, descending)?
            }
            EventQuery::MoveModule { package, module } => {
                let module_id = ModuleId::new(
                    AccountAddress::from(package),
                    Identifier::from_str(&module)?,
                );
                index_store.events_by_module_id(&module_id, tx_num, event_num, limit, descending)?
            }
            EventQuery::MoveEvent(struct_name) => {
                let struct_name = parse_sui_struct_tag(&struct_name)?;
                index_store.events_by_move_event_struct_name(
                    &struct_name,
                    tx_num,
                    event_num,
                    limit,
                    descending,
                )?
            }
            EventQuery::Sender(sender) => {
                index_store.events_by_sender(&sender, tx_num, event_num, limit, descending)?
            }
            EventQuery::Recipient(recipient) => {
                index_store.events_by_recipient(&recipient, tx_num, event_num, limit, descending)?
            }
            EventQuery::Object(object) => {
                index_store.events_by_object(&object, tx_num, event_num, limit, descending)?
            }
            EventQuery::TimeRange {
                start_time,
                end_time,
            } => index_store
                .event_iterator(start_time, end_time, tx_num, event_num, limit, descending)?,
            EventQuery::EventType(event_type) => {
                index_store.events_by_type(&event_type, tx_num, event_num, limit, descending)?
            }
        };

        // skip one event if exclusive cursor is provided,
        // otherwise truncate to the original limit.
        if cursor.is_some() {
            if !event_keys.is_empty() {
                event_keys.remove(0);
            }
        } else {
            event_keys.truncate(limit - 1);
        }
        let keys = event_keys.iter().map(|(digest, _, seq, _)| (*digest, *seq));

        let stored_events = self
            .database
            .perpetual_tables
            .events
            .multi_get(keys)?
            .into_iter()
            .zip(event_keys.into_iter())
            .map(|(e, (digest, tx_digest, event_seq, timestamp))| {
                e.map(|e| (e, tx_digest, event_seq, timestamp))
                    .ok_or(SuiError::TransactionEventsNotFound { digest })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut events = vec![];
        for (e, tx_digest, event_seq, timestamp) in stored_events {
            let id = EventID {
                tx_digest,
                event_seq: event_seq as i64,
            };
            events.push((
                id.clone(),
                SuiEventEnvelope {
                    timestamp,
                    tx_digest,
                    id,
                    // threading the epoch_store through this API does not
                    // seem possible, so we just read it from the state (self) and fetch
                    // the module cache out of it.
                    // Notice that no matter what module cache we get things
                    // should work
                    event: SuiEvent::try_from(e, &**self.epoch_store.load().module_cache())?,
                },
            ))
        }
        Ok(events)
    }

    pub async fn insert_genesis_object(&self, object: Object) {
        self.database
            .insert_genesis_object(object)
            .await
            .expect("Cannot insert genesis object")
    }

    pub async fn insert_genesis_objects(&self, objects: &[Object]) {
        futures::future::join_all(
            objects
                .iter()
                .map(|o| self.insert_genesis_object(o.clone())),
        )
        .await;
    }

    pub fn get_certified_transaction(
        &self,
        tx_digest: &TransactionDigest,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> Result<Option<VerifiedCertificate>, SuiError> {
        let Some(cert_sig) = epoch_store.get_transaction_cert_sig(tx_digest)? else {
            return Ok(None);
        };
        let Some(transaction) = self.database.get_transaction(tx_digest)? else {
            return Ok(None);
        };

        Ok(Some(VerifiedCertificate::new_unchecked(
            CertifiedTransaction::new_from_data_and_sig(
                transaction.into_inner().into_data(),
                cert_sig,
            ),
        )))
    }

    /// Make a status response for a transaction
    pub fn get_transaction_status(
        &self,
        transaction_digest: &TransactionDigest,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> Result<Option<(SenderSignedData, TransactionStatus)>, SuiError> {
        // TODO: In the case of read path, we should not have to re-sign the effects.
        if let Some(effects) =
            self.get_signed_effects_and_maybe_resign(transaction_digest, epoch_store)?
        {
            if let Some(transaction) = self.database.get_transaction(transaction_digest)? {
                let cert_sig = epoch_store.get_transaction_cert_sig(transaction_digest)?;
                let events = if let Some(digest) = effects.events_digest() {
                    self.get_transaction_events(digest)?
                } else {
                    TransactionEvents::default()
                };
                return Ok(Some((
                    transaction.into_message(),
                    TransactionStatus::Executed(cert_sig, effects.into_inner(), events),
                )));
            } else {
                // The read of effects and read of transaction are not atomic. It's possible that we reverted
                // the transaction (during epoch change) in between the above two reads, and we end up
                // having effects but not transaction. In this case, we just fall through.
                debug!(tx_digest=?transaction_digest, "Signed effects exist but no transaction found");
            }
        }
        if let Some(signed) = epoch_store.get_signed_transaction(transaction_digest)? {
            self.metrics.tx_already_processed.inc();
            let (transaction, sig) = signed.into_inner().into_data_and_sig();
            Ok(Some((transaction, TransactionStatus::Signed(sig))))
        } else {
            Ok(None)
        }
    }

    /// Get the signed effects of the given transaction. If the effects was signed in a previous
    /// epoch, re-sign it so that the caller is able to form a cert of the effects in the current
    /// epoch.
    pub fn get_signed_effects_and_maybe_resign(
        &self,
        transaction_digest: &TransactionDigest,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<Option<VerifiedSignedTransactionEffects>> {
        let effects = self.database.get_executed_effects(transaction_digest)?;
        match effects {
            Some(effects) => Ok(Some(self.sign_effects(effects, epoch_store)?)),
            None => Ok(None),
        }
    }

    pub fn sign_effects(
        &self,
        effects: TransactionEffects,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> Result<VerifiedSignedTransactionEffects, SuiError> {
        let tx_digest = *effects.transaction_digest();
        let signed_effects = match epoch_store.get_effects_signature(&tx_digest)? {
            Some(sig) if sig.epoch == epoch_store.epoch() => {
                SignedTransactionEffects::new_from_data_and_sig(effects, sig)
            }
            _ => {
                // If the transaction was executed in previous epochs, the validator will
                // re-sign the effects with new current epoch so that a client is always able to
                // obtain an effects certificate at the current epoch.
                //
                // Why is this necessary? Consider the following case:
                // - assume there are 4 validators
                // - Quorum driver gets 2 signed effects before reconfig halt
                // - The tx makes it into final checkpoint.
                // - 2 validators go away and are replaced in the new epoch.
                // - The new epoch begins.
                // - The quorum driver cannot complete the partial effects cert from the previous epoch,
                //   because it may not be able to reach either of the 2 former validators.
                // - But, if the 2 validators that stayed are willing to re-sign the effects in the new
                //   epoch, the QD can make a new effects cert and return it to the client.
                //
                // This is a considered a short-term workaround. Eventually, Quorum Driver should be able
                // to return either an effects certificate, -or- a proof of inclusion in a checkpoint. In
                // the case above, the Quorum Driver would return a proof of inclusion in the final
                // checkpoint, and this code would no longer be necessary.
                //
                // Alternatively, some of the confusion around re-signing could be resolved if
                // CertifiedTransactionEffects included both the epoch in which the transaction became
                // final, as well as the epoch at which the effects were certified. In this case, there
                // would be nothing terribly odd about the validators from epoch N certifying that a
                // given TX became final in epoch N - 1. The confusion currently arises from the fact that
                // the epoch field in AuthoritySignInfo is overloaded both to identify the provenance of
                // the authority's signature, as well as to identify in which epoch the transaction was
                // executed.
                debug!(
                    ?tx_digest,
                    epoch=?epoch_store.epoch(),
                    "Re-signing the effects with the current epoch"
                );
                SignedTransactionEffects::new(
                    epoch_store.epoch(),
                    effects,
                    &*self.secret,
                    self.name,
                )
            }
        };
        Ok(VerifiedSignedTransactionEffects::new_unchecked(
            signed_effects,
        ))
    }

    // Helper function to manage transaction_locks

    /// Set the transaction lock to a specific transaction
    #[instrument(level = "trace", skip_all)]
    pub async fn set_transaction_lock(
        &self,
        mutable_input_objects: &[ObjectRef],
        signed_transaction: VerifiedSignedTransaction,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> Result<(), SuiError> {
        self.lock_and_write_transaction(mutable_input_objects, signed_transaction, epoch_store)
            .await
    }

    /// Acquires the transaction lock for a specific transaction, writing the transaction
    /// to the transaction column family if acquiring the lock succeeds.
    /// The lock service is used to atomically acquire locks.
    async fn lock_and_write_transaction(
        &self,
        owned_input_objects: &[ObjectRef],
        transaction: VerifiedSignedTransaction,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> Result<(), SuiError> {
        let tx_digest = *transaction.digest();

        // Acquire the lock on input objects
        self.database
            .acquire_transaction_locks(epoch_store.epoch(), owned_input_objects, tx_digest)
            .await?;

        // TODO: we should have transaction insertion be atomic with lock acquisition, or retry.
        // For now write transactions after because if we write before, there is a chance the lock can fail
        // and this can cause invalid transactions to be inserted in the table.
        // https://github.com/MystenLabs/sui/issues/1990
        epoch_store.insert_signed_transaction(transaction)?;

        Ok(())
    }

    /// Commit effects of transaction execution to data store.
    #[instrument(level = "trace", skip_all)]
    pub(crate) async fn commit_certificate(
        &self,
        inner_temporary_store: InnerTemporaryStore,
        certificate: &VerifiedExecutableTransaction,
        effects: &TransactionEffects,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        let _metrics_guard = self.metrics.commit_certificate_latency.start_timer();

        let tx_digest = certificate.digest();
        // Only need to sign effects if we are a validator.
        let effects_sig = if self.is_validator(epoch_store) {
            Some(AuthoritySignInfo::new(
                epoch_store.epoch(),
                effects,
                Intent::default().with_scope(IntentScope::TransactionEffects),
                self.name,
                &*self.secret,
            ))
        } else {
            None
        };
        // The insertion to epoch_store is not atomic with the insertion to the perpetual store. This is OK because
        // we insert to the epoch store first. And during lookups we always look up in the perpetual store first.
        epoch_store.insert_tx_cert_and_effects_signature(
            tx_digest,
            certificate.certificate_sig(),
            effects_sig.as_ref(),
        )?;

        self.database
            .update_state(
                inner_temporary_store,
                &certificate.clone().into_unsigned(),
                effects,
            )
            .await
            .tap_ok(|_| {
                debug!(?tx_digest, "commit_certificate finished");
            })?;

        // todo - ideally move this metric in NotifyRead once we have metrics in AuthorityStore
        self.metrics
            .pending_notify_read
            .set(self.database.executed_effects_notify_read.num_pending() as i64);

        Ok(())
    }

    /// Get the TransactionEnvelope that currently locks the given object, if any.
    /// Since object locks are only valid for one epoch, we also need the epoch_id in the query.
    /// Returns UserInputError::ObjectNotFound if no lock records for the given object can be found.
    /// Returns UserInputError::ObjectVersionUnavailableForConsumption if the object record is at a different version.
    /// Returns Some(VerifiedEnvelope) if the given ObjectRef is locked by a certain transaction.
    /// Returns None if the a lock record is initialized for the given ObjectRef but not yet locked by any transaction,
    ///     or cannot find the transaction in transaction table, because of data race etc.
    pub async fn get_transaction_lock(
        &self,
        object_ref: &ObjectRef,
        epoch_store: &AuthorityPerEpochStore,
    ) -> Result<Option<VerifiedSignedTransaction>, SuiError> {
        let lock_info = self
            .database
            .get_lock(*object_ref, epoch_store.epoch())
            .map_err(SuiError::from)?;
        let lock_info = match lock_info {
            ObjectLockStatus::LockedAtDifferentVersion { locked_ref } => {
                return Err(UserInputError::ObjectVersionUnavailableForConsumption {
                    provided_obj_ref: *object_ref,
                    current_version: locked_ref.1,
                }
                .into());
            }
            ObjectLockStatus::Initialized => {
                return Ok(None);
            }
            ObjectLockStatus::LockedToTx { locked_by_tx } => locked_by_tx,
        };
        // Returns None if either no TX with the lock, or TX present but no entry in transactions table.
        // However we retry a couple times because the TX is written after the lock is acquired, so it might
        // just be a race.
        let tx_digest = &lock_info.tx_digest;
        let mut retry_strategy = ExponentialBackoff::from_millis(2)
            .factor(10)
            .map(jitter)
            .take(3);

        let mut tx_option = epoch_store.get_signed_transaction(tx_digest)?;
        while tx_option.is_none() {
            if let Some(duration) = retry_strategy.next() {
                // Wait to retry
                tokio::time::sleep(duration).await;
                trace!("Retrying getting pending transaction");
            } else {
                // No more retries, just quit
                break;
            }
            tx_option = epoch_store.get_signed_transaction(tx_digest)?;
        }
        Ok(tx_option)
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

    /// Get the set of system packages that are compiled in to this build, if those packages are
    /// compatible with the current versions of those packages on-chain.
    pub async fn get_available_system_packages(&self) -> Vec<ObjectRef> {
        let Some(move_stdlib) = self.compare_system_package(
            MOVE_STDLIB_OBJECT_ID,
            sui_framework::get_move_stdlib(), &[],
        ) .await else {
            return vec![];
        };

        let (std_move_pkg, _) = sui_framework::make_std_sui_move_pkgs();
        let Some(sui_framework) = self.compare_system_package(
            SUI_FRAMEWORK_OBJECT_ID,
            sui_framework_injection::get_modules(self.name), [&std_move_pkg],
        ).await else {
            return vec![];
        };

        vec![move_stdlib, sui_framework]
    }

    /// Check whether the framework defined by `modules` is compatible with the framework that is
    /// already on-chain at `id`.
    ///
    /// - Returns `None` if the current package at `id` cannot be loaded, or the compatibility check
    ///   fails (This is grounds not to upgrade).
    /// - Panics if the object at `id` can be loaded but is not a package -- this is an invariant
    ///   violation.
    /// - Returns the digest of the current framework (and version) if it is equivalent to the new
    ///   framework (indicates support for a protocol upgrade without a framework upgrade).
    /// - Returns the digest of the new framework (and version) if it is compatible (indicates
    ///   support for a protocol upgrade with a framework upgrade).
    async fn compare_system_package<'p>(
        &self,
        id: ObjectID,
        modules: Vec<CompiledModule>,
        dependencies: impl IntoIterator<Item = &'p MovePackage>,
    ) -> Option<ObjectRef> {
        let cur_object = match self.get_object(&id).await {
            Ok(Some(cur_object)) => cur_object,

            Ok(None) => {
                error!("No framework package at {id}");
                return None;
            }

            Err(e) => {
                error!("Error loading framework object at {id}: {e:?}");
                return None;
            }
        };

        let cur_ref = cur_object.compute_object_reference();
        let cur_pkg = cur_object
            .data
            .try_as_package()
            .expect("Framework not package");

        let mut new_object = match Object::new_system_package(
            modules,
            // Start at the same version as the current package, and increment if compatibility is
            // successful
            cur_object.version(),
            cur_object.previous_transaction,
            dependencies,
        ) {
            Ok(object) => object,
            Err(e) => {
                error!("Failed to create new framework package for {id}: {e:?}");
                return None;
            }
        };

        if cur_ref == new_object.compute_object_reference() {
            return Some(cur_ref);
        }

        let check_struct_and_pub_function_linking = true;
        let check_struct_layout = true;
        let check_friend_linking = false;
        let compatibility = Compatibility::new(
            check_struct_and_pub_function_linking,
            check_struct_layout,
            check_friend_linking,
        );

        let new_pkg = new_object
            .data
            .try_as_package_mut()
            .expect("Created as package");

        let cur_normalized = cur_pkg.normalize().expect("Normalize existing package");
        let mut new_normalized = new_pkg.normalize().ok()?;

        for (name, cur_module) in cur_normalized {
            let Some(new_module) = new_normalized.remove(&name) else {
                return None;
            };

            if let Err(e) = compatibility.check(&cur_module, &new_module) {
                error!("Compatibility check failed, for new version of {id}: {e:?}");
                return None;
            }
        }

        new_pkg.increment_version();
        Some(new_object.compute_object_reference())
    }

    /// Return the new versions and module bytes for the packages that have been committed to for a
    /// framework upgrade, in `system_packages`.  Loads the module contents from the binary, and
    /// performs the following checks:
    ///
    /// - Whether its contents matches what is on-chain already, in which case no upgrade is
    ///   required, and its contents are omitted from the output.
    /// - Whether the contents in the binary can form a package whose digest matches the input,
    ///   meaning the framework will be upgraded, and this authority can satisfy that upgrade, in
    ///   which case the contents are included in the output.
    ///
    /// If the current version of the framework can't be loaded, the binary does not contain the
    /// bytes for that framework ID, or the resulting package fails the digest check, `None` is
    /// returned indicating that this authority cannot run the upgrade that the network voted on.
    async fn get_system_package_bytes(
        &self,
        system_packages: Vec<ObjectRef>,
    ) -> Option<Vec<(SequenceNumber, Vec<Vec<u8>>)>> {
        let ids: Vec<_> = system_packages.iter().map(|(id, _, _)| *id).collect();
        let objects = self.get_objects(&ids).await.expect("read cannot fail");

        let mut res = Vec::with_capacity(system_packages.len());
        for (system_package, object) in system_packages.into_iter().zip(objects.iter()) {
            let cur_object = object
                .as_ref()
                .unwrap_or_else(|| panic!("system package {:?} must exist", system_package.0));

            if cur_object.compute_object_reference() == system_package {
                // Skip this one because it doesn't need to be upgraded.
                info!("Framework {} does not need updating", system_package.0);
                continue;
            }

            let (std_move_pkg, _) = sui_framework::make_std_sui_move_pkgs();
            let (bytes, dependencies) = match system_package.0 {
                MOVE_STDLIB_OBJECT_ID => (sui_framework::get_move_stdlib_bytes(), vec![]),
                SUI_FRAMEWORK_OBJECT_ID => (
                    sui_framework_injection::get_bytes(self.name),
                    vec![&std_move_pkg],
                ),
                _ => panic!("Unrecognised framework: {}", system_package.0),
            };

            let new_object = Object::new_system_package(
                bytes
                    .iter()
                    .map(|m| CompiledModule::deserialize(m).unwrap())
                    .collect(),
                system_package.1,
                cur_object.previous_transaction,
                dependencies,
            )
            .unwrap();

            let new_ref = new_object.compute_object_reference();
            if new_ref != system_package {
                error!("Framework mismatch -- binary: {new_ref:?}\n  upgrade: {system_package:?}");
                return None;
            }

            res.push((system_package.1, bytes));
        }

        Some(res)
    }

    fn choose_protocol_version_and_system_packages(
        current_protocol_version: ProtocolVersion,
        committee: &Committee,
        protocol_config: &ProtocolConfig,
        capabilities: Vec<AuthorityCapabilities>,
    ) -> (ProtocolVersion, Vec<ObjectRef>) {
        let next_protocol_version = current_protocol_version + 1;

        // For each validator, gather the protocol version and system packages that it would like
        // to upgrade to in the next epoch.
        let mut desired_upgrades: Vec<_> = capabilities
            .into_iter()
            .filter_map(|mut cap| {
                // A validator that lists no packages is voting against any change at all.
                if cap.available_system_packages.is_empty() {
                    return None;
                }

                cap.available_system_packages.sort();

                info!(
                    "validator {:?} supports {:?} with system packages: {:?}",
                    cap.authority.concise(),
                    cap.supported_protocol_versions,
                    cap.available_system_packages,
                );

                // A validator that only supports the current protocol version is also voting
                // against any change, because framework upgrades always require a protocol version
                // bump.
                cap.supported_protocol_versions
                    .is_version_supported(next_protocol_version)
                    .then_some((cap.available_system_packages, cap.authority))
            })
            .collect();

        // There can only be one set of votes that have a majority, find one if it exists.
        desired_upgrades.sort();
        desired_upgrades
            .into_iter()
            .group_by(|(packages, _authority)| packages.clone())
            .into_iter()
            .find_map(|(packages, group)| {
                // should have been filtered out earlier.
                assert!(!packages.is_empty());

                let mut stake_aggregator: StakeAggregator<(), true> =
                    StakeAggregator::new(Arc::new(committee.clone()));

                for (_, authority) in group {
                    stake_aggregator.insert_generic(authority, ());
                }

                let total_votes = stake_aggregator.total_votes();
                let quorum_threshold = committee.quorum_threshold();
                let f = committee.total_votes - committee.quorum_threshold();
                let buffer_bps = protocol_config.buffer_stake_for_protocol_upgrade_bps();
                // multiple by buffer_bps / 10000, rounded up.
                let buffer_stake = (f * buffer_bps + 9999) / 10000;
                let effective_threshold = quorum_threshold + buffer_stake;

                info!(
                    ?total_votes,
                    ?quorum_threshold,
                    ?buffer_bps,
                    ?effective_threshold,
                    ?next_protocol_version,
                    ?packages,
                    "support for upgrade"
                );

                let has_support = total_votes >= effective_threshold;
                has_support.then_some((next_protocol_version, packages))
            })
            // if there was no majority, there is no upgrade
            .unwrap_or((current_protocol_version, vec![]))
    }

    /// Creates and execute the advance epoch transaction to effects without committing it to the database.
    /// The effects of the change epoch tx are only written to the database after a certified checkpoint has been
    /// formed and executed by CheckpointExecutor.
    ///
    /// When a framework upgraded has been decided on, but the validator does not have the new
    /// versions of the packages locally, the validator cannot form the ChangeEpochTx. In this case
    /// it returns Err, indicating that the checkpoint builder should give up trying to make the
    /// final checkpoint. As long as the network is able to create a certified checkpoint (which
    /// should be ensured by the capabilities vote), it will arrive via state sync and be executed
    /// by CheckpointExecutor.
    pub async fn create_and_execute_advance_epoch_tx(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        gas_cost_summary: &GasCostSummary,
        checkpoint: CheckpointSequenceNumber,
        epoch_start_timestamp_ms: CheckpointTimestamp,
    ) -> anyhow::Result<(SuiSystemState, TransactionEffects)> {
        let next_epoch = epoch_store.epoch() + 1;

        let (next_epoch_protocol_version, next_epoch_system_packages) =
            Self::choose_protocol_version_and_system_packages(
                epoch_store.protocol_version(),
                epoch_store.committee(),
                epoch_store.protocol_config(),
                epoch_store.get_capabilities(),
            );

        let Some(next_epoch_system_package_bytes) = self.get_system_package_bytes(
            next_epoch_system_packages.clone()
        ).await else {
            error!(
                "upgraded system packages {:?} are not locally available, cannot create \
                ChangeEpochTx. validator binary must be upgraded to the correct version!",
                next_epoch_system_packages
            );
            // the checkpoint builder will keep retrying forever when it hits this error.
            // Eventually, one of two things will happen:
            // - The operator will upgrade this binary to one that has the new packages locally,
            //   and this function will succeed.
            // - The final checkpoint will be certified by other validators, we will receive it via
            //   state sync, and execute it. This will upgrade the framework packages, reconfigure,
            //   and most likely shut down in the new epoch (this validator likely doesn't support
            //   the new protocol version, or else it should have had the packages.)
            return Err(anyhow!("missing system packages: cannot form ChangeEpochTx"));
        };

        let tx = VerifiedTransaction::new_change_epoch(
            next_epoch,
            next_epoch_protocol_version,
            gas_cost_summary.storage_cost,
            gas_cost_summary.computation_cost,
            gas_cost_summary.storage_rebate,
            epoch_start_timestamp_ms,
            next_epoch_system_package_bytes,
        );

        let executable_tx = VerifiedExecutableTransaction::new_from_checkpoint(
            tx.clone(),
            epoch_store.epoch(),
            checkpoint,
        );

        let tx_digest = executable_tx.digest();

        info!(
            ?next_epoch,
            ?next_epoch_protocol_version,
            ?next_epoch_system_packages,
            computation_cost=?gas_cost_summary.computation_cost,
            storage_cost=?gas_cost_summary.storage_cost,
            storage_rebase=?gas_cost_summary.storage_rebate,
            ?tx_digest,
            "Creating advance epoch transaction"
        );

        let _tx_lock = epoch_store.acquire_tx_lock(tx_digest).await;

        let execution_guard = self
            .database
            .execution_lock_for_executable_transaction(&executable_tx)
            .await?;
        let (temporary_store, effects) = self
            .prepare_certificate(&execution_guard, &executable_tx, epoch_store)
            .await?;
        let system_obj = temporary_store
            .get_sui_system_state_object()
            .expect("change epoch tx must write to system object");

        // We must write tx and effects to the state sync tables so that state sync is able to
        // deliver to the transaction to CheckpointExecutor after it is included in a certified
        // checkpoint.
        self.database
            .insert_transaction_and_effects(&tx, &effects)
            .map_err(|err| {
                let err: anyhow::Error = err.into();
                err
            })?;

        debug!(
            "Effects summary of the change epoch transaction: {:?}",
            effects.summary_for_debug()
        );
        epoch_store.record_is_safe_mode_metric(system_obj.safe_mode());
        // The change epoch transaction cannot fail to execute.
        assert!(effects.status().is_ok());
        Ok((system_obj, effects))
    }

    /// This function is called at the very end of the epoch.
    /// This step is required before updating new epoch in the db and calling reopen_epoch_db.
    async fn revert_uncommitted_epoch_transactions(
        &self,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult {
        {
            let state = epoch_store.get_reconfig_state_write_lock_guard();
            if state.should_accept_user_certs() {
                // Need to change this so that consensus adapter do not accept certificates from user.
                // This can happen if our local validator did not initiate epoch change locally,
                // but 2f+1 nodes already concluded the epoch.
                //
                // This lock is essentially a barrier for
                // `epoch_store.pending_consensus_certificates` table we are reading on the line after this block
                epoch_store.close_user_certs(state);
            }
            // lock is dropped here
        }
        let pending_certificates = epoch_store.pending_consensus_certificates();
        debug!(
            "Reverting {} locally executed transactions that was not included in the epoch",
            pending_certificates.len()
        );
        for digest in pending_certificates {
            if self
                .database
                .is_transaction_executed_in_checkpoint(&digest)?
            {
                debug!("Not reverting pending consensus transaction {:?} - it was included in checkpoint", digest);
                continue;
            }
            debug!("Reverting {:?} at the end of epoch", digest);
            self.database.revert_state_update(&digest).await?;
        }
        debug!("All uncommitted local transactions reverted");
        Ok(())
    }

    async fn reopen_epoch_db(
        &self,
        cur_epoch_store: &AuthorityPerEpochStore,
        new_committee: Committee,
        epoch_start_configuration: EpochStartConfiguration,
    ) -> SuiResult<Arc<AuthorityPerEpochStore>> {
        let new_epoch = new_committee.epoch;
        info!(new_epoch = ?new_epoch, "re-opening AuthorityEpochTables for new epoch");
        assert_eq!(
            epoch_start_configuration.epoch_start_state().epoch(),
            new_committee.epoch
        );
        self.db()
            .set_epoch_start_configuration(&epoch_start_configuration)
            .await?;
        fail_point!("before-open-new-epoch-store");
        let new_epoch_store = cur_epoch_store.new_at_next_epoch(
            self.name,
            new_committee,
            epoch_start_configuration,
            self.db(),
        );
        self.epoch_store.store(new_epoch_store.clone());
        cur_epoch_store.epoch_terminated().await;
        Ok(new_epoch_store)
    }

    #[cfg(test)]
    pub(crate) fn shutdown_execution_for_test(&self) {
        self.tx_execution_shutdown
            .lock()
            .take()
            .unwrap()
            .send(())
            .unwrap();
    }
}

#[cfg(msim)]
pub mod sui_framework_injection {
    use std::cell::RefCell;

    use super::*;

    // Thread local cache because all simtests run in a single unique thread.
    thread_local! {
        static OVERRIDE: RefCell<FrameworkOverrideConfig> = RefCell::new(FrameworkOverrideConfig::Default);
    }

    type Framework = Vec<CompiledModule>;

    type FrameworkUpgradeCallback =
        Box<dyn Fn(AuthorityName) -> Option<Framework> + Send + Sync + 'static>;

    enum FrameworkOverrideConfig {
        Default,
        Global(Framework),
        PerValidator(FrameworkUpgradeCallback),
    }

    fn compiled_modules_to_bytes(modules: &[CompiledModule]) -> Vec<Vec<u8>> {
        modules
            .iter()
            .map(|m| {
                let mut buf = Vec::new();
                m.serialize(&mut buf).unwrap();
                buf
            })
            .collect()
    }

    pub fn set_override(modules: Vec<CompiledModule>) {
        OVERRIDE.with(|bs| *bs.borrow_mut() = FrameworkOverrideConfig::Global(modules));
    }

    pub fn set_override_cb(func: FrameworkUpgradeCallback) {
        OVERRIDE.with(|bs| *bs.borrow_mut() = FrameworkOverrideConfig::PerValidator(func));
    }

    pub fn get_bytes(name: AuthorityName) -> Vec<Vec<u8>> {
        OVERRIDE.with(|cfg| match &*cfg.borrow() {
            FrameworkOverrideConfig::Default => sui_framework::get_sui_framework_bytes(),
            FrameworkOverrideConfig::Global(framework) => compiled_modules_to_bytes(framework),
            FrameworkOverrideConfig::PerValidator(func) => func(name)
                .map(|fw| compiled_modules_to_bytes(&fw))
                .unwrap_or_else(sui_framework::get_sui_framework_bytes),
        })
    }

    pub fn get_modules(name: AuthorityName) -> Vec<CompiledModule> {
        OVERRIDE.with(|cfg| match &*cfg.borrow() {
            FrameworkOverrideConfig::Default => sui_framework::get_sui_framework(),
            FrameworkOverrideConfig::Global(framework) => framework.clone(),
            FrameworkOverrideConfig::PerValidator(func) => {
                func(name).unwrap_or_else(sui_framework::get_sui_framework)
            }
        })
    }
}

#[cfg(not(msim))]
pub mod sui_framework_injection {
    use move_binary_format::CompiledModule;

    use super::*;

    pub fn get_bytes(_name: AuthorityName) -> Vec<Vec<u8>> {
        sui_framework::get_sui_framework_bytes()
    }

    pub fn get_modules(_name: AuthorityName) -> Vec<CompiledModule> {
        sui_framework::get_sui_framework()
    }
}
