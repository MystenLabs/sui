// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::congestion_tracker::CongestionTracker;
use crate::consensus_adapter::ConsensusOverloadChecker;
use crate::execution_cache::ExecutionCacheTraitPointers;
use crate::execution_cache::TransactionCacheRead;
use crate::jsonrpc_index::CoinIndexKey2;
use crate::rpc_index::RpcIndexStore;
use crate::transaction_outputs::TransactionOutputs;
use crate::verify_indexes::verify_indexes;
use anyhow::anyhow;
use arc_swap::{ArcSwap, Guard};
use async_trait::async_trait;
use authority_per_epoch_store::CertLockGuard;
use fastcrypto::encoding::Base58;
use fastcrypto::encoding::Encoding;
use fastcrypto::hash::MultisetHash;
use itertools::Itertools;
use move_binary_format::binary_config::BinaryConfig;
use move_binary_format::CompiledModule;
use move_core_types::annotated_value::MoveStructLayout;
use move_core_types::language_storage::ModuleId;
use mysten_common::fatal;
use mysten_metrics::{TX_TYPE_SHARED_OBJ_TX, TX_TYPE_SINGLE_WRITER_TX};
use parking_lot::Mutex;
use prometheus::{
    register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry, Histogram,
    HistogramVec, IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Registry,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use std::{
    collections::{HashMap, HashSet},
    fs,
    pin::Pin,
    sync::Arc,
    vec,
};
use sui_config::node::{AuthorityOverloadConfig, StateDebugDumpConfig};
use sui_config::NodeConfig;
use sui_types::crypto::RandomnessRound;
use sui_types::dynamic_field::visitor as DFV;
use sui_types::execution::ExecutionTiming;
use sui_types::execution_status::ExecutionStatus;
use sui_types::inner_temporary_store::PackageStoreWithFallback;
use sui_types::layout_resolver::into_struct_layout;
use sui_types::layout_resolver::LayoutResolver;
use sui_types::messages_consensus::{AuthorityCapabilitiesV1, AuthorityCapabilitiesV2};
use sui_types::object::bounded_visitor::BoundedVisitor;
use sui_types::transaction_executor::SimulateTransactionResult;
use tap::TapFallible;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::RwLock;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::trace;
use tracing::{debug, error, info, instrument, warn};

use self::authority_store::ExecutionLockWriteGuard;
use self::authority_store_pruner::AuthorityStorePruningMetrics;
pub use authority_store::{AuthorityStore, ResolverWrapper, UpdateType};
use mysten_metrics::{monitored_scope, spawn_monitored_task};

use crate::jsonrpc_index::IndexStore;
use crate::jsonrpc_index::{CoinInfo, ObjectIndexChanges};
use mysten_common::debug_fatal;
use shared_crypto::intent::{AppId, Intent, IntentMessage, IntentScope, IntentVersion};
use sui_archival::reader::ArchiveReaderBalancer;
use sui_config::genesis::Genesis;
use sui_config::node::{DBCheckpointConfig, ExpensiveSafetyCheckConfig};
use sui_framework::{BuiltInFramework, SystemPackage};
use sui_json_rpc_types::{
    DevInspectResults, DryRunTransactionBlockResponse, EventFilter, SuiEvent, SuiMoveValue,
    SuiObjectDataFilter, SuiTransactionBlockData, SuiTransactionBlockEffects,
    SuiTransactionBlockEvents, TransactionFilter,
};
use sui_macros::{fail_point, fail_point_async, fail_point_if};
use sui_storage::key_value_store::{TransactionKeyValueStore, TransactionKeyValueStoreTrait};
use sui_storage::key_value_store_metrics::KeyValueStoreMetrics;
use sui_types::authenticator_state::get_authenticator_state;
use sui_types::committee::{EpochId, ProtocolVersion};
use sui_types::crypto::{default_hash, AuthoritySignInfo, Signer};
use sui_types::deny_list_v1::check_coin_deny_list_v1;
use sui_types::digests::ChainIdentifier;
use sui_types::digests::TransactionEventsDigest;
use sui_types::dynamic_field::{DynamicFieldInfo, DynamicFieldName};
use sui_types::effects::{
    InputSharedObject, SignedTransactionEffects, TransactionEffects, TransactionEffectsAPI,
    TransactionEvents, VerifiedSignedTransactionEffects,
};
use sui_types::error::{ExecutionError, UserInputError};
use sui_types::event::{Event, EventID};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::gas::{GasCostSummary, SuiGasStatus};
use sui_types::inner_temporary_store::{
    InnerTemporaryStore, ObjectMap, TemporaryModuleResolver, TxCoins, WrittenObjects,
};
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointCommitment, CheckpointContents, CheckpointContentsDigest,
    CheckpointDigest, CheckpointRequest, CheckpointRequestV2, CheckpointResponse,
    CheckpointResponseV2, CheckpointSequenceNumber, CheckpointSummary, CheckpointSummaryResponse,
    CheckpointTimestamp, ECMHLiveObjectSetDigest, VerifiedCheckpoint,
};
use sui_types::messages_grpc::{
    HandleTransactionResponse, LayoutGenerationOption, ObjectInfoRequest, ObjectInfoRequestKind,
    ObjectInfoResponse, TransactionInfoRequest, TransactionInfoResponse, TransactionStatus,
};
use sui_types::metrics::{BytecodeVerifierMetrics, LimitsMetrics};
use sui_types::object::{MoveObject, Owner, PastObjectRead, OBJECT_START_VERSION};
use sui_types::storage::{
    BackingPackageStore, BackingStore, ObjectKey, ObjectOrTombstone, ObjectStore, WriteKind,
};
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::sui_system_state::{get_sui_system_state, SuiSystemState};
use sui_types::supported_protocol_versions::{ProtocolConfig, SupportedProtocolVersions};
use sui_types::{
    base_types::*,
    committee::Committee,
    crypto::AuthoritySignature,
    error::{SuiError, SuiResult},
    object::{Object, ObjectRead},
    transaction::*,
    SUI_SYSTEM_ADDRESS,
};
use sui_types::{is_system_package, TypeTag};
use typed_store::TypedStoreError;

use crate::authority::authority_per_epoch_store::{AuthorityPerEpochStore, CertTxGuard};
use crate::authority::authority_per_epoch_store_pruner::AuthorityPerEpochStorePruner;
use crate::authority::authority_store::{ExecutionLockReadGuard, ObjectLockStatus};
use crate::authority::authority_store_pruner::{
    AuthorityStorePruner, EPOCH_DURATION_MS_FOR_TESTING,
};
use crate::authority::epoch_start_configuration::EpochStartConfigTrait;
use crate::authority::epoch_start_configuration::EpochStartConfiguration;
use crate::checkpoints::CheckpointStore;
use crate::epoch::committee_store::CommitteeStore;
use crate::execution_cache::{
    CheckpointCache, ExecutionCacheCommit, ExecutionCacheReconfigAPI, ExecutionCacheWrite,
    ObjectCacheRead, StateSyncAPI,
};
use crate::execution_driver::execution_process;
use crate::metrics::LatencyObserver;
use crate::metrics::RateTracker;
use crate::module_cache_metrics::ResolverMetrics;
use crate::overload_monitor::{overload_monitor_accept_tx, AuthorityOverloadInfo};
use crate::stake_aggregator::StakeAggregator;
use crate::state_accumulator::{AccumulatorStore, StateAccumulator, WrappedObject};
use crate::subscription_handler::SubscriptionHandler;
use crate::transaction_input_loader::TransactionInputLoader;
use crate::transaction_manager::TransactionManager;

#[cfg(msim)]
pub use crate::checkpoints::checkpoint_executor::{
    init_checkpoint_timeout_config, CheckpointTimeoutConfig,
};

use crate::authority::authority_store_tables::AuthorityPrunerTables;
use crate::authority_client::NetworkAuthorityClient;
use crate::validator_tx_finalizer::ValidatorTxFinalizer;
#[cfg(msim)]
use sui_types::committee::CommitteeTrait;
use sui_types::deny_list_v2::check_coin_deny_list_v2_during_signing;
use sui_types::execution_config_utils::to_binary_config;

#[cfg(test)]
#[path = "unit_tests/authority_tests.rs"]
pub mod authority_tests;

#[cfg(test)]
#[path = "unit_tests/transaction_tests.rs"]
pub mod transaction_tests;

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

#[cfg(test)]
#[path = "unit_tests/coin_deny_list_tests.rs"]
mod coin_deny_list_tests;

#[cfg(test)]
#[path = "unit_tests/auth_unit_test_utils.rs"]
pub mod auth_unit_test_utils;

pub mod authority_test_utils;

pub mod authority_per_epoch_store;
pub mod authority_per_epoch_store_pruner;

pub mod authority_store_pruner;
pub mod authority_store_tables;
pub mod authority_store_types;
pub mod epoch_start_configuration;
pub mod execution_time_estimator;
pub mod shared_object_congestion_tracker;
pub mod shared_object_version_manager;
pub mod test_authority_builder;
pub mod transaction_deferral;

pub(crate) mod authority_store;
pub mod backpressure;

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

    authority_state_handle_transaction_latency: Histogram,
    authority_state_handle_vote_transaction_latency: Histogram,

    execute_certificate_latency_single_writer: Histogram,
    execute_certificate_latency_shared_object: Histogram,
    await_transaction_latency: Histogram,

    internal_execution_latency: Histogram,
    execution_load_input_objects_latency: Histogram,
    prepare_certificate_latency: Histogram,
    commit_certificate_latency: Histogram,
    db_checkpoint_latency: Histogram,

    pub(crate) transaction_manager_num_enqueued_certificates: IntCounterVec,
    pub(crate) transaction_manager_num_missing_objects: IntGauge,
    pub(crate) transaction_manager_num_pending_certificates: IntGauge,
    pub(crate) transaction_manager_num_executing_certificates: IntGauge,
    pub(crate) transaction_manager_num_ready: IntGauge,
    pub(crate) transaction_manager_object_cache_size: IntGauge,
    pub(crate) transaction_manager_object_cache_hits: IntCounter,
    pub(crate) transaction_manager_object_cache_misses: IntCounter,
    pub(crate) transaction_manager_object_cache_evictions: IntCounter,
    pub(crate) transaction_manager_package_cache_size: IntGauge,
    pub(crate) transaction_manager_package_cache_hits: IntCounter,
    pub(crate) transaction_manager_package_cache_misses: IntCounter,
    pub(crate) transaction_manager_package_cache_evictions: IntCounter,
    pub(crate) transaction_manager_transaction_queue_age_s: Histogram,

    pub(crate) execution_driver_executed_transactions: IntCounter,
    pub(crate) execution_driver_dispatch_queue: IntGauge,
    pub(crate) execution_queueing_delay_s: Histogram,
    pub(crate) prepare_cert_gas_latency_ratio: Histogram,
    pub(crate) execution_gas_latency_ratio: Histogram,

    pub(crate) skipped_consensus_txns: IntCounter,
    pub(crate) skipped_consensus_txns_cache_hit: IntCounter,

    pub(crate) authority_overload_status: IntGauge,
    pub(crate) authority_load_shedding_percentage: IntGauge,

    pub(crate) transaction_overload_sources: IntCounterVec,

    /// Post processing metrics
    post_processing_total_events_emitted: IntCounter,
    post_processing_total_tx_indexed: IntCounter,
    post_processing_total_tx_had_event_processed: IntCounter,
    post_processing_total_failures: IntCounter,

    /// Consensus commit and transaction handler metrics
    pub consensus_handler_processed: IntCounterVec,
    pub consensus_handler_transaction_sizes: HistogramVec,
    pub consensus_handler_num_low_scoring_authorities: IntGauge,
    pub consensus_handler_scores: IntGaugeVec,
    pub consensus_handler_deferred_transactions: IntCounter,
    pub consensus_handler_congested_transactions: IntCounter,
    pub consensus_handler_cancelled_transactions: IntCounter,
    pub consensus_handler_max_object_costs: IntGaugeVec,
    pub consensus_committed_subdags: IntCounterVec,
    pub consensus_committed_messages: IntGaugeVec,
    pub consensus_committed_user_transactions: IntGaugeVec,
    pub consensus_calculated_throughput: IntGauge,
    pub consensus_calculated_throughput_profile: IntGauge,
    pub consensus_transaction_handler_processed: IntCounterVec,
    pub consensus_transaction_handler_fastpath_executions: IntCounter,

    pub limits_metrics: Arc<LimitsMetrics>,

    /// bytecode verifier metrics for tracking timeouts
    pub bytecode_verifier_metrics: Arc<BytecodeVerifierMetrics>,

    /// Count of zklogin signatures
    pub zklogin_sig_count: IntCounter,
    /// Count of multisig signatures
    pub multisig_sig_count: IntCounter,

    // Tracks recent average txn queueing delay between when it is ready for execution
    // until it starts executing.
    pub execution_queueing_latency: LatencyObserver,

    // Tracks the rate of transactions become ready for execution in transaction manager.
    // The need for the Mutex is that the tracker is updated in transaction manager and read
    // in the overload_monitor. There should be low mutex contention because
    // transaction manager is single threaded and the read rate in overload_monitor is
    // low. In the case where transaction manager becomes multi-threaded, we can
    // create one rate tracker per thread.
    pub txn_ready_rate_tracker: Arc<Mutex<RateTracker>>,

    // Tracks the rate of transactions starts execution in execution driver.
    // Similar reason for using a Mutex here as to `txn_ready_rate_tracker`.
    pub execution_rate_tracker: Arc<Mutex<RateTracker>>,
}

// Override default Prom buckets for positive numbers in 0-10M range
const POSITIVE_INT_BUCKETS: &[f64] = &[
    1., 2., 5., 7., 10., 20., 50., 70., 100., 200., 500., 700., 1000., 2000., 5000., 7000., 10000.,
    20000., 50000., 70000., 100000., 200000., 500000., 700000., 1000000., 2000000., 5000000.,
    7000000., 10000000.,
];

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.0005, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1., 2., 3., 4., 5., 6., 7., 8., 9.,
    10., 20., 30., 60., 90.,
];

// Buckets for low latency samples. Starts from 10us.
const LOW_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.00001, 0.00002, 0.00005, 0.0001, 0.0002, 0.0005, 0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1,
    0.2, 0.5, 1., 2., 5., 10., 20., 50., 100.,
];

const GAS_LATENCY_RATIO_BUCKETS: &[f64] = &[
    10.0, 50.0, 100.0, 200.0, 300.0, 400.0, 500.0, 600.0, 700.0, 800.0, 900.0, 1000.0, 2000.0,
    3000.0, 4000.0, 5000.0, 6000.0, 7000.0, 8000.0, 9000.0, 10000.0, 50000.0, 100000.0, 1000000.0,
];

pub const DEV_INSPECT_GAS_COIN_VALUE: u64 = 1_000_000_000_000_000;

