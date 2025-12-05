// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwapOption;
use enum_dispatch::enum_dispatch;
use fastcrypto::groups::bls12381;
use fastcrypto_tbls::dkg_v1;
use fastcrypto_tbls::nodes::PartyId;
use fastcrypto_zkp::bn254::zk_login::{JWK, JwkId, OIDCProvider};
use fastcrypto_zkp::bn254::zk_login_api::ZkLoginEnv;
use futures::FutureExt;
use futures::future::{Either, join_all, select};
use itertools::{Itertools, izip};
use moka::sync::SegmentedCache as MokaCache;
use move_bytecode_utils::module_cache::SyncModuleCache;
use mysten_common::assert_reachable;
use mysten_common::random_util::randomize_cache_capacity_in_tests;
use mysten_common::sync::notify_once::NotifyOnce;
use mysten_common::sync::notify_read::NotifyRead;
use mysten_common::{debug_fatal, fatal};
use mysten_metrics::monitored_scope;
use nonempty::NonEmpty;
use parking_lot::RwLock;
use parking_lot::{Mutex, RwLockReadGuard, RwLockWriteGuard};
use serde::{Deserialize, Serialize};
use sui_config::node::ExpensiveSafetyCheckConfig;
use sui_execution::{self, Executor};
use sui_macros::fail_point;
use sui_protocol_config::{Chain, PerObjectCongestionControlMode, ProtocolConfig, ProtocolVersion};
use sui_storage::mutex_table::{MutexGuard, MutexTable};
use sui_types::authenticator_state::{ActiveJwk, get_authenticator_state};
use sui_types::base_types::{
    AuthorityName, ConsensusObjectSequenceKey, EpochId, FullObjectID, ObjectID, SequenceNumber,
    TransactionDigest,
};
use sui_types::base_types::{ConciseableName, ObjectRef};
use sui_types::committee::Committee;
use sui_types::committee::CommitteeTrait;
use sui_types::crypto::{AuthoritySignInfo, AuthorityStrongQuorumSignInfo, RandomnessRound};
use sui_types::digests::{ChainIdentifier, TransactionEffectsDigest};
use sui_types::dynamic_field::get_dynamic_field_from_store;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::error::{SuiError, SuiErrorKind, SuiResult};
use sui_types::executable_transaction::{
    TrustedExecutableTransaction, VerifiedExecutableTransaction,
};
use sui_types::execution::{ExecutionTimeObservationKey, ExecutionTiming};
use sui_types::global_state_hash::GlobalStateHash;
use sui_types::message_envelope::TrustedEnvelope;
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointSequenceNumber, CheckpointSignatureMessage, CheckpointSummary,
};
use sui_types::messages_consensus::{
    AuthorityCapabilitiesV1, AuthorityCapabilitiesV2, AuthorityIndex, ConsensusPosition,
    ConsensusTransaction, ConsensusTransactionKey, ConsensusTransactionKind, TimestampMs,
    VersionedDkgConfirmation, check_total_jwk_size,
};
use sui_types::signature::GenericSignature;
use sui_types::storage::{BackingPackageStore, InputKey, ObjectStore};
use sui_types::sui_system_state::epoch_start_sui_system_state::{
    EpochStartSystemState, EpochStartSystemStateTrait,
};
use sui_types::sui_system_state::{self, SuiSystemState};
use sui_types::transaction::{
    AuthenticatorStateUpdate, CertifiedTransaction, InputObjectKind, ProgrammableTransaction,
    SenderSignedData, StoredExecutionTimeObservations, Transaction, TransactionData,
    TransactionDataAPI, TransactionKey, TransactionKind, TxValidityCheckContext,
    VerifiedSignedTransaction, VerifiedTransaction,
};
use tap::TapOptional;
use tokio::sync::{OnceCell, mpsc, oneshot};
use tokio::time::Instant;
use tracing::{debug, error, info, instrument, trace, warn};
use typed_store::DBMapUtils;
use typed_store::Map;
use typed_store::rocks::{DBBatch, DBMap, DBOptions, MetricConf, default_db_options};
use typed_store::rocks::{ReadWriteOptions, read_size_from_env};
use typed_store::rocksdb::Options;

use super::authority_store_tables::ENV_VAR_LOCKS_BLOCK_CACHE_SIZE;
use super::consensus_tx_status_cache::{ConsensusTxStatus, ConsensusTxStatusCache};
use super::epoch_start_configuration::EpochStartConfigTrait;
use super::execution_time_estimator::{ConsensusObservations, ExecutionTimeEstimator};
use super::shared_object_congestion_tracker::{
    CongestionPerObjectDebt, SharedObjectCongestionTracker,
};
use super::shared_object_version_manager::AssignedVersions;
use super::submitted_transaction_cache::{
    SubmittedTransactionCache, SubmittedTransactionCacheMetrics,
};
use super::transaction_deferral::{DeferralKey, DeferralReason};
use super::transaction_reject_reason_cache::TransactionRejectReasonCache;
use crate::authority::ResolverWrapper;
use crate::authority::epoch_start_configuration::EpochStartConfiguration;
use crate::authority::execution_time_estimator::{
    EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_CHUNK_COUNT_KEY, EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_KEY,
};
use crate::authority::shared_object_version_manager::{
    AssignedTxAndVersions, ConsensusSharedObjVerAssignment, Schedulable, SharedObjVerManager,
};
use crate::checkpoints::{
    BuilderCheckpointSummary, CheckpointHeight, EpochStats, PendingCheckpoint,
};
use crate::consensus_handler::{
    ConsensusCommitInfo, SequencedConsensusTransaction, SequencedConsensusTransactionKey,
    SequencedConsensusTransactionKind, VerifiedSequencedConsensusTransaction,
};
use crate::epoch::epoch_metrics::EpochMetrics;
use crate::epoch::randomness::{
    RandomnessManager, RandomnessReporter, SINGLETON_KEY, VersionedProcessedMessage,
    VersionedUsedProcessedMessages,
};
use crate::epoch::reconfiguration::ReconfigState;
use crate::execution_cache::ObjectCacheRead;
use crate::execution_cache::cache_types::CacheResult;
use crate::fallback_fetch::do_fallback_lookup;
use crate::module_cache_metrics::ResolverMetrics;
use crate::signature_verifier::*;
use crate::stake_aggregator::{GenericMultiStakeAggregator, StakeAggregator};
use sui_types::execution::ExecutionTimeObservationChunkKey;

/// The key where the latest consensus index is stored in the database.
// TODO: Make a single table (e.g., called `variables`) storing all our lonely variables in one place.
const LAST_CONSENSUS_STATS_ADDR: u64 = 0;
const RECONFIG_STATE_INDEX: u64 = 0;
const OVERRIDE_PROTOCOL_UPGRADE_BUFFER_STAKE_INDEX: u64 = 0;
pub const EPOCH_DB_PREFIX: &str = "epoch_";

// Types for randomness DKG.
pub(crate) type PkG = bls12381::G2Element;
pub(crate) type EncG = bls12381::G2Element;

#[path = "consensus_quarantine.rs"]
pub(crate) mod consensus_quarantine;

use consensus_quarantine::ConsensusCommitOutput;
use consensus_quarantine::ConsensusOutputCache;
use consensus_quarantine::ConsensusOutputQuarantine;

