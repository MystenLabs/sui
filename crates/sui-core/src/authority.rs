// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{collections::HashMap, fs, pin::Pin, sync::Arc};

use anyhow::anyhow;
use arc_swap::{ArcSwap, Guard};
use chrono::prelude::*;
use fastcrypto::encoding::Base58;
use fastcrypto::encoding::Encoding;
use itertools::Itertools;
use move_binary_format::CompiledModule;
use move_core_types::language_storage::ModuleId;
use parking_lot::Mutex;
use prometheus::{
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_vec_with_registry,
    register_int_gauge_with_registry, Histogram, IntCounter, IntCounterVec, IntGauge, IntGaugeVec,
    Registry,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tap::{TapFallible, TapOptional};
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::oneshot;
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tracing::{debug, error, info, instrument, trace, warn, Instrument};

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
use sui_config::node::{
    AuthorityStorePruningConfig, DBCheckpointConfig, ExpensiveSafetyCheckConfig,
};
use sui_config::transaction_deny_config::TransactionDenyConfig;
use sui_framework::{BuiltInFramework, SystemPackage};
use sui_json_rpc_types::{
    Checkpoint, DevInspectResults, DryRunTransactionBlockResponse, EventFilter, SuiEvent,
    SuiMoveValue, SuiObjectDataFilter, SuiTransactionBlockData, SuiTransactionBlockEffects,
    SuiTransactionBlockEvents, TransactionFilter,
};
use sui_macros::{fail_point, fail_point_async};
use sui_protocol_config::{ProtocolConfig, SupportedProtocolVersions};
use sui_storage::indexes::{CoinInfo, ObjectIndexChanges};
use sui_storage::IndexStore;
use sui_types::committee::{EpochId, ProtocolVersion};
use sui_types::crypto::{
    default_hash, AggregateAuthoritySignature, AuthorityKeyPair, AuthoritySignInfo, NetworkKeyPair,
    Signer,
};
use sui_types::digests::TransactionEventsDigest;
use sui_types::dynamic_field::{DynamicFieldInfo, DynamicFieldName, DynamicFieldType, Field};
use sui_types::error::{ExecutionError, UserInputError};
use sui_types::event::{Event, EventID};
use sui_types::gas::{GasCostSummary, SuiGasStatus};
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::{
    CheckpointCommitment, CheckpointContents, CheckpointContentsDigest, CheckpointDigest,
    CheckpointSequenceNumber, CheckpointSummary, CheckpointTimestamp, VerifiedCheckpoint,
};
use sui_types::messages_checkpoint::{CheckpointRequest, CheckpointResponse};
use sui_types::metrics::LimitsMetrics;
use sui_types::object::{MoveObject, Owner, PastObjectRead, OBJECT_START_VERSION};
use sui_types::storage::{ObjectKey, ObjectStore, WriteKind};
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::sui_system_state::SuiSystemStateTrait;
pub use sui_types::temporary_store::TemporaryStore;
use sui_types::temporary_store::{
    InnerTemporaryStore, ObjectMap, TemporaryModuleResolver, TxCoins, WrittenObjects,
};
use sui_types::{
    base_types::*,
    committee::Committee,
    crypto::AuthoritySignature,
    error::{SuiError, SuiResult},
    fp_ensure,
    messages::*,
    object::{Object, ObjectFormatOptions, ObjectRead},
    SUI_SYSTEM_ADDRESS,
};
use sui_types::{is_system_package, TypeTag};
use typed_store::Map;

use crate::authority::authority_per_epoch_store::{AuthorityPerEpochStore, CertTxGuard};
use crate::authority::authority_per_epoch_store_pruner::AuthorityPerEpochStorePruner;
use crate::authority::authority_store::{ExecutionLockReadGuard, InputKey, ObjectLockStatus};
use crate::authority::authority_store_pruner::AuthorityStorePruner;
use crate::authority::epoch_start_configuration::EpochStartConfigTrait;
use crate::authority::epoch_start_configuration::EpochStartConfiguration;
use crate::checkpoints::checkpoint_executor::CheckpointExecutor;
use crate::checkpoints::CheckpointStore;
use crate::epoch::committee_store::CommitteeStore;
use crate::event_handler::SubscriptionHandler;
use crate::execution_driver::execution_process;
use crate::module_cache_metrics::ResolverMetrics;
use crate::stake_aggregator::StakeAggregator;
use crate::state_accumulator::StateAccumulator;
use crate::{transaction_input_checker, transaction_manager::TransactionManager};

#[cfg(test)]
#[cfg(feature = "test-utils")]
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

#[cfg(feature = "test-utils")]
pub mod authority_test_utils;
use once_cell::sync::OnceCell;
use sui_types::effects::{
    SignedTransactionEffects, TransactionEffects, TransactionEffectsAPI, TransactionEvents,
    VerifiedCertifiedTransactionEffects, VerifiedSignedTransactionEffects,
};

pub mod authority_per_epoch_store;
pub mod authority_per_epoch_store_pruner;

pub mod authority_store_pruner;
pub mod authority_store_tables;
pub mod authority_store_types;
pub mod epoch_start_configuration;
pub mod test_authority_builder;

pub(crate) mod authority_notify_read;
pub(crate) mod authority_store;

static CHAIN_IDENTIFIER: OnceCell<CheckpointDigest> = OnceCell::new();

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
    sponsored_tx: IntCounter,
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
    pub(crate) execution_driver_dispatch_queue: IntGauge,

    pub(crate) skipped_consensus_txns: IntCounter,
    pub(crate) skipped_consensus_txns_cache_hit: IntCounter,

    /// Post processing metrics
    post_processing_total_events_emitted: IntCounter,
    post_processing_total_tx_indexed: IntCounter,
    post_processing_total_tx_had_event_processed: IntCounter,

    pending_notify_read: IntGauge,

    /// Consensus handler metrics
    pub consensus_handler_processed_batches: IntCounter,
    pub consensus_handler_processed_bytes: IntCounter,
    pub consensus_handler_processed: IntCounterVec,
    pub consensus_handler_num_low_scoring_authorities: IntGauge,
    pub consensus_handler_scores: IntGaugeVec,
    pub consensus_committed_subdags: IntCounterVec,
    pub consensus_committed_certificates: IntCounterVec,

    pub limits_metrics: Arc<LimitsMetrics>,
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

            sponsored_tx: register_int_counter_with_registry!(
                "num_sponsored_tx",
                "Number of sponsored transactions",
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
            execution_driver_dispatch_queue: register_int_gauge_with_registry!(
                "execution_driver_dispatch_queue",
                "Number of transaction pending in execution driver dispatch queue",
                registry,
            )
            .unwrap(),
            skipped_consensus_txns: register_int_counter_with_registry!(
                "skipped_consensus_txns",
                "Total number of consensus transactions skipped",
                registry,
            )
            .unwrap(),
            skipped_consensus_txns_cache_hit: register_int_counter_with_registry!(
                "skipped_consensus_txns_cache_hit",
                "Total number of consensus transactions skipped because of local cache hit",
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
                .unwrap(),
            consensus_handler_num_low_scoring_authorities: register_int_gauge_with_registry!(
                "consensus_handler_num_low_scoring_authorities",
                "Number of low scoring authorities based on reputation scores from consensus",
                registry
            ).unwrap(),
            consensus_handler_scores: register_int_gauge_vec_with_registry!(
                "consensus_handler_scores",
                "scores from consensus for each authority",
                &["authority"],
                registry,
            )
                .unwrap(),
            consensus_committed_subdags: register_int_counter_vec_with_registry!(
                "consensus_committed_subdags",
                "Number of committed subdags, sliced by author",
                &["authority"],
                registry,
            )
                .unwrap(),
            consensus_committed_certificates: register_int_counter_vec_with_registry!(
                "consensus_committed_certificates",
                "Number of committed certificates, sliced by author",
                &["authority"],
                registry,
            )
                .unwrap(),
            limits_metrics: Arc::new(LimitsMetrics::new(registry)),
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

    pub indexes: Option<Arc<IndexStore>>,

    pub subscription_handler: Arc<SubscriptionHandler>,
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

    /// Config controlling what kind of expensive safety checks to perform.
    expensive_safety_check_config: ExpensiveSafetyCheckConfig,

    transaction_deny_config: TransactionDenyConfig,
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

    pub fn get_epoch_state_commitments(
        &self,
        epoch: EpochId,
    ) -> SuiResult<Option<Vec<CheckpointCommitment>>> {
        let commitments =
            self.checkpoint_store
                .get_epoch_last_checkpoint(epoch)?
                .map(|checkpoint| {
                    checkpoint
                        .end_of_epoch_data
                        .as_ref()
                        .expect("Last checkpoint of epoch expected to have EndOfEpochData")
                        .epoch_commitments
                        .clone()
                });
        Ok(commitments)
    }

    /// This is a private method and should be kept that way. It doesn't check whether
    /// the provided transaction is a system transaction, and hence can only be called internally.
    async fn handle_transaction_impl(
        &self,
        transaction: VerifiedTransaction,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<VerifiedSignedTransaction> {
        let (_gas_status, input_objects) = transaction_input_checker::check_transaction_input(
            &self.database,
            epoch_store.as_ref(),
            &transaction.data().intent_message().value,
            &self.transaction_deny_config,
        )
        .await?;

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
        fp_ensure!(
            !transaction.is_system_tx(),
            SuiError::InvalidSystemTransaction
        );

        let tx_digest = *transaction.digest();
        debug!("handle_transaction");

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
        objects: &Vec<Object>,
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

        if !objects.is_empty() {
            debug!(
                "inserting objects to object store before executing a tx: {:?}",
                objects
                    .iter()
                    .map(|o| (o.id(), o.version()))
                    .collect::<Vec<_>>()
            );
            self.database
                .fullnode_fast_path_insert_objects_to_object_store_maybe(objects)?;
            self.transaction_manager()
                .objects_available(objects.iter().map(InputKey::from).collect(), epoch_store);
        }

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
            // for execution, done in AuthorityPerEpochStore::handle_consensus_transaction().
            // For owned object transactions, they can be enqueued for execution immediately.
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
        expected_effects_digest: Option<TransactionEffectsDigest>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<(TransactionEffects, Option<ExecutionError>)> {
        let _metrics_guard = self.metrics.internal_execution_latency.start_timer();
        let tx_digest = *certificate.digest();
        debug!("execute_certificate_internal");

        // This acquires a lock on the tx digest to prevent multiple concurrent executions of the
        // same tx. While we don't need this for safety (tx sequencing is ultimately atomic), it is
        // very common to receive the same tx multiple times simultaneously due to gossip, so we
        // may as well hold the lock and save the cpu time for other requests.
        let tx_guard = epoch_store.acquire_tx_guard(certificate).await?;

        self.process_certificate(tx_guard, certificate, expected_effects_digest, epoch_store)
            .await
            .tap_err(|e| info!(?tx_digest, "process_certificate failed: {e}"))
    }

    /// Test only wrapper for `try_execute_immediately()` above, useful for checking errors if the
    /// pre-conditions are not satisfied, and executing change epoch transactions.
    pub async fn try_execute_for_test(
        &self,
        certificate: &VerifiedCertificate,
    ) -> SuiResult<(VerifiedSignedTransactionEffects, Option<ExecutionError>)> {
        let epoch_store = self.epoch_store_for_testing();
        let (effects, execution_error_opt) = self
            .try_execute_immediately(
                &VerifiedExecutableTransaction::new_from_certificate(certificate.clone()),
                None,
                &epoch_store,
            )
            .await?;
        let signed_effects = self.sign_effects(effects, &epoch_store)?;
        Ok((signed_effects, execution_error_opt))
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
        tx_guard: CertTxGuard,
        certificate: &VerifiedExecutableTransaction,
        expected_effects_digest: Option<TransactionEffectsDigest>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<(TransactionEffects, Option<ExecutionError>)> {
        let digest = *certificate.digest();
        // The cert could have been processed by a concurrent attempt of the same cert, so check if
        // the effects have already been written.
        if let Some(effects) = self.database.get_executed_effects(&digest)? {
            tx_guard.release();
            return Ok((effects, None));
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
            info!("The epoch of the execution_guard doesn't match the epoch store");
            return Err(SuiError::WrongEpoch {
                expected_epoch: epoch_store.epoch(),
                actual_epoch: *execution_guard,
            });
        }

        // Errors originating from prepare_certificate may be transient (failure to read locks) or
        // non-transient (transaction input is invalid, move vm errors). However, all errors from
        // this function occur before we have written anything to the db, so we commit the tx
        // guard and rely on the client to retry the tx (if it was transient).
        let (inner_temporary_store, effects, execution_error_opt) = match self
            .prepare_certificate(&execution_guard, certificate, epoch_store)
            .await
        {
            Err(e) => {
                info!(name = ?self.name, ?digest, "Error preparing transaction: {e}");
                tx_guard.release();
                return Err(e);
            }
            Ok(res) => res,
        };

        if let Some(expected_effects_digest) = expected_effects_digest {
            if effects.digest() != expected_effects_digest {
                error!(
                    tx_digest = ?digest,
                    ?expected_effects_digest,
                    actual_effects = ?effects,
                    "fork detected!"
                );
                panic!(
                    "Transaction {} is expected to have effects digest {}, but got {}!",
                    digest,
                    expected_effects_digest,
                    effects.digest(),
                );
            }
        }

        fail_point_async!("crash");

        self.commit_cert_and_notify(
            certificate,
            inner_temporary_store,
            &effects,
            tx_guard,
            execution_guard,
            epoch_store,
        )
        .await?;
        Ok((effects, execution_error_opt))
    }

    async fn commit_cert_and_notify(
        &self,
        certificate: &VerifiedExecutableTransaction,
        inner_temporary_store: InnerTemporaryStore,
        effects: &TransactionEffects,
        tx_guard: CertTxGuard,
        _execution_guard: ExecutionLockReadGuard<'_>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        let input_object_count = inner_temporary_store.objects.len();
        let shared_object_count = effects.shared_objects().len();
        let digest = *certificate.digest();

        // If commit_certificate returns an error, tx_guard will be dropped and the certificate
        // will be persisted in the log for later recovery.
        let output_keys: Vec<_> = inner_temporary_store
            .written
            .iter()
            .map(|(_, ((id, seq, _), obj, _))| InputKey(*id, (!obj.is_package()).then_some(*seq)))
            .collect();

        let events = inner_temporary_store.events.clone();

        let loaded_child_objects = if self.is_fullnode(epoch_store) {
            // We only care about this for full nodes
            inner_temporary_store.loaded_child_objects.clone()
        } else {
            BTreeMap::new()
        };

        let tx_coins = self
            .commit_certificate(inner_temporary_store, certificate, effects, epoch_store)
            .await?;

        // commit_certificate finished, the tx is fully committed to the store.
        tx_guard.commit_tx();

        // Notifies transaction manager about transaction and output objects committed.
        // This provides necessary information to transaction manager to start executing
        // additional ready transactions.
        //
        // REQUIRED: this must be called after commit_certificate() (above), which writes output
        // objects into storage. Otherwise, the transaction manager may schedule a transaction
        // before the output objects are actually available.
        self.transaction_manager
            .notify_commit(&digest, output_keys, epoch_store);

        // index certificate
        let _ = self
            .post_process_one_tx(
                certificate,
                effects,
                &events,
                epoch_store,
                tx_coins,
                loaded_child_objects,
            )
            .await
            .tap_err(|e| error!("tx post processing failed: {e}"));

        // Update metrics.
        self.metrics.total_effects.inc();
        self.metrics.total_certs.inc();

        if shared_object_count > 0 {
            self.metrics.shared_obj_tx.inc();
        }

        if certificate.is_sponsored_tx() {
            self.metrics.sponsored_tx.inc();
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
                .intent_message()
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
    ) -> SuiResult<(
        InnerTemporaryStore,
        TransactionEffects,
        Option<ExecutionError>,
    )> {
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
        let transaction_data = &certificate.data().intent_message().value;
        let (kind, signer, gas) = transaction_data.execution_parts();
        let (inner_temp_store, effects, execution_error_opt) =
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
                self.metrics.limits_metrics.clone(),
                // TODO: would be nice to pass the whole NodeConfig here, but it creates a
                // cyclic dependency w/ sui-adapter
                self.expensive_safety_check_config
                    .enable_deep_per_tx_sui_conservation_check(),
            );

        Ok((inner_temp_store, effects, execution_error_opt.err()))
    }

    pub async fn dry_exec_transaction(
        &self,
        transaction: TransactionData,
        transaction_digest: TransactionDigest,
    ) -> Result<
        (
            DryRunTransactionBlockResponse,
            BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)>,
            TransactionEffects,
            Option<ObjectID>,
        ),
        anyhow::Error,
    > {
        let epoch_store = self.load_epoch_store_one_call_per_task();
        if !self.is_fullnode(&epoch_store) {
            return Err(anyhow!("dry-exec is only supported on fullnodes"));
        }
        match transaction.kind() {
            TransactionKind::ProgrammableTransaction(_) => (),
            TransactionKind::ChangeEpoch(_)
            | TransactionKind::Genesis(_)
            | TransactionKind::ConsensusCommitPrologue(_) => {
                return Err(anyhow!("dry-exec does not support system transactions"));
            }
        }

        // make a gas object if one was not provided
        let mut gas_object_refs = transaction.gas().to_vec();
        let ((gas_status, input_objects), mock_gas) = if transaction.gas().is_empty() {
            let sender = transaction.sender();
            // use a 100M sui coin
            const MIST_TO_SUI: u64 = 1_000_000_000;
            const DRY_RUN_SUI: u64 = 100_000_000;
            let max_coin_value = MIST_TO_SUI * DRY_RUN_SUI;
            let gas_object_id = ObjectID::random();
            let gas_object = Object::new_move(
                MoveObject::new_gas_coin(OBJECT_START_VERSION, gas_object_id, max_coin_value),
                Owner::AddressOwner(sender),
                TransactionDigest::genesis(),
            );
            let gas_object_ref = gas_object.compute_object_reference();
            gas_object_refs = vec![gas_object_ref];
            (
                transaction_input_checker::check_transaction_input_with_given_gas(
                    &self.database,
                    epoch_store.as_ref(),
                    &transaction,
                    gas_object,
                )
                .await?,
                Some(gas_object_id),
            )
        } else {
            (
                transaction_input_checker::check_transaction_input(
                    &self.database,
                    epoch_store.as_ref(),
                    &transaction,
                    &self.transaction_deny_config,
                )
                .await?,
                None,
            )
        };

        let shared_object_refs = input_objects.filter_shared_objects();

        let transaction_dependencies = input_objects.transaction_dependencies();
        let temporary_store = TemporaryStore::new_for_mock_transaction(
            self.database.clone(),
            input_objects,
            transaction_digest,
            epoch_store.protocol_config(),
        );
        let (kind, signer, _) = transaction.execution_parts();
        // don't bother with paranoid checks in dry run
        let enable_move_vm_paranoid_checks = false;
        let move_vm = Arc::new(
            adapter::new_move_vm(
                epoch_store.native_functions().clone(),
                epoch_store.protocol_config(),
                enable_move_vm_paranoid_checks,
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
                self.metrics.limits_metrics.clone(),
                false, // enable_expensive_checks
            );
        let tx_digest = *effects.transaction_digest();

        let module_cache =
            TemporaryModuleResolver::new(&inner_temp_store, epoch_store.module_cache().clone());

        // Returning empty vector here because we recalculate changes in the rpc layer.
        let object_changes = Vec::new();

        // Returning empty vector here because we recalculate changes in the rpc layer.
        let balance_changes = Vec::new();

        Ok((
            DryRunTransactionBlockResponse {
                input: SuiTransactionBlockData::try_from(transaction.clone(), &module_cache)?,
                effects: effects.clone().try_into()?,
                events: SuiTransactionBlockEvents::try_from(
                    inner_temp_store.events.clone(),
                    tx_digest,
                    None,
                    &module_cache,
                )?,
                object_changes,
                balance_changes,
            },
            inner_temp_store.written,
            effects,
            mock_gas,
        ))
    }

    /// The object ID for gas can be any object ID, even for an uncreated object
    pub async fn dev_inspect_transaction_block(
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

        let gas_price = match gas_price {
            None => epoch_store.reference_gas_price(),
            Some(gas) => {
                if gas == 0 {
                    epoch_store.reference_gas_price()
                } else {
                    gas
                }
            }
        };
        let protocol_config = epoch_store.protocol_config();
        let max_tx_gas = protocol_config.max_tx_gas();

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

        let gas_budget = max_tx_gas;
        let data = TransactionData::new(
            transaction_kind,
            sender,
            gas_object_ref,
            gas_price,
            gas_budget,
        );
        let transaction_digest = TransactionDigest::new(default_hash(&data));
        let transaction_kind = data.into_kind();
        let transaction_dependencies = input_objects.transaction_dependencies();
        let temporary_store = TemporaryStore::new_for_mock_transaction(
            self.database.clone(),
            input_objects,
            transaction_digest,
            protocol_config,
        );
        let gas_status = SuiGasStatus::new_with_budget(max_tx_gas, gas_price, protocol_config);
        let move_vm = Arc::new(
            adapter::new_move_vm(
                epoch_store.native_functions().clone(),
                epoch_store.protocol_config(),
                self.expensive_safety_check_config
                    .enable_move_vm_paranoid_checks(),
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
                self.metrics.limits_metrics.clone(),
                false, // enable_expensive_checks
            );

        let module_cache =
            TemporaryModuleResolver::new(&inner_temp_store, epoch_store.module_cache().clone());

        DevInspectResults::new(
            effects,
            inner_temp_store.events.clone(),
            execution_result,
            &module_cache,
        )
    }

    // Only used for testing because of how epoch store is loaded.
    pub fn reference_gas_price_for_testing(&self) -> Result<u64, anyhow::Error> {
        let epoch_store = self.epoch_store_for_testing();
        Ok(epoch_store.reference_gas_price())
    }

    pub fn is_tx_already_executed(&self, digest: &TransactionDigest) -> SuiResult<bool> {
        self.database.is_tx_already_executed(digest)
    }

    #[instrument(level = "debug", skip_all, err)]
    async fn index_tx(
        &self,
        indexes: &IndexStore,
        digest: &TransactionDigest,
        // TODO: index_tx really just need the transaction data here.
        cert: &VerifiedExecutableTransaction,
        effects: &TransactionEffects,
        events: &TransactionEvents,
        timestamp_ms: u64,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        tx_coins: Option<TxCoins>,
        loaded_child_objects: BTreeMap<ObjectID, SequenceNumber>,
    ) -> SuiResult<u64> {
        let changes = self
            .process_object_index(effects, epoch_store)
            .tap_err(|e| warn!("{e}"))?;

        indexes
            .index_tx(
                cert.data().intent_message().value.sender(),
                cert.data()
                    .intent_message()
                    .value
                    .input_objects()?
                    .iter()
                    .map(|o| o.object_id()),
                effects
                    .all_changed_objects()
                    .into_iter()
                    .map(|(obj_ref, owner, _kind)| (*obj_ref, *owner)),
                cert.data()
                    .intent_message()
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
                tx_coins,
                loaded_child_objects,
            )
            .await
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
        for (id, _, _) in effects.deleted().iter().chain(effects.wrapped()) {
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

        for (oref, owner, kind) in effects.all_changed_objects() {
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
        let Some(move_object) = o.data.try_as_move().cloned() else {
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
        tx_coins: Option<TxCoins>,
        loaded_child_objects: BTreeMap<ObjectID, SequenceNumber>,
    ) -> SuiResult {
        if self.indexes.is_none() {
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
                    tx_coins,
                    loaded_child_objects,
                )
                .await
                .tap_ok(|_| self.metrics.post_processing_total_tx_indexed.inc())
                .tap_err(|e| error!(?tx_digest, "Post processing - Couldn't index tx: {e}"));
            let effects: SuiTransactionBlockEffects = effects.clone().try_into()?;
            // Emit events
            if res.is_ok() {
                self.subscription_handler
                    .process_tx(
                        certificate.data().transaction_data(),
                        &effects,
                        &SuiTransactionBlockEvents::try_from(
                            events.clone(),
                            *tx_digest,
                            Some(timestamp_ms),
                            epoch_store.module_cache(),
                        )?,
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
                    .get_object_or_tombstone(request.object_id)
                    .await?
                    .ok_or_else(|| {
                        SuiError::from(UserInputError::ObjectNotFound {
                            object_id: request.object_id,
                            version: None,
                        })
                    })?;
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

    pub fn load_fastpath_input_objects(
        &self,
        effects: &TransactionEffects,
    ) -> Result<Vec<Object>, SuiError> {
        // Note: any future addition to the returned object list needs cautions
        // to make sure not to mess up object pruning.

        let clock_ref = effects
            .shared_objects()
            .iter()
            .find(|(id, _, _)| id.is_clock());

        if let Some((id, version, digest)) = clock_ref {
            let clock_obj = self.database.get_object_by_key(id, *version)?;
            debug_assert!(clock_obj.is_some());
            debug_assert_eq!(
                clock_obj.as_ref().unwrap().compute_object_reference().2,
                *digest
            );
            Ok(clock_obj
                .tap_none(|| error!("Clock object not found: {:?}", clock_ref))
                .into_iter()
                .collect())
        } else {
            Ok(vec![])
        }
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

    #[allow(clippy::disallowed_methods)] // allow unbounded_channel()
    pub async fn new(
        name: AuthorityName,
        secret: StableSyncAuthoritySigner,
        supported_protocol_versions: SupportedProtocolVersions,
        store: Arc<AuthorityStore>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        committee_store: Arc<CommitteeStore>,
        indexes: Option<Arc<IndexStore>>,
        checkpoint_store: Arc<CheckpointStore>,
        prometheus_registry: &Registry,
        pruning_config: AuthorityStorePruningConfig,
        genesis_objects: &[Object],
        db_checkpoint_config: &DBCheckpointConfig,
        expensive_safety_check_config: ExpensiveSafetyCheckConfig,
        transaction_deny_config: TransactionDenyConfig,
        indirect_objects_threshold: usize,
    ) -> Arc<Self> {
        Self::check_protocol_version(supported_protocol_versions, epoch_store.protocol_version());

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
            epoch_store.epoch_start_state().epoch_duration_ms(),
            prometheus_registry,
            indirect_objects_threshold,
        );
        let state = Arc::new(AuthorityState {
            name,
            secret,
            epoch_store: ArcSwap::new(epoch_store.clone()),
            database: store,
            indexes,
            subscription_handler: Arc::new(SubscriptionHandler::default()),
            checkpoint_store,
            committee_store,
            transaction_manager,
            tx_execution_shutdown: Mutex::new(Some(tx_execution_shutdown)),
            metrics,
            _objects_pruner,
            _authority_per_epoch_pruner,
            db_checkpoint_config: db_checkpoint_config.clone(),
            expensive_safety_check_config,
            transaction_deny_config,
        });

        // Start a task to execute ready certificates.
        let authority_state = Arc::downgrade(&state);
        spawn_monitored_task!(execution_process(
            authority_state,
            rx_ready_certificates,
            rx_execution_shutdown
        ));

        // TODO: This doesn't belong to the constructor of AuthorityState.
        state
            .create_owner_index_if_empty(genesis_objects, &epoch_store)
            .expect("Error indexing genesis objects.");

        state
    }

    pub fn transaction_manager(&self) -> &Arc<TransactionManager> {
        &self.transaction_manager
    }

    /// Adds certificates to transaction manager for ordered execution.
    /// It is unnecessary to persist the certificates into the pending_execution table,
    /// because only Narwhal output needs to be persisted.
    pub fn enqueue_certificates_for_execution(
        &self,
        certs: Vec<VerifiedCertificate>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<()> {
        self.transaction_manager
            .enqueue_certificates(certs, epoch_store)
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
        checkpoint_executor: &CheckpointExecutor,
        accumulator: Arc<StateAccumulator>,
        expensive_safety_check_config: &ExpensiveSafetyCheckConfig,
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
        self.check_system_consistency(
            cur_epoch_store,
            checkpoint_executor,
            accumulator,
            expensive_safety_check_config.enable_state_consistency_check(),
        );
        if let Some(checkpoint_path) = &self.db_checkpoint_config.checkpoint_path {
            if self
                .db_checkpoint_config
                .perform_db_checkpoints_at_epoch_end
            {
                let checkpoint_indexes = self
                    .db_checkpoint_config
                    .perform_index_db_checkpoints_at_epoch_end
                    .unwrap_or(false);
                let current_epoch = cur_epoch_store.epoch();
                let epoch_checkpoint_path =
                    checkpoint_path.join(format!("epoch_{}", current_epoch));
                self.checkpoint_all_dbs(
                    &epoch_checkpoint_path,
                    cur_epoch_store,
                    checkpoint_indexes,
                )?;
            }
        }
        let new_epoch = new_committee.epoch;
        let new_epoch_store = self
            .reopen_epoch_db(
                cur_epoch_store,
                new_committee,
                epoch_start_configuration,
                expensive_safety_check_config,
            )
            .await?;
        assert_eq!(new_epoch_store.epoch(), new_epoch);
        self.transaction_manager.reconfigure(new_epoch);
        *execution_lock = new_epoch;
        // drop execution_lock after epoch store was updated
        // see also assert in AuthorityState::process_certificate
        // on the epoch store and execution lock epoch match
        Ok(new_epoch_store)
    }

    fn check_system_consistency(
        &self,
        cur_epoch_store: &AuthorityPerEpochStore,
        checkpoint_executor: &CheckpointExecutor,
        accumulator: Arc<StateAccumulator>,
        enable_state_consistency_check: bool,
    ) {
        info!(
            "Performing sui conservation consistency check for epoch {}",
            cur_epoch_store.epoch()
        );

        if let Err(err) = self.database.expensive_check_sui_conservation() {
            if cfg!(debug_assertions) {
                panic!("{}", err);
            } else {
                // We cannot panic in production yet because it is known that there are some
                // inconsistencies in testnet. We will enable this once we make it balanced again in testnet.
                warn!("Sui conservation consistency check failed: {}", err);
            }
        } else {
            info!("Sui conservation consistency check passed");
        }

        // check for root state hash consistency with live object set
        if enable_state_consistency_check {
            info!(
                "Performing state consistency check for epoch {}",
                cur_epoch_store.epoch()
            );
            self.database.expensive_check_is_consistent_state(
                checkpoint_executor,
                accumulator,
                cur_epoch_store.epoch(),
                cfg!(debug_assertions), // panic in debug mode only
            );
        }
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
        checkpoint_indexes: bool,
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

        // NOTE: Do not change the order of invoking these checkpoint calls
        // We want to snapshot checkpoint db first to not race with state sync
        self.checkpoint_store
            .checkpoint_db(&checkpoint_path_tmp.join("checkpoints"))?;

        self.database
            .perpetual_tables
            .checkpoint_db(&store_checkpoint_path_tmp.join("perpetual"))?;
        self.committee_store
            .checkpoint_db(&checkpoint_path_tmp.join("epochs"))?;

        if checkpoint_indexes {
            if let Some(indexes) = self.indexes.as_ref() {
                indexes.checkpoint_db(&checkpoint_path_tmp.join("indexes"))?;
            }
        }

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
        Committee::clone(self.epoch_store_for_testing().committee())
    }

    pub async fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        self.database.get_object(object_id)
    }

    pub async fn get_sui_system_package_object_ref(&self) -> SuiResult<ObjectRef> {
        Ok(self
            .get_object(&SUI_SYSTEM_ADDRESS.into())
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

    pub fn get_object_read(&self, object_id: &ObjectID) -> Result<ObjectRead, SuiError> {
        match self.database.get_object_or_tombstone(*object_id)? {
            None => Ok(ObjectRead::NotExists(*object_id)),
            Some(obj_ref) => {
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

    /// Chain Identifier is the digest of the genesis checkpoint.
    pub fn get_chain_identifier(&self) -> Option<CheckpointDigest> {
        if let Some(digest) = CHAIN_IDENTIFIER.get() {
            return Some(*digest);
        }

        let checkpoint = self
            .get_checkpoint_by_sequence_number(0)
            .tap_err(|e| error!("Failed to get genesis checkpoint: {:?}", e))
            .ok()?
            .tap_none(|| error!("Genesis checkpoint is missing from DB"))?;
        // It's ok if the value is already set due to data races.
        let _ = CHAIN_IDENTIFIER.set(*checkpoint.digest());
        Some(*checkpoint.digest())
    }

    pub fn get_move_object<T>(&self, object_id: &ObjectID) -> SuiResult<T>
    where
        T: DeserializeOwned,
    {
        let o = self.get_object_read(object_id)?.into_object()?;
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
        let field: Field<K, V> = self.get_move_object(&df.object_id).ok()?;
        Some(field.value)
    }

    /// This function aims to serve rpc reads on past objects and
    /// we don't expect it to be called for other purposes.
    /// Depending on the object pruning policies that will be enforced in the
    /// future there is no software-level guarantee/SLA to retrieve an object
    /// with an old version even if it exists/existed.
    pub fn get_past_object_read(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> Result<PastObjectRead, SuiError> {
        // Firstly we see if the object ever exists by getting its latest data
        match self.database.get_object_or_tombstone(*object_id)? {
            None => Ok(PastObjectRead::ObjectNotExists(*object_id)),
            Some(obj_ref) => {
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

    pub fn get_owner_objects(
        &self,
        owner: SuiAddress,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<ObjectID>,
        limit: usize,
        filter: Option<SuiObjectDataFilter>,
    ) -> SuiResult<Vec<ObjectInfo>> {
        if let Some(indexes) = &self.indexes {
            indexes.get_owner_objects(owner, cursor, limit, filter)
        } else {
            Err(SuiError::IndexStoreNotAvailable)
        }
    }

    pub fn get_owned_coins_iterator_with_cursor(
        &self,
        owner: SuiAddress,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: (String, ObjectID),
        limit: usize,
        one_coin_type_only: bool,
    ) -> SuiResult<impl Iterator<Item = (String, ObjectID, CoinInfo)> + '_> {
        if let Some(indexes) = &self.indexes {
            indexes.get_owned_coins_iterator_with_cursor(owner, cursor, limit, one_coin_type_only)
        } else {
            Err(SuiError::IndexStoreNotAvailable)
        }
    }

    pub fn get_owner_objects_iterator(
        &self,
        owner: SuiAddress,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<ObjectID>,
        filter: Option<SuiObjectDataFilter>,
    ) -> SuiResult<impl Iterator<Item = ObjectInfo> + '_> {
        let cursor_u = cursor.unwrap_or(ObjectID::ZERO);
        if let Some(indexes) = &self.indexes {
            indexes.get_owner_objects_iterator(owner, cursor_u, filter)
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
            .get_owner_objects_iterator(owner, None, None)?
            .filter(|o| match &o.type_ {
                ObjectType::Struct(s) => &type_ == s,
                ObjectType::Package => false,
            })
            .map(|info| ObjectKey(info.object_id, info.version))
            .collect::<Vec<_>>();
        let mut move_objects = vec![];

        let objects = self.database.multi_get_object_by_key(&object_ids)?;

        for (o, id) in objects.into_iter().zip(object_ids) {
            let object = o.ok_or_else(|| {
                SuiError::from(UserInputError::ObjectNotFound {
                    object_id: id.0,
                    version: Some(id.1),
                })
            })?;
            let move_object = object.data.try_as_move().ok_or_else(|| {
                SuiError::from(UserInputError::MovePackageAsObject { object_id: id.0 })
            })?;
            move_objects.push(bcs::from_bytes(move_object.contents()).map_err(|e| {
                SuiError::ObjectDeserializationError {
                    error: format!("{e}"),
                }
            })?);
        }
        Ok(move_objects)
    }

    pub fn get_dynamic_fields(
        &self,
        owner: ObjectID,
        // If `Some`, the query will start from the next item after the specified cursor
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
        // If `Some`, the query will start from the next item after the specified cursor
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
        name_type: TypeTag,
        name_bcs_bytes: &[u8],
    ) -> SuiResult<Option<ObjectID>> {
        if let Some(indexes) = &self.indexes {
            indexes.get_dynamic_field_object_id(owner, name_type, name_bcs_bytes)
        } else {
            Err(SuiError::IndexStoreNotAvailable)
        }
    }

    pub fn get_total_transaction_blocks(&self) -> Result<u64, anyhow::Error> {
        Ok(self.get_indexes()?.next_sequence_number())
    }

    pub async fn get_executed_transaction_and_effects(
        &self,
        digest: TransactionDigest,
    ) -> Result<(VerifiedTransaction, TransactionEffects), anyhow::Error> {
        let transaction = self.database.get_transaction_block(&digest)?;
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
        let transaction = self.database.get_transaction_block(&digest)?;
        transaction.ok_or_else(|| anyhow!(SuiError::TransactionNotFound { digest }))
    }

    pub fn get_executed_effects(
        &self,
        digest: TransactionDigest,
    ) -> Result<TransactionEffects, anyhow::Error> {
        let effects = self.database.get_executed_effects(&digest)?;
        effects.ok_or_else(|| anyhow!(SuiError::TransactionNotFound { digest }))
    }

    pub fn multi_get_executed_transactions(
        &self,
        digests: &[TransactionDigest],
    ) -> Result<Vec<Option<VerifiedTransaction>>, anyhow::Error> {
        Ok(self.database.multi_get_transaction_blocks(digests)?)
    }

    pub fn multi_get_executed_effects(
        &self,
        digests: &[TransactionDigest],
    ) -> Result<Vec<Option<TransactionEffects>>, anyhow::Error> {
        Ok(self.database.multi_get_executed_effects(digests)?)
    }

    pub fn multi_get_transaction_checkpoint(
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

    pub fn loaded_child_object_versions(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> SuiResult<Option<Vec<(ObjectID, SequenceNumber)>>> {
        self.get_indexes()?
            .loaded_child_object_versions(transaction_digest)
    }

    pub fn get_transactions(
        &self,
        filter: Option<TransactionFilter>,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        reverse: bool,
    ) -> Result<Vec<TransactionDigest>, anyhow::Error> {
        if let Some(TransactionFilter::Checkpoint(sequence_number)) = filter {
            let checkpoint_contents =
                self.get_checkpoint_contents_by_sequence_number(sequence_number)?;
            let iter = checkpoint_contents.iter().map(|c| c.transaction);
            if reverse {
                let iter = iter
                    .rev()
                    .skip_while(|d| cursor.is_some() && Some(*d) != cursor)
                    .skip(usize::from(cursor.is_some()));
                return Ok(iter.take(limit.unwrap_or(usize::max_value())).collect());
            } else {
                let iter = iter
                    .skip_while(|d| cursor.is_some() && Some(*d) != cursor)
                    .skip(usize::from(cursor.is_some()));
                return Ok(iter.take(limit.unwrap_or(usize::max_value())).collect());
            }
        }
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

    pub fn find_publish_txn_digest(
        &self,
        package_id: ObjectID,
    ) -> Result<TransactionDigest, anyhow::Error> {
        if is_system_package(package_id) {
            return self.find_genesis_txn_digest();
        }
        Ok(self
            .get_object_read(&package_id)?
            .into_object()?
            .previous_transaction)
    }

    pub fn find_genesis_txn_digest(&self) -> Result<TransactionDigest, anyhow::Error> {
        let summary = self
            .get_verified_checkpoint_by_sequence_number(0)?
            .into_message();
        let content = self.get_checkpoint_contents(summary.content_digest)?;
        let genesis_transaction = content.enumerate_transactions(&summary).next();
        Ok(genesis_transaction
            .ok_or(anyhow!("No transactions found in checkpoint content"))?
            .1
            .transaction)
    }

    pub fn get_verified_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<VerifiedCheckpoint, anyhow::Error> {
        let verified_checkpoint = self
            .get_checkpoint_store()
            .get_checkpoint_by_sequence_number(sequence_number)?;
        match verified_checkpoint {
            Some(verified_checkpoint) => Ok(verified_checkpoint),
            None => Err(anyhow!(
                "Verified checkpoint not found for sequence number {}",
                sequence_number
            )),
        }
    }

    pub fn get_verified_checkpoint_summary_by_digest(
        &self,
        digest: CheckpointDigest,
    ) -> Result<VerifiedCheckpoint, anyhow::Error> {
        let verified_checkpoint = self
            .get_checkpoint_store()
            .get_checkpoint_by_digest(&digest)?;
        match verified_checkpoint {
            Some(verified_checkpoint) => Ok(verified_checkpoint),
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

    pub fn get_checkpoints(
        &self,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<CheckpointSequenceNumber>,
        limit: u64,
        descending_order: bool,
    ) -> Result<Vec<Checkpoint>, anyhow::Error> {
        let max_checkpoint = self.get_latest_checkpoint_sequence_number()?;
        let checkpoint_numbers =
            calculate_checkpoint_numbers(cursor, limit, descending_order, max_checkpoint);

        let verified_checkpoints = self
            .get_checkpoint_store()
            .multi_get_checkpoint_by_sequence_number(&checkpoint_numbers)?;

        let checkpoint_summaries_and_signatures: Vec<(
            CheckpointSummary,
            AggregateAuthoritySignature,
        )> = verified_checkpoints
            .into_iter()
            .flatten()
            .map(|check| {
                (
                    check.clone().into_summary_and_sequence().1,
                    check.get_validator_signature(),
                )
            })
            .collect();

        let checkpoint_contents_digest: Vec<CheckpointContentsDigest> =
            checkpoint_summaries_and_signatures
                .iter()
                .map(|summary| summary.0.content_digest)
                .collect();
        let checkpoint_contents = self
            .get_checkpoint_store()
            .multi_get_checkpoint_content(checkpoint_contents_digest.as_slice())?;
        let contents: Vec<CheckpointContents> = checkpoint_contents.into_iter().flatten().collect();

        let mut checkpoints: Vec<Checkpoint> = vec![];

        for (summary_and_sig, content) in checkpoint_summaries_and_signatures
            .into_iter()
            .zip(contents.into_iter())
        {
            checkpoints.push(Checkpoint::from((
                summary_and_sig.0,
                content,
                summary_and_sig.1,
            )));
        }

        Ok(checkpoints)
    }

    pub async fn get_timestamp_ms(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<u64>, anyhow::Error> {
        Ok(self.get_indexes()?.get_timestamp_ms(digest)?)
    }

    pub fn query_events(
        &self,
        query: EventFilter,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<EventID>,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<SuiEvent>, anyhow::Error> {
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
            EventFilter::All(filters) => {
                if filters.is_empty() {
                    index_store.all_events(tx_num, event_num, limit, descending)?
                } else {
                    return Err(anyhow!(
                        "This query type does not currently support filter combinations."
                    ));
                }
            }
            EventFilter::Transaction(digest) => {
                index_store.events_by_transaction(&digest, tx_num, event_num, limit, descending)?
            }
            EventFilter::MoveModule { package, module } => {
                let module_id = ModuleId::new(package.into(), module);
                index_store.events_by_module_id(&module_id, tx_num, event_num, limit, descending)?
            }
            EventFilter::MoveEventType(struct_name) => index_store
                .events_by_move_event_struct_name(
                    &struct_name,
                    tx_num,
                    event_num,
                    limit,
                    descending,
                )?,
            EventFilter::Sender(sender) => {
                index_store.events_by_sender(&sender, tx_num, event_num, limit, descending)?
            }
            EventFilter::TimeRange {
                start_time,
                end_time,
            } => index_store
                .event_iterator(start_time, end_time, tx_num, event_num, limit, descending)?,
            _ => {
                return Err(anyhow!(
                    "This query type is not supported by the full node."
                ))
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
            events.push(SuiEvent::try_from(
                e,
                tx_digest,
                event_seq as u64,
                Some(timestamp),
                &**self.epoch_store.load().module_cache(),
            )?)
        }
        Ok(events)
    }

    pub async fn insert_genesis_object(&self, object: Object) {
        self.database
            .insert_genesis_object(object)
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
        let Some(transaction) = self.database.get_transaction_block(tx_digest)? else {
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
            if let Some(transaction) = self.database.get_transaction_block(transaction_digest)? {
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

        // Write transactions after because if we write before, there is a chance the lock can fail
        // and this can cause invalid transactions to be inserted in the table.
        // It is also safe being non-atomic with above, because if we crash before writing the
        // transaction, we will just come back, re-acquire the same lock and write the transaction
        // again.
        epoch_store.insert_signed_transaction(transaction)?;

        Ok(())
    }

    // Returns coin objects for indexing for fullnode if indexing is enabled.
    fn fullnode_only_get_tx_coins_for_indexing(
        &self,
        inner_temporary_store: &InnerTemporaryStore,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> Option<TxCoins> {
        if self.indexes.is_none() || self.is_validator(epoch_store) {
            return None;
        }
        let written_coin_objects = inner_temporary_store
            .written
            .iter()
            .filter_map(|(k, v)| {
                if v.1.is_coin() {
                    Some((*k, v.clone()))
                } else {
                    None
                }
            })
            .collect::<WrittenObjects>();
        let input_coin_objects = inner_temporary_store
            .objects
            .iter()
            .filter_map(|(k, v)| {
                if v.is_coin() {
                    Some((*k, v.clone()))
                } else {
                    None
                }
            })
            .collect::<ObjectMap>();
        Some((input_coin_objects, written_coin_objects))
    }

    /// Commit effects of transaction execution to data store.
    #[instrument(level = "trace", skip_all)]
    pub(crate) async fn commit_certificate(
        &self,
        inner_temporary_store: InnerTemporaryStore,
        certificate: &VerifiedExecutableTransaction,
        effects: &TransactionEffects,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<Option<TxCoins>> {
        let _metrics_guard = self.metrics.commit_certificate_latency.start_timer();

        let tx_digest = certificate.digest();
        // Only need to sign effects if we are a validator.
        let effects_sig = if self.is_validator(epoch_store) {
            Some(AuthoritySignInfo::new(
                epoch_store.epoch(),
                effects,
                Intent::sui_app(IntentScope::TransactionEffects),
                self.name,
                &*self.secret,
            ))
        } else {
            None
        };

        // Returns coin objects for indexing for fullnode if indexing is enabled.
        let tx_coins =
            self.fullnode_only_get_tx_coins_for_indexing(&inner_temporary_store, epoch_store);

        // The insertion to epoch_store is not atomic with the insertion to the perpetual store. This is OK because
        // we insert to the epoch store first. And during lookups we always look up in the perpetual store first.
        epoch_store.insert_tx_cert_and_effects_signature(
            tx_digest,
            certificate.certificate_sig(),
            effects_sig.as_ref(),
        )?;

        // Allow testing what happens if we crash here.
        fail_point_async!("crash");

        self.database
            .update_state(
                inner_temporary_store,
                &certificate.clone().into_unsigned(),
                effects,
            )
            .await
            .tap_ok(|_| {
                debug!("commit_certificate finished");
            })?;

        // todo - ideally move this metric in NotifyRead once we have metrics in AuthorityStore
        self.metrics
            .pending_notify_read
            .set(self.database.executed_effects_notify_read.num_pending() as i64);

        Ok(tx_coins)
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

    pub async fn get_objects(
        &self,
        _objects: &[ObjectID],
    ) -> Result<Vec<Option<Object>>, SuiError> {
        self.database.get_objects(_objects)
    }

    pub async fn get_object_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<ObjectRef>, SuiError> {
        self.database.get_object_or_tombstone(object_id)
    }

    /// Ordinarily, protocol upgrades occur when 2f + 1 + (f *
    /// ProtocolConfig::buffer_stake_for_protocol_upgrade_bps) vote for the upgrade.
    ///
    /// This method can be used to dynamic adjust the amount of buffer. If set to 0, the upgrade
    /// will go through with only 2f+1 votes.
    ///
    /// IMPORTANT: If this is used, it must be used on >=2f+1 validators (all should have the same
    /// value), or you risk halting the chain.
    pub fn set_override_protocol_upgrade_buffer_stake(
        &self,
        expected_epoch: EpochId,
        buffer_stake_bps: u64,
    ) -> SuiResult {
        let epoch_store = self.load_epoch_store_one_call_per_task();
        let actual_epoch = epoch_store.epoch();
        if actual_epoch != expected_epoch {
            return Err(SuiError::WrongEpoch {
                expected_epoch,
                actual_epoch,
            });
        }

        epoch_store.set_override_protocol_upgrade_buffer_stake(buffer_stake_bps)
    }

    pub fn clear_override_protocol_upgrade_buffer_stake(
        &self,
        expected_epoch: EpochId,
    ) -> SuiResult {
        let epoch_store = self.load_epoch_store_one_call_per_task();
        let actual_epoch = epoch_store.epoch();
        if actual_epoch != expected_epoch {
            return Err(SuiError::WrongEpoch {
                expected_epoch,
                actual_epoch,
            });
        }

        epoch_store.clear_override_protocol_upgrade_buffer_stake()
    }

    /// Get the set of system packages that are compiled in to this build, if those packages are
    /// compatible with the current versions of those packages on-chain.
    pub async fn get_available_system_packages(
        &self,
        max_binary_format_version: u32,
        no_extraneous_module_bytes: bool,
    ) -> Vec<ObjectRef> {
        let mut results = vec![];

        let system_packages = BuiltInFramework::iter_system_packages();

        // Add extra framework packages during simtest
        #[cfg(msim)]
        let extra_packages = framework_injection::get_extra_packages(self.name);
        #[cfg(msim)]
        let system_packages = system_packages.map(|p| p).chain(extra_packages.iter());

        for system_package in system_packages {
            let modules = system_package.modules().to_vec();
            // In simtests, we could override the current built-in framework packages.
            #[cfg(msim)]
            let modules = framework_injection::get_override_modules(system_package.id(), self.name)
                .unwrap_or(modules);

            let Some(obj_ref) = sui_framework::compare_system_package(
                self.database.as_ref(),
                system_package.id(),
                &modules,
                system_package.dependencies().to_vec(),
                max_binary_format_version,
                no_extraneous_module_bytes,
            ).await else {
                return vec![];
            };
            results.push(obj_ref);
        }

        results
    }

    /// Return the new versions, module bytes, and dependencies for the packages that have been
    /// committed to for a framework upgrade, in `system_packages`.  Loads the module contents from
    /// the binary, and performs the following checks:
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
        move_binary_format_version: u32,
        no_extraneous_module_bytes: bool,
    ) -> Option<Vec<(SequenceNumber, Vec<Vec<u8>>, Vec<ObjectID>)>> {
        let ids: Vec<_> = system_packages.iter().map(|(id, _, _)| *id).collect();
        let objects = self.get_objects(&ids).await.expect("read cannot fail");

        let mut res = Vec::with_capacity(system_packages.len());
        for (system_package_ref, object) in system_packages.into_iter().zip(objects.iter()) {
            let prev_transaction = match object {
                Some(cur_object) if cur_object.compute_object_reference() == system_package_ref => {
                    // Skip this one because it doesn't need to be upgraded.
                    info!("Framework {} does not need updating", system_package_ref.0);
                    continue;
                }

                Some(cur_object) => cur_object.previous_transaction,
                None => TransactionDigest::genesis(),
            };

            #[cfg(msim)]
            let SystemPackage {
                id: _,
                bytes,
                dependencies,
            } = framework_injection::get_override_system_package(&system_package_ref.0, self.name)
                .unwrap_or_else(|| {
                    BuiltInFramework::get_package_by_id(&system_package_ref.0).clone()
                });

            #[cfg(not(msim))]
            let SystemPackage {
                id: _,
                bytes,
                dependencies,
            } = BuiltInFramework::get_package_by_id(&system_package_ref.0).clone();

            let modules: Vec<_> = bytes
                .iter()
                .map(|m| {
                    CompiledModule::deserialize_with_config(
                        m,
                        move_binary_format_version,
                        no_extraneous_module_bytes,
                    )
                    .unwrap()
                })
                .collect();

            let new_object = Object::new_system_package(
                &modules,
                system_package_ref.1,
                dependencies.clone(),
                prev_transaction,
            );

            let new_ref = new_object.compute_object_reference();
            if new_ref != system_package_ref {
                error!(
                    "Framework mismatch -- binary: {new_ref:?}\n  upgrade: {system_package_ref:?}"
                );
                return None;
            }

            res.push((system_package_ref.1, bytes, dependencies));
        }

        Some(res)
    }

    fn is_protocol_version_supported(
        current_protocol_version: ProtocolVersion,
        proposed_protocol_version: ProtocolVersion,
        protocol_config: &ProtocolConfig,
        committee: &Committee,
        capabilities: Vec<AuthorityCapabilities>,
        mut buffer_stake_bps: u64,
    ) -> Option<(ProtocolVersion, Vec<ObjectRef>)> {
        if proposed_protocol_version > current_protocol_version + 1
            && !protocol_config.advance_to_highest_supported_protocol_version()
        {
            return None;
        }

        if buffer_stake_bps > 10000 {
            warn!("clamping buffer_stake_bps to 10000");
            buffer_stake_bps = 10000;
        }

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
                    .is_version_supported(proposed_protocol_version)
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
                let f = committee.total_votes() - committee.quorum_threshold();

                // multiple by buffer_stake_bps / 10000, rounded up.
                let buffer_stake = (f * buffer_stake_bps + 9999) / 10000;
                let effective_threshold = quorum_threshold + buffer_stake;

                info!(
                    ?total_votes,
                    ?quorum_threshold,
                    ?buffer_stake_bps,
                    ?effective_threshold,
                    ?proposed_protocol_version,
                    ?packages,
                    "support for upgrade"
                );

                let has_support = total_votes >= effective_threshold;
                has_support.then_some((proposed_protocol_version, packages))
            })
    }

    fn choose_protocol_version_and_system_packages(
        current_protocol_version: ProtocolVersion,
        protocol_config: &ProtocolConfig,
        committee: &Committee,
        capabilities: Vec<AuthorityCapabilities>,
        buffer_stake_bps: u64,
    ) -> (ProtocolVersion, Vec<ObjectRef>) {
        let mut next_protocol_version = current_protocol_version;
        let mut system_packages = vec![];

        while let Some((version, packages)) = Self::is_protocol_version_supported(
            current_protocol_version,
            next_protocol_version + 1,
            protocol_config,
            committee,
            capabilities.clone(),
            buffer_stake_bps,
        ) {
            next_protocol_version = version;
            system_packages = packages;
        }

        (next_protocol_version, system_packages)
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

        let buffer_stake_bps = epoch_store.get_effective_buffer_stake_bps();

        let (next_epoch_protocol_version, next_epoch_system_packages) =
            Self::choose_protocol_version_and_system_packages(
                epoch_store.protocol_version(),
                epoch_store.protocol_config(),
                epoch_store.committee(),
                epoch_store
                    .get_capabilities()
                    .expect("read capabilities from db cannot fail"),
                buffer_stake_bps,
            );

        // since system packages are created during the current epoch, they should abide by the
        // rules of the current epoch, including the current epoch's max Move binary format version
        let config = epoch_store.protocol_config();
        let Some(next_epoch_system_package_bytes) = self.get_system_package_bytes(
            next_epoch_system_packages.clone(),
            config.move_binary_format_version(),
            config.no_extraneous_module_bytes(),
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
            gas_cost_summary.non_refundable_storage_fee,
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
            storage_rebate=?gas_cost_summary.storage_rebate,
            non_refundable_storage_fee=?gas_cost_summary.non_refundable_storage_fee,
            ?tx_digest,
            "Creating advance epoch transaction"
        );

        fail_point_async!("change_epoch_tx_delay");
        let _tx_lock = epoch_store.acquire_tx_lock(tx_digest).await;

        // The tx could have been executed by state sync already - if so simply return an error.
        // The checkpoint builder will shortly be terminated by reconfiguration anyway.
        if self
            .database
            .is_tx_already_executed(tx_digest)
            .expect("read cannot fail")
        {
            warn!("change epoch tx has already been executed via state sync");
            return Err(anyhow::anyhow!(
                "change epoch tx has already been executed via state sync"
            ));
        }

        let execution_guard = self
            .database
            .execution_lock_for_executable_transaction(&executable_tx)
            .await?;
        let (temporary_store, effects, _execution_error_opt) = self
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

        info!(
            "Effects summary of the change epoch transaction: {:?}",
            effects.summary_for_debug()
        );
        epoch_store.record_checkpoint_builder_is_safe_mode_metric(system_obj.safe_mode());
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
        info!(
            "Reverting {} locally executed transactions that was not included in the epoch: {:?}",
            pending_certificates.len(),
            pending_certificates,
        );
        for digest in pending_certificates {
            if self
                .database
                .is_transaction_executed_in_checkpoint(&digest)?
            {
                info!("Not reverting pending consensus transaction {:?} - it was included in checkpoint", digest);
                continue;
            }
            info!("Reverting {:?} at the end of epoch", digest);
            self.database.revert_state_update(&digest).await?;
        }
        info!("All uncommitted local transactions reverted");
        Ok(())
    }

    async fn reopen_epoch_db(
        &self,
        cur_epoch_store: &AuthorityPerEpochStore,
        new_committee: Committee,
        epoch_start_configuration: EpochStartConfiguration,
        expensive_safety_check_config: &ExpensiveSafetyCheckConfig,
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
            expensive_safety_check_config,
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

fn calculate_checkpoint_numbers(
    // If `Some`, the query will start from the next item after the specified cursor
    cursor: Option<CheckpointSequenceNumber>,
    limit: u64,
    descending_order: bool,
    max_checkpoint: CheckpointSequenceNumber,
) -> Vec<CheckpointSequenceNumber> {
    let (start_index, end_index) = match cursor {
        Some(t) => {
            if descending_order {
                let start = std::cmp::min(t.saturating_sub(1), max_checkpoint);
                let end = start.saturating_sub(limit - 1);
                (end, start)
            } else {
                let start =
                    std::cmp::min(t.checked_add(1).unwrap_or(max_checkpoint), max_checkpoint);
                let end = std::cmp::min(
                    start.checked_add(limit - 1).unwrap_or(max_checkpoint),
                    max_checkpoint,
                );
                (start, end)
            }
        }
        None => {
            if descending_order {
                (max_checkpoint.saturating_sub(limit - 1), max_checkpoint)
            } else {
                (0, std::cmp::min(limit - 1, max_checkpoint))
            }
        }
    };

    if descending_order {
        (start_index..=end_index).rev().collect()
    } else {
        (start_index..=end_index).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_checkpoint_numbers() {
        let cursor = Some(10);
        let limit = 5;
        let descending_order = true;
        let max_checkpoint = 15;

        let checkpoint_numbers =
            calculate_checkpoint_numbers(cursor, limit, descending_order, max_checkpoint);

        assert_eq!(checkpoint_numbers, vec![9, 8, 7, 6, 5]);
    }

    #[test]
    fn test_calculate_checkpoint_numbers_descending_no_cursor() {
        let cursor = None;
        let limit = 5;
        let descending_order = true;
        let max_checkpoint = 15;

        let checkpoint_numbers =
            calculate_checkpoint_numbers(cursor, limit, descending_order, max_checkpoint);

        assert_eq!(checkpoint_numbers, vec![15, 14, 13, 12, 11]);
    }

    #[test]
    fn test_calculate_checkpoint_numbers_ascending_no_cursor() {
        let cursor = None;
        let limit = 5;
        let descending_order = false;
        let max_checkpoint = 15;

        let checkpoint_numbers =
            calculate_checkpoint_numbers(cursor, limit, descending_order, max_checkpoint);

        assert_eq!(checkpoint_numbers, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn test_calculate_checkpoint_numbers_ascending_with_cursor() {
        let cursor = Some(10);
        let limit = 5;
        let descending_order = false;
        let max_checkpoint = 15;

        let checkpoint_numbers =
            calculate_checkpoint_numbers(cursor, limit, descending_order, max_checkpoint);

        assert_eq!(checkpoint_numbers, vec![11, 12, 13, 14, 15]);
    }

    #[test]
    fn test_calculate_checkpoint_numbers_ascending_limit_exceeds_max() {
        let cursor = None;
        let limit = 20;
        let descending_order = false;
        let max_checkpoint = 15;

        let checkpoint_numbers =
            calculate_checkpoint_numbers(cursor, limit, descending_order, max_checkpoint);

        assert_eq!(checkpoint_numbers, (0..=15).collect::<Vec<_>>());
    }

    #[test]
    fn test_calculate_checkpoint_numbers_descending_limit_exceeds_max() {
        let cursor = None;
        let limit = 20;
        let descending_order = true;
        let max_checkpoint = 15;

        let checkpoint_numbers =
            calculate_checkpoint_numbers(cursor, limit, descending_order, max_checkpoint);

        assert_eq!(checkpoint_numbers, (0..=15).rev().collect::<Vec<_>>());
    }
}

#[cfg(msim)]
pub mod framework_injection {
    use move_binary_format::CompiledModule;
    use std::collections::BTreeMap;
    use std::{cell::RefCell, collections::BTreeSet};
    use sui_framework::{BuiltInFramework, SystemPackage};
    use sui_types::base_types::{AuthorityName, ObjectID};
    use sui_types::is_system_package;

    type FrameworkOverrideConfig = BTreeMap<ObjectID, PackageOverrideConfig>;

    // Thread local cache because all simtests run in a single unique thread.
    thread_local! {
        static OVERRIDE: RefCell<FrameworkOverrideConfig> = RefCell::new(FrameworkOverrideConfig::default());
    }

    type Framework = Vec<CompiledModule>;

    pub type PackageUpgradeCallback =
        Box<dyn Fn(AuthorityName) -> Option<Framework> + Send + Sync + 'static>;

    enum PackageOverrideConfig {
        Global(Framework),
        PerValidator(PackageUpgradeCallback),
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

    pub fn set_override(package_id: ObjectID, modules: Vec<CompiledModule>) {
        OVERRIDE.with(|bs| {
            bs.borrow_mut()
                .insert(package_id, PackageOverrideConfig::Global(modules))
        });
    }

    pub fn set_override_cb(package_id: ObjectID, func: PackageUpgradeCallback) {
        OVERRIDE.with(|bs| {
            bs.borrow_mut()
                .insert(package_id, PackageOverrideConfig::PerValidator(func))
        });
    }

    pub fn get_override_bytes(package_id: &ObjectID, name: AuthorityName) -> Option<Vec<Vec<u8>>> {
        OVERRIDE.with(|cfg| {
            cfg.borrow().get(package_id).and_then(|entry| match entry {
                PackageOverrideConfig::Global(framework) => {
                    Some(compiled_modules_to_bytes(framework))
                }
                PackageOverrideConfig::PerValidator(func) => {
                    func(name).map(|fw| compiled_modules_to_bytes(&fw))
                }
            })
        })
    }

    pub fn get_override_modules(
        package_id: &ObjectID,
        name: AuthorityName,
    ) -> Option<Vec<CompiledModule>> {
        OVERRIDE.with(|cfg| {
            cfg.borrow().get(package_id).and_then(|entry| match entry {
                PackageOverrideConfig::Global(framework) => Some(framework.clone()),
                PackageOverrideConfig::PerValidator(func) => func(name),
            })
        })
    }

    pub fn get_override_system_package(
        package_id: &ObjectID,
        name: AuthorityName,
    ) -> Option<SystemPackage> {
        let bytes = get_override_bytes(package_id, name)?;
        let dependencies = if is_system_package(*package_id) {
            BuiltInFramework::get_package_by_id(package_id)
                .dependencies()
                .to_vec()
        } else {
            // Assume that entirely new injected packages depend on all existing system packages.
            BuiltInFramework::all_package_ids()
        };
        Some(SystemPackage {
            id: *package_id,
            bytes,
            dependencies,
        })
    }

    pub fn get_extra_packages(name: AuthorityName) -> Vec<SystemPackage> {
        let built_in = BTreeSet::from_iter(BuiltInFramework::all_package_ids().into_iter());
        let extra: Vec<ObjectID> = OVERRIDE.with(|cfg| {
            cfg.borrow()
                .keys()
                .filter_map(|package| (!built_in.contains(package)).then_some(*package))
                .collect()
        });

        extra
            .into_iter()
            .map(|package| SystemPackage {
                id: package,
                bytes: get_override_bytes(&package, name).unwrap(),
                dependencies: BuiltInFramework::all_package_ids(),
            })
            .collect()
    }
}