impl AuthorityMetrics {
    pub fn new(registry: &prometheus::Registry) -> AuthorityMetrics {
        let execute_certificate_latency = register_histogram_vec_with_registry!(
            "authority_state_execute_certificate_latency",
            "Latency of executing certificates, including waiting for inputs",
            &["tx_type"],
            LATENCY_SEC_BUCKETS.to_vec(),
            registry,
        )
        .unwrap();

        let execute_certificate_latency_single_writer =
            execute_certificate_latency.with_label_values(&[TX_TYPE_SINGLE_WRITER_TX]);
        let execute_certificate_latency_shared_object =
            execute_certificate_latency.with_label_values(&[TX_TYPE_SHARED_OBJ_TX]);

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
            authority_state_handle_transaction_latency: register_histogram_with_registry!(
                "authority_state_handle_transaction_latency",
                "Latency of handling transactions",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            authority_state_handle_vote_transaction_latency: register_histogram_with_registry!(
                "authority_state_handle_vote_transaction_latency",
                "Latency of voting on transactions without signing",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            execute_certificate_latency_single_writer,
            execute_certificate_latency_shared_object,
            await_transaction_latency: register_histogram_with_registry!(
                "await_transaction_latency",
                "Latency of awaiting user transaction execution, including waiting for inputs",
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
            execution_load_input_objects_latency: register_histogram_with_registry!(
                "authority_state_execution_load_input_objects_latency",
                "Latency of loading input objects for execution",
                LOW_LATENCY_SEC_BUCKETS.to_vec(),
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
            transaction_manager_object_cache_size: register_int_gauge_with_registry!(
                "transaction_manager_object_cache_size",
                "Current size of object-availability cache in TransactionManager",
                registry,
            )
            .unwrap(),
            transaction_manager_object_cache_hits: register_int_counter_with_registry!(
                "transaction_manager_object_cache_hits",
                "Number of object-availability cache hits in TransactionManager",
                registry,
            )
            .unwrap(),
            authority_overload_status: register_int_gauge_with_registry!(
                "authority_overload_status",
                "Whether authority is current experiencing overload and enters load shedding mode.",
                registry)
            .unwrap(),
            authority_load_shedding_percentage: register_int_gauge_with_registry!(
                "authority_load_shedding_percentage",
                "The percentage of transactions is shed when the authority is in load shedding mode.",
                registry)
            .unwrap(),
            transaction_manager_object_cache_misses: register_int_counter_with_registry!(
                "transaction_manager_object_cache_misses",
                "Number of object-availability cache misses in TransactionManager",
                registry,
            )
            .unwrap(),
            transaction_manager_object_cache_evictions: register_int_counter_with_registry!(
                "transaction_manager_object_cache_evictions",
                "Number of object-availability cache evictions in TransactionManager",
                registry,
            )
            .unwrap(),
            transaction_manager_package_cache_size: register_int_gauge_with_registry!(
                "transaction_manager_package_cache_size",
                "Current size of package-availability cache in TransactionManager",
                registry,
            )
            .unwrap(),
            transaction_manager_package_cache_hits: register_int_counter_with_registry!(
                "transaction_manager_package_cache_hits",
                "Number of package-availability cache hits in TransactionManager",
                registry,
            )
            .unwrap(),
            transaction_manager_package_cache_misses: register_int_counter_with_registry!(
                "transaction_manager_package_cache_misses",
                "Number of package-availability cache misses in TransactionManager",
                registry,
            )
            .unwrap(),
            transaction_manager_package_cache_evictions: register_int_counter_with_registry!(
                "transaction_manager_package_cache_evictions",
                "Number of package-availability cache evictions in TransactionManager",
                registry,
            )
            .unwrap(),
            transaction_manager_transaction_queue_age_s: register_histogram_with_registry!(
                "transaction_manager_transaction_queue_age_s",
                "Time spent in waiting for transaction in the queue",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            transaction_overload_sources: register_int_counter_vec_with_registry!(
                "transaction_overload_sources",
                "Number of times each source indicates transaction overload.",
                &["source"],
                registry)
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
            execution_queueing_delay_s: register_histogram_with_registry!(
                "execution_queueing_delay_s",
                "Queueing delay between a transaction is ready for execution until it starts executing.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            prepare_cert_gas_latency_ratio: register_histogram_with_registry!(
                "prepare_cert_gas_latency_ratio",
                "The ratio of computation gas divided by VM execution latency.",
                GAS_LATENCY_RATIO_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            execution_gas_latency_ratio: register_histogram_with_registry!(
                "execution_gas_latency_ratio",
                "The ratio of computation gas divided by certificate execution latency, include committing certificate.",
                GAS_LATENCY_RATIO_BUCKETS.to_vec(),
                registry
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
            post_processing_total_failures: register_int_counter_with_registry!(
                "post_processing_total_failures",
                "Total number of failure in post processing",
                registry,
            )
            .unwrap(),
            consensus_handler_processed: register_int_counter_vec_with_registry!(
                "consensus_handler_processed",
                "Number of transactions processed by consensus handler",
                &["class"],
                registry
            ).unwrap(),
            consensus_handler_transaction_sizes: register_histogram_vec_with_registry!(
                "consensus_handler_transaction_sizes",
                "Sizes of each type of transactions processed by consensus handler",
                &["class"],
                POSITIVE_INT_BUCKETS.to_vec(),
                registry
            ).unwrap(),
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
            ).unwrap(),
            consensus_handler_deferred_transactions: register_int_counter_with_registry!(
                "consensus_handler_deferred_transactions",
                "Number of transactions deferred by consensus handler",
                registry,
            ).unwrap(),
            consensus_handler_congested_transactions: register_int_counter_with_registry!(
                "consensus_handler_congested_transactions",
                "Number of transactions deferred by consensus handler due to congestion",
                registry,
            ).unwrap(),
            consensus_handler_cancelled_transactions: register_int_counter_with_registry!(
                "consensus_handler_cancelled_transactions",
                "Number of transactions cancelled by consensus handler",
                registry,
            ).unwrap(),
            consensus_handler_max_object_costs: register_int_gauge_vec_with_registry!(
                "consensus_handler_max_congestion_control_object_costs",
                "Max object costs for congestion control in the current consensus commit",
                &["commit_type"],
                registry,
            ).unwrap(),
            consensus_committed_subdags: register_int_counter_vec_with_registry!(
                "consensus_committed_subdags",
                "Number of committed subdags, sliced by author",
                &["authority"],
                registry,
            ).unwrap(),
            consensus_committed_messages: register_int_gauge_vec_with_registry!(
                "consensus_committed_messages",
                "Total number of committed consensus messages, sliced by author",
                &["authority"],
                registry,
            ).unwrap(),
            consensus_committed_user_transactions: register_int_gauge_vec_with_registry!(
                "consensus_committed_user_transactions",
                "Number of committed user transactions, sliced by submitter",
                &["authority"],
                registry,
            ).unwrap(),
            limits_metrics: Arc::new(LimitsMetrics::new(registry)),
            bytecode_verifier_metrics: Arc::new(BytecodeVerifierMetrics::new(registry)),
            zklogin_sig_count: register_int_counter_with_registry!(
                "zklogin_sig_count",
                "Count of zkLogin signatures",
                registry,
            )
            .unwrap(),
            multisig_sig_count: register_int_counter_with_registry!(
                "multisig_sig_count",
                "Count of zkLogin signatures",
                registry,
            )
            .unwrap(),
            consensus_calculated_throughput: register_int_gauge_with_registry!(
                "consensus_calculated_throughput",
                "The calculated throughput from consensus output. Result is calculated based on unique transactions.",
                registry,
            ).unwrap(),
            consensus_calculated_throughput_profile: register_int_gauge_with_registry!(
                "consensus_calculated_throughput_profile",
                "The current active calculated throughput profile",
                registry
            ).unwrap(),
            consensus_transaction_handler_processed: register_int_counter_vec_with_registry!(
                "consensus_transaction_handler_processed",
                "Number of transactions processed by consensus transaction handler, by whether they are certified or rejected.",
                &["outcome"],
                registry
            ).unwrap(),
            consensus_transaction_handler_fastpath_executions: register_int_counter_with_registry!(
                "consensus_transaction_handler_fastpath_executions",
                "Number of fastpath transactions sent for execution by consensus transaction handler",
                registry,
            ).unwrap(),
            execution_queueing_latency: LatencyObserver::new(),
            txn_ready_rate_tracker: Arc::new(Mutex::new(RateTracker::new(Duration::from_secs(10)))),
            execution_rate_tracker: Arc::new(Mutex::new(RateTracker::new(Duration::from_secs(10)))),
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
    input_loader: TransactionInputLoader,
    execution_cache_trait_pointers: ExecutionCacheTraitPointers,

    epoch_store: ArcSwap<AuthorityPerEpochStore>,

    /// This lock denotes current 'execution epoch'.
    /// Execution acquires read lock, checks certificate epoch and holds it until all writes are complete.
    /// Reconfiguration acquires write lock, changes the epoch and revert all transactions
    /// from previous epoch that are executed but did not make into checkpoint.
    execution_lock: RwLock<EpochId>,

    pub indexes: Option<Arc<IndexStore>>,
    pub rpc_index: Option<Arc<RpcIndexStore>>,

    pub subscription_handler: Arc<SubscriptionHandler>,
    checkpoint_store: Arc<CheckpointStore>,

    committee_store: Arc<CommitteeStore>,

    /// Manages pending certificates and their missing input objects.
    transaction_manager: Arc<TransactionManager>,

    /// Shuts down the execution task. Used only in testing.
    #[allow(unused)]
    tx_execution_shutdown: Mutex<Option<oneshot::Sender<()>>>,

    pub metrics: Arc<AuthorityMetrics>,
    _pruner: AuthorityStorePruner,
    _authority_per_epoch_pruner: AuthorityPerEpochStorePruner,

    /// Take db checkpoints of different dbs
    db_checkpoint_config: DBCheckpointConfig,

    pub config: NodeConfig,

    /// Current overload status in this authority. Updated periodically.
    pub overload_info: AuthorityOverloadInfo,

    pub validator_tx_finalizer: Option<Arc<ValidatorTxFinalizer<NetworkAuthorityClient>>>,

    /// The chain identifier is derived from the digest of the genesis checkpoint.
    chain_identifier: ChainIdentifier,

    pub(crate) congestion_tracker: Arc<CongestionTracker>,
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

    pub fn overload_config(&self) -> &AuthorityOverloadConfig {
        &self.config.authority_overload_config
    }

    pub fn get_epoch_state_commitments(
        &self,
        epoch: EpochId,
    ) -> SuiResult<Option<Vec<CheckpointCommitment>>> {
        self.checkpoint_store.get_epoch_state_commitments(epoch)
    }

    fn handle_transaction_deny_checks(
        &self,
        transaction: &VerifiedTransaction,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<CheckedInputObjects> {
        let tx_digest = transaction.digest();
        let tx_data = transaction.data().transaction_data();

        let input_object_kinds = tx_data.input_objects()?;
        let receiving_objects_refs = tx_data.receiving_objects();

        // Note: the deny checks may do redundant package loads but:
        // - they only load packages when there is an active package deny map
        // - the loads are cached anyway
        sui_transaction_checks::deny::check_transaction_for_signing(
            tx_data,
            transaction.tx_signatures(),
            &input_object_kinds,
            &receiving_objects_refs,
            &self.config.transaction_deny_config,
            self.get_backing_package_store().as_ref(),
        )?;

        let (input_objects, receiving_objects) = self.input_loader.read_objects_for_signing(
            Some(tx_digest),
            &input_object_kinds,
            &receiving_objects_refs,
            epoch_store.epoch(),
            epoch_store
                .protocol_config()
                .use_object_per_epoch_marker_table_v2_as_option()
                .unwrap_or(false),
        )?;

        let (_gas_status, checked_input_objects) = sui_transaction_checks::check_transaction_input(
            epoch_store.protocol_config(),
            epoch_store.reference_gas_price(),
            tx_data,
            input_objects,
            &receiving_objects,
            &self.metrics.bytecode_verifier_metrics,
            &self.config.verifier_signing_config,
        )?;

        if epoch_store.coin_deny_list_v1_enabled() {
            check_coin_deny_list_v1(
                tx_data.sender(),
                &checked_input_objects,
                &receiving_objects,
                &self.get_object_store(),
            )?;
        }

        if epoch_store.protocol_config().enable_coin_deny_list_v2() {
            check_coin_deny_list_v2_during_signing(
                tx_data.sender(),
                &checked_input_objects,
                &receiving_objects,
                &self.get_object_store(),
            )?;
        }

        Ok(checked_input_objects)
    }

    /// This is a private method and should be kept that way. It doesn't check whether
    /// the provided transaction is a system transaction, and hence can only be called internally.
    #[instrument(level = "trace", skip_all)]
    fn handle_transaction_impl(
        &self,
        transaction: VerifiedTransaction,
        sign: bool,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<Option<VerifiedSignedTransaction>> {
        // Ensure that validator cannot reconfigure while we are signing the tx
        let _execution_lock = self.execution_lock_for_signing();

        let checked_input_objects =
            self.handle_transaction_deny_checks(&transaction, epoch_store)?;

        let owned_objects = checked_input_objects.inner().filter_owned_objects();

        let tx_digest = *transaction.digest();
        let signed_transaction = if sign {
            Some(VerifiedSignedTransaction::new(
                epoch_store.epoch(),
                transaction,
                self.name,
                &*self.secret,
            ))
        } else {
            None
        };

        // Check and write locks, to signed transaction, into the database
        // The call to self.set_transaction_lock checks the lock is not conflicting,
        // and returns ConflictingTransaction error in case there is a lock on a different
        // existing transaction.
        self.get_cache_writer().acquire_transaction_locks(
            epoch_store,
            &owned_objects,
            tx_digest,
            signed_transaction.clone(),
        )?;

        Ok(signed_transaction)
    }

    /// Initiate a new transaction.
    #[instrument(level = "trace", skip_all)]
    pub async fn handle_transaction(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        transaction: VerifiedTransaction,
    ) -> SuiResult<HandleTransactionResponse> {
        let tx_digest = *transaction.digest();
        debug!("handle_transaction");

        // Ensure an idempotent answer.
        if let Some((_, status)) = self.get_transaction_status(&tx_digest, epoch_store)? {
            return Ok(HandleTransactionResponse { status });
        }

        let _metrics_guard = self
            .metrics
            .authority_state_handle_transaction_latency
            .start_timer();
        self.metrics.tx_orders.inc();

        let signed = self.handle_transaction_impl(transaction, true, epoch_store);
        match signed {
            Ok(Some(s)) => {
                if self.is_validator(epoch_store) {
                    if let Some(validator_tx_finalizer) = &self.validator_tx_finalizer {
                        let tx = s.clone();
                        let validator_tx_finalizer = validator_tx_finalizer.clone();
                        let cache_reader = self.get_transaction_cache_reader().clone();
                        let epoch_store = epoch_store.clone();
                        spawn_monitored_task!(epoch_store.within_alive_epoch(
                            validator_tx_finalizer.track_signed_tx(cache_reader, &epoch_store, tx)
                        ));
                    }
                }
                Ok(HandleTransactionResponse {
                    status: TransactionStatus::Signed(s.into_inner().into_sig()),
                })
            }
            Ok(None) => panic!("handle_transaction_impl should return a signed transaction"),
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

    /// When Ok, returns None if the transaction has not been executed, and returns
    /// (TransactionEffects, TransactionEvents) if the transaction has been executed.
    #[instrument(level = "trace", skip_all)]
    pub(crate) fn handle_vote_transaction(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        transaction: VerifiedTransaction,
    ) -> SuiResult<Option<(TransactionEffects, TransactionEvents)>> {
        let tx_digest = *transaction.digest();
        debug!("handle_vote_transaction");

        // Check if the transaction has already been executed.
        let tx_output = self.get_transaction_output(&tx_digest)?;
        if tx_output.is_some() {
            return Ok(tx_output);
        }

        let _metrics_guard = self
            .metrics
            .authority_state_handle_vote_transaction_latency
            .start_timer();
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

        match self.handle_transaction_impl(transaction, false, epoch_store) {
            Ok(Some(_)) => {
                panic!("handle_transaction_impl should not return a signed transaction")
            }
            Ok(None) => Ok(None),
            // It happens frequently that while we are checking the validity of the transaction, it
            // has just been executed.
            // In that case, we could still return Ok to avoid showing confusing errors.
            Err(e) => self.get_transaction_output(&tx_digest)?.ok_or(e).map(Some),
        }
    }

    pub fn check_system_overload_at_signing(&self) -> bool {
        self.config
            .authority_overload_config
            .check_system_overload_at_signing
    }

    pub fn check_system_overload_at_execution(&self) -> bool {
        self.config
            .authority_overload_config
            .check_system_overload_at_execution
    }

    pub(crate) fn check_system_overload(
        &self,
        consensus_overload_checker: &(impl ConsensusOverloadChecker + ?Sized),
        tx_data: &SenderSignedData,
        do_authority_overload_check: bool,
    ) -> SuiResult {
        if do_authority_overload_check {
            self.check_authority_overload(tx_data).tap_err(|_| {
                self.update_overload_metrics("execution_queue");
            })?;
        }
        self.transaction_manager
            .check_execution_overload(self.overload_config(), tx_data)
            .tap_err(|_| {
                self.update_overload_metrics("execution_pending");
            })?;
        consensus_overload_checker
            .check_consensus_overload()
            .tap_err(|_| {
                self.update_overload_metrics("consensus");
            })?;

        let pending_tx_count = self
            .get_cache_commit()
            .approximate_pending_transaction_count();
        if pending_tx_count > self.config.execution_cache.backpressure_threshold_for_rpc() {
            return Err(SuiError::ValidatorOverloadedRetryAfter {
                retry_after_secs: 10,
            });
        }

        Ok(())
    }

    fn check_authority_overload(&self, tx_data: &SenderSignedData) -> SuiResult {
        if !self.overload_info.is_overload.load(Ordering::Relaxed) {
            return Ok(());
        }

        let load_shedding_percentage = self
            .overload_info
            .load_shedding_percentage
            .load(Ordering::Relaxed);
        overload_monitor_accept_tx(load_shedding_percentage, tx_data.digest())
    }

    fn update_overload_metrics(&self, source: &str) {
        self.metrics
            .transaction_overload_sources
            .with_label_values(&[source])
            .inc();
    }

    /// Executes a certificate for its effects.
    #[instrument(level = "trace", skip_all)]
    pub async fn execute_certificate(
        &self,
        certificate: &VerifiedCertificate,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<TransactionEffects> {
        let _metrics_guard = if certificate.contains_shared_object() {
            self.metrics
                .execute_certificate_latency_shared_object
                .start_timer()
        } else {
            self.metrics
                .execute_certificate_latency_single_writer
                .start_timer()
        };
        trace!("execute_certificate");

        self.metrics.total_cert_attempts.inc();

        // TODO(fastpath): use a separate function to check if a transaction should be executed in fastpath.
        if !certificate.contains_shared_object() {
            // Shared object transactions need to be sequenced by the consensus before enqueueing
            // for execution, done in AuthorityPerEpochStore::handle_consensus_transaction().
            // For owned object transactions, they can be enqueued for execution immediately.
            self.enqueue_certificates_for_execution(vec![certificate.clone()], epoch_store);
        }

        // tx could be reverted when epoch ends, so we must be careful not to return a result
        // here after the epoch ends.
        epoch_store
            .within_alive_epoch(self.notify_read_effects(*certificate.digest()))
            .await
            .map_err(|_| SuiError::EpochEnded(epoch_store.epoch()))
            .and_then(|r| r)
    }

    /// Awaits the effects of executing a user transaction.
    ///
    /// Relies on consensus to enqueue the transaction for execution.
    pub async fn await_transaction_effects(
        &self,
        digest: TransactionDigest,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<TransactionEffects> {
        let _metrics_guard = self.metrics.await_transaction_latency.start_timer();
        debug!("await_transaction");

        // TODO(fastpath): Add handling for transactions rejected by Mysticeti fast path.
        // TODO(fastpath): Can an MFP transaction be reverted after epoch ends? If so,
        // same warning as above applies: We must be careful not to return a result
        // here after the epoch ends.
        epoch_store
            .within_alive_epoch(self.notify_read_effects(digest))
            .await
            .map_err(|_| SuiError::EpochEnded(epoch_store.epoch()))
            .and_then(|r| r)
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
        mut expected_effects_digest: Option<TransactionEffectsDigest>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<(TransactionEffects, Option<ExecutionError>)> {
        let _scope = monitored_scope("Execution::try_execute_immediately");
        let _metrics_guard = self.metrics.internal_execution_latency.start_timer();

        let tx_digest = certificate.digest();

        // prevent concurrent executions of the same tx.
        let tx_guard = epoch_store.acquire_tx_guard(certificate)?;

        // The cert could have been processed by a concurrent attempt of the same cert, so check if
        // the effects have already been written.
        if let Some(effects) = self
            .get_transaction_cache_reader()
            .get_executed_effects(tx_digest)
        {
            tx_guard.release();
            return Ok((effects, None));
        }

        let execution_start_time = Instant::now();

        let input_objects =
            self.read_objects_for_execution(tx_guard.as_lock_guard(), certificate, epoch_store)?;

        if expected_effects_digest.is_none() {
            // We could be re-executing a previously executed but uncommitted transaction, perhaps after
            // restarting with a new binary. In this situation, if we have published an effects signature,
            // we must be sure not to equivocate.
            // TODO: read from cache instead of DB
            expected_effects_digest = epoch_store.get_signed_effects_digest(tx_digest)?;
        }

        let (effects, timings, execution_error_opt) = self
            .process_certificate(
                tx_guard,
                certificate,
                input_objects,
                expected_effects_digest,
                epoch_store,
            )
            .tap_err(|e| info!("process_certificate failed: {e}"))
            .tap_ok(
            |(fx, _, _)| debug!(?tx_digest, fx_digest=?fx.digest(), "process_certificate succeeded"),
        )?;

        epoch_store.record_local_execution_time(
            certificate.data().transaction_data(),
            timings,
            execution_start_time.elapsed(),
        );

        Ok((effects, execution_error_opt))
    }

    pub fn read_objects_for_execution(
        &self,
        tx_lock: &CertLockGuard,
        certificate: &VerifiedExecutableTransaction,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<InputObjects> {
        let _scope = monitored_scope("Execution::load_input_objects");
        let _metrics_guard = self
            .metrics
            .execution_load_input_objects_latency
            .start_timer();
        let input_objects = &certificate.data().transaction_data().input_objects()?;
        self.input_loader.read_objects_for_execution(
            epoch_store,
            &certificate.key(),
            tx_lock,
            input_objects,
            epoch_store.epoch(),
        )
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
        digest: TransactionDigest,
    ) -> SuiResult<TransactionEffects> {
        Ok(self
            .get_transaction_cache_reader()
            .notify_read_executed_effects(&[digest])
            .await
            .pop()
            .expect("must return correct number of effects"))
    }

    fn check_owned_locks(&self, owned_object_refs: &[ObjectRef]) -> SuiResult {
        self.get_object_cache_reader()
            .check_owned_objects_are_live(owned_object_refs)
    }

    /// This function captures the required state to debug a forked transaction.
    /// The dump is written to a file in dir `path`, with name prefixed by the transaction digest.
    /// NOTE: Since this info escapes the validator context,
    /// make sure not to leak any private info here
    pub(crate) fn debug_dump_transaction_state(
        &self,
        tx_digest: &TransactionDigest,
        effects: &TransactionEffects,
        expected_effects_digest: TransactionEffectsDigest,
        inner_temporary_store: &InnerTemporaryStore,
        certificate: &VerifiedExecutableTransaction,
        debug_dump_config: &StateDebugDumpConfig,
    ) -> SuiResult<PathBuf> {
        let dump_dir = debug_dump_config
            .dump_file_directory
            .as_ref()
            .cloned()
            .unwrap_or(std::env::temp_dir());
        let epoch_store = self.load_epoch_store_one_call_per_task();

        NodeStateDump::new(
            tx_digest,
            effects,
            expected_effects_digest,
            self.get_object_store().as_ref(),
            &epoch_store,
            inner_temporary_store,
            certificate,
        )?
        .write_to_file(&dump_dir)
        .map_err(|e| SuiError::FileIOError(e.to_string()))
    }

    #[instrument(level = "trace", skip_all)]
    pub(crate) fn process_certificate(
        &self,
        tx_guard: CertTxGuard,
        certificate: &VerifiedExecutableTransaction,
        input_objects: InputObjects,
        expected_effects_digest: Option<TransactionEffectsDigest>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<(
        TransactionEffects,
        Vec<ExecutionTiming>,
        Option<ExecutionError>,
    )> {
        let process_certificate_start_time = tokio::time::Instant::now();
        let digest = *certificate.digest();

        fail_point_if!("correlated-crash-process-certificate", || {
            if sui_simulator::random::deterministic_probability_once(&digest, 0.01) {
                sui_simulator::task::kill_current_node(None);
            }
        });

        let execution_guard = self.execution_lock_for_executable_transaction(certificate);
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
        let (inner_temporary_store, effects, timings, execution_error_opt) = match self
            .prepare_certificate(&execution_guard, certificate, input_objects, epoch_store)
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
                // We dont want to mask the original error, so we log it and continue.
                match self.debug_dump_transaction_state(
                    &digest,
                    &effects,
                    expected_effects_digest,
                    &inner_temporary_store,
                    certificate,
                    &self.config.state_debug_dump_config,
                ) {
                    Ok(out_path) => {
                        info!(
                            "Dumped node state for transaction {} to {}",
                            digest,
                            out_path.as_path().display().to_string()
                        );
                    }
                    Err(e) => {
                        error!("Error dumping state for transaction {}: {e}", digest);
                    }
                }
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

        fail_point!("crash");

        self.commit_certificate(
            certificate,
            inner_temporary_store,
            &effects,
            tx_guard,
            execution_guard,
            epoch_store,
        )?;

        if let TransactionKind::AuthenticatorStateUpdate(auth_state) =
            certificate.data().transaction_data().kind()
        {
            if let Some(err) = &execution_error_opt {
                debug_fatal!("Authenticator state update failed: {:?}", err);
            }
            epoch_store.update_authenticator_state(auth_state);

            // double check that the signature verifier always matches the authenticator state
            if cfg!(debug_assertions) {
                let authenticator_state = get_authenticator_state(self.get_object_store())
                    .expect("Read cannot fail")
                    .expect("Authenticator state must exist");

                let mut sys_jwks: Vec<_> = authenticator_state
                    .active_jwks
                    .into_iter()
                    .map(|jwk| (jwk.jwk_id, jwk.jwk))
                    .collect();
                let mut active_jwks: Vec<_> = epoch_store
                    .signature_verifier
                    .get_jwks()
                    .into_iter()
                    .collect();
                sys_jwks.sort();
                active_jwks.sort();

                assert_eq!(sys_jwks, active_jwks);
            }
        }

        let elapsed = process_certificate_start_time.elapsed().as_micros() as f64;
        if elapsed > 0.0 {
            self.metrics
                .execution_gas_latency_ratio
                .observe(effects.gas_cost_summary().computation_cost as f64 / elapsed);
        };
        Ok((effects, timings, execution_error_opt))
    }

    #[instrument(level = "trace", skip_all)]
    fn commit_certificate(
        &self,
        certificate: &VerifiedExecutableTransaction,
        inner_temporary_store: InnerTemporaryStore,
        effects: &TransactionEffects,
        tx_guard: CertTxGuard,
        _execution_guard: ExecutionLockReadGuard<'_>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        let _scope: Option<mysten_metrics::MonitoredScopeGuard> =
            monitored_scope("Execution::commit_certificate");
        let _metrics_guard = self.metrics.commit_certificate_latency.start_timer();

        let tx_key = certificate.key();
        let tx_digest = certificate.digest();
        let input_object_count = inner_temporary_store.input_objects.len();
        let shared_object_count = effects.input_shared_objects().len();

        let output_keys = inner_temporary_store.get_output_keys(effects);

        // index certificate
        let _ = self
            .post_process_one_tx(certificate, effects, &inner_temporary_store, epoch_store)
            .tap_err(|e| {
                self.metrics.post_processing_total_failures.inc();
                error!(?tx_digest, "tx post processing failed: {e}");
            });

        // The insertion to epoch_store is not atomic with the insertion to the perpetual store. This is OK because
        // we insert to the epoch store first. And during lookups we always look up in the perpetual store first.
        epoch_store.insert_tx_key(&tx_key, tx_digest)?;

        // Allow testing what happens if we crash here.
        fail_point!("crash");

        let transaction_outputs = TransactionOutputs::build_transaction_outputs(
            certificate.clone().into_unsigned(),
            effects.clone(),
            inner_temporary_store,
        );
        self.get_cache_writer().write_transaction_outputs(
            epoch_store.epoch(),
            transaction_outputs.into(),
            epoch_store
                .protocol_config()
                .use_object_per_epoch_marker_table_v2_as_option()
                .unwrap_or(false),
        );

        if certificate.transaction_data().is_end_of_epoch_tx() {
            // At the end of epoch, since system packages may have been upgraded, force
            // reload them in the cache.
            self.get_object_cache_reader()
                .force_reload_system_packages(&BuiltInFramework::all_package_ids());
        }

        // commit_certificate finished, the tx is fully committed to the store.
        tx_guard.commit_tx();

        // Notifies transaction manager about transaction and output objects committed.
        // This provides necessary information to transaction manager to start executing
        // additional ready transactions.
        self.transaction_manager
            .notify_commit(tx_digest, output_keys, epoch_store);

        self.update_metrics(certificate, input_object_count, shared_object_count);

        Ok(())
    }

    fn update_metrics(
        &self,
        certificate: &VerifiedExecutableTransaction,
        input_object_count: usize,
        shared_object_count: usize,
    ) {
        // count signature by scheme, for zklogin and multisig
        if certificate.has_zklogin_sig() {
            self.metrics.zklogin_sig_count.inc();
        } else if certificate.has_upgraded_multisig() {
            self.metrics.multisig_sig_count.inc();
        }

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
    fn prepare_certificate(
        &self,
        _execution_guard: &ExecutionLockReadGuard<'_>,
        certificate: &VerifiedExecutableTransaction,
        input_objects: InputObjects,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<(
        InnerTemporaryStore,
        TransactionEffects,
        Vec<ExecutionTiming>,
        Option<ExecutionError>,
    )> {
        let _scope = monitored_scope("Execution::prepare_certificate");
        let _metrics_guard = self.metrics.prepare_certificate_latency.start_timer();
        let prepare_certificate_start_time = tokio::time::Instant::now();

        // TODO: We need to move this to a more appropriate place to avoid redundant checks.
        let tx_data = certificate.data().transaction_data();
        tx_data.validity_check(epoch_store.protocol_config())?;

        // The cost of partially re-auditing a transaction before execution is tolerated.
        let (gas_status, input_objects) = sui_transaction_checks::check_certificate_input(
            certificate,
            input_objects,
            epoch_store.protocol_config(),
            epoch_store.reference_gas_price(),
        )?;

        let owned_object_refs = input_objects.inner().filter_owned_objects();
        self.check_owned_locks(&owned_object_refs)?;
        let tx_digest = *certificate.digest();
        let protocol_config = epoch_store.protocol_config();
        let transaction_data = &certificate.data().intent_message().value;
        let (kind, signer, gas) = transaction_data.execution_parts();

        #[allow(unused_mut)]
        let (inner_temp_store, _, mut effects, timings, execution_error_opt) =
            epoch_store.executor().execute_transaction_to_effects(
                self.get_backing_store().as_ref(),
                protocol_config,
                self.metrics.limits_metrics.clone(),
                // TODO: would be nice to pass the whole NodeConfig here, but it creates a
                // cyclic dependency w/ sui-adapter
                self.config
                    .expensive_safety_check_config
                    .enable_deep_per_tx_sui_conservation_check(),
                self.config.certificate_deny_config.certificate_deny_set(),
                &epoch_store.epoch_start_config().epoch_data().epoch_id(),
                epoch_store
                    .epoch_start_config()
                    .epoch_data()
                    .epoch_start_timestamp(),
                input_objects,
                gas,
                gas_status,
                kind,
                signer,
                tx_digest,
                &mut None,
            );

        fail_point_if!("cp_execution_nondeterminism", || {
            #[cfg(msim)]
            self.create_fail_state(certificate, epoch_store, &mut effects);
        });

        let elapsed = prepare_certificate_start_time.elapsed().as_micros() as f64;
        if elapsed > 0.0 {
            self.metrics
                .prepare_cert_gas_latency_ratio
                .observe(effects.gas_cost_summary().computation_cost as f64 / elapsed);
        }

        Ok((
            inner_temp_store,
            effects,
            timings,
            execution_error_opt.err(),
        ))
    }

    pub fn prepare_certificate_for_benchmark(
        &self,
        certificate: &VerifiedExecutableTransaction,
        input_objects: InputObjects,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<(
        InnerTemporaryStore,
        TransactionEffects,
        Option<ExecutionError>,
    )> {
        let lock = RwLock::new(epoch_store.epoch());
        let execution_guard = lock.try_read().unwrap();

        let (inner_temp_store, effects, _timings, execution_error_opt) =
            self.prepare_certificate(&execution_guard, certificate, input_objects, epoch_store)?;
        Ok((inner_temp_store, effects, execution_error_opt))
    }

    #[instrument(skip_all)]
    #[allow(clippy::type_complexity)]
    pub async fn dry_exec_transaction(
        &self,
        transaction: TransactionData,
        transaction_digest: TransactionDigest,
    ) -> SuiResult<(
        DryRunTransactionBlockResponse,
        BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)>,
        TransactionEffects,
        Option<ObjectID>,
    )> {
        let epoch_store = self.load_epoch_store_one_call_per_task();
        if !self.is_fullnode(&epoch_store) {
            return Err(SuiError::UnsupportedFeatureError {
                error: "dry-exec is only supported on fullnodes".to_string(),
            });
        }

        if transaction.kind().is_system_tx() {
            return Err(SuiError::UnsupportedFeatureError {
                error: "dry-exec does not support system transactions".to_string(),
            });
        }

        self.dry_exec_transaction_impl(&epoch_store, transaction, transaction_digest)
    }

    #[allow(clippy::type_complexity)]
    pub fn dry_exec_transaction_for_benchmark(
        &self,
        transaction: TransactionData,
        transaction_digest: TransactionDigest,
    ) -> SuiResult<(
        DryRunTransactionBlockResponse,
        BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)>,
        TransactionEffects,
        Option<ObjectID>,
    )> {
        let epoch_store = self.load_epoch_store_one_call_per_task();
        self.dry_exec_transaction_impl(&epoch_store, transaction, transaction_digest)
    }

    #[allow(clippy::type_complexity)]
    fn dry_exec_transaction_impl(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        transaction: TransactionData,
        transaction_digest: TransactionDigest,
    ) -> SuiResult<(
        DryRunTransactionBlockResponse,
        BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)>,
        TransactionEffects,
        Option<ObjectID>,
    )> {
        // Cheap validity checks for a transaction, including input size limits.
        transaction.validity_check_no_gas_check(epoch_store.protocol_config())?;

        let input_object_kinds = transaction.input_objects()?;
        let receiving_object_refs = transaction.receiving_objects();

        sui_transaction_checks::deny::check_transaction_for_signing(
            &transaction,
            &[],
            &input_object_kinds,
            &receiving_object_refs,
            &self.config.transaction_deny_config,
            self.get_backing_package_store().as_ref(),
        )?;

        let (input_objects, receiving_objects) = self.input_loader.read_objects_for_signing(
            // We don't want to cache this transaction since it's a dry run.
            None,
            &input_object_kinds,
            &receiving_object_refs,
            epoch_store.epoch(),
            epoch_store
                .protocol_config()
                .use_object_per_epoch_marker_table_v2_as_option()
                .unwrap_or(false),
        )?;

        // make a gas object if one was not provided
        let mut gas_object_refs = transaction.gas().to_vec();
        let ((gas_status, checked_input_objects), mock_gas) = if transaction.gas().is_empty() {
            let sender = transaction.sender();
            // use a 1B sui coin
            const MIST_TO_SUI: u64 = 1_000_000_000;
            const DRY_RUN_SUI: u64 = 1_000_000_000;
            let max_coin_value = MIST_TO_SUI * DRY_RUN_SUI;
            let gas_object_id = ObjectID::random();
            let gas_object = Object::new_move(
                MoveObject::new_gas_coin(OBJECT_START_VERSION, gas_object_id, max_coin_value),
                Owner::AddressOwner(sender),
                TransactionDigest::genesis_marker(),
            );
            let gas_object_ref = gas_object.compute_object_reference();
            gas_object_refs = vec![gas_object_ref];
            (
                sui_transaction_checks::check_transaction_input_with_given_gas(
                    epoch_store.protocol_config(),
                    epoch_store.reference_gas_price(),
                    &transaction,
                    input_objects,
                    receiving_objects,
                    gas_object,
                    &self.metrics.bytecode_verifier_metrics,
                    &self.config.verifier_signing_config,
                )?,
                Some(gas_object_id),
            )
        } else {
            (
                sui_transaction_checks::check_transaction_input(
                    epoch_store.protocol_config(),
                    epoch_store.reference_gas_price(),
                    &transaction,
                    input_objects,
                    &receiving_objects,
                    &self.metrics.bytecode_verifier_metrics,
                    &self.config.verifier_signing_config,
                )?,
                None,
            )
        };

        let protocol_config = epoch_store.protocol_config();
        let (kind, signer, _) = transaction.execution_parts();

        let silent = true;
        let executor = sui_execution::executor(protocol_config, silent, None)
            .expect("Creating an executor should not fail here");

        let expensive_checks = false;
        let (inner_temp_store, _, effects, _timings, _execution_error) = executor
            .execute_transaction_to_effects(
                self.get_backing_store().as_ref(),
                protocol_config,
                self.metrics.limits_metrics.clone(),
                expensive_checks,
                self.config.certificate_deny_config.certificate_deny_set(),
                &epoch_store.epoch_start_config().epoch_data().epoch_id(),
                epoch_store
                    .epoch_start_config()
                    .epoch_data()
                    .epoch_start_timestamp(),
                checked_input_objects,
                gas_object_refs,
                gas_status,
                kind,
                signer,
                transaction_digest,
                &mut None,
            );
        let tx_digest = *effects.transaction_digest();

        let module_cache =
            TemporaryModuleResolver::new(&inner_temp_store, epoch_store.module_cache().clone());

        let mut layout_resolver =
            epoch_store
                .executor()
                .type_layout_resolver(Box::new(PackageStoreWithFallback::new(
                    &inner_temp_store,
                    self.get_backing_package_store(),
                )));
        // Returning empty vector here because we recalculate changes in the rpc layer.
        let object_changes = Vec::new();

        // Returning empty vector here because we recalculate changes in the rpc layer.
        let balance_changes = Vec::new();

        let written_with_kind = effects
            .created()
            .into_iter()
            .map(|(oref, _)| (oref, WriteKind::Create))
            .chain(
                effects
                    .unwrapped()
                    .into_iter()
                    .map(|(oref, _)| (oref, WriteKind::Unwrap)),
            )
            .chain(
                effects
                    .mutated()
                    .into_iter()
                    .map(|(oref, _)| (oref, WriteKind::Mutate)),
            )
            .map(|(oref, kind)| {
                let obj = inner_temp_store.written.get(&oref.0).unwrap();
                // TODO: Avoid clones.
                (oref.0, (oref, obj.clone(), kind))
            })
            .collect();

        Ok((
            DryRunTransactionBlockResponse {
                input: SuiTransactionBlockData::try_from(transaction, &module_cache).map_err(
                    |e| SuiError::TransactionSerializationError {
                        error: format!(
                            "Failed to convert transaction to SuiTransactionBlockData: {}",
                            e
                        ),
                    },
                )?, // TODO: replace the underlying try_from to SuiError. This one goes deep
                effects: effects.clone().try_into()?,
                events: SuiTransactionBlockEvents::try_from(
                    inner_temp_store.events.clone(),
                    tx_digest,
                    None,
                    layout_resolver.as_mut(),
                )?,
                object_changes,
                balance_changes,
            },
            written_with_kind,
            effects,
            mock_gas,
        ))
    }

    pub fn simulate_transaction(
        &self,
        transaction: TransactionData,
    ) -> SuiResult<SimulateTransactionResult> {
        if transaction.kind().is_system_tx() {
            return Err(SuiError::UnsupportedFeatureError {
                error: "simulate does not support system transactions".to_string(),
            });
        }

        let epoch_store = self.load_epoch_store_one_call_per_task();
        if !self.is_fullnode(&epoch_store) {
            return Err(SuiError::UnsupportedFeatureError {
                error: "simulate is only supported on fullnodes".to_string(),
            });
        }

        self.simulate_transaction_impl(&epoch_store, transaction)
    }

    fn simulate_transaction_impl(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        transaction: TransactionData,
    ) -> SuiResult<SimulateTransactionResult> {
        // Cheap validity checks for a transaction, including input size limits.
        transaction.validity_check_no_gas_check(epoch_store.protocol_config())?;

        let input_object_kinds = transaction.input_objects()?;
        let receiving_object_refs = transaction.receiving_objects();

        sui_transaction_checks::deny::check_transaction_for_signing(
            &transaction,
            &[],
            &input_object_kinds,
            &receiving_object_refs,
            &self.config.transaction_deny_config,
            self.get_backing_package_store().as_ref(),
        )?;

        let (input_objects, receiving_objects) = self.input_loader.read_objects_for_signing(
            // We don't want to cache this transaction since it's a dry run.
            None,
            &input_object_kinds,
            &receiving_object_refs,
            epoch_store.epoch(),
            epoch_store
                .protocol_config()
                .use_object_per_epoch_marker_table_v2_as_option()
                .unwrap_or(false),
        )?;

        // make a gas object if one was not provided
        let mut gas_object_refs = transaction.gas().to_vec();
        let ((gas_status, checked_input_objects), mock_gas) = if transaction.gas().is_empty() {
            let sender = transaction.sender();
            // use a 1B sui coin
            const MIST_TO_SUI: u64 = 1_000_000_000;
            const DRY_RUN_SUI: u64 = 1_000_000_000;
            let max_coin_value = MIST_TO_SUI * DRY_RUN_SUI;
            let gas_object_id = ObjectID::MAX;
            let gas_object = Object::new_move(
                MoveObject::new_gas_coin(OBJECT_START_VERSION, gas_object_id, max_coin_value),
                Owner::AddressOwner(sender),
                TransactionDigest::genesis_marker(),
            );
            let gas_object_ref = gas_object.compute_object_reference();
            gas_object_refs = vec![gas_object_ref];
            (
                sui_transaction_checks::check_transaction_input_with_given_gas(
                    epoch_store.protocol_config(),
                    epoch_store.reference_gas_price(),
                    &transaction,
                    input_objects,
                    receiving_objects,
                    gas_object,
                    &self.metrics.bytecode_verifier_metrics,
                    &self.config.verifier_signing_config,
                )?,
                Some(gas_object_id),
            )
        } else {
            (
                sui_transaction_checks::check_transaction_input(
                    epoch_store.protocol_config(),
                    epoch_store.reference_gas_price(),
                    &transaction,
                    input_objects,
                    &receiving_objects,
                    &self.metrics.bytecode_verifier_metrics,
                    &self.config.verifier_signing_config,
                )?,
                None,
            )
        };

        let protocol_config = epoch_store.protocol_config();
        let (kind, signer, _) = transaction.execution_parts();

        let silent = true;
        let executor = sui_execution::executor(protocol_config, silent, None)
            .expect("Creating an executor should not fail here");

        let expensive_checks = false;
        let (inner_temp_store, _, effects, _timings, _execution_error) = executor
            .execute_transaction_to_effects(
                self.get_backing_store().as_ref(),
                protocol_config,
                self.metrics.limits_metrics.clone(),
                expensive_checks,
                self.config.certificate_deny_config.certificate_deny_set(),
                &epoch_store.epoch_start_config().epoch_data().epoch_id(),
                epoch_store
                    .epoch_start_config()
                    .epoch_data()
                    .epoch_start_timestamp(),
                checked_input_objects,
                gas_object_refs,
                gas_status,
                kind,
                signer,
                transaction.digest(),
                &mut None,
            );

        Ok(SimulateTransactionResult {
            input_objects: inner_temp_store.input_objects,
            output_objects: inner_temp_store.written,
            events: effects.events_digest().map(|_| inner_temp_store.events),
            effects,
            mock_gas_id: mock_gas,
        })
    }

    /// The object ID for gas can be any object ID, even for an uncreated object
    #[allow(clippy::collapsible_else_if)]
    #[instrument(skip_all)]
    pub async fn dev_inspect_transaction_block(
        &self,
        sender: SuiAddress,
        transaction_kind: TransactionKind,
        gas_price: Option<u64>,
        gas_budget: Option<u64>,
        gas_sponsor: Option<SuiAddress>,
        gas_objects: Option<Vec<ObjectRef>>,
        show_raw_txn_data_and_effects: Option<bool>,
        skip_checks: Option<bool>,
    ) -> SuiResult<DevInspectResults> {
        let epoch_store = self.load_epoch_store_one_call_per_task();

        if !self.is_fullnode(&epoch_store) {
            return Err(SuiError::UnsupportedFeatureError {
                error: "dev-inspect is only supported on fullnodes".to_string(),
            });
        }

        if transaction_kind.is_system_tx() {
            return Err(SuiError::UnsupportedFeatureError {
                error: "system transactions are not supported".to_string(),
            });
        }

        let show_raw_txn_data_and_effects = show_raw_txn_data_and_effects.unwrap_or(false);
        let skip_checks = skip_checks.unwrap_or(true);
        let reference_gas_price = epoch_store.reference_gas_price();
        let protocol_config = epoch_store.protocol_config();
        let max_tx_gas = protocol_config.max_tx_gas();

        let price = gas_price.unwrap_or(reference_gas_price);
        let budget = gas_budget.unwrap_or(max_tx_gas);
        let owner = gas_sponsor.unwrap_or(sender);
        // Payment might be empty here, but it's fine we'll have to deal with it later after reading all the input objects.
        let payment = gas_objects.unwrap_or_default();
        let transaction = TransactionData::V1(TransactionDataV1 {
            kind: transaction_kind.clone(),
            sender,
            gas_data: GasData {
                payment,
                owner,
                price,
                budget,
            },
            expiration: TransactionExpiration::None,
        });

        let raw_txn_data = if show_raw_txn_data_and_effects {
            bcs::to_bytes(&transaction).map_err(|_| SuiError::TransactionSerializationError {
                error: "Failed to serialize transaction during dev inspect".to_string(),
            })?
        } else {
            vec![]
        };

        transaction.validity_check_no_gas_check(protocol_config)?;

        let input_object_kinds = transaction.input_objects()?;
        let receiving_object_refs = transaction.receiving_objects();

        sui_transaction_checks::deny::check_transaction_for_signing(
            &transaction,
            &[],
            &input_object_kinds,
            &receiving_object_refs,
            &self.config.transaction_deny_config,
            self.get_backing_package_store().as_ref(),
        )?;

        let (mut input_objects, receiving_objects) = self.input_loader.read_objects_for_signing(
            // We don't want to cache this transaction since it's a dev inspect.
            None,
            &input_object_kinds,
            &receiving_object_refs,
            epoch_store.epoch(),
            epoch_store
                .protocol_config()
                .use_object_per_epoch_marker_table_v2_as_option()
                .unwrap_or(false),
        )?;

        // Create and use a dummy gas object if there is no gas object provided.
        let dummy_gas_object = Object::new_gas_with_balance_and_owner_for_testing(
            DEV_INSPECT_GAS_COIN_VALUE,
            transaction.gas_owner(),
        );

        let gas_objects = if transaction.gas().is_empty() {
            let gas_object_ref = dummy_gas_object.compute_object_reference();
            vec![gas_object_ref]
        } else {
            transaction.gas().to_vec()
        };

        let (gas_status, checked_input_objects) = if skip_checks {
            // If we are skipping checks, then we call the check_dev_inspect_input function which will perform
            // only lightweight checks on the transaction input. And if the gas field is empty, that means we will
            // use the dummy gas object so we need to add it to the input objects vector.
            if transaction.gas().is_empty() {
                input_objects.push(ObjectReadResult::new(
                    InputObjectKind::ImmOrOwnedMoveObject(gas_objects[0]),
                    dummy_gas_object.into(),
                ));
            }
            let checked_input_objects = sui_transaction_checks::check_dev_inspect_input(
                protocol_config,
                &transaction_kind,
                input_objects,
                receiving_objects,
            )?;
            let gas_status = SuiGasStatus::new(
                max_tx_gas,
                transaction.gas_price(),
                reference_gas_price,
                protocol_config,
            )?;

            (gas_status, checked_input_objects)
        } else {
            // If we are not skipping checks, then we call the check_transaction_input function and its dummy gas
            // variant which will perform full fledged checks just like a real transaction execution.
            if transaction.gas().is_empty() {
                sui_transaction_checks::check_transaction_input_with_given_gas(
                    epoch_store.protocol_config(),
                    epoch_store.reference_gas_price(),
                    &transaction,
                    input_objects,
                    receiving_objects,
                    dummy_gas_object,
                    &self.metrics.bytecode_verifier_metrics,
                    &self.config.verifier_signing_config,
                )?
            } else {
                sui_transaction_checks::check_transaction_input(
                    epoch_store.protocol_config(),
                    epoch_store.reference_gas_price(),
                    &transaction,
                    input_objects,
                    &receiving_objects,
                    &self.metrics.bytecode_verifier_metrics,
                    &self.config.verifier_signing_config,
                )?
            }
        };

        let executor = sui_execution::executor(protocol_config, /* silent */ true, None)
            .expect("Creating an executor should not fail here");
        let intent_msg = IntentMessage::new(
            Intent {
                version: IntentVersion::V0,
                scope: IntentScope::TransactionData,
                app_id: AppId::Sui,
            },
            transaction,
        );
        let transaction_digest = TransactionDigest::new(default_hash(&intent_msg.value));
        let (inner_temp_store, _, effects, execution_result) = executor.dev_inspect_transaction(
            self.get_backing_store().as_ref(),
            protocol_config,
            self.metrics.limits_metrics.clone(),
            /* expensive checks */ false,
            self.config.certificate_deny_config.certificate_deny_set(),
            &epoch_store.epoch_start_config().epoch_data().epoch_id(),
            epoch_store
                .epoch_start_config()
                .epoch_data()
                .epoch_start_timestamp(),
            checked_input_objects,
            gas_objects,
            gas_status,
            transaction_kind,
            sender,
            transaction_digest,
            skip_checks,
        );

        let raw_effects = if show_raw_txn_data_and_effects {
            bcs::to_bytes(&effects).map_err(|_| SuiError::TransactionSerializationError {
                error: "Failed to serialize transaction effects during dev inspect".to_string(),
            })?
        } else {
            vec![]
        };

        let mut layout_resolver =
            epoch_store
                .executor()
                .type_layout_resolver(Box::new(PackageStoreWithFallback::new(
                    &inner_temp_store,
                    self.get_backing_package_store(),
                )));

        DevInspectResults::new(
            effects,
            inner_temp_store.events.clone(),
            execution_result,
            raw_txn_data,
            raw_effects,
            layout_resolver.as_mut(),
        )
    }

    // Only used for testing because of how epoch store is loaded.
    pub fn reference_gas_price_for_testing(&self) -> Result<u64, anyhow::Error> {
        let epoch_store = self.epoch_store_for_testing();
        Ok(epoch_store.reference_gas_price())
    }

    pub fn is_tx_already_executed(&self, digest: &TransactionDigest) -> bool {
        self.get_transaction_cache_reader()
            .is_tx_already_executed(digest)
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
        tx_coins: Option<TxCoins>,
        written: &WrittenObjects,
        inner_temporary_store: &InnerTemporaryStore,
    ) -> SuiResult<u64> {
        let changes = self
            .process_object_index(effects, written, inner_temporary_store)
            .tap_err(|e| warn!(tx_digest=?digest, "Failed to process object index, index_tx is skipped: {e}"))?;

        indexes.index_tx(
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
                .map(|(obj_ref, owner, _kind)| (obj_ref, owner)),
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
        )
    }

    #[cfg(msim)]
    fn create_fail_state(
        &self,
        certificate: &VerifiedExecutableTransaction,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        effects: &mut TransactionEffects,
    ) {
        use std::cell::RefCell;
        thread_local! {
            static FAIL_STATE: RefCell<(u64, HashSet<AuthorityName>)> = RefCell::new((0, HashSet::new()));
        }
        if !certificate.data().intent_message().value.is_system_tx() {
            let committee = epoch_store.committee();
            let cur_stake = (**committee).weight(&self.name);
            if cur_stake > 0 {
                FAIL_STATE.with_borrow_mut(|fail_state| {
                    //let (&mut failing_stake, &mut failing_validators) = fail_state;
                    if fail_state.0 < committee.validity_threshold() {
                        fail_state.0 += cur_stake;
                        fail_state.1.insert(self.name);
                    }

                    if fail_state.1.contains(&self.name) {
                        info!("cp_exec failing tx");
                        effects.gas_cost_summary_mut_for_testing().computation_cost += 1;
                    }
                });
            }
        }
    }

    fn process_object_index(
        &self,
        effects: &TransactionEffects,
        written: &WrittenObjects,
        inner_temporary_store: &InnerTemporaryStore,
    ) -> SuiResult<ObjectIndexChanges> {
        let epoch_store = self.load_epoch_store_one_call_per_task();
        let mut layout_resolver =
            epoch_store
                .executor()
                .type_layout_resolver(Box::new(PackageStoreWithFallback::new(
                    inner_temporary_store,
                    self.get_backing_package_store(),
                )));

        let modified_at_version = effects
            .modified_at_versions()
            .into_iter()
            .collect::<HashMap<_, _>>();

        let tx_digest = effects.transaction_digest();
        let mut deleted_owners = vec![];
        let mut deleted_dynamic_fields = vec![];
        for (id, _, _) in effects.deleted().into_iter().chain(effects.wrapped()) {
            let old_version = modified_at_version.get(&id).unwrap();
            // When we process the index, the latest object hasn't been written yet so
            // the old object must be present.
            match self.get_owner_at_version(&id, *old_version).unwrap_or_else(
                |e| panic!("tx_digest={:?}, error processing object owner index, cannot find owner for object {:?} at version {:?}. Err: {:?}", tx_digest, id, old_version, e),
            ) {
                Owner::AddressOwner(addr) => deleted_owners.push((addr, id)),
                Owner::ObjectOwner(object_id) => {
                    deleted_dynamic_fields.push((ObjectID::from(object_id), id))
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
                let Some(old_version) = modified_at_version.get(id) else {
                    panic!("tx_digest={:?}, error processing object owner index, cannot find modified at version for mutated object [{id}].", tx_digest);
                };
                // When we process the index, the latest object hasn't been written yet so
                // the old object must be present.
                let Some(old_object) = self.get_object_store().get_object_by_key(id, *old_version)
                else {
                    panic!("tx_digest={:?}, error processing object owner index, cannot find owner for object {:?} at version {:?}", tx_digest, id, old_version);
                };
                if old_object.owner != owner {
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
                    let new_object = written.get(id).unwrap_or_else(
                        || panic!("tx_digest={:?}, error processing object owner index, written does not contain object {:?}", tx_digest, id)
                    );
                    assert_eq!(new_object.version(), oref.1, "tx_digest={:?} error processing object owner index, object {:?} from written has mismatched version. Actual: {}, expected: {}", tx_digest, id, new_object.version(), oref.1);

                    let type_ = new_object
                        .type_()
                        .map(|type_| ObjectType::Struct(type_.clone()))
                        .unwrap_or(ObjectType::Package);

                    new_owners.push((
                        (addr, *id),
                        ObjectInfo {
                            object_id: *id,
                            version: oref.1,
                            digest: oref.2,
                            type_,
                            owner,
                            previous_transaction: *effects.transaction_digest(),
                        },
                    ));
                }
                Owner::ObjectOwner(owner) => {
                    let new_object = written.get(id).unwrap_or_else(
                        || panic!("tx_digest={:?}, error processing object owner index, written does not contain object {:?}", tx_digest, id)
                    );
                    assert_eq!(new_object.version(), oref.1, "tx_digest={:?} error processing object owner index, object {:?} from written has mismatched version. Actual: {}, expected: {}", tx_digest, id, new_object.version(), oref.1);

                    let Some(df_info) = self
                        .try_create_dynamic_field_info(new_object, written, layout_resolver.as_mut())
                        .unwrap_or_else(|e| {
                            error!("try_create_dynamic_field_info should not fail, {}, new_object={:?}", e, new_object);
                            None
                        }
                    )
                        else {
                            // Skip indexing for non dynamic field objects.
                            continue;
                        };
                    new_dynamic_fields.push(((ObjectID::from(owner), *id), df_info))
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
        written: &WrittenObjects,
        resolver: &mut dyn LayoutResolver,
    ) -> SuiResult<Option<DynamicFieldInfo>> {
        // Skip if not a move object
        let Some(move_object) = o.data.try_as_move().cloned() else {
            return Ok(None);
        };

        // We only index dynamic field objects
        if !move_object.type_().is_dynamic_field() {
            return Ok(None);
        }

        let layout = resolver
            .get_annotated_layout(&move_object.type_().clone().into())?
            .into_layout();

        let field =
            DFV::FieldVisitor::deserialize(move_object.contents(), &layout).map_err(|e| {
                SuiError::ObjectDeserializationError {
                    error: e.to_string(),
                }
            })?;

        let type_ = field.kind;
        let name_type: TypeTag = field.name_layout.into();
        let bcs_name = field.name_bytes.to_owned();

        let name_value = BoundedVisitor::deserialize_value(field.name_bytes, field.name_layout)
            .map_err(|e| {
                warn!("{e}");
                SuiError::ObjectDeserializationError {
                    error: e.to_string(),
                }
            })?;

        let name = DynamicFieldName {
            type_: name_type,
            value: SuiMoveValue::from(name_value).to_json_value(),
        };

        let value_metadata = field.value_metadata().map_err(|e| {
            warn!("{e}");
            SuiError::ObjectDeserializationError {
                error: e.to_string(),
            }
        })?;

        Ok(Some(match value_metadata {
            DFV::ValueMetadata::DynamicField(object_type) => DynamicFieldInfo {
                name,
                bcs_name,
                type_,
                object_type: object_type.to_canonical_string(/* with_prefix */ true),
                object_id: o.id(),
                version: o.version(),
                digest: o.digest(),
            },

            DFV::ValueMetadata::DynamicObjectField(object_id) => {
                // Find the actual object from storage using the object id obtained from the wrapper.

                // Try to find the object in the written objects first.
                let (version, digest, object_type) = if let Some(object) = written.get(&object_id) {
                    let version = object.version();
                    let digest = object.digest();
                    let object_type = object.data.type_().unwrap().clone();
                    (version, digest, object_type)
                } else {
                    // If not found, try to find it in the database.
                    let object = self
                        .get_object_store()
                        .get_object_by_key(&object_id, o.version())
                        .ok_or_else(|| UserInputError::ObjectNotFound {
                            object_id,
                            version: Some(o.version()),
                        })?;
                    let version = object.version();
                    let digest = object.digest();
                    let object_type = object.data.type_().unwrap().clone();
                    (version, digest, object_type)
                };

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
        }))
    }

    #[instrument(level = "trace", skip_all, err)]
    fn post_process_one_tx(
        &self,
        certificate: &VerifiedExecutableTransaction,
        effects: &TransactionEffects,
        inner_temporary_store: &InnerTemporaryStore,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        if self.indexes.is_none() {
            return Ok(());
        }

        let tx_digest = certificate.digest();
        let timestamp_ms = Self::unixtime_now_ms();
        let events = &inner_temporary_store.events;
        let written = &inner_temporary_store.written;
        let tx_coins =
            self.fullnode_only_get_tx_coins_for_indexing(inner_temporary_store, epoch_store);

        // Index tx
        if let Some(indexes) = &self.indexes {
            let _ = self
                .index_tx(
                    indexes.as_ref(),
                    tx_digest,
                    certificate,
                    effects,
                    events,
                    timestamp_ms,
                    tx_coins,
                    written,
                    inner_temporary_store,
                )
                .tap_ok(|_| self.metrics.post_processing_total_tx_indexed.inc())
                .tap_err(|e| error!(?tx_digest, "Post processing - Couldn't index tx: {e}"))
                .expect("Indexing tx should not fail");

            let effects: SuiTransactionBlockEffects = effects.clone().try_into()?;
            let events = self.make_transaction_block_events(
                events.clone(),
                *tx_digest,
                timestamp_ms,
                epoch_store,
                inner_temporary_store,
            )?;
            // Emit events
            self.subscription_handler
                .process_tx(certificate.data().transaction_data(), &effects, &events)
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
        };
        Ok(())
    }

    fn make_transaction_block_events(
        &self,
        transaction_events: TransactionEvents,
        digest: TransactionDigest,
        timestamp_ms: u64,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        inner_temporary_store: &InnerTemporaryStore,
    ) -> SuiResult<SuiTransactionBlockEvents> {
        let mut layout_resolver =
            epoch_store
                .executor()
                .type_layout_resolver(Box::new(PackageStoreWithFallback::new(
                    inner_temporary_store,
                    self.get_backing_package_store(),
                )));
        SuiTransactionBlockEvents::try_from(
            transaction_events,
            digest,
            Some(timestamp_ms),
            layout_resolver.as_mut(),
        )
    }

    pub fn unixtime_now_ms() -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();
        u64::try_from(now).expect("Travelling in time machine")
    }

    // TODO(fastpath): update this handler for Mysticeti fastpath.
    // There will no longer be validator quorum signed transactions or effects.
    // The proof of finality needs to come from checkpoints.
    #[instrument(level = "trace", skip_all)]
    pub async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> SuiResult<TransactionInfoResponse> {
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

    #[instrument(level = "trace", skip_all)]
    pub async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> SuiResult<ObjectInfoResponse> {
        let epoch_store = self.load_epoch_store_one_call_per_task();

        let requested_object_seq = match request.request_kind {
            ObjectInfoRequestKind::LatestObjectInfo => {
                let (_, seq, _) = self
                    .get_object_or_tombstone(request.object_id)
                    .await
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
            .get_object_store()
            .get_object_by_key(&request.object_id, requested_object_seq)
            .ok_or_else(|| {
                SuiError::from(UserInputError::ObjectNotFound {
                    object_id: request.object_id,
                    version: Some(requested_object_seq),
                })
            })?;

        let layout = if let (LayoutGenerationOption::Generate, Some(move_obj)) =
            (request.generate_layout, object.data.try_as_move())
        {
            Some(into_struct_layout(
                self.load_epoch_store_one_call_per_task()
                    .executor()
                    .type_layout_resolver(Box::new(self.get_backing_package_store().as_ref()))
                    .get_annotated_layout(&move_obj.type_().clone().into())?,
            )?)
        } else {
            None
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

    #[instrument(level = "trace", skip_all)]
    pub fn handle_checkpoint_request(
        &self,
        request: &CheckpointRequest,
    ) -> SuiResult<CheckpointResponse> {
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

    #[instrument(level = "trace", skip_all)]
    pub fn handle_checkpoint_request_v2(
        &self,
        request: &CheckpointRequestV2,
    ) -> SuiResult<CheckpointResponseV2> {
        let summary = if request.certified {
            let summary = match request.sequence_number {
                Some(seq) => self
                    .checkpoint_store
                    .get_checkpoint_by_sequence_number(seq)?,
                None => self.checkpoint_store.get_latest_certified_checkpoint(),
            }
            .map(|v| v.into_inner());
            summary.map(CheckpointSummaryResponse::Certified)
        } else {
            let summary = match request.sequence_number {
                Some(seq) => self.checkpoint_store.get_locally_computed_checkpoint(seq)?,
                None => self
                    .checkpoint_store
                    .get_latest_locally_computed_checkpoint(),
            };
            summary.map(CheckpointSummaryResponse::Pending)
        };
        let contents = match &summary {
            Some(s) => self
                .checkpoint_store
                .get_checkpoint_contents(&s.content_digest())?,
            None => None,
        };
        Ok(CheckpointResponseV2 {
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

    #[allow(clippy::disallowed_methods)] // allow unbounded_channel()
    pub async fn new(
        name: AuthorityName,
        secret: StableSyncAuthoritySigner,
        supported_protocol_versions: SupportedProtocolVersions,
        store: Arc<AuthorityStore>,
        execution_cache_trait_pointers: ExecutionCacheTraitPointers,
        epoch_store: Arc<AuthorityPerEpochStore>,
        committee_store: Arc<CommitteeStore>,
        indexes: Option<Arc<IndexStore>>,
        rpc_index: Option<Arc<RpcIndexStore>>,
        checkpoint_store: Arc<CheckpointStore>,
        prometheus_registry: &Registry,
        genesis_objects: &[Object],
        db_checkpoint_config: &DBCheckpointConfig,
        config: NodeConfig,
        archive_readers: ArchiveReaderBalancer,
        validator_tx_finalizer: Option<Arc<ValidatorTxFinalizer<NetworkAuthorityClient>>>,
        chain_identifier: ChainIdentifier,
        pruner_db: Option<Arc<AuthorityPrunerTables>>,
    ) -> Arc<Self> {
        Self::check_protocol_version(supported_protocol_versions, epoch_store.protocol_version());

        let metrics = Arc::new(AuthorityMetrics::new(prometheus_registry));
        let (tx_ready_certificates, rx_ready_certificates) = unbounded_channel();
        let transaction_manager = Arc::new(TransactionManager::new(
            execution_cache_trait_pointers.object_cache_reader.clone(),
            execution_cache_trait_pointers
                .transaction_cache_reader
                .clone(),
            &epoch_store,
            tx_ready_certificates,
            metrics.clone(),
        ));
        let (tx_execution_shutdown, rx_execution_shutdown) = oneshot::channel();

        let _authority_per_epoch_pruner = AuthorityPerEpochStorePruner::new(
            epoch_store.get_parent_path(),
            &config.authority_store_pruning_config,
        );
        let _pruner = AuthorityStorePruner::new(
            store.perpetual_tables.clone(),
            checkpoint_store.clone(),
            rpc_index.clone(),
            config.authority_store_pruning_config.clone(),
            epoch_store.committee().authority_exists(&name),
            epoch_store.epoch_start_state().epoch_duration_ms(),
            prometheus_registry,
            archive_readers,
            pruner_db,
        );
        let input_loader =
            TransactionInputLoader::new(execution_cache_trait_pointers.object_cache_reader.clone());
        let epoch = epoch_store.epoch();
        let state = Arc::new(AuthorityState {
            name,
            secret,
            execution_lock: RwLock::new(epoch),
            epoch_store: ArcSwap::new(epoch_store.clone()),
            input_loader,
            execution_cache_trait_pointers,
            indexes,
            rpc_index,
            subscription_handler: Arc::new(SubscriptionHandler::new(prometheus_registry)),
            checkpoint_store,
            committee_store,
            transaction_manager,
            tx_execution_shutdown: Mutex::new(Some(tx_execution_shutdown)),
            metrics,
            _pruner,
            _authority_per_epoch_pruner,
            db_checkpoint_config: db_checkpoint_config.clone(),
            config,
            overload_info: AuthorityOverloadInfo::default(),
            validator_tx_finalizer,
            chain_identifier,
            congestion_tracker: Arc::new(CongestionTracker::new()),
        });

        // Start a task to execute ready certificates.
        let authority_state = Arc::downgrade(&state);
        spawn_monitored_task!(execution_process(
            authority_state,
            rx_ready_certificates,
            rx_execution_shutdown,
        ));

        // TODO: This doesn't belong to the constructor of AuthorityState.
        state
            .create_owner_index_if_empty(genesis_objects, &epoch_store)
            .expect("Error indexing genesis objects.");

        state
    }

    // TODO: Consolidate our traits to reduce the number of methods here.
    pub fn get_object_cache_reader(&self) -> &Arc<dyn ObjectCacheRead> {
        &self.execution_cache_trait_pointers.object_cache_reader
    }

    pub fn get_transaction_cache_reader(&self) -> &Arc<dyn TransactionCacheRead> {
        &self.execution_cache_trait_pointers.transaction_cache_reader
    }

    pub fn get_cache_writer(&self) -> &Arc<dyn ExecutionCacheWrite> {
        &self.execution_cache_trait_pointers.cache_writer
    }

    pub fn get_backing_store(&self) -> &Arc<dyn BackingStore + Send + Sync> {
        &self.execution_cache_trait_pointers.backing_store
    }

    pub fn get_backing_package_store(&self) -> &Arc<dyn BackingPackageStore + Send + Sync> {
        &self.execution_cache_trait_pointers.backing_package_store
    }

    pub fn get_object_store(&self) -> &Arc<dyn ObjectStore + Send + Sync> {
        &self.execution_cache_trait_pointers.object_store
    }

    pub fn get_reconfig_api(&self) -> &Arc<dyn ExecutionCacheReconfigAPI> {
        &self.execution_cache_trait_pointers.reconfig_api
    }

    pub fn get_accumulator_store(&self) -> &Arc<dyn AccumulatorStore> {
        &self.execution_cache_trait_pointers.accumulator_store
    }

    pub fn get_checkpoint_cache(&self) -> &Arc<dyn CheckpointCache> {
        &self.execution_cache_trait_pointers.checkpoint_cache
    }

    pub fn get_state_sync_store(&self) -> &Arc<dyn StateSyncAPI> {
        &self.execution_cache_trait_pointers.state_sync_store
    }

    pub fn get_cache_commit(&self) -> &Arc<dyn ExecutionCacheCommit> {
        &self.execution_cache_trait_pointers.cache_commit
    }

    pub fn database_for_testing(&self) -> Arc<AuthorityStore> {
        self.execution_cache_trait_pointers
            .testing_api
            .database_for_testing()
    }

    pub async fn prune_checkpoints_for_eligible_epochs_for_testing(
        &self,
        config: NodeConfig,
        metrics: Arc<AuthorityStorePruningMetrics>,
    ) -> anyhow::Result<()> {
        let archive_readers =
            ArchiveReaderBalancer::new(config.archive_reader_config(), &Registry::default())?;
        AuthorityStorePruner::prune_checkpoints_for_eligible_epochs(
            &self.database_for_testing().perpetual_tables,
            &self.checkpoint_store,
            self.rpc_index.as_deref(),
            None,
            config.authority_store_pruning_config,
            metrics,
            archive_readers,
            EPOCH_DURATION_MS_FOR_TESTING,
        )
        .await
    }

    pub fn transaction_manager(&self) -> &Arc<TransactionManager> {
        &self.transaction_manager
    }

    /// Adds transactions / certificates to transaction manager for ordered execution.
    pub fn enqueue_transactions_for_execution(
        &self,
        txns: Vec<VerifiedExecutableTransaction>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        self.transaction_manager.enqueue(txns, epoch_store)
    }
    pub fn enqueue_certificates_for_execution(
        &self,
        certs: Vec<VerifiedCertificate>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        self.transaction_manager
            .enqueue_certificates(certs, epoch_store)
    }

    pub fn enqueue_with_expected_effects_digest(
        &self,
        certs: Vec<(VerifiedExecutableTransaction, TransactionEffectsDigest)>,
        epoch_store: &AuthorityPerEpochStore,
    ) {
        self.transaction_manager
            .enqueue_with_expected_effects_digest(certs, epoch_store)
    }

    fn create_owner_index_if_empty(
        &self,
        genesis_objects: &[Object],
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        let Some(index_store) = &self.indexes else {
            return Ok(());
        };
        if !index_store.is_empty() {
            return Ok(());
        }

        let mut new_owners = vec![];
        let mut new_dynamic_fields = vec![];
        let mut layout_resolver = epoch_store
            .executor()
            .type_layout_resolver(Box::new(self.get_backing_package_store().as_ref()));
        for o in genesis_objects.iter() {
            match o.owner {
                Owner::AddressOwner(addr) => new_owners.push((
                    (addr, o.id()),
                    ObjectInfo::new(&o.compute_object_reference(), o),
                )),
                Owner::ObjectOwner(object_id) => {
                    let id = o.id();
                    let Some(info) = self.try_create_dynamic_field_info(
                        o,
                        &BTreeMap::new(),
                        layout_resolver.as_mut(),
                    )?
                    else {
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

    /// Attempts to acquire execution lock for an executable transaction.
    /// Returns the lock if the transaction is matching current executed epoch
    /// Returns None otherwise
    pub fn execution_lock_for_executable_transaction(
        &self,
        transaction: &VerifiedExecutableTransaction,
    ) -> SuiResult<ExecutionLockReadGuard> {
        let lock = self
            .execution_lock
            .try_read()
            .map_err(|_| SuiError::ValidatorHaltedAtEpochEnd)?;
        if *lock == transaction.auth_sig().epoch() {
            Ok(lock)
        } else {
            Err(SuiError::WrongEpoch {
                expected_epoch: *lock,
                actual_epoch: transaction.auth_sig().epoch(),
            })
        }
    }

    /// Acquires the execution lock for the duration of a transaction signing request.
    /// This prevents reconfiguration from starting until we are finished handling the signing request.
    /// Otherwise, in-memory lock state could be cleared (by `ObjectLocks::clear_cached_locks`)
    /// while we are attempting to acquire locks for the transaction.
    pub fn execution_lock_for_signing(&self) -> SuiResult<ExecutionLockReadGuard> {
        self.execution_lock
            .try_read()
            .map_err(|_| SuiError::ValidatorHaltedAtEpochEnd)
    }

    pub async fn execution_lock_for_reconfiguration(&self) -> ExecutionLockWriteGuard {
        self.execution_lock.write().await
    }

    #[instrument(level = "error", skip_all)]
    pub async fn reconfigure(
        &self,
        cur_epoch_store: &AuthorityPerEpochStore,
        supported_protocol_versions: SupportedProtocolVersions,
        new_committee: Committee,
        epoch_start_configuration: EpochStartConfiguration,
        accumulator: Arc<StateAccumulator>,
        expensive_safety_check_config: &ExpensiveSafetyCheckConfig,
        epoch_last_checkpoint: CheckpointSequenceNumber,
    ) -> SuiResult<Arc<AuthorityPerEpochStore>> {
        Self::check_protocol_version(
            supported_protocol_versions,
            epoch_start_configuration
                .epoch_start_state()
                .protocol_version(),
        );

        self.committee_store.insert_new_committee(&new_committee)?;

        // Wait until no transactions are being executed.
        let mut execution_lock = self.execution_lock_for_reconfiguration().await;

        // Terminate all epoch-specific tasks (those started with within_alive_epoch).
        cur_epoch_store.epoch_terminated().await;

        let highest_locally_built_checkpoint_seq = self
            .checkpoint_store
            .get_latest_locally_computed_checkpoint()
            .map(|c| *c.sequence_number())
            .unwrap_or(0);

        assert!(
            epoch_last_checkpoint >= highest_locally_built_checkpoint_seq,
            "{epoch_last_checkpoint} >= {highest_locally_built_checkpoint_seq}"
        );
        if highest_locally_built_checkpoint_seq == epoch_last_checkpoint {
            // if we built the last checkpoint locally (as opposed to receiving it from a peer),
            // then all shared_version_assignments except the one for the ChangeEpoch transaction
            // should have been removed
            let num_shared_version_assignments = cur_epoch_store.num_shared_version_assignments();
            // Note that while 1 is the typical value, 0 is possible if the node restarts after
            // committing the last checkpoint but before reconfiguring.
            if num_shared_version_assignments > 1 {
                // If this happens in prod, we have a memory leak, but not a correctness issue.
                debug_fatal!("all shared_version_assignments should have been removed (num_shared_version_assignments: {num_shared_version_assignments})");
            }
        }

        // Safe to being reconfiguration now. No transactions are being executed,
        // and no epoch-specific tasks are running.

        // TODO: revert_uncommitted_epoch_transactions will soon be unnecessary -
        // clear_state_end_of_epoch() can simply drop all uncommitted transactions
        self.revert_uncommitted_epoch_transactions(cur_epoch_store)
            .await?;
        self.get_reconfig_api()
            .clear_state_end_of_epoch(&execution_lock);
        self.check_system_consistency(cur_epoch_store, accumulator, expensive_safety_check_config);
        self.maybe_reaccumulate_state_hash(
            cur_epoch_store,
            epoch_start_configuration
                .epoch_start_state()
                .protocol_version(),
        );
        self.get_reconfig_api()
            .set_epoch_start_configuration(&epoch_start_configuration);
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

        self.get_reconfig_api()
            .reconfigure_cache(&epoch_start_configuration)
            .await;

        let new_epoch = new_committee.epoch;
        let new_epoch_store = self
            .reopen_epoch_db(
                cur_epoch_store,
                new_committee,
                epoch_start_configuration,
                expensive_safety_check_config,
                epoch_last_checkpoint,
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

    /// Advance the epoch store to the next epoch for testing only.
    /// This only manually sets all the places where we have the epoch number.
    /// It doesn't properly reconfigure the node, hence should be only used for testing.
    pub async fn reconfigure_for_testing(&self) {
        let mut execution_lock = self.execution_lock_for_reconfiguration().await;
        let epoch_store = self.epoch_store_for_testing().clone();
        let protocol_config = epoch_store.protocol_config().clone();
        // The current protocol config used in the epoch store may have been overridden and diverged from
        // the protocol config definitions. That override may have now been dropped when the initial guard was dropped.
        // We reapply the override before creating the new epoch store, to make sure that
        // the new epoch store has the same protocol config as the current one.
        // Since this is for testing only, we mostly like to keep the protocol config the same
        // across epochs.
        let _guard =
            ProtocolConfig::apply_overrides_for_testing(move |_, _| protocol_config.clone());
        let new_epoch_store = epoch_store.new_at_next_epoch_for_testing(
            self.get_backing_package_store().clone(),
            self.get_object_store().clone(),
            &self.config.expensive_safety_check_config,
            self.checkpoint_store
                .get_epoch_last_checkpoint(epoch_store.epoch())
                .unwrap()
                .map(|c| *c.sequence_number())
                .unwrap_or_default(),
        );
        let new_epoch = new_epoch_store.epoch();
        self.transaction_manager.reconfigure(new_epoch);
        self.epoch_store.store(new_epoch_store);
        epoch_store.epoch_terminated().await;
        *execution_lock = new_epoch;
    }

    /// This is a temporary method to be used when we enable simplified_unwrap_then_delete.
    /// It re-accumulates state hash for the new epoch if simplified_unwrap_then_delete is enabled.
    #[instrument(level = "error", skip_all)]
    fn maybe_reaccumulate_state_hash(
        &self,
        cur_epoch_store: &AuthorityPerEpochStore,
        new_protocol_version: ProtocolVersion,
    ) {
        self.get_reconfig_api()
            .maybe_reaccumulate_state_hash(cur_epoch_store, new_protocol_version);
    }

    #[instrument(level = "error", skip_all)]
    fn check_system_consistency(
        &self,
        cur_epoch_store: &AuthorityPerEpochStore,
        accumulator: Arc<StateAccumulator>,
        expensive_safety_check_config: &ExpensiveSafetyCheckConfig,
    ) {
        info!(
            "Performing sui conservation consistency check for epoch {}",
            cur_epoch_store.epoch()
        );

        if cfg!(debug_assertions) {
            cur_epoch_store.check_all_executed_transactions_in_checkpoint();
        }

        if let Err(err) = self
            .get_reconfig_api()
            .expensive_check_sui_conservation(cur_epoch_store)
        {
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
        if expensive_safety_check_config.enable_state_consistency_check() {
            info!(
                "Performing state consistency check for epoch {}",
                cur_epoch_store.epoch()
            );
            self.expensive_check_is_consistent_state(
                accumulator,
                cur_epoch_store,
                cfg!(debug_assertions), // panic in debug mode only
            );
        }

        if expensive_safety_check_config.enable_secondary_index_checks() {
            if let Some(indexes) = self.indexes.clone() {
                verify_indexes(self.get_accumulator_store().as_ref(), indexes)
                    .expect("secondary indexes are inconsistent");
            }
        }
    }

    fn expensive_check_is_consistent_state(
        &self,
        accumulator: Arc<StateAccumulator>,
        cur_epoch_store: &AuthorityPerEpochStore,
        panic: bool,
    ) {
        let live_object_set_hash = accumulator.digest_live_object_set(
            !cur_epoch_store
                .protocol_config()
                .simplified_unwrap_then_delete(),
        );

        let root_state_hash: ECMHLiveObjectSetDigest = self
            .get_accumulator_store()
            .get_root_state_accumulator_for_epoch(cur_epoch_store.epoch())
            .expect("Retrieving root state hash cannot fail")
            .expect("Root state hash for epoch must exist")
            .1
            .digest()
            .into();

        let is_inconsistent = root_state_hash != live_object_set_hash;
        if is_inconsistent {
            if panic {
                panic!(
                    "Inconsistent state detected: root state hash: {:?}, live object set hash: {:?}",
                    root_state_hash, live_object_set_hash
                );
            } else {
                error!(
                    "Inconsistent state detected: root state hash: {:?}, live object set hash: {:?}",
                    root_state_hash, live_object_set_hash
                );
            }
        } else {
            info!("State consistency check passed");
        }

        if !panic {
            accumulator.set_inconsistent_state(is_inconsistent);
        }
    }

    pub fn current_epoch_for_testing(&self) -> EpochId {
        self.epoch_store_for_testing().epoch()
    }

    #[instrument(level = "error", skip_all)]
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

        self.get_reconfig_api()
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

    #[instrument(level = "trace", skip_all)]
    pub async fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.get_object_store().get_object(object_id)
    }

    pub async fn get_sui_system_package_object_ref(&self) -> SuiResult<ObjectRef> {
        Ok(self
            .get_object(&SUI_SYSTEM_ADDRESS.into())
            .await
            .expect("framework object should always exist")
            .compute_object_reference())
    }

    // This function is only used for testing.
    pub fn get_sui_system_state_object_for_testing(&self) -> SuiResult<SuiSystemState> {
        self.get_object_cache_reader()
            .get_sui_system_state_object_unsafe()
    }

    #[instrument(level = "trace", skip_all)]
    fn get_transaction_checkpoint_sequence(
        &self,
        digest: &TransactionDigest,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult<Option<CheckpointSequenceNumber>> {
        epoch_store.get_transaction_checkpoint(digest)
    }

    #[instrument(level = "trace", skip_all)]
    pub fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> SuiResult<Option<VerifiedCheckpoint>> {
        Ok(self
            .checkpoint_store
            .get_checkpoint_by_sequence_number(sequence_number)?)
    }

    #[instrument(level = "trace", skip_all)]
    pub fn get_transaction_checkpoint_for_tests(
        &self,
        digest: &TransactionDigest,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult<Option<VerifiedCheckpoint>> {
        let checkpoint = self.get_transaction_checkpoint_sequence(digest, epoch_store)?;
        let Some(checkpoint) = checkpoint else {
            return Ok(None);
        };
        let checkpoint = self
            .checkpoint_store
            .get_checkpoint_by_sequence_number(checkpoint)?;
        Ok(checkpoint)
    }

    #[instrument(level = "trace", skip_all)]
    pub fn get_object_read(&self, object_id: &ObjectID) -> SuiResult<ObjectRead> {
        Ok(
            match self
                .get_object_cache_reader()
                .get_latest_object_or_tombstone(*object_id)
            {
                Some((_, ObjectOrTombstone::Object(object))) => {
                    let layout = self.get_object_layout(&object)?;
                    ObjectRead::Exists(object.compute_object_reference(), object, layout)
                }
                Some((_, ObjectOrTombstone::Tombstone(objref))) => ObjectRead::Deleted(objref),
                None => ObjectRead::NotExists(*object_id),
            },
        )
    }

    /// Chain Identifier is the digest of the genesis checkpoint.
    pub fn get_chain_identifier(&self) -> ChainIdentifier {
        self.chain_identifier
    }

    #[instrument(level = "trace", skip_all)]
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

    /// This function aims to serve rpc reads on past objects and
    /// we don't expect it to be called for other purposes.
    /// Depending on the object pruning policies that will be enforced in the
    /// future there is no software-level guarantee/SLA to retrieve an object
    /// with an old version even if it exists/existed.
    #[instrument(level = "trace", skip_all)]
    pub fn get_past_object_read(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<PastObjectRead> {
        // Firstly we see if the object ever existed by getting its latest data
        let Some(obj_ref) = self
            .get_object_cache_reader()
            .get_latest_object_ref_or_tombstone(*object_id)
        else {
            return Ok(PastObjectRead::ObjectNotExists(*object_id));
        };

        if version > obj_ref.1 {
            return Ok(PastObjectRead::VersionTooHigh {
                object_id: *object_id,
                asked_version: version,
                latest_version: obj_ref.1,
            });
        }

        if version < obj_ref.1 {
            // Read past objects
            return Ok(match self.read_object_at_version(object_id, version)? {
                Some((object, layout)) => {
                    let obj_ref = object.compute_object_reference();
                    PastObjectRead::VersionFound(obj_ref, object, layout)
                }

                None => PastObjectRead::VersionNotFound(*object_id, version),
            });
        }

        if !obj_ref.2.is_alive() {
            return Ok(PastObjectRead::ObjectDeleted(obj_ref));
        }

        match self.read_object_at_version(object_id, obj_ref.1)? {
            Some((object, layout)) => Ok(PastObjectRead::VersionFound(obj_ref, object, layout)),
            None => {
                error!(
                    "Object with in parent_entry is missing from object store, datastore is \
                     inconsistent",
                );
                Err(UserInputError::ObjectNotFound {
                    object_id: *object_id,
                    version: Some(obj_ref.1),
                }
                .into())
            }
        }
    }

    #[instrument(level = "trace", skip_all)]
    fn read_object_at_version(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<(Object, Option<MoveStructLayout>)>> {
        let Some(object) = self
            .get_object_cache_reader()
            .get_object_by_key(object_id, version)
        else {
            return Ok(None);
        };

        let layout = self.get_object_layout(&object)?;
        Ok(Some((object, layout)))
    }

    fn get_object_layout(&self, object: &Object) -> SuiResult<Option<MoveStructLayout>> {
        let layout = object
            .data
            .try_as_move()
            .map(|object| {
                into_struct_layout(
                    self.load_epoch_store_one_call_per_task()
                        .executor()
                        // TODO(cache) - must read through cache
                        .type_layout_resolver(Box::new(self.get_backing_package_store().as_ref()))
                        .get_annotated_layout(&object.type_().clone().into())?,
                )
            })
            .transpose()?;
        Ok(layout)
    }

    fn get_owner_at_version(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Owner> {
        self.get_object_store()
            .get_object_by_key(object_id, version)
            .ok_or_else(|| {
                SuiError::from(UserInputError::ObjectNotFound {
                    object_id: *object_id,
                    version: Some(version),
                })
            })
            .map(|o| o.owner.clone())
    }

    #[instrument(level = "trace", skip_all)]
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

    #[instrument(level = "trace", skip_all)]
    pub fn get_owned_coins_iterator_with_cursor(
        &self,
        owner: SuiAddress,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: (String, u64, ObjectID),
        limit: usize,
        one_coin_type_only: bool,
    ) -> SuiResult<impl Iterator<Item = (CoinIndexKey2, CoinInfo)> + '_> {
        if let Some(indexes) = &self.indexes {
            indexes.get_owned_coins_iterator_with_cursor(owner, cursor, limit, one_coin_type_only)
        } else {
            Err(SuiError::IndexStoreNotAvailable)
        }
    }

    #[instrument(level = "trace", skip_all)]
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

    #[instrument(level = "trace", skip_all)]
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

        let objects = self
            .get_object_store()
            .multi_get_objects_by_key(&object_ids);

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

    #[instrument(level = "trace", skip_all)]
    pub fn get_dynamic_fields(
        &self,
        owner: ObjectID,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> SuiResult<Vec<(ObjectID, DynamicFieldInfo)>> {
        Ok(self
            .get_dynamic_fields_iterator(owner, cursor)?
            .take(limit)
            .collect::<Result<Vec<_>, _>>()?)
    }

    fn get_dynamic_fields_iterator(
        &self,
        owner: ObjectID,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<ObjectID>,
    ) -> SuiResult<impl Iterator<Item = Result<(ObjectID, DynamicFieldInfo), TypedStoreError>> + '_>
    {
        if let Some(indexes) = &self.indexes {
            indexes.get_dynamic_fields_iterator(owner, cursor)
        } else {
            Err(SuiError::IndexStoreNotAvailable)
        }
    }

    #[instrument(level = "trace", skip_all)]
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

    #[instrument(level = "trace", skip_all)]
    pub fn get_total_transaction_blocks(&self) -> SuiResult<u64> {
        Ok(self.get_indexes()?.next_sequence_number())
    }

    #[instrument(level = "trace", skip_all)]
    pub async fn get_executed_transaction_and_effects(
        &self,
        digest: TransactionDigest,
        kv_store: Arc<TransactionKeyValueStore>,
    ) -> SuiResult<(Transaction, TransactionEffects)> {
        let transaction = kv_store.get_tx(digest).await?;
        let effects = kv_store.get_fx_by_tx_digest(digest).await?;
        Ok((transaction, effects))
    }

    #[instrument(level = "trace", skip_all)]
    pub fn multi_get_checkpoint_by_sequence_number(
        &self,
        sequence_numbers: &[CheckpointSequenceNumber],
    ) -> SuiResult<Vec<Option<VerifiedCheckpoint>>> {
        Ok(self
            .checkpoint_store
            .multi_get_checkpoint_by_sequence_number(sequence_numbers)?)
    }

    #[instrument(level = "trace", skip_all)]
    pub fn get_transaction_events(
        &self,
        digest: &TransactionEventsDigest,
    ) -> SuiResult<TransactionEvents> {
        self.get_transaction_cache_reader()
            .get_events(digest)
            .ok_or(SuiError::TransactionEventsNotFound { digest: *digest })
    }

    pub fn get_transaction_input_objects(
        &self,
        effects: &TransactionEffects,
    ) -> anyhow::Result<Vec<Object>> {
        let input_object_keys = effects
            .modified_at_versions()
            .into_iter()
            .map(|(object_id, version)| ObjectKey(object_id, version))
            .collect::<Vec<_>>();

        let input_objects = self
            .get_object_store()
            .multi_get_objects_by_key(&input_object_keys)
            .into_iter()
            .enumerate()
            .map(|(idx, maybe_object)| {
                maybe_object.ok_or_else(|| {
                    anyhow::anyhow!(
                        "missing input object key {:?} from tx {}",
                        input_object_keys[idx],
                        effects.transaction_digest()
                    )
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(input_objects)
    }

    pub fn get_transaction_output_objects(
        &self,
        effects: &TransactionEffects,
    ) -> anyhow::Result<Vec<Object>> {
        let output_object_keys = effects
            .all_changed_objects()
            .into_iter()
            .map(|(object_ref, _owner, _kind)| ObjectKey::from(object_ref))
            .collect::<Vec<_>>();

        let output_objects = self
            .get_object_store()
            .multi_get_objects_by_key(&output_object_keys)
            .into_iter()
            .enumerate()
            .map(|(idx, maybe_object)| {
                maybe_object.ok_or_else(|| {
                    anyhow::anyhow!(
                        "missing output object key {:?} from tx {}",
                        output_object_keys[idx],
                        effects.transaction_digest()
                    )
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(output_objects)
    }

    fn get_indexes(&self) -> SuiResult<Arc<IndexStore>> {
        match &self.indexes {
            Some(i) => Ok(i.clone()),
            None => Err(SuiError::UnsupportedFeatureError {
                error: "extended object indexing is not enabled on this server".into(),
            }),
        }
    }

    pub async fn get_transactions_for_tests(
        self: &Arc<Self>,
        filter: Option<TransactionFilter>,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        reverse: bool,
    ) -> SuiResult<Vec<TransactionDigest>> {
        let metrics = KeyValueStoreMetrics::new_for_tests();
        let kv_store = Arc::new(TransactionKeyValueStore::new(
            "rocksdb",
            metrics,
            self.clone(),
        ));
        self.get_transactions(&kv_store, filter, cursor, limit, reverse)
            .await
    }

    #[instrument(level = "trace", skip_all)]
    pub async fn get_transactions(
        &self,
        kv_store: &Arc<TransactionKeyValueStore>,
        filter: Option<TransactionFilter>,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        reverse: bool,
    ) -> SuiResult<Vec<TransactionDigest>> {
        if let Some(TransactionFilter::Checkpoint(sequence_number)) = filter {
            let checkpoint_contents = kv_store.get_checkpoint_contents(sequence_number).await?;
            let iter = checkpoint_contents.iter().map(|c| c.transaction);
            if reverse {
                let iter = iter
                    .rev()
                    .skip_while(|d| cursor.is_some() && Some(*d) != cursor)
                    .skip(usize::from(cursor.is_some()));
                return Ok(iter.take(limit.unwrap_or(usize::MAX)).collect());
            } else {
                let iter = iter
                    .skip_while(|d| cursor.is_some() && Some(*d) != cursor)
                    .skip(usize::from(cursor.is_some()));
                return Ok(iter.take(limit.unwrap_or(usize::MAX)).collect());
            }
        }
        self.get_indexes()?
            .get_transactions(filter, cursor, limit, reverse)
    }

    pub fn get_checkpoint_store(&self) -> &Arc<CheckpointStore> {
        &self.checkpoint_store
    }

    pub fn get_latest_checkpoint_sequence_number(&self) -> SuiResult<CheckpointSequenceNumber> {
        self.get_checkpoint_store()
            .get_highest_executed_checkpoint_seq_number()?
            .ok_or(SuiError::UserInputError {
                error: UserInputError::LatestCheckpointSequenceNumberNotFound,
            })
    }

    #[cfg(msim)]
    pub fn get_highest_pruned_checkpoint_for_testing(&self) -> SuiResult<CheckpointSequenceNumber> {
        self.database_for_testing()
            .perpetual_tables
            .get_highest_pruned_checkpoint()
    }

    #[instrument(level = "trace", skip_all)]
    pub fn get_checkpoint_summary_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> SuiResult<CheckpointSummary> {
        let verified_checkpoint = self
            .get_checkpoint_store()
            .get_checkpoint_by_sequence_number(sequence_number)?;
        match verified_checkpoint {
            Some(verified_checkpoint) => Ok(verified_checkpoint.into_inner().into_data()),
            None => Err(SuiError::UserInputError {
                error: UserInputError::VerifiedCheckpointNotFound(sequence_number),
            }),
        }
    }

    #[instrument(level = "trace", skip_all)]
    pub fn get_checkpoint_summary_by_digest(
        &self,
        digest: CheckpointDigest,
    ) -> SuiResult<CheckpointSummary> {
        let verified_checkpoint = self
            .get_checkpoint_store()
            .get_checkpoint_by_digest(&digest)?;
        match verified_checkpoint {
            Some(verified_checkpoint) => Ok(verified_checkpoint.into_inner().into_data()),
            None => Err(SuiError::UserInputError {
                error: UserInputError::VerifiedCheckpointDigestNotFound(Base58::encode(digest)),
            }),
        }
    }

    #[instrument(level = "trace", skip_all)]
    pub fn find_publish_txn_digest(&self, package_id: ObjectID) -> SuiResult<TransactionDigest> {
        if is_system_package(package_id) {
            return self.find_genesis_txn_digest();
        }
        Ok(self
            .get_object_read(&package_id)?
            .into_object()?
            .previous_transaction)
    }

    #[instrument(level = "trace", skip_all)]
    pub fn find_genesis_txn_digest(&self) -> SuiResult<TransactionDigest> {
        let summary = self
            .get_verified_checkpoint_by_sequence_number(0)?
            .into_message();
        let content = self.get_checkpoint_contents(summary.content_digest)?;
        let genesis_transaction = content.enumerate_transactions(&summary).next();
        Ok(genesis_transaction
            .ok_or(SuiError::UserInputError {
                error: UserInputError::GenesisTransactionNotFound,
            })?
            .1
            .transaction)
    }

    #[instrument(level = "trace", skip_all)]
    pub fn get_verified_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> SuiResult<VerifiedCheckpoint> {
        let verified_checkpoint = self
            .get_checkpoint_store()
            .get_checkpoint_by_sequence_number(sequence_number)?;
        match verified_checkpoint {
            Some(verified_checkpoint) => Ok(verified_checkpoint),
            None => Err(SuiError::UserInputError {
                error: UserInputError::VerifiedCheckpointNotFound(sequence_number),
            }),
        }
    }

    #[instrument(level = "trace", skip_all)]
    pub fn get_verified_checkpoint_summary_by_digest(
        &self,
        digest: CheckpointDigest,
    ) -> SuiResult<VerifiedCheckpoint> {
        let verified_checkpoint = self
            .get_checkpoint_store()
            .get_checkpoint_by_digest(&digest)?;
        match verified_checkpoint {
            Some(verified_checkpoint) => Ok(verified_checkpoint),
            None => Err(SuiError::UserInputError {
                error: UserInputError::VerifiedCheckpointDigestNotFound(Base58::encode(digest)),
            }),
        }
    }

    #[instrument(level = "trace", skip_all)]
    pub fn get_checkpoint_contents(
        &self,
        digest: CheckpointContentsDigest,
    ) -> SuiResult<CheckpointContents> {
        self.get_checkpoint_store()
            .get_checkpoint_contents(&digest)?
            .ok_or(SuiError::UserInputError {
                error: UserInputError::CheckpointContentsNotFound(digest),
            })
    }

    #[instrument(level = "trace", skip_all)]
    pub fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> SuiResult<CheckpointContents> {
        let verified_checkpoint = self
            .get_checkpoint_store()
            .get_checkpoint_by_sequence_number(sequence_number)?;
        match verified_checkpoint {
            Some(verified_checkpoint) => {
                let content_digest = verified_checkpoint.into_inner().content_digest;
                self.get_checkpoint_contents(content_digest)
            }
            None => Err(SuiError::UserInputError {
                error: UserInputError::VerifiedCheckpointNotFound(sequence_number),
            }),
        }
    }

    #[instrument(level = "trace", skip_all)]
    pub async fn query_events(
        &self,
        kv_store: &Arc<TransactionKeyValueStore>,
        query: EventFilter,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<EventID>,
        limit: usize,
        descending: bool,
    ) -> SuiResult<Vec<SuiEvent>> {
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
            EventFilter::All([]) => index_store.all_events(tx_num, event_num, limit, descending)?,
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
            EventFilter::MoveEventModule { package, module } => index_store
                .events_by_move_event_module(
                    &ModuleId::new(package.into(), module),
                    tx_num,
                    event_num,
                    limit,
                    descending,
                )?,
            // not using "_ =>" because we want to make sure we remember to add new variants here
            EventFilter::Any(_) => {
                return Err(SuiError::UserInputError {
                    error: UserInputError::Unsupported(
                        "'Any' queries are not supported by the fullnode.".to_string(),
                    ),
                })
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

        // get the unique set of digests from the event_keys
        let transaction_digests = event_keys
            .iter()
            .map(|(_, digest, _, _)| *digest)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        let events = kv_store
            .multi_get_events_by_tx_digests(&transaction_digests)
            .await?;

        let events_map: HashMap<_, _> =
            transaction_digests.iter().zip(events.into_iter()).collect();

        let stored_events = event_keys
            .into_iter()
            .map(|k| {
                (
                    k,
                    events_map
                        .get(&k.1)
                        .expect("fetched digest is missing")
                        .clone()
                        .and_then(|e| e.data.get(k.2).cloned()),
                )
            })
            .map(|((digest, tx_digest, event_seq, timestamp), event)| {
                event
                    .map(|e| (e, tx_digest, event_seq, timestamp))
                    .ok_or(SuiError::TransactionEventsNotFound { digest })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let epoch_store = self.load_epoch_store_one_call_per_task();
        let backing_store = self.get_backing_package_store().as_ref();
        let mut layout_resolver = epoch_store
            .executor()
            .type_layout_resolver(Box::new(backing_store));
        let mut events = vec![];
        for (e, tx_digest, event_seq, timestamp) in stored_events.into_iter() {
            events.push(SuiEvent::try_from(
                e.clone(),
                tx_digest,
                event_seq as u64,
                Some(timestamp),
                layout_resolver.get_annotated_layout(&e.type_)?,
            )?)
        }
        Ok(events)
    }

    pub async fn insert_genesis_object(&self, object: Object) {
        self.get_reconfig_api().insert_genesis_object(object);
    }

    pub async fn insert_genesis_objects(&self, objects: &[Object]) {
        futures::future::join_all(
            objects
                .iter()
                .map(|o| self.insert_genesis_object(o.clone())),
        )
        .await;
    }

    /// Gets the execution outputs of a transaction if they exist
    #[instrument(level = "trace", skip_all)]
    pub fn get_transaction_output(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> SuiResult<Option<(TransactionEffects, TransactionEvents)>> {
        let effects = self
            .get_transaction_cache_reader()
            .get_executed_effects(transaction_digest);
        if let Some(effects) = effects {
            let events = if let Some(digest) = effects.events_digest() {
                self.get_transaction_events(digest)?
            } else {
                TransactionEvents::default()
            };
            Ok(Some((effects, events)))
        } else {
            Ok(None)
        }
    }

    /// Make a status response for a transaction
    #[instrument(level = "trace", skip_all)]
    pub fn get_transaction_status(
        &self,
        transaction_digest: &TransactionDigest,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<Option<(SenderSignedData, TransactionStatus)>> {
        // TODO: In the case of read path, we should not have to re-sign the effects.
        if let Some(effects) =
            self.get_signed_effects_and_maybe_resign(transaction_digest, epoch_store)?
        {
            if let Some(transaction) = self
                .get_transaction_cache_reader()
                .get_transaction_block(transaction_digest)
            {
                let cert_sig = epoch_store.get_transaction_cert_sig(transaction_digest)?;
                let events = if let Some(digest) = effects.events_digest() {
                    self.get_transaction_events(digest)?
                } else {
                    TransactionEvents::default()
                };
                return Ok(Some((
                    (*transaction).clone().into_message(),
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
    #[instrument(level = "trace", skip_all)]
    pub fn get_signed_effects_and_maybe_resign(
        &self,
        transaction_digest: &TransactionDigest,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<Option<VerifiedSignedTransactionEffects>> {
        let effects = self
            .get_transaction_cache_reader()
            .get_executed_effects(transaction_digest);
        match effects {
            Some(effects) => Ok(Some(self.sign_effects(effects, epoch_store)?)),
            None => Ok(None),
        }
    }

    #[instrument(level = "trace", skip_all)]
    pub(crate) fn sign_effects(
        &self,
        effects: TransactionEffects,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<VerifiedSignedTransactionEffects> {
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
                debug!(
                    ?tx_digest,
                    epoch=?epoch_store.epoch(),
                    "Re-signing the effects with the current epoch"
                );

                let sig = AuthoritySignInfo::new(
                    epoch_store.epoch(),
                    &effects,
                    Intent::sui_app(IntentScope::TransactionEffects),
                    self.name,
                    &*self.secret,
                );

                let effects = SignedTransactionEffects::new_from_data_and_sig(effects, sig.clone());

                epoch_store.insert_effects_digest_and_signature(
                    &tx_digest,
                    effects.digest(),
                    &sig,
                )?;

                effects
            }
        };

        Ok(VerifiedSignedTransactionEffects::new_unchecked(
            signed_effects,
        ))
    }

    // Returns coin objects for indexing for fullnode if indexing is enabled.
    #[instrument(level = "trace", skip_all)]
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
                if v.is_coin() {
                    Some((*k, v.clone()))
                } else {
                    None
                }
            })
            .collect();
        let input_coin_objects = inner_temporary_store
            .input_objects
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

    /// Get the TransactionEnvelope that currently locks the given object, if any.
    /// Since object locks are only valid for one epoch, we also need the epoch_id in the query.
    /// Returns UserInputError::ObjectNotFound if no lock records for the given object can be found.
    /// Returns UserInputError::ObjectVersionUnavailableForConsumption if the object record is at a different version.
    /// Returns Some(VerifiedEnvelope) if the given ObjectRef is locked by a certain transaction.
    /// Returns None if a lock record is initialized for the given ObjectRef but not yet locked by any transaction,
    ///     or cannot find the transaction in transaction table, because of data race etc.
    #[instrument(level = "trace", skip_all)]
    pub async fn get_transaction_lock(
        &self,
        object_ref: &ObjectRef,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult<Option<VerifiedSignedTransaction>> {
        let lock_info = self
            .get_object_cache_reader()
            .get_lock(*object_ref, epoch_store)
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

        epoch_store.get_signed_transaction(&lock_info.tx_digest)
    }

    pub async fn get_objects(&self, objects: &[ObjectID]) -> Vec<Option<Object>> {
        self.get_object_cache_reader().get_objects(objects)
    }

    pub async fn get_object_or_tombstone(&self, object_id: ObjectID) -> Option<ObjectRef> {
        self.get_object_cache_reader()
            .get_latest_object_ref_or_tombstone(object_id)
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
        binary_config: &BinaryConfig,
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
            let modules = framework_injection::get_override_modules(&system_package.id, self.name)
                .unwrap_or(modules);

            let Some(obj_ref) = sui_framework::compare_system_package(
                &self.get_object_store(),
                &system_package.id,
                &modules,
                system_package.dependencies.to_vec(),
                binary_config,
            )
            .await
            else {
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
        binary_config: &BinaryConfig,
    ) -> Option<Vec<(SequenceNumber, Vec<Vec<u8>>, Vec<ObjectID>)>> {
        let ids: Vec<_> = system_packages.iter().map(|(id, _, _)| *id).collect();
        let objects = self.get_objects(&ids).await;

        let mut res = Vec::with_capacity(system_packages.len());
        for (system_package_ref, object) in system_packages.into_iter().zip(objects.iter()) {
            let prev_transaction = match object {
                Some(cur_object) if cur_object.compute_object_reference() == system_package_ref => {
                    // Skip this one because it doesn't need to be upgraded.
                    info!("Framework {} does not need updating", system_package_ref.0);
                    continue;
                }

                Some(cur_object) => cur_object.previous_transaction,
                None => TransactionDigest::genesis_marker(),
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
                .map(|m| CompiledModule::deserialize_with_config(m, binary_config).unwrap())
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

    // TODO: delete once authority_capabilities_v2 is deployed everywhere
    fn is_protocol_version_supported_v1(
        current_protocol_version: ProtocolVersion,
        proposed_protocol_version: ProtocolVersion,
        protocol_config: &ProtocolConfig,
        committee: &Committee,
        capabilities: Vec<AuthorityCapabilitiesV1>,
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
            .chunk_by(|(packages, _authority)| packages.clone())
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

    fn is_protocol_version_supported_v2(
        current_protocol_version: ProtocolVersion,
        proposed_protocol_version: ProtocolVersion,
        protocol_config: &ProtocolConfig,
        committee: &Committee,
        capabilities: Vec<AuthorityCapabilitiesV2>,
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
                    .get_version_digest(proposed_protocol_version)
                    .map(|digest| (digest, cap.available_system_packages, cap.authority))
            })
            .collect();

        // There can only be one set of votes that have a majority, find one if it exists.
        desired_upgrades.sort();
        desired_upgrades
            .into_iter()
            .chunk_by(|(digest, packages, _authority)| (*digest, packages.clone()))
            .into_iter()
            .find_map(|((digest, packages), group)| {
                // should have been filtered out earlier.
                assert!(!packages.is_empty());

                let mut stake_aggregator: StakeAggregator<(), true> =
                    StakeAggregator::new(Arc::new(committee.clone()));

                for (_, _, authority) in group {
                    stake_aggregator.insert_generic(authority, ());
                }

                let total_votes = stake_aggregator.total_votes();
                let quorum_threshold = committee.quorum_threshold();
                let f = committee.total_votes() - committee.quorum_threshold();

                // multiple by buffer_stake_bps / 10000, rounded up.
                let buffer_stake = (f * buffer_stake_bps + 9999) / 10000;
                let effective_threshold = quorum_threshold + buffer_stake;

                info!(
                    protocol_config_digest = ?digest,
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

    // TODO: delete once authority_capabilities_v2 is deployed everywhere
    fn choose_protocol_version_and_system_packages_v1(
        current_protocol_version: ProtocolVersion,
        protocol_config: &ProtocolConfig,
        committee: &Committee,
        capabilities: Vec<AuthorityCapabilitiesV1>,
        buffer_stake_bps: u64,
    ) -> (ProtocolVersion, Vec<ObjectRef>) {
        let mut next_protocol_version = current_protocol_version;
        let mut system_packages = vec![];

        while let Some((version, packages)) = Self::is_protocol_version_supported_v1(
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

    fn choose_protocol_version_and_system_packages_v2(
        current_protocol_version: ProtocolVersion,
        protocol_config: &ProtocolConfig,
        committee: &Committee,
        capabilities: Vec<AuthorityCapabilitiesV2>,
        buffer_stake_bps: u64,
    ) -> (ProtocolVersion, Vec<ObjectRef>) {
        let mut next_protocol_version = current_protocol_version;
        let mut system_packages = vec![];

        while let Some((version, packages)) = Self::is_protocol_version_supported_v2(
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

    #[instrument(level = "debug", skip_all)]
    fn create_authenticator_state_tx(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> Option<EndOfEpochTransactionKind> {
        if !epoch_store.protocol_config().enable_jwk_consensus_updates() {
            info!("authenticator state transactions not enabled");
            return None;
        }

        let authenticator_state_exists = epoch_store.authenticator_state_exists();
        let tx = if authenticator_state_exists {
            let next_epoch = epoch_store.epoch().checked_add(1).expect("epoch overflow");
            let min_epoch =
                next_epoch.saturating_sub(epoch_store.protocol_config().max_age_of_jwk_in_epochs());
            let authenticator_obj_initial_shared_version = epoch_store
                .epoch_start_config()
                .authenticator_obj_initial_shared_version()
                .expect("initial version must exist");

            let tx = EndOfEpochTransactionKind::new_authenticator_state_expire(
                min_epoch,
                authenticator_obj_initial_shared_version,
            );

            info!(?min_epoch, "Creating AuthenticatorStateExpire tx",);

            tx
        } else {
            let tx = EndOfEpochTransactionKind::new_authenticator_state_create();
            info!("Creating AuthenticatorStateCreate tx");
            tx
        };
        Some(tx)
    }

    #[instrument(level = "debug", skip_all)]
    fn create_randomness_state_tx(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> Option<EndOfEpochTransactionKind> {
        if !epoch_store.protocol_config().random_beacon() {
            info!("randomness state transactions not enabled");
            return None;
        }

        if epoch_store.randomness_state_exists() {
            return None;
        }

        let tx = EndOfEpochTransactionKind::new_randomness_state_create();
        info!("Creating RandomnessStateCreate tx");
        Some(tx)
    }

    #[instrument(level = "debug", skip_all)]
    fn create_bridge_tx(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> Option<EndOfEpochTransactionKind> {
        if !epoch_store.protocol_config().enable_bridge() {
            info!("bridge not enabled");
            return None;
        }
        if epoch_store.bridge_exists() {
            return None;
        }
        let tx = EndOfEpochTransactionKind::new_bridge_create(epoch_store.get_chain_identifier());
        info!("Creating Bridge Create tx");
        Some(tx)
    }

    #[instrument(level = "debug", skip_all)]
    fn init_bridge_committee_tx(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> Option<EndOfEpochTransactionKind> {
        if !epoch_store.protocol_config().enable_bridge() {
            info!("bridge not enabled");
            return None;
        }
        if !epoch_store
            .protocol_config()
            .should_try_to_finalize_bridge_committee()
        {
            info!("should not try to finalize bridge committee yet");
            return None;
        }
        // Only create this transaction if bridge exists
        if !epoch_store.bridge_exists() {
            return None;
        }

        if epoch_store.bridge_committee_initiated() {
            return None;
        }

        let bridge_initial_shared_version = epoch_store
            .epoch_start_config()
            .bridge_obj_initial_shared_version()
            .expect("initial version must exist");
        let tx = EndOfEpochTransactionKind::init_bridge_committee(bridge_initial_shared_version);
        info!("Init Bridge committee tx");
        Some(tx)
    }

    #[instrument(level = "debug", skip_all)]
    fn create_deny_list_state_tx(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> Option<EndOfEpochTransactionKind> {
        if !epoch_store.protocol_config().enable_coin_deny_list_v1() {
            return None;
        }

        if epoch_store.coin_deny_list_state_exists() {
            return None;
        }

        let tx = EndOfEpochTransactionKind::new_deny_list_state_create();
        info!("Creating DenyListStateCreate tx");
        Some(tx)
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
    #[instrument(level = "error", skip_all)]
    pub async fn create_and_execute_advance_epoch_tx(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        gas_cost_summary: &GasCostSummary,
        checkpoint: CheckpointSequenceNumber,
        epoch_start_timestamp_ms: CheckpointTimestamp,
    ) -> anyhow::Result<(SuiSystemState, TransactionEffects)> {
        let mut txns = Vec::new();

        if let Some(tx) = self.create_authenticator_state_tx(epoch_store) {
            txns.push(tx);
        }
        if let Some(tx) = self.create_randomness_state_tx(epoch_store) {
            txns.push(tx);
        }
        if let Some(tx) = self.create_bridge_tx(epoch_store) {
            txns.push(tx);
        }
        if let Some(tx) = self.init_bridge_committee_tx(epoch_store) {
            txns.push(tx);
        }
        if let Some(tx) = self.create_deny_list_state_tx(epoch_store) {
            txns.push(tx);
        }

        let next_epoch = epoch_store.epoch() + 1;

        let buffer_stake_bps = epoch_store.get_effective_buffer_stake_bps();

        let (next_epoch_protocol_version, next_epoch_system_packages) =
            if epoch_store.protocol_config().authority_capabilities_v2() {
                Self::choose_protocol_version_and_system_packages_v2(
                    epoch_store.protocol_version(),
                    epoch_store.protocol_config(),
                    epoch_store.committee(),
                    epoch_store
                        .get_capabilities_v2()
                        .expect("read capabilities from db cannot fail"),
                    buffer_stake_bps,
                )
            } else {
                Self::choose_protocol_version_and_system_packages_v1(
                    epoch_store.protocol_version(),
                    epoch_store.protocol_config(),
                    epoch_store.committee(),
                    epoch_store
                        .get_capabilities_v1()
                        .expect("read capabilities from db cannot fail"),
                    buffer_stake_bps,
                )
            };

        // since system packages are created during the current epoch, they should abide by the
        // rules of the current epoch, including the current epoch's max Move binary format version
        let config = epoch_store.protocol_config();
        let binary_config = to_binary_config(config);
        let Some(next_epoch_system_package_bytes) = self
            .get_system_package_bytes(next_epoch_system_packages.clone(), &binary_config)
            .await
        else {
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
            return Err(anyhow!(
                "missing system packages: cannot form ChangeEpochTx"
            ));
        };

        let tx = if epoch_store
            .protocol_config()
            .end_of_epoch_transaction_supported()
        {
            txns.push(EndOfEpochTransactionKind::new_change_epoch(
                next_epoch,
                next_epoch_protocol_version,
                gas_cost_summary.storage_cost,
                gas_cost_summary.computation_cost,
                gas_cost_summary.storage_rebate,
                gas_cost_summary.non_refundable_storage_fee,
                epoch_start_timestamp_ms,
                next_epoch_system_package_bytes,
            ));

            VerifiedTransaction::new_end_of_epoch_transaction(txns)
        } else {
            VerifiedTransaction::new_change_epoch(
                next_epoch,
                next_epoch_protocol_version,
                gas_cost_summary.storage_cost,
                gas_cost_summary.computation_cost,
                gas_cost_summary.storage_rebate,
                gas_cost_summary.non_refundable_storage_fee,
                epoch_start_timestamp_ms,
                next_epoch_system_package_bytes,
            )
        };

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
        let tx_lock = epoch_store.acquire_tx_lock(tx_digest);

        // The tx could have been executed by state sync already - if so simply return an error.
        // The checkpoint builder will shortly be terminated by reconfiguration anyway.
        if self
            .get_transaction_cache_reader()
            .is_tx_already_executed(tx_digest)
        {
            warn!("change epoch tx has already been executed via state sync");
            return Err(anyhow::anyhow!(
                "change epoch tx has already been executed via state sync"
            ));
        }

        let execution_guard = self.execution_lock_for_executable_transaction(&executable_tx)?;

        // We must manually assign the shared object versions to the transaction before executing it.
        // This is because we do not sequence end-of-epoch transactions through consensus.
        epoch_store.assign_shared_object_versions_idempotent(
            self.get_object_cache_reader().as_ref(),
            &[executable_tx.clone()],
        )?;

        let input_objects =
            self.read_objects_for_execution(&tx_lock, &executable_tx, epoch_store)?;

        let (temporary_store, effects, _timings, _execution_error_opt) =
            self.prepare_certificate(&execution_guard, &executable_tx, input_objects, epoch_store)?;
        let system_obj = get_sui_system_state(&temporary_store.written)
            .expect("change epoch tx must write to system object");

        // We must write tx and effects to the state sync tables so that state sync is able to
        // deliver to the transaction to CheckpointExecutor after it is included in a certified
        // checkpoint.
        self.get_state_sync_store()
            .insert_transaction_and_effects(&tx, &effects);

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
    #[instrument(level = "error", skip_all)]
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
            if epoch_store.is_transaction_executed_in_checkpoint(&digest)? {
                info!("Not reverting pending consensus transaction {:?} - it was included in checkpoint", digest);
                continue;
            }
            info!("Reverting {:?} at the end of epoch", digest);
            epoch_store.revert_executed_transaction(&digest)?;
            self.get_reconfig_api().revert_state_update(&digest);
        }
        info!("All uncommitted local transactions reverted");
        Ok(())
    }

    #[instrument(level = "error", skip_all)]
    async fn reopen_epoch_db(
        &self,
        cur_epoch_store: &AuthorityPerEpochStore,
        new_committee: Committee,
        epoch_start_configuration: EpochStartConfiguration,
        expensive_safety_check_config: &ExpensiveSafetyCheckConfig,
        epoch_last_checkpoint: CheckpointSequenceNumber,
    ) -> SuiResult<Arc<AuthorityPerEpochStore>> {
        let new_epoch = new_committee.epoch;
        info!(new_epoch = ?new_epoch, "re-opening AuthorityEpochTables for new epoch");
        assert_eq!(
            epoch_start_configuration.epoch_start_state().epoch(),
            new_committee.epoch
        );
        fail_point!("before-open-new-epoch-store");
        let new_epoch_store = cur_epoch_store.new_at_next_epoch(
            self.name,
            new_committee,
            epoch_start_configuration,
            self.get_backing_package_store().clone(),
            self.get_object_store().clone(),
            expensive_safety_check_config,
            cur_epoch_store.get_chain_identifier(),
            epoch_last_checkpoint,
        );
        self.epoch_store.store(new_epoch_store.clone());
        Ok(new_epoch_store)
    }

    #[cfg(test)]
    pub(crate) fn iter_live_object_set_for_testing(
        &self,
    ) -> impl Iterator<Item = authority_store_tables::LiveObject> + '_ {
        let include_wrapped_object = !self
            .epoch_store_for_testing()
            .protocol_config()
            .simplified_unwrap_then_delete();
        self.get_accumulator_store()
            .iter_cached_live_object_set_for_testing(include_wrapped_object)
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

    /// NOTE: this function is only to be used for fuzzing and testing. Never use in prod
    pub async fn insert_objects_unsafe_for_testing_only(&self, objects: &[Object]) -> SuiResult {
        self.get_reconfig_api().bulk_insert_genesis_objects(objects);
        self.get_object_cache_reader()
            .force_reload_system_packages(&BuiltInFramework::all_package_ids());
        self.get_reconfig_api()
            .clear_state_end_of_epoch(&self.execution_lock_for_reconfiguration().await);
        Ok(())
    }
}

pub struct RandomnessRoundReceiver {
    authority_state: Arc<AuthorityState>,
    randomness_rx: mpsc::Receiver<(EpochId, RandomnessRound, Vec<u8>)>,
}

impl RandomnessRoundReceiver {
    pub fn spawn(
        authority_state: Arc<AuthorityState>,
        randomness_rx: mpsc::Receiver<(EpochId, RandomnessRound, Vec<u8>)>,
    ) -> JoinHandle<()> {
        let rrr = RandomnessRoundReceiver {
            authority_state,
            randomness_rx,
        };
        spawn_monitored_task!(rrr.run())
    }

    async fn run(mut self) {
        info!("RandomnessRoundReceiver event loop started");

        loop {
            tokio::select! {
                maybe_recv = self.randomness_rx.recv() => {
                    if let Some((epoch, round, bytes)) = maybe_recv {
                        self.handle_new_randomness(epoch, round, bytes);
                    } else {
                        break;
                    }
                },
            }
        }

        info!("RandomnessRoundReceiver event loop ended");
    }

    #[instrument(level = "debug", skip_all, fields(?epoch, ?round))]
    fn handle_new_randomness(&self, epoch: EpochId, round: RandomnessRound, bytes: Vec<u8>) {
        let epoch_store = self.authority_state.load_epoch_store_one_call_per_task();
        if epoch_store.epoch() != epoch {
            warn!(
                "dropping randomness for epoch {epoch}, round {round}, because we are in epoch {}",
                epoch_store.epoch()
            );
            return;
        }
        let transaction = VerifiedTransaction::new_randomness_state_update(
            epoch,
            round,
            bytes,
            epoch_store
                .epoch_start_config()
                .randomness_obj_initial_shared_version()
                .expect("randomness state obj must exist"),
        );
        debug!(
            "created randomness state update transaction with digest: {:?}",
            transaction.digest()
        );
        let transaction = VerifiedExecutableTransaction::new_system(transaction, epoch);
        let digest = *transaction.digest();

        // Randomness state updates contain the full bls signature for the random round,
        // which cannot necessarily be reconstructed again later. Therefore we must immediately
        // persist this transaction. If we crash before its outputs are committed, this
        // ensures we will be able to re-execute it.
        self.authority_state
            .get_cache_commit()
            .persist_transaction(&transaction);

        // Send transaction to TransactionManager for execution.
        self.authority_state
            .transaction_manager()
            .enqueue(vec![transaction], &epoch_store);

        let authority_state = self.authority_state.clone();
        spawn_monitored_task!(async move {
            // Wait for transaction execution in a separate task, to avoid deadlock in case of
            // out-of-order randomness generation. (Each RandomnessStateUpdate depends on the
            // output of the RandomnessStateUpdate from the previous round.)
            //
            // We set a very long timeout so that in case this gets stuck for some reason, the
            // validator will eventually crash rather than continuing in a zombie mode.
            const RANDOMNESS_STATE_UPDATE_EXECUTION_TIMEOUT: Duration = Duration::from_secs(300);
            let result = tokio::time::timeout(
                RANDOMNESS_STATE_UPDATE_EXECUTION_TIMEOUT,
                authority_state
                    .get_transaction_cache_reader()
                    .notify_read_executed_effects(&[digest]),
            )
            .await;
            let mut effects = match result {
                Ok(result) => result,
                Err(_) => {
                    if cfg!(debug_assertions) {
                        // Crash on randomness update execution timeout in debug builds.
                        panic!("randomness state update transaction execution timed out at epoch {epoch}, round {round}");
                    }
                    warn!("randomness state update transaction execution timed out at epoch {epoch}, round {round}");
                    // Continue waiting as long as necessary in non-debug builds.
                    authority_state
                        .get_transaction_cache_reader()
                        .notify_read_executed_effects(&[digest])
                        .await
                }
            };

            let effects = effects.pop().expect("should return effects");
            if *effects.status() != ExecutionStatus::Success {
                fatal!("failed to execute randomness state update transaction at epoch {epoch}, round {round}: {effects:?}");
            }
            debug!("successfully executed randomness state update transaction at epoch {epoch}, round {round}");
        });
    }
}

#[async_trait]
impl TransactionKeyValueStoreTrait for AuthorityState {
    #[instrument(skip(self))]
    async fn multi_get(
        &self,
        transactions: &[TransactionDigest],
        effects: &[TransactionDigest],
    ) -> SuiResult<(Vec<Option<Transaction>>, Vec<Option<TransactionEffects>>)> {
        let txns = if !transactions.is_empty() {
            self.get_transaction_cache_reader()
                .multi_get_transaction_blocks(transactions)
                .into_iter()
                .map(|t| t.map(|t| (*t).clone().into_inner()))
                .collect()
        } else {
            vec![]
        };

        let fx = if !effects.is_empty() {
            self.get_transaction_cache_reader()
                .multi_get_executed_effects(effects)
        } else {
            vec![]
        };

        Ok((txns, fx))
    }

    #[instrument(skip(self))]
    async fn multi_get_checkpoints(
        &self,
        checkpoint_summaries: &[CheckpointSequenceNumber],
        checkpoint_contents: &[CheckpointSequenceNumber],
        checkpoint_summaries_by_digest: &[CheckpointDigest],
    ) -> SuiResult<(
        Vec<Option<CertifiedCheckpointSummary>>,
        Vec<Option<CheckpointContents>>,
        Vec<Option<CertifiedCheckpointSummary>>,
    )> {
        // TODO: use multi-get methods if it ever becomes important (unlikely)
        let mut summaries = Vec::with_capacity(checkpoint_summaries.len());
        let store = self.get_checkpoint_store();
        for seq in checkpoint_summaries {
            let checkpoint = store
                .get_checkpoint_by_sequence_number(*seq)?
                .map(|c| c.into_inner());

            summaries.push(checkpoint);
        }

        let mut contents = Vec::with_capacity(checkpoint_contents.len());
        for seq in checkpoint_contents {
            let checkpoint = store
                .get_checkpoint_by_sequence_number(*seq)?
                .and_then(|summary| {
                    store
                        .get_checkpoint_contents(&summary.content_digest)
                        .expect("db read cannot fail")
                });
            contents.push(checkpoint);
        }

        let mut summaries_by_digest = Vec::with_capacity(checkpoint_summaries_by_digest.len());
        for digest in checkpoint_summaries_by_digest {
            let checkpoint = store
                .get_checkpoint_by_digest(digest)?
                .map(|c| c.into_inner());
            summaries_by_digest.push(checkpoint);
        }
        Ok((summaries, contents, summaries_by_digest))
    }

    #[instrument(skip(self))]
    async fn deprecated_get_transaction_checkpoint(
        &self,
        digest: TransactionDigest,
    ) -> SuiResult<Option<CheckpointSequenceNumber>> {
        Ok(self
            .get_checkpoint_cache()
            .deprecated_get_transaction_checkpoint(&digest)
            .map(|(_epoch, checkpoint)| checkpoint))
    }

    #[instrument(skip(self))]
    async fn get_object(
        &self,
        object_id: ObjectID,
        version: VersionNumber,
    ) -> SuiResult<Option<Object>> {
        Ok(self
            .get_object_cache_reader()
            .get_object_by_key(&object_id, version))
    }

    #[instrument(skip(self))]
    async fn multi_get_transaction_checkpoint(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<CheckpointSequenceNumber>>> {
        let res = self
            .get_checkpoint_cache()
            .deprecated_multi_get_transaction_checkpoint(digests);

        Ok(res
            .into_iter()
            .map(|maybe| maybe.map(|(_epoch, checkpoint)| checkpoint))
            .collect())
    }

    #[instrument(skip(self))]
    async fn multi_get_events_by_tx_digests(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEvents>>> {
        if digests.is_empty() {
            return Ok(vec![]);
        }
        let events_digests: Vec<_> = self
            .get_transaction_cache_reader()
            .multi_get_executed_effects(digests)
            .into_iter()
            .map(|t| t.and_then(|t| t.events_digest().cloned()))
            .collect();
        let non_empty_events: Vec<_> = events_digests.iter().filter_map(|e| *e).collect();
        let mut events = self
            .get_transaction_cache_reader()
            .multi_get_events(&non_empty_events)
            .into_iter();
        Ok(events_digests
            .into_iter()
            .map(|ev| ev.and_then(|_| events.next()?))
            .collect())
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
                m.serialize_with_version(m.version, &mut buf).unwrap();
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
                .dependencies
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ObjDumpFormat {
    pub id: ObjectID,
    pub version: VersionNumber,
    pub digest: ObjectDigest,
    pub object: Object,
}

impl ObjDumpFormat {
    fn new(object: Object) -> Self {
        let oref = object.compute_object_reference();
        Self {
            id: oref.0,
            version: oref.1,
            digest: oref.2,
            object,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeStateDump {
    pub tx_digest: TransactionDigest,
    pub sender_signed_data: SenderSignedData,
    pub executed_epoch: u64,
    pub reference_gas_price: u64,
    pub protocol_version: u64,
    pub epoch_start_timestamp_ms: u64,
    pub computed_effects: TransactionEffects,
    pub expected_effects_digest: TransactionEffectsDigest,
    pub relevant_system_packages: Vec<ObjDumpFormat>,
    pub shared_objects: Vec<ObjDumpFormat>,
    pub loaded_child_objects: Vec<ObjDumpFormat>,
    pub modified_at_versions: Vec<ObjDumpFormat>,
    pub runtime_reads: Vec<ObjDumpFormat>,
    pub input_objects: Vec<ObjDumpFormat>,
}

impl NodeStateDump {
    pub fn new(
        tx_digest: &TransactionDigest,
        effects: &TransactionEffects,
        expected_effects_digest: TransactionEffectsDigest,
        object_store: &dyn ObjectStore,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        inner_temporary_store: &InnerTemporaryStore,
        certificate: &VerifiedExecutableTransaction,
    ) -> SuiResult<Self> {
        // Epoch info
        let executed_epoch = epoch_store.epoch();
        let reference_gas_price = epoch_store.reference_gas_price();
        let epoch_start_config = epoch_store.epoch_start_config();
        let protocol_version = epoch_store.protocol_version().as_u64();
        let epoch_start_timestamp_ms = epoch_start_config.epoch_data().epoch_start_timestamp();

        // Record all system packages at this version
        let mut relevant_system_packages = Vec::new();
        for sys_package_id in BuiltInFramework::all_package_ids() {
            if let Some(w) = object_store.get_object(&sys_package_id) {
                relevant_system_packages.push(ObjDumpFormat::new(w))
            }
        }

        // Record all the shared objects
        let mut shared_objects = Vec::new();
        for kind in effects.input_shared_objects() {
            match kind {
                InputSharedObject::Mutate(obj_ref) | InputSharedObject::ReadOnly(obj_ref) => {
                    if let Some(w) = object_store.get_object_by_key(&obj_ref.0, obj_ref.1) {
                        shared_objects.push(ObjDumpFormat::new(w))
                    }
                }
                InputSharedObject::ReadDeleted(..)
                | InputSharedObject::MutateDeleted(..)
                | InputSharedObject::Cancelled(..) => (), // TODO: consider record congested objects.
            }
        }

        // Record all loaded child objects
        // Child objects which are read but not mutated are not tracked anywhere else
        let mut loaded_child_objects = Vec::new();
        for (id, meta) in &inner_temporary_store.loaded_runtime_objects {
            if let Some(w) = object_store.get_object_by_key(id, meta.version) {
                loaded_child_objects.push(ObjDumpFormat::new(w))
            }
        }

        // Record all modified objects
        let mut modified_at_versions = Vec::new();
        for (id, ver) in effects.modified_at_versions() {
            if let Some(w) = object_store.get_object_by_key(&id, ver) {
                modified_at_versions.push(ObjDumpFormat::new(w))
            }
        }

        // Packages read at runtime, which were not previously loaded into the temoorary store
        // Some packages may be fetched at runtime and wont show up in input objects
        let mut runtime_reads = Vec::new();
        for obj in inner_temporary_store
            .runtime_packages_loaded_from_db
            .values()
        {
            runtime_reads.push(ObjDumpFormat::new(obj.object().clone()));
        }

        // All other input objects should already be in `inner_temporary_store.objects`

        Ok(Self {
            tx_digest: *tx_digest,
            executed_epoch,
            reference_gas_price,
            epoch_start_timestamp_ms,
            protocol_version,
            relevant_system_packages,
            shared_objects,
            loaded_child_objects,
            modified_at_versions,
            runtime_reads,
            sender_signed_data: certificate.clone().into_message(),
            input_objects: inner_temporary_store
                .input_objects
                .values()
                .map(|o| ObjDumpFormat::new(o.clone()))
                .collect(),
            computed_effects: effects.clone(),
            expected_effects_digest,
        })
    }

    pub fn all_objects(&self) -> Vec<ObjDumpFormat> {
        let mut objects = Vec::new();
        objects.extend(self.relevant_system_packages.clone());
        objects.extend(self.shared_objects.clone());
        objects.extend(self.loaded_child_objects.clone());
        objects.extend(self.modified_at_versions.clone());
        objects.extend(self.runtime_reads.clone());
        objects.extend(self.input_objects.clone());
        objects
    }

    pub fn write_to_file(&self, path: &Path) -> Result<PathBuf, anyhow::Error> {
        let file_name = format!(
            "{}_{}_NODE_DUMP.json",
            self.tx_digest,
            AuthorityState::unixtime_now_ms()
        );
        let mut path = path.to_path_buf();
        path.push(&file_name);
        let mut file = File::create(path.clone())?;
        file.write_all(serde_json::to_string_pretty(self)?.as_bytes())?;
        Ok(path)
    }

    pub fn read_from_file(path: &PathBuf) -> Result<Self, anyhow::Error> {
        let file = File::open(path)?;
        serde_json::from_reader(file).map_err(|e| anyhow::anyhow!(e))
    }
}