// CertLockGuard and CertTxGuard are functionally identical right now, but we retain a distinction
// anyway. If we need to support distributed object storage, having this distinction will be
// useful, as we will most likely have to re-implement a retry / write-ahead-log at that point.
pub struct CertLockGuard(#[allow(unused)] MutexGuard);
pub struct CertTxGuard(#[allow(unused)] CertLockGuard);

impl CertTxGuard {
    pub fn release(self) {}
    pub fn commit_tx(self) {}
    pub fn as_lock_guard(&self) -> &CertLockGuard {
        &self.0
    }
}

impl CertLockGuard {
    pub fn dummy_for_tests() -> Self {
        let lock = Arc::new(parking_lot::Mutex::new(()));
        Self(lock.try_lock_arc().unwrap())
    }
}

type JwkAggregator = GenericMultiStakeAggregator<(JwkId, JWK), true>;

type LocalExecutionTimeData = (
    ProgrammableTransaction,
    Vec<ExecutionTiming>,
    Duration,
    u64, // gas_price
);

pub enum CancelConsensusCertificateReason {
    CongestionOnObjects(Vec<ObjectID>),
    DkgFailed,
}

pub enum ConsensusCertificateResult {
    /// The consensus message was ignored (e.g. because it has already been processed).
    Ignored,
    /// An executable transaction (can be a user tx or a system tx)
    SuiTransaction(VerifiedExecutableTransaction),
    /// The transaction should be re-processed at a future commit, specified by the DeferralKey
    Deferred(DeferralKey),
    /// A message was processed which updates randomness state.
    RandomnessConsensusMessage,
    /// Everything else, e.g. AuthorityCapabilities, CheckpointSignatures, etc.
    ConsensusMessage,
    /// A system message in consensus was ignored (e.g. because of end of epoch).
    IgnoredSystem,
    /// A will-be-cancelled transaction. It'll still go through execution engine (but not be executed),
    /// unlock any owned objects, and return corresponding cancellation error according to
    /// `CancelConsensusCertificateReason`.
    Cancelled(
        (
            VerifiedExecutableTransaction,
            CancelConsensusCertificateReason,
        ),
    ),
}

/// ConsensusStats is versioned because we may iterate on the struct, and it is
/// stored on disk.
#[enum_dispatch]
pub trait ConsensusStatsAPI {
    fn is_initialized(&self) -> bool;

    fn get_num_messages(&self, authority: usize) -> u64;
    fn inc_num_messages(&mut self, authority: usize) -> u64;

    fn get_num_user_transactions(&self, authority: usize) -> u64;
    fn inc_num_user_transactions(&mut self, authority: usize) -> u64;
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[enum_dispatch(ConsensusStatsAPI)]
pub enum ConsensusStats {
    V1(ConsensusStatsV1),
}

impl ConsensusStats {
    pub fn new(size: usize) -> Self {
        Self::V1(ConsensusStatsV1 {
            num_messages: vec![0; size],
            num_user_transactions: vec![0; size],
        })
    }
}

impl Default for ConsensusStats {
    fn default() -> Self {
        Self::new(0)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ConsensusStatsV1 {
    pub num_messages: Vec<u64>,
    pub num_user_transactions: Vec<u64>,
}

impl ConsensusStatsAPI for ConsensusStatsV1 {
    fn is_initialized(&self) -> bool {
        !self.num_messages.is_empty()
    }

    fn get_num_messages(&self, authority: usize) -> u64 {
        self.num_messages[authority]
    }

    fn inc_num_messages(&mut self, authority: usize) -> u64 {
        self.num_messages[authority] += 1;
        self.num_messages[authority]
    }

    fn get_num_user_transactions(&self, authority: usize) -> u64 {
        self.num_user_transactions[authority]
    }

    fn inc_num_user_transactions(&mut self, authority: usize) -> u64 {
        self.num_user_transactions[authority] += 1;
        self.num_user_transactions[authority]
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq, Copy)]
pub struct ExecutionIndices {
    /// The round number of the last committed leader.
    pub last_committed_round: u64,
    /// The index of the last sub-DAG that was executed (either fully or partially).
    pub sub_dag_index: u64,
    /// The index of the last transaction was executed (used for crash-recovery).
    pub transaction_index: u64,
}

impl Ord for ExecutionIndices {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (
            self.last_committed_round,
            self.sub_dag_index,
            self.transaction_index,
        )
            .cmp(&(
                other.last_committed_round,
                other.sub_dag_index,
                other.transaction_index,
            ))
    }
}

impl PartialOrd for ExecutionIndices {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionIndicesWithStats {
    pub index: ExecutionIndices,
    // Hash is always 0 and kept for compatibility only.
    pub hash: u64,
    pub stats: ConsensusStats,
}

type ExecutionModuleCache = SyncModuleCache<ResolverWrapper>;

// Data related to VM and Move execution and type layout
pub struct ExecutionComponents {
    pub(crate) executor: Arc<dyn Executor + Send + Sync>,
    // TODO: use strategies (e.g. LRU?) to constraint memory usage
    pub(crate) module_cache: Arc<ExecutionModuleCache>,
    metrics: Arc<ResolverMetrics>,
}

#[cfg(test)]
#[path = "../unit_tests/authority_per_epoch_store_tests.rs"]
pub mod authority_per_epoch_store_tests;

pub struct AuthorityPerEpochStore {
    /// The name of this authority.
    pub(crate) name: AuthorityName,

    /// Committee of validators for the current epoch.
    committee: Arc<Committee>,

    /// Holds the underlying per-epoch typed store tables.
    /// This is an ArcSwapOption because it needs to be used concurrently,
    /// and it needs to be cleared at the end of the epoch.
    tables: ArcSwapOption<AuthorityEpochTables>,

    /// Holds the outputs of both consensus handler and checkpoint builder in memory
    /// until they are proven not to have forked by a certified checkpoint.
    pub(crate) consensus_quarantine: RwLock<ConsensusOutputQuarantine>,
    /// Holds variouis data from consensus_quarantine in a more easily accessible form.
    pub(crate) consensus_output_cache: ConsensusOutputCache,

    protocol_config: ProtocolConfig,

    // needed for re-opening epoch db.
    parent_path: PathBuf,
    db_options: Option<Options>,

    /// In-memory cache of the content from the reconfig_state db table.
    reconfig_state_mem: RwLock<ReconfigState>,
    consensus_notify_read: NotifyRead<SequencedConsensusTransactionKey, ()>,

    // Subscribers will get notified when a transaction is executed via checkpoint execution.
    executed_transactions_to_checkpoint_notify_read:
        NotifyRead<TransactionDigest, CheckpointSequenceNumber>,

    /// Batch verifier for certificates - also caches certificates and tx sigs that are known to have
    /// valid signatures. Lives in per-epoch store because the caching/batching is only valid
    /// within for certs within the current epoch.
    pub(crate) signature_verifier: SignatureVerifier,

    pub(crate) checkpoint_state_notify_read: NotifyRead<CheckpointSequenceNumber, GlobalStateHash>,

    running_root_notify_read: NotifyRead<CheckpointSequenceNumber, GlobalStateHash>,

    executed_digests_notify_read: NotifyRead<TransactionKey, TransactionDigest>,

    /// This is used to notify all epoch specific tasks that epoch has ended.
    epoch_alive_notify: NotifyOnce,

    /// Used to notify all epoch specific tasks that user certs are closed.
    user_certs_closed_notify: NotifyOnce,

    /// This lock acts as a barrier for tasks that should not be executed in parallel with reconfiguration
    /// See comments in AuthorityPerEpochStore::epoch_terminated() on how this is used
    /// Crash recovery note: we write next epoch in the database first, and then use this lock to
    /// wait for in-memory tasks for the epoch to finish. If node crashes at this stage validator
    /// will start with the new epoch(and will open instance of per-epoch store for a new epoch).
    epoch_alive: tokio::sync::RwLock<bool>,
    pub(crate) end_of_publish: Mutex<StakeAggregator<(), true>>,
    /// Pending certificates that are waiting to be sequenced by the consensus.
    /// This is an in-memory 'index' of a AuthorityPerEpochTables::pending_consensus_transactions.
    /// We need to keep track of those in order to know when to send EndOfPublish message.
    /// Lock ordering: this is a 'leaf' lock, no other locks should be acquired in the scope of this lock
    /// In particular, this lock is always acquired after taking read or write lock on reconfig state
    pending_consensus_certificates: RwLock<HashSet<TransactionDigest>>,

    /// MutexTable for transaction locks (prevent concurrent execution of same transaction)
    mutex_table: MutexTable<TransactionDigest>,
    /// Mutex table for shared version assignment
    version_assignment_mutex_table: MutexTable<ObjectID>,

    /// The moment when the current epoch started locally on this validator. Note that this
    /// value could be skewed if the node crashed and restarted in the middle of the epoch. That's
    /// ok because this is used for metric purposes and we could tolerate some skews occasionally.
    pub(crate) epoch_open_time: Instant,

    /// The moment when epoch is closed. We don't care much about crash recovery because it's
    /// a metric that doesn't have to be available for each epoch, and it's only used during
    /// the last few seconds of an epoch.
    epoch_close_time: RwLock<Option<Instant>>,
    pub(crate) metrics: Arc<EpochMetrics>,
    epoch_start_configuration: Arc<EpochStartConfiguration>,

    /// Execution state that has to restart at each epoch change
    execution_component: ExecutionComponents,

    /// ChainIdentifier is always the true id (digest of genesis checkpoint). Chain is the
    /// nominal identifier and can be overridden for testing purposes.
    chain: (ChainIdentifier, Chain),

    /// aggregator for JWK votes
    jwk_aggregator: Mutex<JwkAggregator>,

    /// State machine managing randomness DKG and generation.
    pub(crate) randomness_manager: OnceCell<tokio::sync::Mutex<RandomnessManager>>,
    randomness_reporter: OnceCell<RandomnessReporter>,

    /// Manages recording execution time observations and generating estimates.
    pub(crate) execution_time_estimator: tokio::sync::Mutex<Option<ExecutionTimeEstimator>>,
    tx_local_execution_time: OnceCell<mpsc::Sender<LocalExecutionTimeData>>,
    pub(crate) tx_object_debts: OnceCell<mpsc::Sender<Vec<ObjectID>>>,
    // Saved at end of epoch for propagating observations to the next.
    pub(crate) end_of_epoch_execution_time_observations: OnceCell<StoredExecutionTimeObservations>,

    pub(crate) consensus_tx_status_cache: Option<ConsensusTxStatusCache>,

    /// A cache that maintains the reject vote reason for a transaction.
    pub(crate) tx_reject_reason_cache: Option<TransactionRejectReasonCache>,

    /// A cache that tracks submitted transactions to prevent DoS through excessive resubmissions.
    pub(crate) submitted_transaction_cache: SubmittedTransactionCache,

    /// A cache which tracks recently finalized transactions.
    pub(crate) finalized_transactions_cache: MokaCache<TransactionDigest, ()>,

    /// Waiters for settlement transactions. Used by execution scheduler to wait for
    /// settlement transaction keys to resolve to transactions.
    /// Stored in AuthorityPerEpochStore so that it is automatically cleaned up at the end of the epoch.
    settlement_registrations: Arc<Mutex<HashMap<TransactionKey, SettlementRegistration>>>,
}
enum SettlementRegistration {
    Ready(Vec<VerifiedExecutableTransaction>),
    Waiting(oneshot::Sender<Vec<VerifiedExecutableTransaction>>),
}

/// AuthorityEpochTables contains tables that contain data that is only valid within an epoch.
#[derive(DBMapUtils)]
#[cfg_attr(tidehunter, tidehunter)]
pub struct AuthorityEpochTables {
    /// This is map between the transaction digest and transactions found in the `transaction_lock`.
    #[default_options_override_fn = "signed_transactions_table_default_config"]
    signed_transactions:
        DBMap<TransactionDigest, TrustedEnvelope<SenderSignedData, AuthoritySignInfo>>,

    /// Map from ObjectRef to transaction locking that object
    #[default_options_override_fn = "owned_object_transaction_locks_table_default_config"]
    owned_object_locked_transactions: DBMap<ObjectRef, LockDetailsWrapper>,

    /// Signatures over transaction effects that we have signed and returned to users.
    /// We store this to avoid re-signing the same effects twice.
    /// Note that this may contain signatures for effects from previous epochs, in the case
    /// that a user requests a signature for effects from a previous epoch. However, the
    /// signature is still epoch-specific and so is stored in the epoch store.
    effects_signatures: DBMap<TransactionDigest, AuthoritySignInfo>,

    /// When we sign a TransactionEffects, we must record the digest of the effects in order
    /// to detect and prevent equivocation when re-executing a transaction that may not have been
    /// committed to disk.
    /// Entries are removed from this table after the transaction in question has been committed
    /// to disk.
    signed_effects_digests: DBMap<TransactionDigest, TransactionEffectsDigest>,

    /// Signatures of transaction certificates that are executed locally.
    transaction_cert_signatures: DBMap<TransactionDigest, AuthorityStrongQuorumSignInfo>,

    /// Next available shared object versions for each shared object.
    next_shared_object_versions_v2: DBMap<ConsensusObjectSequenceKey, SequenceNumber>,

    /// Track which transactions have been processed in handle_consensus_transaction. We must be
    /// sure to advance next_shared_object_versions exactly once for each transaction we receive from
    /// consensus. But, we may also be processing transactions from checkpoints, so we need to
    /// track this state separately.
    ///
    /// Entries in this table can be garbage collected whenever we can prove that we won't receive
    /// another handle_consensus_transaction call for the given digest. This probably means at
    /// epoch change.
    consensus_message_processed: DBMap<SequencedConsensusTransactionKey, bool>,

    /// Map stores pending transactions that this authority submitted to consensus
    #[default_options_override_fn = "pending_consensus_transactions_table_default_config"]
    pending_consensus_transactions: DBMap<ConsensusTransactionKey, ConsensusTransaction>,

    /// The following table is used to store a single value (the corresponding key is a constant). The value
    /// represents the index of the latest consensus message this authority processed, running hash of
    /// transactions, and accumulated stats of consensus output.
    /// This field is written by a single process (consensus handler).
    last_consensus_stats: DBMap<u64, ExecutionIndicesWithStats>,

    /// This table contains current reconfiguration state for validator for current epoch
    reconfig_state: DBMap<u64, ReconfigState>,

    /// Validators that have sent EndOfPublish message in this epoch
    end_of_publish: DBMap<AuthorityName, ()>,

    /// Checkpoint builder maintains internal list of transactions it included in checkpoints here
    builder_digest_to_checkpoint: DBMap<TransactionDigest, CheckpointSequenceNumber>,

    /// Maps non-digest TransactionKeys to the corresponding digest after execution, for use
    /// by checkpoint builder.
    transaction_key_to_digest: DBMap<TransactionKey, TransactionDigest>,

    /// Stores pending signatures
    /// The key in this table is checkpoint sequence number and an arbitrary integer
    pub(crate) pending_checkpoint_signatures:
        DBMap<(CheckpointSequenceNumber, u64), CheckpointSignatureMessage>,

    /// Maps sequence number to checkpoint summary, used by CheckpointBuilder to build checkpoint within epoch
    builder_checkpoint_summary_v2: DBMap<CheckpointSequenceNumber, BuilderCheckpointSummary>,

    // Maps checkpoint sequence number to an accumulator with accumulated state
    // only for the checkpoint that the key references. Append-only, i.e.,
    // the accumulator is complete wrt the checkpoint
    pub state_hash_by_checkpoint: DBMap<CheckpointSequenceNumber, GlobalStateHash>,

    /// Maps checkpoint sequence number to the running (non-finalized) root state
    /// accumulator up th that checkpoint. This should be equivalent to the root
    /// state hash at end of epoch. Guaranteed to be written to in checkpoint
    /// sequence number order.
    #[rename = "running_root_accumulators"]
    pub running_root_state_hash: DBMap<CheckpointSequenceNumber, GlobalStateHash>,

    /// Record of the capabilities advertised by each authority.
    authority_capabilities: DBMap<AuthorityName, AuthorityCapabilitiesV1>,
    authority_capabilities_v2: DBMap<AuthorityName, AuthorityCapabilitiesV2>,

    /// Contains a single key, which overrides the value of
    /// ProtocolConfig::buffer_stake_for_protocol_upgrade_bps
    override_protocol_upgrade_buffer_stake: DBMap<u64, u64>,

    /// When transaction is executed via checkpoint executor, we store association here
    pub(crate) executed_transactions_to_checkpoint:
        DBMap<TransactionDigest, CheckpointSequenceNumber>,

    /// JWKs that have been voted for by one or more authorities but are not yet active.
    pending_jwks: DBMap<(AuthorityName, JwkId, JWK), ()>,

    /// JWKs that are currently available for zklogin authentication, and the round in which they
    /// became active.
    /// This would normally be stored as (JwkId, JWK) -> u64, but we need to be able to scan to
    /// find all Jwks for a given round
    active_jwks: DBMap<(u64, (JwkId, JWK)), ()>,

    /// Transactions that are being deferred until some future time
    deferred_transactions_v2: DBMap<DeferralKey, Vec<TrustedExecutableTransaction>>,

    // Tables for recording state for RandomnessManager.
    /// Records messages processed from other nodes. Updated when receiving a new dkg::Message
    /// via consensus.
    pub(crate) dkg_processed_messages_v2: DBMap<PartyId, VersionedProcessedMessage>,

    /// Records messages used to generate a DKG confirmation. Updated when enough DKG
    /// messages are received to progress to the next phase.
    pub(crate) dkg_used_messages_v2: DBMap<u64, VersionedUsedProcessedMessages>,

    /// Records confirmations received from other nodes. Updated when receiving a new
    /// dkg::Confirmation via consensus.
    pub(crate) dkg_confirmations_v2: DBMap<PartyId, VersionedDkgConfirmation>,
    /// Records the final output of DKG after completion, including the public VSS key and
    /// any local private shares.
    pub(crate) dkg_output: DBMap<u64, dkg_v1::Output<PkG, EncG>>,
    /// Holds the value of the next RandomnessRound to be generated.
    pub(crate) randomness_next_round: DBMap<u64, RandomnessRound>,
    /// Holds the value of the highest completed RandomnessRound (as reported to RandomnessReporter).
    pub(crate) randomness_highest_completed_round: DBMap<u64, RandomnessRound>,
    /// Holds the timestamp of the most recently generated round of randomness.
    pub(crate) randomness_last_round_timestamp: DBMap<u64, TimestampMs>,

    /// Accumulated per-object debts for congestion control.
    pub(crate) congestion_control_object_debts: DBMap<ObjectID, CongestionPerObjectDebt>,
    pub(crate) congestion_control_randomness_object_debts: DBMap<ObjectID, CongestionPerObjectDebt>,

    /// Execution time observations for congestion control.
    pub(crate) execution_time_observations:
        DBMap<(u64, AuthorityIndex), Vec<(ExecutionTimeObservationKey, Duration)>>,
}

fn signed_transactions_table_default_config() -> DBOptions {
    default_db_options()
        .optimize_for_write_throughput()
        .optimize_for_large_values_no_scan(1 << 10)
}

fn owned_object_transaction_locks_table_default_config() -> DBOptions {
    DBOptions {
        options: default_db_options()
            .optimize_for_write_throughput()
            .optimize_for_read(read_size_from_env(ENV_VAR_LOCKS_BLOCK_CACHE_SIZE).unwrap_or(1024))
            .options,
        rw_options: ReadWriteOptions::default().set_ignore_range_deletions(false),
    }
}

fn pending_consensus_transactions_table_default_config() -> DBOptions {
    default_db_options()
        .optimize_for_write_throughput()
        .optimize_for_large_values_no_scan(1 << 10)
}

impl AuthorityEpochTables {
    #[cfg(not(tidehunter))]
    pub fn open(epoch: EpochId, parent_path: &Path, db_options: Option<Options>) -> Self {
        Self::open_tables_read_write(
            Self::path(epoch, parent_path),
            MetricConf::new("epoch"),
            db_options,
            None,
        )
    }

    #[cfg(tidehunter)]
    pub fn open(epoch: EpochId, parent_path: &Path, db_options: Option<Options>) -> Self {
        tracing::warn!("AuthorityEpochTables using tidehunter");
        use typed_store::tidehunter_util::{
            KeyIndexing, KeySpaceConfig, KeyType, ThConfig, default_cells_per_mutex,
            default_mutex_count, default_value_cache_size,
        };
        let mutexes = default_mutex_count() * 2;
        let mut digest_prefix = vec![0; 8];
        digest_prefix[7] = 32;
        let value_cache_size = default_value_cache_size() * 2;
        let bloom_config = KeySpaceConfig::new().with_bloom_filter(0.001, 32_000);
        let lru_bloom_config = bloom_config.clone().with_value_cache_size(value_cache_size);
        let lru_only_config = KeySpaceConfig::new().with_value_cache_size(value_cache_size);
        let pending_checkpoint_signatures_config = KeySpaceConfig::new()
            .disable_unload()
            .with_value_cache_size(default_value_cache_size());
        let builder_checkpoint_summary_v2_config = pending_checkpoint_signatures_config.clone();
        let object_ref_indexing = KeyIndexing::Hash;
        let tx_digest_indexing = KeyIndexing::key_reduction(32, 0..16);
        let uniform_key = KeyType::uniform(default_cells_per_mutex());
        let sequence_key = KeyType::from_prefix_bits(1 * 8 + 4);
        let configs = vec![
            (
                "signed_transactions".to_string(),
                ThConfig::new_with_rm_prefix_indexing(
                    tx_digest_indexing.clone(),
                    mutexes,
                    uniform_key,
                    lru_bloom_config.clone(),
                    digest_prefix.clone(),
                ),
            ),
            (
                "owned_object_locked_transactions".to_string(),
                ThConfig::new_with_config_indexing(
                    object_ref_indexing,
                    mutexes * 2,
                    uniform_key,
                    bloom_config.clone(),
                ),
            ),
            (
                "effects_signatures".to_string(),
                ThConfig::new_with_rm_prefix_indexing(
                    tx_digest_indexing.clone(),
                    mutexes,
                    uniform_key,
                    lru_bloom_config.clone(),
                    digest_prefix.clone(),
                ),
            ),
            (
                "signed_effects_digests".to_string(),
                ThConfig::new_with_rm_prefix_indexing(
                    tx_digest_indexing.clone(),
                    mutexes,
                    uniform_key,
                    bloom_config.clone(),
                    digest_prefix.clone(),
                ),
            ),
            (
                "transaction_cert_signatures".to_string(),
                ThConfig::new_with_rm_prefix_indexing(
                    tx_digest_indexing.clone(),
                    mutexes,
                    uniform_key,
                    lru_bloom_config.clone(),
                    digest_prefix.clone(),
                ),
            ),
            (
                "next_shared_object_versions_v2".to_string(),
                ThConfig::new_with_config(32 + 8, mutexes, uniform_key, lru_only_config.clone()),
            ),
            (
                "consensus_message_processed".to_string(),
                ThConfig::new_with_config_indexing(
                    KeyIndexing::Hash,
                    mutexes,
                    uniform_key,
                    bloom_config.clone(),
                ),
            ),
            (
                "pending_consensus_transactions".to_string(),
                ThConfig::new_with_config_indexing(
                    KeyIndexing::Hash,
                    mutexes,
                    uniform_key,
                    KeySpaceConfig::default(),
                ),
            ),
            (
                "last_consensus_stats".to_string(),
                ThConfig::new(8, 1, KeyType::uniform(1)),
            ),
            (
                "reconfig_state".to_string(),
                ThConfig::new(8, 1, KeyType::uniform(1)),
            ),
            (
                "end_of_publish".to_string(),
                ThConfig::new(104, 1, KeyType::uniform(1)),
            ),
            (
                "builder_digest_to_checkpoint".to_string(),
                ThConfig::new_with_rm_prefix_indexing(
                    tx_digest_indexing.clone(),
                    mutexes * 4,
                    uniform_key,
                    lru_bloom_config.clone(),
                    digest_prefix.clone(),
                ),
            ),
            (
                "transaction_key_to_digest".to_string(),
                ThConfig::new_with_config_indexing(
                    KeyIndexing::Hash,
                    mutexes,
                    uniform_key,
                    KeySpaceConfig::default(),
                ),
            ),
            (
                "pending_checkpoint_signatures".to_string(),
                ThConfig::new_with_config(
                    8 + 8,
                    mutexes,
                    uniform_key,
                    pending_checkpoint_signatures_config,
                ),
            ),
            (
                "builder_checkpoint_summary_v2".to_string(),
                ThConfig::new_with_config(
                    8,
                    mutexes,
                    sequence_key,
                    builder_checkpoint_summary_v2_config,
                ),
            ),
            (
                "state_hash_by_checkpoint".to_string(),
                ThConfig::new_with_config(8, mutexes, sequence_key, bloom_config.clone()),
            ),
            (
                "running_root_accumulators".to_string(),
                ThConfig::new_with_config(8, mutexes, sequence_key, bloom_config.clone()),
            ),
            (
                "authority_capabilities".to_string(),
                ThConfig::new(104, mutexes, uniform_key),
            ),
            (
                "authority_capabilities_v2".to_string(),
                ThConfig::new(104, 1, KeyType::uniform(1)),
            ),
            (
                "override_protocol_upgrade_buffer_stake".to_string(),
                ThConfig::new(8, 1, KeyType::uniform(1)),
            ),
            (
                "executed_transactions_to_checkpoint".to_string(),
                ThConfig::new_with_rm_prefix_indexing(
                    tx_digest_indexing.clone(),
                    mutexes * 4,
                    uniform_key,
                    lru_bloom_config.clone(),
                    digest_prefix.clone(),
                ),
            ),
            (
                "pending_jwks".to_string(),
                ThConfig::new_with_config_indexing(
                    KeyIndexing::VariableLength,
                    1,
                    KeyType::uniform(1),
                    KeySpaceConfig::default(),
                ),
            ),
            (
                "active_jwks".to_string(),
                ThConfig::new_with_config_indexing(
                    KeyIndexing::VariableLength,
                    1,
                    KeyType::uniform(1),
                    KeySpaceConfig::default(),
                ),
            ),
            (
                "deferred_transactions".to_string(),
                ThConfig::new_with_indexing(KeyIndexing::Hash, mutexes, uniform_key),
            ),
            (
                "deferred_transactions_v2".to_string(),
                ThConfig::new_with_indexing(KeyIndexing::Hash, mutexes, uniform_key),
            ),
            (
                "dkg_processed_messages_v2".to_string(),
                ThConfig::new(2, 1, KeyType::uniform(1)),
            ),
            (
                "dkg_used_messages_v2".to_string(),
                ThConfig::new(8, 1, KeyType::uniform(1)),
            ),
            (
                "dkg_confirmations_v2".to_string(),
                ThConfig::new(2, 1, KeyType::uniform(1)),
            ),
            (
                "dkg_output".to_string(),
                ThConfig::new(8, 1, KeyType::uniform(1)),
            ),
            (
                "randomness_next_round".to_string(),
                ThConfig::new(8, 1, KeyType::uniform(1)),
            ),
            (
                "randomness_highest_completed_round".to_string(),
                ThConfig::new(8, 1, KeyType::uniform(1)),
            ),
            (
                "randomness_last_round_timestamp".to_string(),
                ThConfig::new(8, 1, KeyType::uniform(1)),
            ),
            (
                "congestion_control_object_debts".to_string(),
                ThConfig::new_with_config(32, mutexes, uniform_key, lru_bloom_config.clone()),
            ),
            (
                "congestion_control_randomness_object_debts".to_string(),
                ThConfig::new(32, mutexes, uniform_key),
            ),
            (
                "execution_time_observations".to_string(),
                ThConfig::new(8 + 4, mutexes, uniform_key),
            ),
        ];
        Self::open_tables_read_write(
            Self::path(epoch, parent_path),
            MetricConf::new("epoch"),
            configs.into_iter().collect(),
        )
    }

    pub fn open_readonly(epoch: EpochId, parent_path: &Path) -> AuthorityEpochTablesReadOnly {
        Self::get_read_only_handle(
            Self::path(epoch, parent_path),
            None,
            None,
            MetricConf::new("epoch_readonly"),
        )
    }

    pub fn path(epoch: EpochId, parent_path: &Path) -> PathBuf {
        parent_path.join(format!("{}{}", EPOCH_DB_PREFIX, epoch))
    }

    fn load_reconfig_state(&self) -> SuiResult<ReconfigState> {
        let state = self
            .reconfig_state
            .get(&RECONFIG_STATE_INDEX)?
            .unwrap_or_default();
        Ok(state)
    }

    pub fn get_all_pending_consensus_transactions(&self) -> SuiResult<Vec<ConsensusTransaction>> {
        Ok(self
            .pending_consensus_transactions
            .safe_iter()
            .map(|item| item.map(|(_k, v)| v))
            .collect::<Result<Vec<_>, _>>()?)
    }

    /// WARNING: This method is very subtle and can corrupt the database if used incorrectly.
    /// It should only be used in one-off cases or tests after fully understanding the risk.
    pub fn remove_executed_tx_subtle(&self, digest: &TransactionDigest) -> SuiResult {
        self.executed_transactions_to_checkpoint.remove(digest)?;
        Ok(())
    }

    pub fn get_last_consensus_index(&self) -> SuiResult<Option<ExecutionIndices>> {
        Ok(self
            .last_consensus_stats
            .get(&LAST_CONSENSUS_STATS_ADDR)?
            .map(|s| s.index))
    }

    pub fn get_last_consensus_stats(&self) -> SuiResult<Option<ExecutionIndicesWithStats>> {
        Ok(self.last_consensus_stats.get(&LAST_CONSENSUS_STATS_ADDR)?)
    }

    pub fn get_locked_transaction(&self, obj_ref: &ObjectRef) -> SuiResult<Option<LockDetails>> {
        Ok(self
            .owned_object_locked_transactions
            .get(obj_ref)?
            .map(|l| l.migrate().into_inner()))
    }

    pub fn multi_get_locked_transactions(
        &self,
        owned_input_objects: &[ObjectRef],
    ) -> SuiResult<Vec<Option<LockDetails>>> {
        Ok(self
            .owned_object_locked_transactions
            .multi_get(owned_input_objects)?
            .into_iter()
            .map(|l| l.map(|l| l.migrate().into_inner()))
            .collect())
    }

    pub fn write_transaction_locks(
        &self,
        signed_transaction: Option<VerifiedSignedTransaction>,
        locks_to_write: impl Iterator<Item = (ObjectRef, LockDetails)>,
    ) -> SuiResult {
        let mut batch = self.owned_object_locked_transactions.batch();
        batch.insert_batch(
            &self.owned_object_locked_transactions,
            locks_to_write.map(|(obj_ref, lock)| (obj_ref, LockDetailsWrapper::from(lock))),
        )?;
        if let Some(signed_transaction) = signed_transaction {
            batch.insert_batch(
                &self.signed_transactions,
                std::iter::once((
                    *signed_transaction.digest(),
                    signed_transaction.serializable_ref(),
                )),
            )?;
        }
        batch.write()?;
        Ok(())
    }

    fn get_all_deferred_transactions_v2(
        &self,
    ) -> SuiResult<BTreeMap<DeferralKey, Vec<VerifiedExecutableTransaction>>> {
        Ok(self
            .deferred_transactions_v2
            .safe_iter()
            .map(|item| item.map(|(key, txs)| (key, txs.into_iter().map(Into::into).collect())))
            .collect::<Result<_, _>>()?)
    }
}

pub(crate) const MUTEX_TABLE_SIZE: usize = 1024;

impl AuthorityPerEpochStore {
    #[instrument(name = "AuthorityPerEpochStore::new", level = "error", skip_all, fields(epoch = committee.epoch))]
    pub fn new(
        name: AuthorityName,
        committee: Arc<Committee>,
        parent_path: &Path,
        db_options: Option<Options>,
        metrics: Arc<EpochMetrics>,
        epoch_start_configuration: EpochStartConfiguration,
        backing_package_store: Arc<dyn BackingPackageStore + Send + Sync>,
        object_store: Arc<dyn ObjectStore + Send + Sync>,
        cache_metrics: Arc<ResolverMetrics>,
        signature_verifier_metrics: Arc<SignatureVerifierMetrics>,
        expensive_safety_check_config: &ExpensiveSafetyCheckConfig,
        chain: (ChainIdentifier, Chain),
        highest_executed_checkpoint: CheckpointSequenceNumber,
        submitted_transaction_cache_metrics: Arc<SubmittedTransactionCacheMetrics>,
    ) -> SuiResult<Arc<Self>> {
        let current_time = Instant::now();
        let epoch_id = committee.epoch;
        metrics.current_epoch.set(epoch_id as i64);
        metrics
            .current_voting_right
            .set(committee.weight(&name) as i64);

        let tables = AuthorityEpochTables::open(epoch_id, parent_path, db_options.clone());
        let end_of_publish =
            StakeAggregator::from_iter(committee.clone(), tables.end_of_publish.safe_iter())?;
        let reconfig_state = tables
            .load_reconfig_state()
            .expect("Load reconfig state at initialization cannot fail");

        let epoch_alive_notify = NotifyOnce::new();
        let pending_consensus_transactions = tables.get_all_pending_consensus_transactions()?;
        let pending_consensus_certificates: HashSet<_> = pending_consensus_transactions
            .iter()
            .filter_map(|transaction| {
                if let ConsensusTransactionKind::CertifiedTransaction(certificate) =
                    &transaction.kind
                {
                    Some(*certificate.digest())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            epoch_start_configuration.epoch_start_state().epoch(),
            epoch_id
        );

        let epoch_start_configuration = Arc::new(epoch_start_configuration);
        info!("epoch flags: {:?}", epoch_start_configuration.flags());
        let protocol_version = epoch_start_configuration
            .epoch_start_state()
            .protocol_version();

        let chain_from_id = chain.0.chain();
        if chain_from_id == Chain::Mainnet || chain_from_id == Chain::Testnet {
            assert_eq!(
                chain_from_id, chain.1,
                "cannot override chain on production networks!"
            );
        }
        info!(
            "initializing epoch store from chain id {:?} to chain id {:?}",
            chain_from_id, chain.1
        );

        let protocol_config = ProtocolConfig::get_for_version(protocol_version, chain.1);

        let execution_component = ExecutionComponents::new(
            &protocol_config,
            backing_package_store,
            cache_metrics,
            expensive_safety_check_config,
        );

        let zklogin_env = match chain.1 {
            // Testnet and mainnet are treated the same since it is permanent.
            Chain::Mainnet | Chain::Testnet => ZkLoginEnv::Prod,
            _ => ZkLoginEnv::Test,
        };

        let supported_providers = protocol_config
            .zklogin_supported_providers()
            .iter()
            .map(|s| OIDCProvider::from_str(s).expect("Invalid provider string"))
            .collect::<Vec<_>>();

        let signature_verifier = SignatureVerifier::new(
            committee.clone(),
            signature_verifier_metrics,
            supported_providers,
            zklogin_env,
            protocol_config.verify_legacy_zklogin_address(),
            protocol_config.accept_zklogin_in_multisig(),
            protocol_config.accept_passkey_in_multisig(),
            protocol_config.zklogin_max_epoch_upper_bound_delta(),
            protocol_config.additional_multisig_checks(),
        );

        let authenticator_state_exists = epoch_start_configuration
            .authenticator_obj_initial_shared_version()
            .is_some();
        let authenticator_state_enabled =
            authenticator_state_exists && protocol_config.enable_jwk_consensus_updates();

        if authenticator_state_enabled {
            info!("authenticator_state enabled");
            let authenticator_state = get_authenticator_state(&*object_store)
                .expect("Read cannot fail")
                .expect("Authenticator state must exist");

            for active_jwk in &authenticator_state.active_jwks {
                let ActiveJwk { jwk_id, jwk, epoch } = active_jwk;
                assert!(epoch <= &epoch_id);
                signature_verifier.insert_jwk(jwk_id, jwk);
            }
        } else {
            info!("authenticator_state disabled");
        }

        let mut jwk_aggregator = JwkAggregator::new(committee.clone());

        for item in tables.pending_jwks.safe_iter() {
            let ((authority, id, jwk), _) = item?;
            jwk_aggregator.insert(authority, (id, jwk));
        }

        let jwk_aggregator = Mutex::new(jwk_aggregator);

        let consensus_output_cache = ConsensusOutputCache::new(&epoch_start_configuration, &tables);

        let execution_time_observations = tables
            .execution_time_observations
            .safe_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let execution_time_estimator =
            if let PerObjectCongestionControlMode::ExecutionTimeEstimate(protocol_params) =
                protocol_config.per_object_congestion_control_mode()
            {
                Some(ExecutionTimeEstimator::new(
                    committee.clone(),
                    protocol_params,
                    // Load observations stored at end of previous epoch.
                    Self::get_stored_execution_time_observations(
                        &protocol_config,
                        committee.clone(),
                        &*object_store,
                        &metrics,
                        protocol_params.default_none_duration_for_new_keys,
                    )
                    // Load observations stored during the current epoch.
                    .chain(execution_time_observations.into_iter().flat_map(
                        |((generation, source), observations)| {
                            observations.into_iter().map(move |(key, duration)| {
                                (source, Some(generation), key, duration)
                            })
                        },
                    )),
                ))
            } else {
                None
            };

        let consensus_tx_status_cache = if protocol_config.mysticeti_fastpath() {
            Some(ConsensusTxStatusCache::new(protocol_config.gc_depth()))
        } else {
            None
        };

        let tx_reject_reason_cache = if protocol_config.mysticeti_fastpath() {
            Some(TransactionRejectReasonCache::new(None, epoch_id))
        } else {
            None
        };

        let submitted_transaction_cache =
            SubmittedTransactionCache::new(None, submitted_transaction_cache_metrics);

        let finalized_transactions_cache = MokaCache::builder(8)
            .max_capacity(randomize_cache_capacity_in_tests(100_000))
            .eviction_policy(moka::policy::EvictionPolicy::lru())
            .build();

        let s = Arc::new(Self {
            name,
            committee: committee.clone(),
            protocol_config,
            tables: ArcSwapOption::new(Some(Arc::new(tables))),
            consensus_output_cache,
            consensus_quarantine: RwLock::new(ConsensusOutputQuarantine::new(
                highest_executed_checkpoint,
                metrics.clone(),
            )),
            parent_path: parent_path.to_path_buf(),
            db_options,
            reconfig_state_mem: RwLock::new(reconfig_state),
            epoch_alive_notify,
            user_certs_closed_notify: NotifyOnce::new(),
            epoch_alive: tokio::sync::RwLock::new(true),
            consensus_notify_read: NotifyRead::new(),
            executed_transactions_to_checkpoint_notify_read: NotifyRead::new(),
            signature_verifier,
            checkpoint_state_notify_read: NotifyRead::new(),
            running_root_notify_read: NotifyRead::new(),
            executed_digests_notify_read: NotifyRead::new(),
            end_of_publish: Mutex::new(end_of_publish),
            pending_consensus_certificates: RwLock::new(pending_consensus_certificates),
            mutex_table: MutexTable::new(MUTEX_TABLE_SIZE),
            version_assignment_mutex_table: MutexTable::new(MUTEX_TABLE_SIZE),
            epoch_open_time: current_time,
            epoch_close_time: Default::default(),
            metrics,
            epoch_start_configuration,
            execution_component,
            chain,
            jwk_aggregator,
            randomness_manager: OnceCell::new(),
            randomness_reporter: OnceCell::new(),
            execution_time_estimator: tokio::sync::Mutex::new(execution_time_estimator),
            tx_local_execution_time: OnceCell::new(),
            tx_object_debts: OnceCell::new(),
            end_of_epoch_execution_time_observations: OnceCell::new(),
            consensus_tx_status_cache,
            tx_reject_reason_cache,
            submitted_transaction_cache,
            finalized_transactions_cache,
            settlement_registrations: Default::default(),
        });

        s.update_buffer_stake_metric();
        Ok(s)
    }

    pub fn tables(&self) -> SuiResult<Arc<AuthorityEpochTables>> {
        match self.tables.load_full() {
            Some(tables) => Ok(tables),
            None => Err(SuiErrorKind::EpochEnded(self.epoch()).into()),
        }
    }

    // Ideally the epoch tables handle should have the same lifetime as the outer AuthorityPerEpochStore,
    // and this function should be unnecessary. But unfortunately, Arc<AuthorityPerEpochStore> outlives the
    // epoch significantly right now, so we need to manually release the tables to release its memory usage.
    pub fn release_db_handles(&self) {
        // When the logic to release DB handles becomes obsolete, it may still be useful
        // to make sure AuthorityEpochTables is not used after the next epoch starts.
        self.tables.store(None);
    }

    // Returns true if authenticator state is enabled in the protocol config *and* the
    // authenticator state object already exists
    pub fn authenticator_state_enabled(&self) -> bool {
        self.protocol_config().enable_jwk_consensus_updates() && self.authenticator_state_exists()
    }

    pub fn authenticator_state_exists(&self) -> bool {
        self.epoch_start_configuration
            .authenticator_obj_initial_shared_version()
            .is_some()
    }

    // Returns true if randomness state is enabled in the protocol config *and* the
    // randomness state object already exists
    pub fn randomness_state_enabled(&self) -> bool {
        self.protocol_config().random_beacon() && self.randomness_state_exists()
    }

    pub fn randomness_state_exists(&self) -> bool {
        self.epoch_start_configuration
            .randomness_obj_initial_shared_version()
            .is_some()
    }

    pub fn randomness_reporter(&self) -> Option<RandomnessReporter> {
        self.randomness_reporter.get().cloned()
    }

    pub async fn set_randomness_manager(
        &self,
        mut randomness_manager: RandomnessManager,
    ) -> SuiResult<()> {
        let reporter = randomness_manager.reporter();
        let result = randomness_manager.start_dkg().await;
        if self
            .randomness_manager
            .set(tokio::sync::Mutex::new(randomness_manager))
            .is_err()
        {
            debug_fatal!(
                "BUG: `set_randomness_manager` called more than once; this should never happen"
            );
        }
        if self.randomness_reporter.set(reporter).is_err() {
            debug_fatal!(
                "BUG: `set_randomness_manager` called more than once; this should never happen"
            );
        }
        result
    }

    pub fn accumulator_root_exists(&self) -> bool {
        self.epoch_start_configuration
            .accumulator_root_obj_initial_shared_version()
            .is_some()
    }

    pub fn accumulators_enabled(&self) -> bool {
        if !self.protocol_config().enable_accumulators() {
            return false;
        }
        assert!(self.accumulator_root_exists());
        true
    }

    pub fn coin_registry_exists(&self) -> bool {
        self.epoch_start_configuration
            .coin_registry_obj_initial_shared_version()
            .is_some()
    }

    pub fn display_registry_exists(&self) -> bool {
        self.epoch_start_configuration
            .display_registry_obj_initial_shared_version()
            .is_some()
    }

    pub fn coin_deny_list_state_exists(&self) -> bool {
        self.epoch_start_configuration
            .coin_deny_list_obj_initial_shared_version()
            .is_some()
    }

    pub fn coin_deny_list_v1_enabled(&self) -> bool {
        self.protocol_config().enable_coin_deny_list_v1() && self.coin_deny_list_state_exists()
    }

    pub fn bridge_exists(&self) -> bool {
        self.epoch_start_configuration
            .bridge_obj_initial_shared_version()
            .is_some()
    }

    pub fn bridge_committee_initiated(&self) -> bool {
        self.epoch_start_configuration.bridge_committee_initiated()
    }

    pub fn get_parent_path(&self) -> PathBuf {
        self.parent_path.clone()
    }

    /// Returns `&Arc<EpochStartConfiguration>`
    /// User can treat this `Arc` as `&EpochStartConfiguration`, or clone the Arc to pass as owned object
    pub fn epoch_start_config(&self) -> &Arc<EpochStartConfiguration> {
        &self.epoch_start_configuration
    }

    pub fn epoch_start_state(&self) -> &EpochStartSystemState {
        self.epoch_start_configuration.epoch_start_state()
    }

    pub fn get_chain_identifier(&self) -> ChainIdentifier {
        self.chain.0
    }

    pub fn get_chain(&self) -> Chain {
        self.chain.1
    }

    pub fn new_at_next_epoch(
        &self,
        name: AuthorityName,
        new_committee: Committee,
        epoch_start_configuration: EpochStartConfiguration,
        backing_package_store: Arc<dyn BackingPackageStore + Send + Sync>,
        object_store: Arc<dyn ObjectStore + Send + Sync>,
        expensive_safety_check_config: &ExpensiveSafetyCheckConfig,
        previous_epoch_last_checkpoint: CheckpointSequenceNumber,
    ) -> SuiResult<Arc<Self>> {
        assert_eq!(self.epoch() + 1, new_committee.epoch);
        self.record_reconfig_halt_duration_metric();
        self.record_epoch_total_duration_metric();
        Self::new(
            name,
            Arc::new(new_committee),
            &self.parent_path,
            self.db_options.clone(),
            self.metrics.clone(),
            epoch_start_configuration,
            backing_package_store,
            object_store,
            self.execution_component.metrics(),
            self.signature_verifier.metrics.clone(),
            expensive_safety_check_config,
            self.chain,
            previous_epoch_last_checkpoint,
            self.submitted_transaction_cache.metrics(),
        )
    }

    pub fn new_at_next_epoch_for_testing(
        &self,
        backing_package_store: Arc<dyn BackingPackageStore + Send + Sync>,
        object_store: Arc<dyn ObjectStore + Send + Sync>,
        expensive_safety_check_config: &ExpensiveSafetyCheckConfig,
        previous_epoch_last_checkpoint: CheckpointSequenceNumber,
    ) -> Arc<Self> {
        let next_epoch = self.epoch() + 1;
        let next_committee = Committee::new(
            next_epoch,
            self.committee.voting_rights.iter().cloned().collect(),
        );
        self.new_at_next_epoch(
            self.name,
            next_committee,
            self.epoch_start_configuration
                .new_at_next_epoch_for_testing(),
            backing_package_store,
            object_store,
            expensive_safety_check_config,
            previous_epoch_last_checkpoint,
        )
        .expect("failed to create new authority per epoch store")
    }

    pub fn committee(&self) -> &Arc<Committee> {
        &self.committee
    }

    pub fn protocol_config(&self) -> &ProtocolConfig {
        &self.protocol_config
    }

    pub fn epoch(&self) -> EpochId {
        self.committee.epoch
    }

    pub fn tx_validity_check_context(&self) -> TxValidityCheckContext<'_> {
        TxValidityCheckContext {
            config: &self.protocol_config,
            epoch: self.epoch(),
            accumulator_object_init_shared_version: self
                .epoch_start_configuration
                .accumulator_root_obj_initial_shared_version(),
            chain_identifier: self.get_chain_identifier(),
        }
    }

    pub fn get_state_hash_for_checkpoint(
        &self,
        checkpoint: &CheckpointSequenceNumber,
    ) -> SuiResult<Option<GlobalStateHash>> {
        Ok(self
            .tables()?
            .state_hash_by_checkpoint
            .get(checkpoint)
            .expect("db error"))
    }

    pub fn insert_state_hash_for_checkpoint(
        &self,
        checkpoint: &CheckpointSequenceNumber,
        accumulator: &GlobalStateHash,
    ) -> SuiResult {
        self.tables()?
            .state_hash_by_checkpoint
            .insert(checkpoint, accumulator)
            .expect("db error");
        Ok(())
    }

    pub fn get_running_root_state_hash(
        &self,
        checkpoint: CheckpointSequenceNumber,
    ) -> SuiResult<Option<GlobalStateHash>> {
        Ok(self
            .tables()?
            .running_root_state_hash
            .get(&checkpoint)
            .expect("db error"))
    }

    pub fn get_highest_running_root_state_hash(
        &self,
    ) -> SuiResult<Option<(CheckpointSequenceNumber, GlobalStateHash)>> {
        Ok(self
            .tables()?
            .running_root_state_hash
            .reversed_safe_iter_with_bounds(None, None)?
            .next()
            .transpose()?)
    }

    pub fn insert_running_root_state_hash(
        &self,
        checkpoint: &CheckpointSequenceNumber,
        hash: &GlobalStateHash,
    ) -> SuiResult {
        self.tables()?
            .running_root_state_hash
            .insert(checkpoint, hash)?;
        self.running_root_notify_read.notify(checkpoint, hash);

        Ok(())
    }

    pub fn clear_state_hashes_after_checkpoint(
        &self,
        last_committed_checkpoint: CheckpointSequenceNumber,
    ) -> SuiResult {
        let tables = self.tables()?;

        let mut keys_to_remove = Vec::new();
        for kv in tables
            .running_root_state_hash
            .safe_iter_with_bounds(Some(last_committed_checkpoint + 1), None)
        {
            let (checkpoint_seq, _) = kv?;
            if checkpoint_seq > last_committed_checkpoint {
                keys_to_remove.push(checkpoint_seq);
            }
        }

        let mut checkpoint_keys_to_remove = Vec::new();
        for kv in tables
            .state_hash_by_checkpoint
            .safe_iter_with_bounds(Some(last_committed_checkpoint + 1), None)
        {
            let (checkpoint_seq, _) = kv?;
            if checkpoint_seq > last_committed_checkpoint {
                checkpoint_keys_to_remove.push(checkpoint_seq);
            }
        }

        if !keys_to_remove.is_empty() || !checkpoint_keys_to_remove.is_empty() {
            let mut batch = self.db_batch()?;
            if !keys_to_remove.is_empty() {
                batch
                    .delete_batch(&tables.running_root_state_hash, keys_to_remove.clone())
                    .expect("db error");
            }
            if !checkpoint_keys_to_remove.is_empty() {
                batch
                    .delete_batch(
                        &tables.state_hash_by_checkpoint,
                        checkpoint_keys_to_remove.clone(),
                    )
                    .expect("db error");
            }
            batch.write().expect("db error");
            for key in keys_to_remove {
                info!(
                    "Cleared running root state hash for checkpoint {} (after last committed checkpoint {})",
                    key, last_committed_checkpoint
                );
            }
            for key in checkpoint_keys_to_remove {
                info!(
                    "Cleared checkpoint state hash for checkpoint {} (after last committed checkpoint {})",
                    key, last_committed_checkpoint
                );
            }
        }

        Ok(())
    }

    pub fn reference_gas_price(&self) -> u64 {
        self.epoch_start_state().reference_gas_price()
    }

    pub fn protocol_version(&self) -> ProtocolVersion {
        self.epoch_start_state().protocol_version()
    }

    pub fn module_cache(&self) -> &Arc<ExecutionModuleCache> {
        &self.execution_component.module_cache
    }

    pub fn executor(&self) -> &Arc<dyn Executor + Send + Sync> {
        &self.execution_component.executor
    }

    pub fn set_local_execution_time_channels(
        &self,
        tx_local_execution_time: mpsc::Sender<(
            ProgrammableTransaction,
            Vec<ExecutionTiming>,
            Duration,
            u64, // gas_price
        )>,
        tx_object_debts: mpsc::Sender<Vec<ObjectID>>,
    ) {
        if let Err(e) = self.tx_local_execution_time.set(tx_local_execution_time) {
            debug_fatal!(
                "failed to set tx_local_execution_time channel on AuthorityPerEpochStore: {e:?}"
            );
        }
        if let Err(e) = self.tx_object_debts.set(tx_object_debts) {
            debug_fatal!("failed to set tx_object_debts channel on AuthorityPerEpochStore: {e:?}");
        }
    }

    pub fn record_local_execution_time(
        &self,
        tx: &TransactionData,
        effects: &TransactionEffects,
        timings: Vec<ExecutionTiming>,
        total_duration: Duration,
    ) {
        let Some(tx_local_execution_time) = self.tx_local_execution_time.get() else {
            // Drop observations if no ExecutionTimeObserver has been configured.
            return;
        };

        if effects.status().is_cancelled() {
            return;
        }

        // Only record timings for PTBs with shared inputs.
        let TransactionKind::ProgrammableTransaction(ptb) = tx.kind() else {
            return;
        };
        if !ptb.has_shared_inputs() {
            return;
        }

        if let Err(e) = tx_local_execution_time.try_send((
            ptb.clone(),
            timings,
            total_duration,
            tx.gas_data().price,
        )) {
            // This channel should not overflow, but if it does, don't wait; just log an error
            // and drop the observation.
            self.metrics.epoch_execution_time_measurements_dropped.inc();
            warn!("failed to send local execution time to observer: {e}");
        }
    }

    pub fn get_stored_execution_time_observations(
        protocol_config: &ProtocolConfig,
        committee: Arc<Committee>,
        object_store: &dyn ObjectStore,
        metrics: &EpochMetrics,
        use_none_generation: bool,
    ) -> impl Iterator<
        Item = (
            AuthorityIndex,
            Option<u64>,
            ExecutionTimeObservationKey,
            Duration,
        ),
    > {
        if !matches!(
            protocol_config.per_object_congestion_control_mode(),
            PerObjectCongestionControlMode::ExecutionTimeEstimate(_)
        ) {
            return itertools::Either::Left(std::iter::empty());
        }

        // Load stored execution time observations from the SuiSystemState object.
        let system_state =
            sui_system_state::get_sui_system_state(object_store).expect("System state must exist");
        let system_state = match system_state {
            SuiSystemState::V2(system_state) => system_state,
            SuiSystemState::V1(_) => {
                if committee.epoch() > 1 {
                    error!(
                        "`PerObjectCongestionControlMode::ExecutionTimeEstimate` cannot load execution time observations to SuiSystemState because it has an old version. This should not happen outside tests."
                    );
                }
                return itertools::Either::Left(std::iter::empty());
            }
            #[cfg(msim)]
            SuiSystemState::SimTestV1(_)
            | SuiSystemState::SimTestShallowV2(_)
            | SuiSystemState::SimTestDeepV2(_) => {
                return itertools::Either::Left(std::iter::empty());
            }
        };
        let stored_observations = if protocol_config.enable_observation_chunking() {
            if let Ok::<u64, _>(chunk_count) = get_dynamic_field_from_store(
                object_store,
                system_state.extra_fields.id.id.bytes,
                &EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_CHUNK_COUNT_KEY,
            ) {
                let mut chunks = Vec::new();
                for chunk_index in 0..chunk_count {
                    let chunk_key = ExecutionTimeObservationChunkKey { chunk_index };
                    let Ok::<Vec<u8>, _>(chunk_bytes) = get_dynamic_field_from_store(
                        object_store,
                        system_state.extra_fields.id.id.bytes,
                        &chunk_key,
                    ) else {
                        debug_fatal!(
                            "Could not find stored execution time observation chunk {}",
                            chunk_index
                        );
                        return itertools::Either::Left(std::iter::empty());
                    };

                    // This is stored as a vector<u8> in Move, so we double-deserialize to get back
                    // the observation chunk.
                    let chunk: StoredExecutionTimeObservations = bcs::from_bytes(&chunk_bytes)
                        .expect("failed to deserialize stored execution time estimates chunk");
                    chunks.push(chunk);
                }

                StoredExecutionTimeObservations::merge_sorted_chunks(chunks).unwrap_v1()
            } else {
                warn!(
                    "Could not read stored execution time chunk count. This should only happen in the first epoch where chunking is enabled."
                );
                return itertools::Either::Left(std::iter::empty());
            }
        } else {
            // TODO: Remove this once we've enabled chunking on mainnet.
            let Ok::<Vec<u8>, _>(stored_observations_bytes) = get_dynamic_field_from_store(
                object_store,
                system_state.extra_fields.id.id.bytes,
                &EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_KEY,
            ) else {
                warn!(
                    "Could not find stored execution time observations. This should only happen in the first epoch where ExecutionTimeEstimate mode is enabled."
                );
                return itertools::Either::Left(std::iter::empty());
            };
            // This is stored as a vector<u8> in Move, so we double-deserialize to get back
            //`StoredExecutionTimeObservations`.
            let stored_observations: StoredExecutionTimeObservations =
                bcs::from_bytes(&stored_observations_bytes)
                    .expect("failed to deserialize stored execution time estimates");
            stored_observations.unwrap_v1()
        };

        info!(
            "loaded stored execution time observations for {} keys",
            stored_observations.len()
        );
        metrics
            .epoch_execution_time_observations_loaded
            .set(stored_observations.len() as i64);
        assert_reachable!("successfully loads stored execution time observations");

        // Make a single flattened iterator with every stored observation, for consumption
        // by the `ExecutionTimeEstimator` constructor.
        itertools::Either::Right(stored_observations.into_iter().flat_map(
            move |(key, observations)| {
                let committee = committee.clone();
                observations
                    .into_iter()
                    .filter_map(move |(authority, duration)| {
                        committee
                            .authority_index(&authority)
                            .map(|authority_index| {
                                (
                                    authority_index,
                                    // For bug compatibility with previous version, can be
                                    // removed once set to true on mainnet.
                                    if use_none_generation { None } else { Some(0) },
                                    key.clone(),
                                    duration,
                                )
                            })
                    })
            },
        ))
    }

    pub fn get_end_of_epoch_execution_time_observations(&self) -> &StoredExecutionTimeObservations {
        self.end_of_epoch_execution_time_observations.get().expect(
            "`get_end_of_epoch_execution_time_observations` must not be called until end of epoch",
        )
    }

    pub fn acquire_tx_guard(&self, cert: &VerifiedExecutableTransaction) -> CertTxGuard {
        let digest = cert.digest();
        CertTxGuard(self.acquire_tx_lock(digest))
    }

    /// Acquire the lock for a tx without writing to the WAL.
    pub fn acquire_tx_lock(&self, digest: &TransactionDigest) -> CertLockGuard {
        CertLockGuard(self.mutex_table.acquire_lock(*digest))
    }

    pub fn store_reconfig_state(&self, new_state: &ReconfigState) -> SuiResult {
        self.tables()?
            .reconfig_state
            .insert(&RECONFIG_STATE_INDEX, new_state)?;
        Ok(())
    }

    pub fn insert_signed_transaction(&self, transaction: VerifiedSignedTransaction) -> SuiResult {
        Ok(self
            .tables()?
            .signed_transactions
            .insert(transaction.digest(), transaction.serializable_ref())?)
    }

    #[cfg(test)]
    pub fn delete_signed_transaction_for_test(&self, transaction: &TransactionDigest) {
        self.tables()
            .expect("test should not cross epoch boundary")
            .signed_transactions
            .remove(transaction)
            .unwrap();
    }

    #[cfg(test)]
    pub fn delete_object_locks_for_test(&self, objects: &[ObjectRef]) {
        for object in objects {
            self.tables()
                .expect("test should not cross epoch boundary")
                .owned_object_locked_transactions
                .remove(object)
                .unwrap();
        }
    }

    pub fn get_signed_transaction(
        &self,
        tx_digest: &TransactionDigest,
    ) -> SuiResult<Option<VerifiedSignedTransaction>> {
        Ok(self
            .tables()?
            .signed_transactions
            .get(tx_digest)?
            .map(|t| t.into()))
    }

    #[instrument(level = "trace", skip_all)]
    pub fn insert_tx_cert_sig(
        &self,
        tx_digest: &TransactionDigest,
        cert_sig: &AuthorityStrongQuorumSignInfo,
    ) -> SuiResult {
        let tables = self.tables()?;
        Ok(tables
            .transaction_cert_signatures
            .insert(tx_digest, cert_sig)?)
    }

    /// Record that a transaction has been executed in the current epoch.
    /// Used by checkpoint builder to cull dependencies from previous epochs.
    #[instrument(level = "trace", skip_all)]
    pub fn insert_executed_in_epoch(&self, tx_digest: &TransactionDigest) {
        self.consensus_output_cache
            .insert_executed_in_epoch(*tx_digest);
    }

    /// Record a mapping from a transaction key (such as TransactionKey::RandomRound) to its digest.
    pub(crate) fn insert_tx_key(
        &self,
        tx_key: TransactionKey,
        tx_digest: TransactionDigest,
    ) -> SuiResult {
        let _metrics_scope =
            mysten_metrics::monitored_scope("AuthorityPerEpochStore::insert_tx_key");

        if matches!(tx_key, TransactionKey::Digest(_)) {
            debug_fatal!("useless to insert a digest key");
            return Ok(());
        }

        let tables = self.tables()?;
        tables
            .transaction_key_to_digest
            .insert(&tx_key, &tx_digest)?;
        self.executed_digests_notify_read
            .notify(&tx_key, &tx_digest);
        Ok(())
    }

    pub fn tx_key_to_digest(&self, key: &TransactionKey) -> SuiResult<Option<TransactionDigest>> {
        let tables = self.tables()?;
        if let TransactionKey::Digest(digest) = key {
            Ok(Some(*digest))
        } else {
            Ok(tables.transaction_key_to_digest.get(key).expect("db error"))
        }
    }

    pub(crate) fn notify_settlement_transactions_ready(
        &self,
        tx_key: TransactionKey,
        txns: Vec<VerifiedExecutableTransaction>,
    ) {
        debug_assert!(matches!(tx_key, TransactionKey::AccumulatorSettlement(..)));
        let mut registrations = self.settlement_registrations.lock();
        if let Some(registration) = registrations.remove(&tx_key) {
            let SettlementRegistration::Waiting(tx) = registration else {
                fatal!("Settlement registration should be waiting");
            };
            tx.send(txns).unwrap();
        } else {
            registrations.insert(tx_key, SettlementRegistration::Ready(txns));
        }
    }

    pub(crate) async fn wait_for_settlement_transactions(
        &self,
        key: TransactionKey,
    ) -> Vec<VerifiedExecutableTransaction> {
        let rx = {
            let mut registrations = self.settlement_registrations.lock();
            if let Some(registration) = registrations.remove(&key) {
                let SettlementRegistration::Ready(txns) = registration else {
                    fatal!("Settlement registration should be ready");
                };
                return txns;
            } else {
                let (tx, rx) = oneshot::channel();
                registrations.insert(key, SettlementRegistration::Waiting(tx));
                rx
            }
        };

        rx.await.unwrap()
    }

    pub fn insert_effects_digest_and_signature(
        &self,
        tx_digest: &TransactionDigest,
        effects_digest: &TransactionEffectsDigest,
        effects_signature: &AuthoritySignInfo,
    ) -> SuiResult {
        let tables = self.tables()?;
        let mut batch = tables.effects_signatures.batch();
        batch.insert_batch(&tables.effects_signatures, [(tx_digest, effects_signature)])?;
        batch.insert_batch(
            &tables.signed_effects_digests,
            [(tx_digest, effects_digest)],
        )?;
        batch.write()?;
        Ok(())
    }

    pub fn transactions_executed_in_cur_epoch(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<bool>> {
        let tables = self.tables()?;
        Ok(do_fallback_lookup(
            digests,
            |digest| {
                if self
                    .consensus_output_cache
                    .executed_in_current_epoch(digest)
                {
                    CacheResult::Hit(true)
                } else {
                    CacheResult::Miss
                }
            },
            |digests| {
                tables
                    .executed_transactions_to_checkpoint
                    .multi_contains_keys(digests)
                    .expect("db error")
            },
        ))
    }

    pub fn get_effects_signature(
        &self,
        tx_digest: &TransactionDigest,
    ) -> SuiResult<Option<AuthoritySignInfo>> {
        let tables = self.tables()?;
        Ok(tables.effects_signatures.get(tx_digest)?)
    }

    pub fn get_signed_effects_digest(
        &self,
        tx_digest: &TransactionDigest,
    ) -> SuiResult<Option<TransactionEffectsDigest>> {
        let tables = self.tables()?;
        Ok(tables.signed_effects_digests.get(tx_digest)?)
    }

    pub fn get_transaction_cert_sig(
        &self,
        tx_digest: &TransactionDigest,
    ) -> SuiResult<Option<AuthorityStrongQuorumSignInfo>> {
        Ok(self.tables()?.transaction_cert_signatures.get(tx_digest)?)
    }

    /// Resolves InputObjectKinds into InputKeys. `assigned_versions` is used to map shared inputs
    /// to specific object versions.
    pub(crate) fn get_input_object_keys(
        &self,
        key: &TransactionKey,
        objects: &[InputObjectKind],
        assigned_versions: &AssignedVersions,
    ) -> BTreeSet<InputKey> {
        let assigned_shared_versions = assigned_versions
            .iter()
            .cloned()
            .collect::<BTreeMap<_, _>>();
        objects
            .iter()
            .map(|kind| {
                match kind {
                    InputObjectKind::SharedMoveObject {
                        id,
                        initial_shared_version,
                        ..
                    } => {
                        // If we found assigned versions, but they are missing the assignment for
                        // this object, it indicates a serious inconsistency!
                        let Some(version) = assigned_shared_versions.get(&(*id, *initial_shared_version)) else {
                            panic!(
                                "Shared object version should have been assigned. key: {key:?}, \
                                obj id: {id:?}, initial_shared_version: {initial_shared_version:?}, \
                                assigned_shared_versions: {assigned_shared_versions:?}",
                            )
                        };
                        InputKey::VersionedObject {
                            id: FullObjectID::new(*id, Some(*initial_shared_version)),
                            version: *version,
                        }
                    }
                    InputObjectKind::MovePackage(id) => InputKey::Package { id: *id },
                    InputObjectKind::ImmOrOwnedMoveObject(objref) => InputKey::VersionedObject {
                        id: FullObjectID::new(objref.0, None),
                        version: objref.1,
                    },
                }
            })
            .collect()
    }

    pub fn get_last_consensus_stats(&self) -> SuiResult<ExecutionIndicesWithStats> {
        assert!(
            self.consensus_quarantine.read().is_empty(),
            "get_last_consensus_stats should only be called at startup"
        );
        match self.tables()?.get_last_consensus_stats()? {
            Some(stats) => Ok(stats),
            None => {
                let indices = self
                    .tables()?
                    .get_last_consensus_index()
                    .map(|x| x.unwrap_or_default())?;
                Ok(ExecutionIndicesWithStats {
                    index: indices,
                    hash: 0, // unused
                    stats: ConsensusStats::default(),
                })
            }
        }
    }

    pub fn get_accumulators_in_checkpoint_range(
        &self,
        from_checkpoint: CheckpointSequenceNumber,
        to_checkpoint: CheckpointSequenceNumber,
    ) -> SuiResult<Vec<(CheckpointSequenceNumber, GlobalStateHash)>> {
        self.tables()?
            .state_hash_by_checkpoint
            .safe_range_iter(from_checkpoint..=to_checkpoint)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Returns future containing the state accumulator for the given epoch
    /// once available.
    pub async fn notify_read_checkpoint_state_hasher(
        &self,
        checkpoints: &[CheckpointSequenceNumber],
    ) -> SuiResult<Vec<GlobalStateHash>> {
        let tables = self.tables()?;
        Ok(self
            .checkpoint_state_notify_read
            .read(
                "notify_read_checkpoint_state_hasher",
                checkpoints,
                |checkpoints| {
                    tables
                        .state_hash_by_checkpoint
                        .multi_get(checkpoints)
                        .expect("db error")
                },
            )
            .await)
    }

    pub async fn notify_read_running_root(
        &self,
        checkpoint: CheckpointSequenceNumber,
    ) -> SuiResult<GlobalStateHash> {
        let registration = self.running_root_notify_read.register_one(&checkpoint);
        let acc = self.tables()?.running_root_state_hash.get(&checkpoint)?;

        let result = match acc {
            Some(ready) => Either::Left(futures::future::ready(ready)),
            None => Either::Right(registration),
        }
        .await;

        Ok(result)
    }

    /// Called when transaction outputs are committed to disk
    #[instrument(level = "trace", skip_all)]
    pub fn handle_finalized_checkpoint(
        &self,
        checkpoint: &CheckpointSummary,
        digests: &[TransactionDigest],
    ) -> SuiResult<()> {
        let tables = match self.tables() {
            Ok(tables) => tables,
            // After Epoch ends, it is no longer necessary to remove pending transactions
            // because the table will not be used anymore and be deleted eventually.
            Err(e) if matches!(e.as_inner(), SuiErrorKind::EpochEnded(_)) => return Ok(()),
            Err(e) => return Err(e),
        };
        let mut batch = tables.signed_effects_digests.batch();

        // Now that the transaction effects are committed, we will never re-execute, so we
        // don't need to worry about equivocating.
        batch.delete_batch(&tables.signed_effects_digests, digests)?;

        let seq = *checkpoint.sequence_number();

        let mut quarantine = self.consensus_quarantine.write();
        quarantine.update_highest_executed_checkpoint(seq, self, &mut batch)?;
        batch.write()?;

        self.consensus_output_cache
            .remove_executed_in_epoch(digests);

        Ok(())
    }

    pub fn get_all_pending_consensus_transactions(&self) -> Vec<ConsensusTransaction> {
        self.tables()
            .expect("recovery should not cross epoch boundary")
            .get_all_pending_consensus_transactions()
            .expect("failed to get pending consensus transactions")
    }

    #[cfg(test)]
    pub fn get_next_object_version(
        &self,
        obj: &ObjectID,
        start_version: SequenceNumber,
    ) -> Option<SequenceNumber> {
        self.tables()
            .expect("test should not cross epoch boundary")
            .next_shared_object_versions_v2
            .get(&(*obj, start_version))
            .unwrap()
    }

    pub fn insert_finalized_transactions(
        &self,
        digests: &[TransactionDigest],
        sequence: CheckpointSequenceNumber,
    ) -> SuiResult {
        let _metrics_scope = mysten_metrics::monitored_scope(
            "AuthorityPerEpochStore::insert_finalized_transactions",
        );

        let mut batch = self.tables()?.executed_transactions_to_checkpoint.batch();
        batch.insert_batch(
            &self.tables()?.executed_transactions_to_checkpoint,
            digests.iter().map(|d| (*d, sequence)),
        )?;
        batch.write()?;
        trace!("Transactions {digests:?} finalized at checkpoint {sequence}");

        // Notify all readers that the transactions have been finalized as part of a checkpoint execution.
        for digest in digests {
            self.executed_transactions_to_checkpoint_notify_read
                .notify(digest, &sequence);
        }

        Ok(())
    }

    pub fn is_transaction_executed_in_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<bool> {
        Ok(self
            .tables()?
            .executed_transactions_to_checkpoint
            .contains_key(digest)?)
    }

    pub fn transactions_executed_in_checkpoint(
        &self,
        digests: impl Iterator<Item = TransactionDigest>,
    ) -> SuiResult<Vec<bool>> {
        Ok(self
            .tables()?
            .executed_transactions_to_checkpoint
            .multi_contains_keys(digests)?)
    }

    pub fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<Option<CheckpointSequenceNumber>> {
        Ok(self
            .tables()?
            .executed_transactions_to_checkpoint
            .get(digest)?)
    }

    pub fn multi_get_transaction_checkpoint(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<CheckpointSequenceNumber>>> {
        Ok(self
            .tables()?
            .executed_transactions_to_checkpoint
            .multi_get(digests)?
            .into_iter()
            .collect())
    }

    // For each key in objects_to_init, return the next version for that key as recorded in the
    // next_shared_object_versions table.
    //
    // If any keys are missing, then we need to initialize the table. We first check if a previous
    // version of that object has been written. If so, then the object was written in a previous
    // epoch, and we initialize next_shared_object_versions to that value. If no version of the
    // object has yet been written, we initialize the object to the initial version recorded in the
    // certificate (which is a function of the lamport version computation of the transaction that
    // created the shared object originally - which transaction may not yet have been executed on
    // this node).
    //
    // Because all paths that assign shared versions for a shared object transaction call this
    // function, it is impossible for parent_sync to be updated before this function completes
    // successfully for each affected object id.
    pub(crate) fn get_or_init_next_object_versions(
        &self,
        objects_to_init: &[ConsensusObjectSequenceKey],
        cache_reader: &dyn ObjectCacheRead,
    ) -> SuiResult<HashMap<ConsensusObjectSequenceKey, SequenceNumber>> {
        // get_or_init_next_object_versions can be called
        // from consensus or checkpoint executor,
        // so we need to protect version assignment with a critical section
        let _locks = self
            .version_assignment_mutex_table
            .acquire_locks(objects_to_init.iter().map(|(id, _)| *id));
        let tables = self.tables()?;

        let next_versions = self
            .consensus_quarantine
            .read()
            .get_next_shared_object_versions(&tables, objects_to_init)?;

        let uninitialized_objects: Vec<ConsensusObjectSequenceKey> = next_versions
            .iter()
            .zip(objects_to_init)
            .filter_map(|(next_version, id_and_version)| match next_version {
                None => Some(*id_and_version),
                Some(_) => None,
            })
            .collect();

        // The common case is that there are no uninitialized versions - this early return will
        // happen every time except the first time an object is used in an epoch.
        if uninitialized_objects.is_empty() {
            // unwrap ok - we already verified that next_versions is not missing any keys.
            return Ok(izip!(
                objects_to_init.iter().cloned(),
                next_versions.into_iter().map(|v| v.unwrap())
            )
            .collect());
        }

        let versions_to_write: Vec<_> = uninitialized_objects
            .iter()
            .map(|(id, initial_version)| {
                // Note: we don't actually need to read from the transaction here, as no writer
                // can update object_store until after get_or_init_next_object_versions
                // completes.
                match cache_reader.get_object(id) {
                    Some(obj) => {
                        if obj.owner().start_version() == Some(*initial_version) {
                            ((*id, *initial_version), obj.version())
                        } else {
                            // If we can't find a matching start version, treat the object as
                            // if it's absent.
                            if self.protocol_config().reshare_at_same_initial_version()
                                && let Some(obj_start_version) = obj.owner().start_version() {
                                    assert!(*initial_version >= obj_start_version,
                                            "should be impossible to certify a transaction with a start version that must have only existed in a previous epoch; obj = {obj:?} initial_version = {initial_version:?}, obj_start_version = {obj_start_version:?}");
                                }
                            ((*id, *initial_version), *initial_version)
                        }
                    }
                    None => ((*id, *initial_version), *initial_version),
                }
            })
            .collect();

        let ret = izip!(objects_to_init.iter().cloned(), next_versions.into_iter(),)
            // take all the previously initialized versions
            .filter_map(|(key, next_version)| next_version.map(|v| (key, v)))
            // add all the versions we're going to write
            .chain(versions_to_write.iter().cloned())
            .collect();

        debug!(
            ?versions_to_write,
            "initializing next_shared_object_versions"
        );
        tables
            .next_shared_object_versions_v2
            .multi_insert(versions_to_write)?;

        Ok(ret)
    }

    /// Given list of certificates, assign versions for all shared objects used in them.
    /// We start with the current next_shared_object_versions table for each object, and build
    /// up the versions based on the dependencies of each certificate.
    /// However, in the end we do not update the next_shared_object_versions table, which keeps
    /// this function idempotent. We should call this function when we are assigning shared object
    /// versions outside of consensus and do not want to taint the next_shared_object_versions table.
    pub fn assign_shared_object_versions_idempotent<'a>(
        &self,
        cache_reader: &dyn ObjectCacheRead,
        assignables: impl Iterator<Item = &'a Schedulable<&'a VerifiedExecutableTransaction>> + Clone,
    ) -> SuiResult<AssignedTxAndVersions> {
        Ok(SharedObjVerManager::assign_versions_from_consensus(
            self,
            cache_reader,
            assignables,
            &BTreeMap::new(),
        )?
        .assigned_versions)
    }

    pub(crate) fn load_deferred_transactions_for_randomness_v2(
        &self,
        output: &mut ConsensusCommitOutput,
    ) -> SuiResult<Vec<(DeferralKey, Vec<VerifiedExecutableTransaction>)>> {
        let (min, max) = DeferralKey::full_range_for_randomness();
        self.load_deferred_transactions_v2(output, min, max)
    }

    pub(crate) fn load_deferred_transactions_for_up_to_consensus_round_v2(
        &self,
        output: &mut ConsensusCommitOutput,
        consensus_round: u64,
    ) -> SuiResult<Vec<(DeferralKey, Vec<VerifiedExecutableTransaction>)>> {
        let (min, max) = DeferralKey::range_for_up_to_consensus_round(consensus_round);
        self.load_deferred_transactions_v2(output, min, max)
    }

    // factoring of the above
    fn load_deferred_transactions_v2(
        &self,
        output: &mut ConsensusCommitOutput,
        min: DeferralKey,
        max: DeferralKey,
    ) -> SuiResult<Vec<(DeferralKey, Vec<VerifiedExecutableTransaction>)>> {
        debug!("Query epoch store to load deferred txn {:?} {:?}", min, max);

        let (keys, txns) = {
            let mut keys = Vec::new();
            let mut txns = Vec::new();

            let deferred_transactions = self.consensus_output_cache.deferred_transactions_v2.lock();

            for (key, transactions) in deferred_transactions.range(min..max) {
                debug!(
                    "Loaded {:?} deferred txn with deferral key {:?}",
                    transactions.len(),
                    key
                );
                keys.push(*key);
                txns.push((*key, transactions.clone()));
            }

            (keys, txns)
        };

        // verify that there are no duplicates - should be impossible due to
        // is_consensus_message_processed
        #[cfg(debug_assertions)]
        {
            let mut seen = HashSet::new();
            for deferred_txn_batch in &txns {
                for txn in &deferred_txn_batch.1 {
                    assert!(seen.insert(txn.digest()));
                }
            }
        }

        output.delete_loaded_deferred_transactions(&keys);

        Ok(txns)
    }

    pub fn get_all_deferred_transactions_for_test(
        &self,
    ) -> Vec<(DeferralKey, Vec<VerifiedExecutableTransaction>)> {
        self.consensus_output_cache
            .deferred_transactions_v2
            .lock()
            .iter()
            .map(|(key, txs)| (*key, txs.clone()))
            .collect()
    }

    pub(crate) fn should_defer(
        &self,
        tx_cost: Option<u64>,
        cert: &VerifiedExecutableTransaction,
        commit_info: &ConsensusCommitInfo,
        dkg_failed: bool,
        generating_randomness: bool,
        previously_deferred_tx_digests: &HashMap<TransactionDigest, DeferralKey>,
        shared_object_congestion_tracker: &SharedObjectCongestionTracker,
    ) -> Option<(DeferralKey, DeferralReason)> {
        // Defer transaction if it uses randomness but we aren't generating any this round.
        // Don't defer if DKG has permanently failed; in that case we need to ignore.
        if !dkg_failed
            && !generating_randomness
            && self.randomness_state_enabled()
            && cert.transaction_data().uses_randomness()
        {
            // Propagate original deferred_from_round when re-deferring
            let deferred_from_round = previously_deferred_tx_digests
                .get(cert.digest())
                .map(|previous_key| previous_key.deferred_from_round())
                .unwrap_or(commit_info.round);
            return Some((
                DeferralKey::new_for_randomness(deferred_from_round),
                DeferralReason::RandomnessNotReady,
            ));
        }

        // Defer transaction if it uses shared objects that are congested.
        if let Some((deferral_key, congested_objects)) = shared_object_congestion_tracker
            .should_defer_due_to_object_congestion(
                tx_cost,
                cert,
                previously_deferred_tx_digests,
                commit_info,
            )
        {
            Some((
                deferral_key,
                DeferralReason::SharedObjectCongestion(congested_objects),
            ))
        } else {
            None
        }
    }

    /// Assign a sequence number for the shared objects of the input transaction based on the
    /// effects of that transaction.
    /// Used by full nodes who don't listen to consensus, and validators who catch up by state sync.
    // TODO: We should be able to pass in a vector of certs/effects and acquire them all at once.
    #[instrument(level = "trace", skip_all)]
    pub fn acquire_shared_version_assignments_from_effects(
        &self,
        certificate: &VerifiedExecutableTransaction,
        effects: &TransactionEffects,
        accumulator_version: Option<SequenceNumber>,
        cache_reader: &dyn ObjectCacheRead,
    ) -> SuiResult<AssignedVersions> {
        let assigned_versions = SharedObjVerManager::assign_versions_from_effects(
            &[(certificate, effects, accumulator_version)],
            self,
            cache_reader,
        );
        let (_, assigned_versions) = assigned_versions.0.into_iter().next().unwrap();
        Ok(assigned_versions)
    }

    /// When submitting a certificate caller **must** provide a ReconfigState lock guard
    /// and verify that it allows new user certificates
    pub fn insert_pending_consensus_transactions(
        &self,
        transactions: &[ConsensusTransaction],
        lock: Option<&RwLockReadGuard<ReconfigState>>,
    ) -> SuiResult {
        let key_value_pairs = transactions.iter().filter_map(|tx| {
            if tx.is_mfp_transaction() {
                // UserTransaction does not need to be resubmitted on recovery.
                None
            } else {
                debug!("Inserting pending consensus transaction: {:?}", tx.key());
                Some((tx.key(), tx))
            }
        });
        self.tables()?
            .pending_consensus_transactions
            .multi_insert(key_value_pairs)?;

        let digests: Vec<_> = transactions
            .iter()
            .filter_map(|tx| match &tx.kind {
                ConsensusTransactionKind::CertifiedTransaction(cert) => Some(cert.digest()),
                _ => None,
            })
            .collect();
        if !digests.is_empty() {
            let state = lock.expect("Must pass reconfiguration lock when storing certificate");
            // Caller is responsible for performing graceful check
            assert!(
                state.should_accept_user_certs(),
                "Reconfiguration state should allow accepting user transactions"
            );
            let mut pending_consensus_certificates = self.pending_consensus_certificates.write();
            pending_consensus_certificates.extend(digests);
        }

        Ok(())
    }

    pub fn remove_pending_consensus_transactions(
        &self,
        keys: &[ConsensusTransactionKey],
    ) -> SuiResult {
        debug!("Removing pending consensus transactions: {:?}", keys);
        self.tables()?
            .pending_consensus_transactions
            .multi_remove(keys)?;
        let mut pending_consensus_certificates = self.pending_consensus_certificates.write();
        for key in keys {
            if let ConsensusTransactionKey::Certificate(digest) = key {
                pending_consensus_certificates.remove(digest);
            }
        }
        Ok(())
    }

    pub fn pending_consensus_certificates_count(&self) -> usize {
        self.pending_consensus_certificates.read().len()
    }

    pub fn pending_consensus_certificates_empty(&self) -> bool {
        self.pending_consensus_certificates.read().is_empty()
    }

    pub fn pending_consensus_certificates(&self) -> HashSet<TransactionDigest> {
        self.pending_consensus_certificates.read().clone()
    }

    pub fn is_pending_consensus_certificate(&self, tx_digest: &TransactionDigest) -> bool {
        self.pending_consensus_certificates
            .read()
            .contains(tx_digest)
    }

    pub fn deferred_transactions_empty_v2(&self) -> bool {
        self.consensus_output_cache
            .deferred_transactions_v2
            .lock()
            .is_empty()
    }

    /// Check whether any certificates were processed by consensus.
    /// This handles multiple certificates at once.
    pub fn is_any_tx_certs_consensus_message_processed<'a>(
        &self,
        certificates: impl Iterator<Item = &'a CertifiedTransaction>,
    ) -> SuiResult<bool> {
        let keys = certificates.map(|cert| {
            SequencedConsensusTransactionKey::External(ConsensusTransactionKey::Certificate(
                *cert.digest(),
            ))
        });
        Ok(self
            .check_consensus_messages_processed(keys)?
            .into_iter()
            .any(|processed| processed))
    }

    /// Returns true if all messages with the given keys were processed by consensus.
    pub fn all_external_consensus_messages_processed(
        &self,
        keys: impl Iterator<Item = ConsensusTransactionKey>,
    ) -> SuiResult<bool> {
        let keys = keys.map(SequencedConsensusTransactionKey::External);
        Ok(self
            .check_consensus_messages_processed(keys)?
            .into_iter()
            .all(|processed| processed))
    }

    pub fn is_consensus_message_processed(
        &self,
        key: &SequencedConsensusTransactionKey,
    ) -> SuiResult<bool> {
        Ok(self
            .consensus_quarantine
            .read()
            .is_consensus_message_processed(key)
            || self
                .tables()?
                .consensus_message_processed
                .contains_key(key)?)
    }

    pub fn check_consensus_messages_processed(
        &self,
        keys: impl Iterator<Item = SequencedConsensusTransactionKey>,
    ) -> SuiResult<Vec<bool>> {
        let keys = keys.collect::<Vec<_>>();

        let consensus_quarantine = self.consensus_quarantine.read();
        let tables = self.tables()?;

        Ok(do_fallback_lookup(
            &keys,
            |key| {
                if consensus_quarantine.is_consensus_message_processed(key) {
                    CacheResult::Hit(true)
                } else {
                    CacheResult::Miss
                }
            },
            |keys| {
                tables
                    .consensus_message_processed
                    .multi_contains_keys(keys)
                    .expect("db error")
            },
        ))
    }

    pub async fn consensus_messages_processed_notify(
        &self,
        keys: Vec<SequencedConsensusTransactionKey>,
    ) -> Result<(), SuiError> {
        let registrations = self.consensus_notify_read.register_all(&keys);

        let unprocessed_keys_registrations = registrations
            .into_iter()
            .zip(self.check_consensus_messages_processed(keys.into_iter())?)
            .filter(|(_, processed)| !processed)
            .map(|(registration, _)| registration);

        join_all(unprocessed_keys_registrations).await;
        Ok(())
    }

    /// Get notified when transactions get executed as part of a checkpoint execution.
    pub async fn transactions_executed_in_checkpoint_notify(
        &self,
        digests: Vec<TransactionDigest>,
    ) -> Result<Vec<CheckpointSequenceNumber>, SuiError> {
        let tables = self.tables()?;

        Ok(self
            .executed_transactions_to_checkpoint_notify_read
            .read(
                "transactions_executed_in_checkpoint_notify",
                &digests,
                |digests| {
                    tables
                        .executed_transactions_to_checkpoint
                        .multi_get(digests)
                        .expect("db error")
                },
            )
            .await)
    }

    pub fn has_received_end_of_publish_from(&self, authority: &AuthorityName) -> bool {
        self.end_of_publish
            .try_lock()
            .expect("No contention on end_of_publish lock")
            .contains_key(authority)
    }

    // Converts transaction keys to digests, waiting for digests to become available for any
    // non-digest keys.
    pub async fn notify_read_tx_key_to_digest(
        &self,
        keys: &[TransactionKey],
    ) -> SuiResult<Vec<TransactionDigest>> {
        let non_digest_keys: Vec<_> = keys
            .iter()
            .filter_map(|key| {
                if matches!(key, TransactionKey::Digest(_)) {
                    None
                } else {
                    Some(*key)
                }
            })
            .collect();

        let registrations = self
            .executed_digests_notify_read
            .register_all(&non_digest_keys);
        let executed_digests = self
            .tables()?
            .transaction_key_to_digest
            .multi_get(&non_digest_keys)?;
        let futures = executed_digests
            .into_iter()
            .zip(registrations)
            .map(|(d, r)| match d {
                // Note that Some() clause also drops registration that is already fulfilled
                Some(ready) => Either::Left(futures::future::ready(ready)),
                None => Either::Right(r),
            });
        let mut results = VecDeque::from(join_all(futures).await);

        Ok(keys
            .iter()
            .map(|key| {
                if let TransactionKey::Digest(digest) = key {
                    *digest
                } else {
                    results
                        .pop_front()
                        .expect("number of returned results should match number of non-digest keys")
                }
            })
            .collect())
    }

    /// Caller must call consensus_message_processed_notify before calling this to ensure that all
    /// user signatures are available.
    pub fn user_signatures_for_checkpoint(
        &self,
        transactions: &[VerifiedTransaction],
        digests: &[TransactionDigest],
    ) -> Vec<Vec<GenericSignature>> {
        assert_eq!(transactions.len(), digests.len());

        fn is_signature_expected(transaction: &VerifiedTransaction) -> bool {
            !matches!(
                transaction.inner().transaction_data().kind(),
                TransactionKind::RandomnessStateUpdate(_)
                    | TransactionKind::ProgrammableSystemTransaction(_)
            )
        }

        let result: Vec<_> = {
            let mut user_sigs = self
                .consensus_output_cache
                .user_signatures_for_checkpoints
                .lock();
            digests
                .iter()
                .zip(transactions.iter())
                .map(|(d, t)| {
                    // Some transactions (RandomnessStateUpdate and settlement transactions) don't go through
                    // consensus, but have system-generated signatures that are guaranteed to be the same,
                    // so we can just pull them from the transaction.
                    if is_signature_expected(t) {
                        // Expect is safe as long as consensus_message_processed_notify is called
                        // before this call, to ensure that all canonical user signatures are
                        // available.
                        user_sigs.remove(d).expect("signature should be available")
                    } else {
                        t.tx_signatures().to_vec()
                    }
                })
                .collect()
        };

        result
    }

    pub fn clear_override_protocol_upgrade_buffer_stake(&self) -> SuiResult {
        warn!(
            epoch = ?self.epoch(),
            "clearing buffer_stake_for_protocol_upgrade_bps override"
        );
        self.tables()?
            .override_protocol_upgrade_buffer_stake
            .remove(&OVERRIDE_PROTOCOL_UPGRADE_BUFFER_STAKE_INDEX)?;
        self.update_buffer_stake_metric();
        Ok(())
    }

    pub fn set_override_protocol_upgrade_buffer_stake(&self, new_stake_bps: u64) -> SuiResult {
        warn!(
            ?new_stake_bps,
            epoch = ?self.epoch(),
            "storing buffer_stake_for_protocol_upgrade_bps override"
        );
        self.tables()?
            .override_protocol_upgrade_buffer_stake
            .insert(
                &OVERRIDE_PROTOCOL_UPGRADE_BUFFER_STAKE_INDEX,
                &new_stake_bps,
            )?;
        self.update_buffer_stake_metric();
        Ok(())
    }

    fn update_buffer_stake_metric(&self) {
        self.metrics
            .effective_buffer_stake
            .set(self.get_effective_buffer_stake_bps() as i64);
    }

    pub fn get_effective_buffer_stake_bps(&self) -> u64 {
        self.tables()
            .expect("epoch initialization should have finished")
            .override_protocol_upgrade_buffer_stake
            .get(&OVERRIDE_PROTOCOL_UPGRADE_BUFFER_STAKE_INDEX)
            .expect("force_protocol_upgrade read cannot fail")
            .tap_some(|b| warn!("using overridden buffer stake value of {}", b))
            .unwrap_or_else(|| {
                self.protocol_config()
                    .buffer_stake_for_protocol_upgrade_bps()
            })
    }

    /// Record most recently advertised capabilities of all authorities
    pub fn record_capabilities(&self, capabilities: &AuthorityCapabilitiesV1) -> SuiResult {
        info!("received capabilities {:?}", capabilities);
        let authority = &capabilities.authority;
        let tables = self.tables()?;

        // Read-compare-write pattern assumes we are only called from the consensus handler task.
        if let Some(cap) = tables.authority_capabilities.get(authority)?
            && cap.generation >= capabilities.generation
        {
            debug!(
                "ignoring new capabilities {:?} in favor of previous capabilities {:?}",
                capabilities, cap
            );
            return Ok(());
        }
        tables
            .authority_capabilities
            .insert(authority, capabilities)?;
        Ok(())
    }

    /// Record most recently advertised capabilities of all authorities
    pub fn record_capabilities_v2(&self, capabilities: &AuthorityCapabilitiesV2) -> SuiResult {
        info!("received capabilities v2 {:?}", capabilities);
        let authority = &capabilities.authority;
        let tables = self.tables()?;

        // Read-compare-write pattern assumes we are only called from the consensus handler task.
        if let Some(cap) = tables.authority_capabilities_v2.get(authority)?
            && cap.generation >= capabilities.generation
        {
            debug!(
                "ignoring new capabilities {:?} in favor of previous capabilities {:?}",
                capabilities, cap
            );
            return Ok(());
        }
        tables
            .authority_capabilities_v2
            .insert(authority, capabilities)?;
        Ok(())
    }

    pub fn get_capabilities_v1(&self) -> SuiResult<Vec<AuthorityCapabilitiesV1>> {
        assert!(!self.protocol_config.authority_capabilities_v2());
        Ok(self
            .tables()?
            .authority_capabilities
            .safe_iter()
            .map(|item| item.map(|(_, v)| v))
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_capabilities_v2(&self) -> SuiResult<Vec<AuthorityCapabilitiesV2>> {
        assert!(self.protocol_config.authority_capabilities_v2());
        Ok(self
            .tables()?
            .authority_capabilities_v2
            .safe_iter()
            .map(|item| item.map(|(_, v)| v))
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub(crate) fn record_jwk_vote(
        &self,
        output: &mut ConsensusCommitOutput,
        round: u64,
        authority: AuthorityName,
        id: &JwkId,
        jwk: &JWK,
    ) {
        info!(
            ?round,
            "received jwk vote from {:?} for jwk ({:?}, {:?})",
            authority.concise(),
            id,
            jwk
        );

        if !self.authenticator_state_enabled() {
            info!(
                "ignoring vote because authenticator state object does exist yet
                (it will be created at the end of this epoch)"
            );
            return;
        }

        let mut jwk_aggregator = self.jwk_aggregator.lock();

        let votes = jwk_aggregator.votes_for_authority(authority);
        if votes
            >= self
                .protocol_config()
                .max_jwk_votes_per_validator_per_epoch()
        {
            warn!(
                "validator {:?} has already voted {} times this epoch, ignoring vote",
                authority, votes,
            );
            return;
        }

        output.insert_pending_jwk(authority, id.clone(), jwk.clone());

        let key = (id.clone(), jwk.clone());
        let previously_active = jwk_aggregator.has_quorum_for_key(&key);
        let insert_result = jwk_aggregator.insert(authority, key.clone());

        if !previously_active && insert_result.is_quorum_reached() {
            info!(epoch = ?self.epoch(), ?round, jwk = ?key, "jwk became active");
            output.insert_active_jwk(round, key);
        }
    }

    pub(crate) fn get_new_jwks(&self, round: u64) -> SuiResult<Vec<ActiveJwk>> {
        info!("Getting new jwks for round {:?}", round);
        self.consensus_quarantine.read().get_new_jwks(self, round)
    }

    pub fn jwk_active_in_current_epoch(&self, jwk_id: &JwkId, jwk: &JWK) -> bool {
        let jwk_aggregator = self.jwk_aggregator.lock();
        jwk_aggregator.has_quorum_for_key(&(jwk_id.clone(), jwk.clone()))
    }

    pub(crate) fn get_randomness_last_round_timestamp(&self) -> SuiResult<Option<TimestampMs>> {
        if let Some(ts) = self
            .consensus_quarantine
            .read()
            .get_randomness_last_round_timestamp()
        {
            Ok(Some(ts))
        } else {
            Ok(self
                .tables()?
                .randomness_last_round_timestamp
                .get(&SINGLETON_KEY)?)
        }
    }

    #[cfg(test)]
    pub fn test_insert_user_signature(
        &self,
        digest: TransactionDigest,
        signatures: Vec<GenericSignature>,
    ) {
        self.consensus_output_cache
            .user_signatures_for_checkpoints
            .lock()
            .insert(digest, signatures);
        let key = ConsensusTransactionKey::Certificate(digest);
        let key = SequencedConsensusTransactionKey::External(key);

        let mut output = ConsensusCommitOutput::default();
        output.record_consensus_message_processed(key.clone());
        output.set_default_commit_stats_for_testing();
        self.consensus_quarantine
            .write()
            .push_consensus_output(output, self)
            .expect("push_consensus_output should not fail");
        self.consensus_notify_read.notify(&key, &());
    }

    #[cfg(test)]
    pub(crate) fn push_consensus_output_for_tests(&self, output: ConsensusCommitOutput) {
        self.consensus_quarantine
            .write()
            .push_consensus_output(output, self)
            .expect("push_consensus_output should not fail");
    }

    pub(crate) fn process_user_signatures<'a>(
        &self,
        certificates: impl Iterator<Item = &'a Schedulable>,
    ) {
        let sigs: Vec<_> = certificates
            .filter_map(|s| match s {
                Schedulable::Transaction(certificate) => {
                    Some((*certificate.digest(), certificate.tx_signatures().to_vec()))
                }
                Schedulable::RandomnessStateUpdate(_, _) => None,
                Schedulable::AccumulatorSettlement(_, _) => None,
                Schedulable::ConsensusCommitPrologue(_, _, _) => None,
            })
            .collect();

        let mut user_sigs = self
            .consensus_output_cache
            .user_signatures_for_checkpoints
            .lock();

        user_sigs.reserve(sigs.len());
        for (digest, sigs) in sigs {
            // User signatures are written in the same batch as consensus certificate processed flag,
            // which means we won't attempt to insert this twice for the same tx digest
            assert!(
                user_sigs.insert(digest, sigs).is_none(),
                "duplicate user signatures for transaction digest: {:?}",
                digest
            );
        }
    }

    pub fn get_reconfig_state_read_lock_guard(&self) -> RwLockReadGuard<'_, ReconfigState> {
        self.reconfig_state_mem.read()
    }

    pub(crate) fn get_reconfig_state_write_lock_guard(
        &self,
    ) -> RwLockWriteGuard<'_, ReconfigState> {
        self.reconfig_state_mem.write()
    }

    pub fn close_user_certs(&self, mut lock_guard: RwLockWriteGuard<'_, ReconfigState>) {
        lock_guard.close_user_certs();
        self.store_reconfig_state(&lock_guard)
            .expect("Updating reconfig state cannot fail");

        // Set epoch_close_time for metric purpose.
        let mut epoch_close_time = self.epoch_close_time.write();
        if epoch_close_time.is_none() {
            // Only update it the first time epoch is closed.
            *epoch_close_time = Some(Instant::now());

            self.user_certs_closed_notify
                .notify()
                .expect("user_certs_closed_notify called twice on same epoch store");
        }
    }

    pub async fn user_certs_closed_notify(&self) {
        self.user_certs_closed_notify.wait().await
    }

    /// Notify epoch is terminated, can only be called once on epoch store
    pub async fn epoch_terminated(&self) {
        // Notify interested tasks that epoch has ended
        self.epoch_alive_notify
            .notify()
            .expect("epoch_terminated called twice on same epoch store");
        // This `write` acts as a barrier - it waits for futures executing in
        // `within_alive_epoch` to terminate before we can continue here
        info!("Epoch terminated - waiting for pending tasks to complete");
        *self.epoch_alive.write().await = false;
        info!("All pending epoch tasks completed");
    }

    /// Waits for the notification about epoch termination
    pub async fn wait_epoch_terminated(&self) {
        self.epoch_alive_notify.wait().await
    }

    /// This function executes given future until epoch_terminated is called
    /// If future finishes before epoch_terminated is called, future result is returned
    /// If epoch_terminated is called before future is resolved, error is returned
    ///
    /// In addition to the early termination guarantee, this function also prevents epoch_terminated()
    /// if future is being executed.
    #[allow(clippy::result_unit_err)]
    pub async fn within_alive_epoch<F: Future + Send>(&self, f: F) -> Result<F::Output, ()> {
        // This guard is kept in the future until it resolves, preventing `epoch_terminated` to
        // acquire a write lock
        let guard = self.epoch_alive.read().await;
        if !*guard {
            return Err(());
        }
        let terminated = self.wait_epoch_terminated().boxed();
        let f = f.boxed();
        match select(terminated, f).await {
            Either::Left((_, _f)) => Err(()),
            Either::Right((result, _)) => Ok(result),
        }
    }

    #[instrument(level = "trace", skip_all)]
    pub fn verify_transaction(&self, tx: Transaction) -> SuiResult<VerifiedTransaction> {
        self.signature_verifier
            .verify_tx(tx.data())
            .map(|_| VerifiedTransaction::new_from_verified(tx))
    }

    /// Verifies transaction signatures and other data
    /// Important: This function can potentially be called in parallel and you can not rely on order of transactions to perform verification
    /// If this function return an error, transaction is skipped and is not passed to handle_consensus_transaction
    /// This function returns unit error and is responsible for emitting log messages for internal errors
    pub(crate) fn verify_consensus_transaction(
        &self,
        transaction: SequencedConsensusTransaction,
    ) -> Option<VerifiedSequencedConsensusTransaction> {
        let _scope = monitored_scope("VerifyConsensusTransaction");

        // Signatures are verified as part of the consensus payload verification in SuiTxValidator
        match &transaction.transaction {
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::CertifiedTransaction(_certificate),
                ..
            }) => {}
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::UserTransaction(_tx),
                ..
            }) => {}
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind:
                    ConsensusTransactionKind::CheckpointSignature(data)
                    | ConsensusTransactionKind::CheckpointSignatureV2(data),
                ..
            }) => {
                if transaction.sender_authority() != data.summary.auth_sig().authority {
                    warn!(
                        "CheckpointSignature authority {} does not match its author from consensus {}",
                        data.summary.auth_sig().authority,
                        transaction.certificate_author_index
                    );
                    return None;
                }
            }
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::EndOfPublish(authority),
                ..
            }) => {
                if &transaction.sender_authority() != authority {
                    warn!(
                        "EndOfPublish authority {} does not match its author from consensus {}",
                        authority, transaction.certificate_author_index
                    );
                    return None;
                }
            }
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind:
                    ConsensusTransactionKind::CapabilityNotification(AuthorityCapabilitiesV1 {
                        authority,
                        ..
                    }),
                ..
            })
            | SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind:
                    ConsensusTransactionKind::CapabilityNotificationV2(AuthorityCapabilitiesV2 {
                        authority,
                        ..
                    }),
                ..
            }) => {
                if transaction.sender_authority() != *authority {
                    warn!(
                        "CapabilityNotification authority {} does not match its author from consensus {}",
                        authority, transaction.certificate_author_index
                    );
                    return None;
                }
            }
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::NewJWKFetched(authority, id, jwk),
                ..
            }) => {
                if transaction.sender_authority() != *authority {
                    warn!(
                        "NewJWKFetched authority {} does not match its author from consensus {}",
                        authority, transaction.certificate_author_index,
                    );
                    return None;
                }
                if !check_total_jwk_size(id, jwk) {
                    warn!(
                        "{:?} sent jwk that exceeded max size",
                        transaction.sender_authority().concise()
                    );
                    return None;
                }
            }
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::RandomnessStateUpdate(_round, _bytes),
                ..
            }) => {}
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::RandomnessDkgMessage(authority, _bytes),
                ..
            }) => {
                if transaction.sender_authority() != *authority {
                    warn!(
                        "RandomnessDkgMessage authority {} does not match its author from consensus {}",
                        authority, transaction.certificate_author_index
                    );
                    return None;
                }
            }
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::RandomnessDkgConfirmation(authority, _bytes),
                ..
            }) => {
                if transaction.sender_authority() != *authority {
                    warn!(
                        "RandomnessDkgConfirmation authority {} does not match its author from consensus {}",
                        authority, transaction.certificate_author_index
                    );
                    return None;
                }
            }
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::ExecutionTimeObservation(msg),
                ..
            }) => {
                if transaction.sender_authority() != msg.authority {
                    warn!(
                        "ExecutionTimeObservation authority {} does not match its author from consensus {}",
                        msg.authority, transaction.certificate_author_index
                    );
                    return None;
                }
            }
            SequencedConsensusTransactionKind::System(_) => {}
        }
        Some(VerifiedSequencedConsensusTransaction(transaction))
    }

    fn db_batch(&self) -> SuiResult<DBBatch> {
        Ok(self.tables()?.last_consensus_stats.batch())
    }

    #[cfg(test)]
    pub fn db_batch_for_test(&self) -> DBBatch {
        self.db_batch()
            .expect("test should not be write past end of epoch")
    }

    pub(crate) fn calculate_pending_checkpoint_height(&self, consensus_round: u64) -> u64 {
        if self.randomness_state_enabled() {
            consensus_round * 2
        } else {
            consensus_round
        }
    }

    // Assigns shared object versions to transactions and updates the next shared object version state.
    // Shared object versions in cancelled transactions are assigned to special versions that will
    // cause the transactions to be cancelled in execution engine.
    pub(crate) fn process_consensus_transaction_shared_object_versions<'a>(
        &'a self,
        cache_reader: &dyn ObjectCacheRead,
        non_randomness_transactions: impl Iterator<Item = &'a Schedulable> + Clone,
        randomness_transactions: impl Iterator<Item = &'a Schedulable> + Clone,
        cancelled_txns: &BTreeMap<TransactionDigest, CancelConsensusCertificateReason>,
        output: &mut ConsensusCommitOutput,
    ) -> SuiResult<AssignedTxAndVersions> {
        let all_certs = non_randomness_transactions.chain(randomness_transactions);

        let ConsensusSharedObjVerAssignment {
            shared_input_next_versions,
            assigned_versions,
        } = SharedObjVerManager::assign_versions_from_consensus(
            self,
            cache_reader,
            all_certs,
            cancelled_txns,
        )?;
        debug!(
            "Assigned versions from consensus processing: {:?}",
            assigned_versions
        );

        output.set_next_shared_object_versions(shared_input_next_versions);
        Ok(assigned_versions)
    }

    pub fn get_highest_pending_checkpoint_height(&self) -> CheckpointHeight {
        self.consensus_quarantine
            .read()
            .get_highest_pending_checkpoint_height()
            .unwrap_or_default()
    }

    pub fn assign_shared_object_versions_for_tests(
        self: &Arc<Self>,
        cache_reader: &dyn ObjectCacheRead,
        transactions: &[VerifiedExecutableTransaction],
    ) -> SuiResult<AssignedTxAndVersions> {
        let mut output = ConsensusCommitOutput::new(0);
        let transactions: Vec<_> = transactions
            .iter()
            .cloned()
            .map(Schedulable::Transaction)
            .collect();

        // Record consensus messages as processed for each transaction
        for tx in transactions.iter() {
            if let Schedulable::Transaction(exec_tx) = tx {
                let key = SequencedConsensusTransactionKey::External(
                    ConsensusTransactionKey::Certificate(*exec_tx.digest()),
                );
                output.record_consensus_message_processed(key);
            }
        }

        let assigned_versions = self.process_consensus_transaction_shared_object_versions(
            cache_reader,
            transactions.iter(),
            std::iter::empty(),
            &BTreeMap::new(),
            &mut output,
        )?;
        let mut batch = self.db_batch()?;
        output.set_default_commit_stats_for_testing();
        output.write_to_batch(self, &mut batch)?;
        batch.write()?;
        Ok(assigned_versions)
    }

    pub(crate) fn process_notifications<'a>(
        &'a self,
        notifications: impl Iterator<Item = &'a SequencedConsensusTransactionKey>,
    ) {
        for key in notifications {
            self.consensus_notify_read.notify(key, &());
        }
    }

    /// If reconfig state is RejectUserCerts, and there is no fastpath transaction left to be
    /// finalized, send EndOfPublish to signal to other authorities that this authority is
    /// not voting for or executing more transactions in this epoch.
    pub(crate) fn should_send_end_of_publish(&self) -> bool {
        let reconfig_state = self.get_reconfig_state_read_lock_guard();
        if !reconfig_state.is_reject_user_certs() {
            // Still accepting user transactions, or already received 2f+1 EOP messages.
            // Either way EOP cannot or does not need to be sent.
            return false;
        }

        // EOP can only be sent after finalizing remaining transactions.
        self.pending_consensus_certificates_empty()
            && self
                .consensus_tx_status_cache
                .as_ref()
                .is_none_or(|c| c.get_num_fastpath_certified() == 0)
    }

    pub(crate) fn write_pending_checkpoint(
        &self,
        output: &mut ConsensusCommitOutput,
        checkpoint: &PendingCheckpoint,
    ) -> SuiResult {
        assert!(
            !self.pending_checkpoint_exists(&checkpoint.height())?,
            "Duplicate pending checkpoint notification at height {:?}",
            checkpoint.height()
        );

        debug!(
            checkpoint_commit_height = checkpoint.height(),
            "Pending checkpoint has {} roots",
            checkpoint.roots().len(),
        );
        trace!(
            checkpoint_commit_height = checkpoint.height(),
            "Transaction roots for pending checkpoint: {:?}",
            checkpoint.roots()
        );

        output.insert_pending_checkpoint(checkpoint.clone());

        Ok(())
    }

    pub fn get_pending_checkpoints(
        &self,
        last: Option<CheckpointHeight>,
    ) -> SuiResult<Vec<(CheckpointHeight, PendingCheckpoint)>> {
        Ok(self
            .consensus_quarantine
            .read()
            .get_pending_checkpoints(last))
    }

    pub fn pending_checkpoint_exists(&self, index: &CheckpointHeight) -> SuiResult<bool> {
        Ok(self
            .consensus_quarantine
            .read()
            .pending_checkpoint_exists(index))
    }

    pub fn process_constructed_checkpoint(
        &self,
        commit_height: CheckpointHeight,
        content_info: NonEmpty<(CheckpointSummary, CheckpointContents)>,
    ) {
        let mut consensus_quarantine = self.consensus_quarantine.write();
        for (position_in_commit, (summary, transactions)) in content_info.into_iter().enumerate() {
            let sequence_number = summary.sequence_number;
            let summary = BuilderCheckpointSummary {
                summary,
                checkpoint_height: Some(commit_height),
                position_in_commit,
            };

            consensus_quarantine.insert_builder_summary(sequence_number, summary, transactions);
        }

        // Because builder can run behind state sync, the data may be immediately ready to be committed.
        consensus_quarantine
            .commit(self)
            .expect("commit cannot fail");
    }

    /// Register genesis checkpoint in builder DB
    pub fn put_genesis_checkpoint_in_builder(
        &self,
        summary: &CheckpointSummary,
        contents: &CheckpointContents,
    ) -> SuiResult<()> {
        let sequence = summary.sequence_number;
        for transaction in contents.iter() {
            let digest = transaction.transaction;
            debug!(
                "Manually inserting genesis transaction in checkpoint DB: {:?}",
                digest
            );
            self.tables()?
                .builder_digest_to_checkpoint
                .insert(&digest, &sequence)?;
        }
        let builder_summary = BuilderCheckpointSummary {
            summary: summary.clone(),
            checkpoint_height: None,
            position_in_commit: 0,
        };
        self.tables()?
            .builder_checkpoint_summary_v2
            .insert(summary.sequence_number(), &builder_summary)?;
        Ok(())
    }

    pub fn last_built_checkpoint_builder_summary(
        &self,
    ) -> SuiResult<Option<BuilderCheckpointSummary>> {
        if let Some(summary) = self.consensus_quarantine.read().last_built_summary() {
            return Ok(Some(summary.clone()));
        }

        Ok(self
            .tables()?
            .builder_checkpoint_summary_v2
            .reversed_safe_iter_with_bounds(None, None)?
            .next()
            .transpose()?
            .map(|(_, s)| s))
    }

    pub fn last_built_checkpoint_summary(
        &self,
    ) -> SuiResult<Option<(CheckpointSequenceNumber, CheckpointSummary)>> {
        if let Some(BuilderCheckpointSummary { summary, .. }) =
            self.consensus_quarantine.read().last_built_summary()
        {
            let seq = *summary.sequence_number();
            debug!(
                "returning last_built_summary from consensus quarantine: {:?}",
                seq
            );
            Ok(Some((seq, summary.clone())))
        } else {
            let seq = self
                .tables()?
                .builder_checkpoint_summary_v2
                .reversed_safe_iter_with_bounds(None, None)?
                .next()
                .transpose()?
                .map(|(seq, s)| (seq, s.summary));
            debug!(
                "returning last_built_summary from builder_checkpoint_summary_v2: {:?}",
                seq
            );
            Ok(seq)
        }
    }

    pub fn get_built_checkpoint_summary(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> SuiResult<Option<CheckpointSummary>> {
        if let Some(BuilderCheckpointSummary { summary, .. }) =
            self.consensus_quarantine.read().get_built_summary(sequence)
        {
            return Ok(Some(summary.clone()));
        }

        Ok(self
            .tables()?
            .builder_checkpoint_summary_v2
            .get(&sequence)?
            .map(|s| s.summary))
    }

    pub(crate) fn get_lowest_non_genesis_checkpoint_summary(
        &self,
    ) -> SuiResult<Option<CheckpointSummary>> {
        for result in self
            .tables()?
            .builder_checkpoint_summary_v2
            .safe_iter_with_bounds(None, None)
        {
            let (seq, bcs) = result?;
            if seq > 0 {
                return Ok(Some(bcs.summary));
            }
        }
        Ok(None)
    }

    pub fn builder_included_transactions_in_checkpoint<'a>(
        &self,
        digests: impl Iterator<Item = &'a TransactionDigest>,
    ) -> SuiResult<Vec<bool>> {
        let digests: Vec<_> = digests.cloned().collect();
        let tables = self.tables()?;
        Ok(do_fallback_lookup(
            &digests,
            |digest| {
                let consensus_quarantine = self.consensus_quarantine.read();
                if consensus_quarantine.included_transaction_in_checkpoint(digest) {
                    CacheResult::Hit(true)
                } else {
                    CacheResult::Miss
                }
            },
            |remaining| {
                tables
                    .builder_digest_to_checkpoint
                    .multi_contains_keys(remaining)
                    .expect("db error")
            },
        ))
    }

    pub fn get_last_checkpoint_signature_index(&self) -> SuiResult<u64> {
        Ok(self
            .tables()?
            .pending_checkpoint_signatures
            .reversed_safe_iter_with_bounds(None, None)?
            .next()
            .transpose()?
            .map(|((_, index), _)| index)
            .unwrap_or_default())
    }

    pub fn insert_checkpoint_signature(
        &self,
        checkpoint_seq: CheckpointSequenceNumber,
        index: u64,
        info: &CheckpointSignatureMessage,
    ) -> SuiResult<()> {
        Ok(self
            .tables()?
            .pending_checkpoint_signatures
            .insert(&(checkpoint_seq, index), info)?)
    }

    pub(crate) fn record_epoch_pending_certs_process_time_metric(&self) {
        if let Some(epoch_close_time) = *self.epoch_close_time.read() {
            self.metrics
                .epoch_pending_certs_processed_time_since_epoch_close_ms
                .set(epoch_close_time.elapsed().as_millis() as i64);
        }
    }

    pub fn record_end_of_message_quorum_time_metric(&self) {
        if let Some(epoch_close_time) = *self.epoch_close_time.read() {
            self.metrics
                .epoch_end_of_publish_quorum_time_since_epoch_close_ms
                .set(epoch_close_time.elapsed().as_millis() as i64);
        }
    }

    pub(crate) fn report_epoch_metrics_at_last_checkpoint(&self, stats: EpochStats) {
        if let Some(epoch_close_time) = *self.epoch_close_time.read() {
            self.metrics
                .epoch_last_checkpoint_created_time_since_epoch_close_ms
                .set(epoch_close_time.elapsed().as_millis() as i64);
        }
        info!(epoch=?self.epoch(), "Epoch statistics: checkpoint_count={:?}, transaction_count={:?}, total_gas_reward={:?}", stats.checkpoint_count, stats.transaction_count, stats.total_gas_reward);
        self.metrics
            .epoch_checkpoint_count
            .set(stats.checkpoint_count as i64);
        self.metrics
            .epoch_transaction_count
            .set(stats.transaction_count as i64);
        self.metrics
            .epoch_total_gas_reward
            .set(stats.total_gas_reward as i64);
    }

    pub fn record_epoch_reconfig_start_time_metric(&self) {
        if let Some(epoch_close_time) = *self.epoch_close_time.read() {
            self.metrics
                .epoch_reconfig_start_time_since_epoch_close_ms
                .set(epoch_close_time.elapsed().as_millis() as i64);
        }
    }

    fn record_reconfig_halt_duration_metric(&self) {
        if let Some(epoch_close_time) = *self.epoch_close_time.read() {
            self.metrics
                .epoch_validator_halt_duration_ms
                .set(epoch_close_time.elapsed().as_millis() as i64);
        }
    }

    pub(crate) fn record_epoch_first_checkpoint_creation_time_metric(&self) {
        self.metrics
            .epoch_first_checkpoint_created_time_since_epoch_begin_ms
            .set(self.epoch_open_time.elapsed().as_millis() as i64);
    }

    pub fn record_is_safe_mode_metric(&self, safe_mode: bool) {
        self.metrics.is_safe_mode.set(safe_mode as i64);
    }

    pub fn record_checkpoint_builder_is_safe_mode_metric(&self, safe_mode: bool) {
        if safe_mode {
            // allow tests to inject a panic here.
            fail_point!("record_checkpoint_builder_is_safe_mode_metric");
        }
        self.metrics
            .checkpoint_builder_advance_epoch_is_safe_mode
            .set(safe_mode as i64)
    }

    fn record_epoch_total_duration_metric(&self) {
        self.metrics
            .epoch_total_duration
            .set(self.epoch_open_time.elapsed().as_millis() as i64);
    }

    pub(crate) fn update_authenticator_state(&self, update: &AuthenticatorStateUpdate) {
        info!("Updating authenticator state: {:?}", update);
        for active_jwk in &update.new_active_jwks {
            let ActiveJwk { jwk_id, jwk, .. } = active_jwk;
            self.signature_verifier.insert_jwk(jwk_id, jwk);
        }
    }

    pub fn clear_signature_cache(&self) {
        self.signature_verifier.clear_signature_cache();
    }

    pub(crate) fn set_consensus_tx_status(
        &self,
        position: ConsensusPosition,
        status: ConsensusTxStatus,
    ) {
        if let Some(cache) = self.consensus_tx_status_cache.as_ref() {
            cache.set_transaction_status(position, status);
        }
    }

    pub(crate) fn set_rejection_vote_reason(&self, position: ConsensusPosition, reason: &SuiError) {
        if let Some(tx_reject_reason_cache) = self.tx_reject_reason_cache.as_ref() {
            tx_reject_reason_cache.set_rejection_vote_reason(position, reason);
        }
    }

    pub(crate) fn get_rejection_vote_reason(
        &self,
        position: ConsensusPosition,
    ) -> Option<SuiError> {
        if let Some(tx_reject_reason_cache) = self.tx_reject_reason_cache.as_ref() {
            tx_reject_reason_cache.get_rejection_vote_reason(position)
        } else {
            None
        }
    }

    /// Caches recent finalized transactions, to avoid revoting them.
    pub(crate) fn cache_recently_finalized_transaction(&self, tx_digest: TransactionDigest) {
        self.finalized_transactions_cache.insert(tx_digest, ());
    }

    /// If true, transaction is recently finalized and should not be voted on.
    /// If false, the transaction may never be finalized, or has been finalized
    /// but the info has been evicted from the cache.
    pub(crate) fn is_recently_finalized(&self, tx_digest: &TransactionDigest) -> bool {
        self.finalized_transactions_cache.contains_key(tx_digest)
    }

    /// Only used by admin API
    pub async fn get_estimated_tx_cost(&self, tx: &TransactionData) -> Option<u64> {
        self.execution_time_estimator
            .lock()
            .await
            .as_ref()
            .map(|estimator| estimator.get_estimate(tx).as_micros() as u64)
    }

    pub async fn get_consensus_tx_cost_estimates(
        &self,
    ) -> Vec<(ExecutionTimeObservationKey, ConsensusObservations)> {
        self.execution_time_estimator
            .lock()
            .await
            .as_ref()
            .map(|estimator| estimator.get_observations())
            .unwrap_or_default()
    }

    /// Whether this node is a validator in this epoch.
    pub fn is_validator(&self) -> bool {
        self.committee.authority_exists(&self.name)
    }
}

impl ExecutionComponents {
    fn new(
        protocol_config: &ProtocolConfig,
        store: Arc<dyn BackingPackageStore + Send + Sync>,
        metrics: Arc<ResolverMetrics>,
        // Keep this as a parameter for possible future use
        _expensive_safety_check_config: &ExpensiveSafetyCheckConfig,
    ) -> Self {
        let silent = true;
        let executor = sui_execution::executor(protocol_config, silent)
            .expect("Creating an executor should not fail here");

        let module_cache = Arc::new(SyncModuleCache::new(ResolverWrapper::new(
            store,
            metrics.clone(),
        )));
        Self {
            executor,
            module_cache,
            metrics,
        }
    }

    pub(crate) fn metrics(&self) -> Arc<ResolverMetrics> {
        self.metrics.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LockDetailsWrapper {
    V1(TransactionDigest),
}

impl LockDetailsWrapper {
    pub fn migrate(self) -> Self {
        // TODO: when there are multiple versions, we must iteratively migrate from version N to
        // N+1 until we arrive at the latest version
        self
    }

    // Always returns the most recent version. Older versions are migrated to the latest version at
    // read time, so there is never a need to access older versions.
    pub fn inner(&self) -> &LockDetails {
        match self {
            Self::V1(v1) => v1,

            // can remove #[allow] when there are multiple versions
            #[allow(unreachable_patterns)]
            _ => panic!("lock details should have been migrated to latest version at read time"),
        }
    }
    pub fn into_inner(self) -> LockDetails {
        match self {
            Self::V1(v1) => v1,

            // can remove #[allow] when there are multiple versions
            #[allow(unreachable_patterns)]
            _ => panic!("lock details should have been migrated to latest version at read time"),
        }
    }
}

pub type LockDetails = TransactionDigest;

impl From<LockDetails> for LockDetailsWrapper {
    fn from(details: LockDetails) -> Self {
        // always use latest version.
        LockDetailsWrapper::V1(details)
    }
}
