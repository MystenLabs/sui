// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use arc_swap::ArcSwapOption;
use enum_dispatch::enum_dispatch;
use fastcrypto::groups::bls12381;
use fastcrypto_tbls::nodes::PartyId;
use fastcrypto_tbls::{dkg, dkg_v0};
use fastcrypto_zkp::bn254::zk_login::{JwkId, OIDCProvider, JWK};
use fastcrypto_zkp::bn254::zk_login_api::ZkLoginEnv;
use futures::future::{join_all, select, Either};
use futures::FutureExt;
use itertools::{izip, Itertools};
use narwhal_executor::ExecutionIndices;
use parking_lot::RwLock;
use parking_lot::{Mutex, RwLockReadGuard, RwLockWriteGuard};
use rocksdb::Options;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sui_config::node::ExpensiveSafetyCheckConfig;
use sui_macros::fail_point_arg;
use sui_types::accumulator::Accumulator;
use sui_types::authenticator_state::{get_authenticator_state, ActiveJwk};
use sui_types::base_types::{AuthorityName, EpochId, ObjectID, SequenceNumber, TransactionDigest};
use sui_types::base_types::{ConciseableName, ObjectRef};
use sui_types::committee::Committee;
use sui_types::committee::CommitteeTrait;
use sui_types::crypto::{AuthoritySignInfo, AuthorityStrongQuorumSignInfo, RandomnessRound};
use sui_types::digests::ChainIdentifier;
use sui_types::error::{SuiError, SuiResult};
use sui_types::signature::GenericSignature;
use sui_types::storage::{BackingPackageStore, InputKey, ObjectStore};
use sui_types::transaction::{
    AuthenticatorStateUpdate, CertifiedTransaction, InputObjectKind, SenderSignedData, Transaction,
    TransactionDataAPI, TransactionKey, TransactionKind, VerifiedCertificate,
    VerifiedSignedTransaction, VerifiedTransaction,
};
use tokio::sync::OnceCell;
use tracing::{debug, error, info, instrument, trace, warn};
use typed_store::rocks::{read_size_from_env, ReadWriteOptions};
use typed_store::{
    rocks::{default_db_options, DBBatch, DBMap, DBOptions, MetricConf},
    traits::{TableSummary, TypedStoreDebug},
    TypedStoreError,
};

use super::authority_store_tables::ENV_VAR_LOCKS_BLOCK_CACHE_SIZE;
use super::epoch_start_configuration::EpochStartConfigTrait;
use super::shared_object_congestion_tracker::SharedObjectCongestionTracker;
use super::transaction_deferral::{transaction_deferral_within_limit, DeferralKey, DeferralReason};
use crate::authority::epoch_start_configuration::{EpochFlag, EpochStartConfiguration};
use crate::authority::AuthorityMetrics;
use crate::authority::ResolverWrapper;
use crate::checkpoints::{
    BuilderCheckpointSummary, CheckpointHeight, CheckpointServiceNotify, EpochStats,
    PendingCheckpoint, PendingCheckpointInfo, PendingCheckpointV2, PendingCheckpointV2Contents,
};

use crate::authority::shared_object_version_manager::{
    AssignedTxAndVersions, ConsensusSharedObjVerAssignment, SharedObjVerManager,
};
use crate::consensus_handler::{
    ConsensusCommitInfo, SequencedConsensusTransaction, SequencedConsensusTransactionKey,
    SequencedConsensusTransactionKind, VerifiedSequencedConsensusTransaction,
};
use crate::epoch::epoch_metrics::EpochMetrics;
use crate::epoch::randomness::{
    DkgStatus, RandomnessManager, RandomnessReporter, VersionedProcessedMessage,
    VersionedUsedProcessedMessages,
};
use crate::epoch::reconfiguration::ReconfigState;
use crate::execution_cache::ObjectCacheRead;
use crate::module_cache_metrics::ResolverMetrics;
use crate::post_consensus_tx_reorder::PostConsensusTxReorder;
use crate::signature_verifier::*;
use crate::stake_aggregator::{GenericMultiStakeAggregator, StakeAggregator};
use move_bytecode_utils::module_cache::SyncModuleCache;
use mysten_common::sync::notify_once::NotifyOnce;
use mysten_common::sync::notify_read::NotifyRead;
use mysten_metrics::monitored_scope;
use narwhal_types::{Round, TimestampMs};
use prometheus::IntCounter;
use std::str::FromStr;
use sui_execution::{self, Executor};
use sui_macros::fail_point;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_storage::mutex_table::{MutexGuard, MutexTable};
use sui_types::effects::TransactionEffects;
use sui_types::executable_transaction::{
    TrustedExecutableTransaction, VerifiedExecutableTransaction,
};
use sui_types::message_envelope::TrustedEnvelope;
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointSequenceNumber, CheckpointSignatureMessage, CheckpointSummary,
};
use sui_types::messages_consensus::{
    check_total_jwk_size, AuthorityCapabilities, ConsensusTransaction, ConsensusTransactionKey,
    ConsensusTransactionKind,
};
use sui_types::messages_consensus::{VersionedDkgConfimation, VersionedDkgMessage};
use sui_types::storage::GetSharedLocks;
use sui_types::sui_system_state::epoch_start_sui_system_state::{
    EpochStartSystemState, EpochStartSystemStateTrait,
};
use tap::TapOptional;
use tokio::time::Instant;
use typed_store::{retry_transaction_forever, Map};
use typed_store_derive::DBMapUtils;

/// The key where the latest consensus index is stored in the database.
// TODO: Make a single table (e.g., called `variables`) storing all our lonely variables in one place.
const LAST_CONSENSUS_STATS_ADDR: u64 = 0;
const RECONFIG_STATE_INDEX: u64 = 0;
const OVERRIDE_PROTOCOL_UPGRADE_BUFFER_STAKE_INDEX: u64 = 0;
pub const EPOCH_DB_PREFIX: &str = "epoch_";

// Types for randomness DKG.
pub(crate) type PkG = bls12381::G2Element;
pub(crate) type EncG = bls12381::G2Element;

// CertLockGuard and CertTxGuard are functionally identical right now, but we retain a distinction
// anyway. If we need to support distributed object storage, having this distinction will be
// useful, as we will most likely have to re-implement a retry / write-ahead-log at that point.
pub struct CertLockGuard(MutexGuard);
pub struct CertTxGuard(CertLockGuard);

impl CertTxGuard {
    pub fn release(self) {}
    pub fn commit_tx(self) {}
}

type JwkAggregator = GenericMultiStakeAggregator<(JwkId, JWK), true>;

pub enum CancelConsensusCertificateReason {
    CongestionOnObjects(Vec<ObjectID>),
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

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionIndicesWithHash {
    pub index: ExecutionIndices,
    pub hash: u64,
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

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionIndicesWithStats {
    pub index: ExecutionIndices,
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

pub struct AuthorityPerEpochStore {
    /// The name of this authority.
    pub(crate) name: AuthorityName,

    /// Committee of validators for the current epoch.
    committee: Arc<Committee>,

    /// Holds the underlying per-epoch typed store tables.
    /// This is an ArcSwapOption because it needs to be used concurrently,
    /// and it needs to be cleared at the end of the epoch.
    tables: ArcSwapOption<AuthorityEpochTables>,

    protocol_config: ProtocolConfig,

    // needed for re-opening epoch db.
    parent_path: PathBuf,
    db_options: Option<Options>,

    /// In-memory cache of the content from the reconfig_state db table.
    reconfig_state_mem: RwLock<ReconfigState>,
    consensus_notify_read: NotifyRead<SequencedConsensusTransactionKey, ()>,

    /// Batch verifier for certificates - also caches certificates and tx sigs that are known to have
    /// valid signatures. Lives in per-epoch store because the caching/batching is only valid
    /// within for certs within the current epoch.
    pub(crate) signature_verifier: SignatureVerifier,

    pub(crate) checkpoint_state_notify_read: NotifyRead<CheckpointSequenceNumber, Accumulator>,

    running_root_notify_read: NotifyRead<CheckpointSequenceNumber, Accumulator>,

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
    end_of_publish: Mutex<StakeAggregator<(), true>>,
    /// Pending certificates that are waiting to be sequenced by the consensus.
    /// This is an in-memory 'index' of a AuthorityPerEpochTables::pending_consensus_transactions.
    /// We need to keep track of those in order to know when to send EndOfPublish message.
    /// Lock ordering: this is a 'leaf' lock, no other locks should be acquired in the scope of this lock
    /// In particular, this lock is always acquired after taking read or write lock on reconfig state
    pending_consensus_certificates: Mutex<HashSet<TransactionDigest>>,

    /// MutexTable for transaction locks (prevent concurrent execution of same transaction)
    mutex_table: MutexTable<TransactionDigest>,

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

    /// Chain identifier
    chain_identifier: ChainIdentifier,

    /// aggregator for JWK votes
    jwk_aggregator: Mutex<JwkAggregator>,

    /// State machine managing randomness DKG and generation.
    randomness_manager: OnceCell<tokio::sync::Mutex<RandomnessManager>>,
    randomness_reporter: OnceCell<RandomnessReporter>,
}

/// AuthorityEpochTables contains tables that contain data that is only valid within an epoch.
#[derive(DBMapUtils)]
pub struct AuthorityEpochTables {
    /// This is map between the transaction digest and transactions found in the `transaction_lock`.
    #[default_options_override_fn = "signed_transactions_table_default_config"]
    signed_transactions:
        DBMap<TransactionDigest, TrustedEnvelope<SenderSignedData, AuthoritySignInfo>>,

    /// Map from ObjectRef to transaction locking that object
    #[default_options_override_fn = "owned_object_transaction_locks_table_default_config"]
    owned_object_locked_transactions: DBMap<ObjectRef, LockDetailsWrapper>,

    /// Signatures over transaction effects that were executed in the current epoch.
    /// Store this to avoid re-signing the same effects twice.
    effects_signatures: DBMap<TransactionDigest, AuthoritySignInfo>,

    /// Signatures of transaction certificates that are executed locally.
    pub(crate) transaction_cert_signatures: DBMap<TransactionDigest, AuthorityStrongQuorumSignInfo>,

    /// The tables below manage shared object locks / versions. There are three ways they can be
    /// updated:
    /// 1. (validators only): Upon receiving a certified transaction from consensus, the authority
    /// assigns the next version to each shared object of the transaction. The next versions of
    /// the shared objects are updated as well.
    /// 2. (validators only): Upon receiving a new consensus commit, the authority assigns the
    /// next version of the randomness state object to an expected future transaction to be
    /// generated after the next random value is available. The next version of the randomness
    /// state object is updated as well.
    /// 3. (fullnodes + validators): Upon receiving a certified effect from state sync, or
    /// transaction orchestrator fast execution path, the node assigns the shared object
    /// versions from the transaction effect. Next object versions are not updated.
    ///
    /// REQUIRED: all authorities must assign the same shared object versions for each transaction.
    assigned_shared_object_versions: DBMap<TransactionDigest, Vec<(ObjectID, SequenceNumber)>>,
    assigned_shared_object_versions_v2: DBMap<TransactionKey, Vec<(ObjectID, SequenceNumber)>>,
    next_shared_object_versions: DBMap<ObjectID, SequenceNumber>,

    /// Certificates that have been received from clients or received from consensus, but not yet
    /// executed. Entries are cleared after execution.
    /// This table is critical for crash recovery, because usually the consensus output progress
    /// is updated after a certificate is committed into this table.
    ///
    /// In theory, this table may be superseded by storing consensus and checkpoint execution
    /// progress. But it is more complex, because it would be necessary to track inflight
    /// executions not ordered by indices. For now, tracking inflight certificates as a map
    /// seems easier.
    #[default_options_override_fn = "pending_execution_table_default_config"]
    pub(crate) pending_execution: DBMap<TransactionDigest, TrustedExecutableTransaction>,

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

    /// this table is not used
    #[allow(dead_code)]
    consensus_message_order: DBMap<ExecutionIndices, TransactionDigest>,

    /// The following table is used to store a single value (the corresponding key is a constant). The value
    /// represents the index of the latest consensus message this authority processed. This field is written
    /// by a single process acting as consensus (light) client. It is used to ensure the authority processes
    /// every message output by consensus (and in the right order).
    last_consensus_index: DBMap<u64, ExecutionIndicesWithHash>,

    /// The following table is used to store a single value (the corresponding key is a constant). The value
    /// represents the index of the latest consensus message this authority processed, running hash of
    /// transactions, and accumulated stats of consensus output.
    /// This field is written by a single process (consensus handler).
    last_consensus_stats: DBMap<u64, ExecutionIndicesWithStats>,

    /// this table is not used
    #[allow(dead_code)]
    checkpoint_boundary: DBMap<u64, u64>,

    /// This table contains current reconfiguration state for validator for current epoch
    reconfig_state: DBMap<u64, ReconfigState>,

    /// Validators that have sent EndOfPublish message in this epoch
    end_of_publish: DBMap<AuthorityName, ()>,

    // TODO: Unused. Remove when removal of DBMap tables is supported.
    #[allow(dead_code)]
    final_epoch_checkpoint: DBMap<u64, u64>,

    /// This table has information for the checkpoints for which we constructed all the data
    /// from consensus, but not yet constructed actual checkpoint.
    ///
    /// Key in this table is the consensus commit height and not a checkpoint sequence number.
    ///
    /// Non-empty list of transactions here might result in empty list when we are forming checkpoint.
    /// Because we don't want to create checkpoints with empty content(see CheckpointBuilder::write_checkpoint),
    /// the sequence number of checkpoint does not match height here.
    #[default_options_override_fn = "pending_checkpoints_table_default_config"]
    pending_checkpoints: DBMap<CheckpointHeight, PendingCheckpoint>,
    #[default_options_override_fn = "pending_checkpoints_table_default_config"]
    pending_checkpoints_v2: DBMap<CheckpointHeight, PendingCheckpointV2>,

    /// Checkpoint builder maintains internal list of transactions it included in checkpoints here
    builder_digest_to_checkpoint: DBMap<TransactionDigest, CheckpointSequenceNumber>,

    /// Maps non-digest TransactionKeys to the corresponding digest after execution, for use
    /// by checkpoint builder.
    transaction_key_to_digest: DBMap<TransactionKey, TransactionDigest>,

    /// Stores pending signatures
    /// The key in this table is checkpoint sequence number and an arbitrary integer
    pending_checkpoint_signatures:
        DBMap<(CheckpointSequenceNumber, u64), CheckpointSignatureMessage>,

    /// When we see certificate through consensus for the first time, we record
    /// user signature for this transaction here. This will be included in the checkpoint later.
    user_signatures_for_checkpoints: DBMap<TransactionDigest, Vec<GenericSignature>>,

    /// This table is not used
    #[allow(dead_code)]
    builder_checkpoint_summary: DBMap<CheckpointSequenceNumber, CheckpointSummary>,
    /// Maps sequence number to checkpoint summary, used by CheckpointBuilder to build checkpoint within epoch
    builder_checkpoint_summary_v2: DBMap<CheckpointSequenceNumber, BuilderCheckpointSummary>,

    // Maps checkpoint sequence number to an accumulator with accumulated state
    // only for the checkpoint that the key references. Append-only, i.e.,
    // the accumulator is complete wrt the checkpoint
    pub state_hash_by_checkpoint: DBMap<CheckpointSequenceNumber, Accumulator>,

    /// Maps checkpoint sequence number to the running (non-finalized) root state
    /// accumulator up th that checkpoint. This should be equivalent to the root
    /// state hash at end of epoch. Guaranteed to be written to in checkpoint
    /// sequence number order.
    pub running_root_accumulators: DBMap<CheckpointSequenceNumber, Accumulator>,

    /// Record of the capabilities advertised by each authority.
    authority_capabilities: DBMap<AuthorityName, AuthorityCapabilities>,

    /// Contains a single key, which overrides the value of
    /// ProtocolConfig::buffer_stake_for_protocol_upgrade_bps
    override_protocol_upgrade_buffer_stake: DBMap<u64, u64>,

    /// When transaction is executed via checkpoint executor, we store association here
    pub(crate) executed_transactions_to_checkpoint:
        DBMap<TransactionDigest, CheckpointSequenceNumber>,

    /// This table is no longer used (can be removed when DBMap supports removing tables)
    #[allow(dead_code)]
    oauth_provider_jwk: DBMap<JwkId, JWK>,

    /// JWKs that have been voted for by one or more authorities but are not yet active.
    pending_jwks: DBMap<(AuthorityName, JwkId, JWK), ()>,

    /// JWKs that are currently available for zklogin authentication, and the round in which they
    /// became active.
    /// This would normally be stored as (JwkId, JWK) -> u64, but we need to be able to scan to
    /// find all Jwks for a given round
    active_jwks: DBMap<(u64, (JwkId, JWK)), ()>,

    /// Transactions that are being deferred until some future time
    deferred_transactions: DBMap<DeferralKey, Vec<VerifiedSequencedConsensusTransaction>>,

    /// This table is no longer used (can be removed when DBMap supports removing tables)
    #[allow(dead_code)]
    randomness_rounds_written: DBMap<narwhal_types::RandomnessRound, ()>,

    /// Tables for recording state for RandomnessManager.

    /// Records messages processed from other nodes. Updated when receiving a new dkg::Message
    /// via consensus.
    pub(crate) dkg_processed_messages_v2: DBMap<PartyId, VersionedProcessedMessage>,
    #[deprecated]
    pub(crate) dkg_processed_messages: DBMap<PartyId, dkg_v0::ProcessedMessage<PkG, EncG>>,

    /// Records messages used to generate a DKG confirmation. Updated when enough DKG
    /// messages are received to progress to the next phase.
    pub(crate) dkg_used_messages_v2: DBMap<u64, VersionedUsedProcessedMessages>,
    #[deprecated]
    pub(crate) dkg_used_messages: DBMap<u64, dkg_v0::UsedProcessedMessages<PkG, EncG>>,

    /// Records confirmations received from other nodes. Updated when receiving a new
    /// dkg::Confirmation via consensus.
    pub(crate) dkg_confirmations_v2: DBMap<PartyId, VersionedDkgConfimation>,
    #[deprecated]
    pub(crate) dkg_confirmations: DBMap<PartyId, dkg::Confirmation<EncG>>,
    /// Records the final output of DKG after completion, including the public VSS key and
    /// any local private shares.
    pub(crate) dkg_output: DBMap<u64, dkg::Output<PkG, EncG>>,
    /// This table is no longer used (can be removed when DBMap supports removing tables)
    #[allow(dead_code)]
    randomness_rounds_pending: DBMap<RandomnessRound, ()>,
    /// Holds the value of the next RandomnessRound to be generated.
    pub(crate) randomness_next_round: DBMap<u64, RandomnessRound>,
    /// Holds the value of the highest completed RandomnessRound (as reported to RandomnessReporter).
    pub(crate) randomness_highest_completed_round: DBMap<u64, RandomnessRound>,
    /// Holds the timestamp of the most recently generated round of randomness.
    pub(crate) randomness_last_round_timestamp: DBMap<u64, TimestampMs>,
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

fn pending_execution_table_default_config() -> DBOptions {
    default_db_options()
        .optimize_for_write_throughput()
        .optimize_for_large_values_no_scan(1 << 10)
}

fn pending_consensus_transactions_table_default_config() -> DBOptions {
    default_db_options()
        .optimize_for_write_throughput()
        .optimize_for_large_values_no_scan(1 << 10)
}

fn pending_checkpoints_table_default_config() -> DBOptions {
    default_db_options()
        .optimize_for_write_throughput()
        .optimize_for_large_values_no_scan(1 << 10)
}

impl AuthorityEpochTables {
    pub fn open(epoch: EpochId, parent_path: &Path, db_options: Option<Options>) -> Self {
        Self::open_tables_transactional(
            Self::path(epoch, parent_path),
            MetricConf::new("epoch"),
            db_options,
            None,
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

    pub fn get_all_pending_consensus_transactions(&self) -> Vec<ConsensusTransaction> {
        self.pending_consensus_transactions
            .unbounded_iter()
            .map(|(_k, v)| v)
            .collect()
    }

    pub fn reset_db_for_execution_since_genesis(&self) -> SuiResult {
        // TODO: Add new tables that get added to the db automatically
        self.executed_transactions_to_checkpoint.unsafe_clear()?;
        Ok(())
    }

    /// WARNING: This method is very subtle and can corrupt the database if used incorrectly.
    /// It should only be used in one-off cases or tests after fully understanding the risk.
    pub fn remove_executed_tx_subtle(&self, digest: &TransactionDigest) -> SuiResult {
        self.executed_transactions_to_checkpoint.remove(digest)?;
        Ok(())
    }

    pub fn get_last_consensus_index(&self) -> SuiResult<Option<ExecutionIndicesWithHash>> {
        Ok(self.last_consensus_index.get(&LAST_CONSENSUS_STATS_ADDR)?)
    }

    pub fn get_last_consensus_stats(&self) -> SuiResult<Option<ExecutionIndicesWithStats>> {
        Ok(self.last_consensus_stats.get(&LAST_CONSENSUS_STATS_ADDR)?)
    }

    pub fn get_pending_checkpoint_signatures_iter(
        &self,
        checkpoint_seq: CheckpointSequenceNumber,
        starting_index: u64,
    ) -> SuiResult<
        impl Iterator<Item = ((CheckpointSequenceNumber, u64), CheckpointSignatureMessage)> + '_,
    > {
        let key = (checkpoint_seq, starting_index);
        debug!("Scanning pending checkpoint signatures from {:?}", key);
        let iter = self
            .pending_checkpoint_signatures
            .unbounded_iter()
            .skip_to(&key)?;
        Ok::<_, SuiError>(iter)
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
        transaction: VerifiedSignedTransaction,
        locks_to_write: impl Iterator<Item = (ObjectRef, LockDetails)>,
    ) -> SuiResult {
        let mut batch = self.owned_object_locked_transactions.batch();
        batch.insert_batch(
            &self.owned_object_locked_transactions,
            locks_to_write.map(|(obj_ref, lock)| (obj_ref, LockDetailsWrapper::from(lock))),
        )?;
        batch.insert_batch(
            &self.signed_transactions,
            std::iter::once((*transaction.digest(), transaction.serializable_ref())),
        )?;
        batch.write()?;
        Ok(())
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
        chain_identifier: ChainIdentifier,
    ) -> Arc<Self> {
        let current_time = Instant::now();
        let epoch_id = committee.epoch;

        let tables = AuthorityEpochTables::open(epoch_id, parent_path, db_options.clone());
        let end_of_publish =
            StakeAggregator::from_iter(committee.clone(), tables.end_of_publish.unbounded_iter());
        let reconfig_state = tables
            .load_reconfig_state()
            .expect("Load reconfig state at initialization cannot fail");

        let epoch_alive_notify = NotifyOnce::new();
        let pending_consensus_transactions = tables.get_all_pending_consensus_transactions();
        let pending_consensus_certificates: HashSet<_> = pending_consensus_transactions
            .iter()
            .filter_map(|transaction| {
                if let ConsensusTransactionKind::UserTransaction(certificate) = &transaction.kind {
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
        metrics.current_epoch.set(epoch_id as i64);
        metrics
            .current_voting_right
            .set(committee.weight(&name) as i64);
        let protocol_version = epoch_start_configuration
            .epoch_start_state()
            .protocol_version();
        let protocol_config =
            ProtocolConfig::get_for_version(protocol_version, chain_identifier.chain());

        let execution_component = ExecutionComponents::new(
            &protocol_config,
            backing_package_store,
            cache_metrics,
            expensive_safety_check_config,
        );

        let zklogin_env = match chain_identifier.chain() {
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
            protocol_config.zklogin_max_epoch_upper_bound_delta(),
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

        for ((authority, id, jwk), _) in tables.pending_jwks.unbounded_iter().seek_to_first() {
            jwk_aggregator.insert(authority, (id, jwk));
        }

        let jwk_aggregator = Mutex::new(jwk_aggregator);

        let s = Arc::new(Self {
            name,
            committee,
            protocol_config,
            tables: ArcSwapOption::new(Some(Arc::new(tables))),
            parent_path: parent_path.to_path_buf(),
            db_options,
            reconfig_state_mem: RwLock::new(reconfig_state),
            epoch_alive_notify,
            user_certs_closed_notify: NotifyOnce::new(),
            epoch_alive: tokio::sync::RwLock::new(true),
            consensus_notify_read: NotifyRead::new(),
            signature_verifier,
            checkpoint_state_notify_read: NotifyRead::new(),
            running_root_notify_read: NotifyRead::new(),
            executed_digests_notify_read: NotifyRead::new(),
            end_of_publish: Mutex::new(end_of_publish),
            pending_consensus_certificates: Mutex::new(pending_consensus_certificates),
            mutex_table: MutexTable::new(MUTEX_TABLE_SIZE),
            epoch_open_time: current_time,
            epoch_close_time: Default::default(),
            metrics,
            epoch_start_configuration,
            execution_component,
            chain_identifier,
            jwk_aggregator,
            randomness_manager: OnceCell::new(),
            randomness_reporter: OnceCell::new(),
        });
        s.update_buffer_stake_metric();
        s
    }

    pub fn tables(&self) -> SuiResult<Arc<AuthorityEpochTables>> {
        match self.tables.load_full() {
            Some(tables) => Ok(tables),
            None => Err(SuiError::EpochEnded(self.epoch())),
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

    pub fn set_randomness_manager(
        &self,
        mut randomness_manager: RandomnessManager,
    ) -> SuiResult<()> {
        let reporter = randomness_manager.reporter();
        let result = randomness_manager.start_dkg();
        if self
            .randomness_manager
            .set(tokio::sync::Mutex::new(randomness_manager))
            .is_err()
        {
            error!("BUG: `set_randomness_manager` called more than once; this should never happen");
        }
        if self.randomness_reporter.set(reporter).is_err() {
            error!("BUG: `set_randomness_manager` called more than once; this should never happen");
        }
        result
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

    pub fn state_accumulator_v2_enabled(&self) -> bool {
        let flag = match self.get_chain_identifier().chain() {
            Chain::Unknown | Chain::Testnet => EpochFlag::StateAccumulatorV2EnabledTestnet,
            Chain::Mainnet => EpochFlag::StateAccumulatorV2EnabledMainnet,
        };

        self.epoch_start_configuration.flags().contains(&flag)
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
        self.chain_identifier
    }

    pub fn new_at_next_epoch(
        &self,
        name: AuthorityName,
        new_committee: Committee,
        epoch_start_configuration: EpochStartConfiguration,
        backing_package_store: Arc<dyn BackingPackageStore + Send + Sync>,
        object_store: Arc<dyn ObjectStore + Send + Sync>,
        expensive_safety_check_config: &ExpensiveSafetyCheckConfig,
        chain_identifier: ChainIdentifier,
    ) -> Arc<Self> {
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
            chain_identifier,
        )
    }

    pub fn new_at_next_epoch_for_testing(
        &self,
        backing_package_store: Arc<dyn BackingPackageStore + Send + Sync>,
        object_store: Arc<dyn ObjectStore + Send + Sync>,
        expensive_safety_check_config: &ExpensiveSafetyCheckConfig,
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
            self.chain_identifier,
        )
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

    pub fn get_state_hash_for_checkpoint(
        &self,
        checkpoint: &CheckpointSequenceNumber,
    ) -> SuiResult<Option<Accumulator>> {
        Ok(self.tables()?.state_hash_by_checkpoint.get(checkpoint)?)
    }

    pub fn insert_state_hash_for_checkpoint(
        &self,
        checkpoint: &CheckpointSequenceNumber,
        accumulator: &Accumulator,
    ) -> SuiResult {
        Ok(self
            .tables()?
            .state_hash_by_checkpoint
            .insert(checkpoint, accumulator)?)
    }

    pub fn get_running_root_accumulator(
        &self,
        checkpoint: &CheckpointSequenceNumber,
    ) -> SuiResult<Option<Accumulator>> {
        Ok(self.tables()?.running_root_accumulators.get(checkpoint)?)
    }

    pub fn get_highest_running_root_accumulator(
        &self,
    ) -> SuiResult<Option<(CheckpointSequenceNumber, Accumulator)>> {
        Ok(self
            .tables()?
            .running_root_accumulators
            .unbounded_iter()
            .skip_to_last()
            .next())
    }

    pub fn insert_running_root_accumulator(
        &self,
        checkpoint: &CheckpointSequenceNumber,
        acc: &Accumulator,
    ) -> SuiResult {
        self.tables()?
            .running_root_accumulators
            .insert(checkpoint, acc)?;
        self.running_root_notify_read.notify(checkpoint, acc);

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

    pub async fn acquire_tx_guard(
        &self,
        cert: &VerifiedExecutableTransaction,
    ) -> SuiResult<CertTxGuard> {
        let digest = cert.digest();
        Ok(CertTxGuard(self.acquire_tx_lock(digest).await))
    }

    /// Acquire the lock for a tx without writing to the WAL.
    pub async fn acquire_tx_lock(&self, digest: &TransactionDigest) -> CertLockGuard {
        CertLockGuard(self.mutex_table.acquire_lock(*digest).await)
    }

    pub fn store_reconfig_state(&self, new_state: &ReconfigState) -> SuiResult {
        self.tables()?
            .reconfig_state
            .insert(&RECONFIG_STATE_INDEX, new_state)?;
        Ok(())
    }

    fn store_reconfig_state_batch(
        &self,
        new_state: &ReconfigState,
        batch: &mut DBBatch,
    ) -> SuiResult {
        batch.insert_batch(
            &self.tables()?.reconfig_state,
            [(&RECONFIG_STATE_INDEX, new_state)],
        )?;
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
    pub fn insert_tx_cert_and_effects_signature(
        &self,
        tx_key: &TransactionKey,
        tx_digest: &TransactionDigest,
        cert_sig: Option<&AuthorityStrongQuorumSignInfo>,
        effects_signature: Option<&AuthoritySignInfo>,
    ) -> SuiResult {
        let mut batch = self.tables()?.effects_signatures.batch();
        if let Some(cert_sig) = cert_sig {
            batch.insert_batch(
                &self.tables()?.transaction_cert_signatures,
                [(tx_digest, cert_sig)],
            )?;
        }
        if let Some(effects_signature) = effects_signature {
            batch.insert_batch(
                &self.tables()?.effects_signatures,
                [(tx_digest, effects_signature)],
            )?;
        }
        if !matches!(tx_key, TransactionKey::Digest(_)) {
            batch.insert_batch(
                &self.tables()?.transaction_key_to_digest,
                [(tx_key, tx_digest)],
            )?;
        }
        batch.write()?;

        if !matches!(tx_key, TransactionKey::Digest(_)) {
            self.executed_digests_notify_read.notify(tx_key, tx_digest);
        }
        Ok(())
    }

    pub fn effects_signatures_exists<'a>(
        &self,
        digests: impl IntoIterator<Item = &'a TransactionDigest>,
    ) -> SuiResult<Vec<bool>> {
        Ok(self
            .tables()?
            .effects_signatures
            .multi_contains_keys(digests)?)
    }

    pub fn get_effects_signature(
        &self,
        tx_digest: &TransactionDigest,
    ) -> SuiResult<Option<AuthoritySignInfo>> {
        Ok(self.tables()?.effects_signatures.get(tx_digest)?)
    }

    pub fn get_transaction_cert_sig(
        &self,
        tx_digest: &TransactionDigest,
    ) -> SuiResult<Option<AuthorityStrongQuorumSignInfo>> {
        Ok(self.tables()?.transaction_cert_signatures.get(tx_digest)?)
    }

    /// Resolves InputObjectKinds into InputKeys, by consulting the shared object version
    /// assignment table.
    pub(crate) fn get_input_object_keys(
        &self,
        key: &TransactionKey,
        objects: &[InputObjectKind],
    ) -> BTreeSet<InputKey> {
        let mut shared_locks = HashMap::<ObjectID, SequenceNumber>::new();
        objects
            .iter()
            .map(|kind| {
                match kind {
                    InputObjectKind::SharedMoveObject { id, .. } => {
                        if shared_locks.is_empty() {
                            shared_locks = self
                                .get_shared_locks(key)
                                .expect("Read from storage should not fail!")
                                .into_iter()
                                .collect();
                        }
                        // If we can't find the locked version, it means
                        // 1. either we have a bug that skips shared object version assignment
                        // 2. or we have some DB corruption
                        let Some(version) = shared_locks.get(id) else {
                            panic!(
                                "Shared object locks should have been set. key: {key:?}, obj \
                                id: {id:?}",
                            )
                        };
                        InputKey::VersionedObject {
                            id: *id,
                            version: *version,
                        }
                    }
                    InputObjectKind::MovePackage(id) => InputKey::Package { id: *id },
                    InputObjectKind::ImmOrOwnedMoveObject(objref) => InputKey::VersionedObject {
                        id: objref.0,
                        version: objref.1,
                    },
                }
            })
            .collect()
    }

    pub fn get_last_consensus_index(&self) -> SuiResult<ExecutionIndicesWithHash> {
        self.tables()?
            .get_last_consensus_index()
            .map(|x| x.unwrap_or_default())
            .map_err(SuiError::from)
    }

    pub fn get_last_consensus_stats(&self) -> SuiResult<ExecutionIndicesWithStats> {
        match self
            .tables()?
            .get_last_consensus_stats()
            .map_err(SuiError::from)?
        {
            Some(stats) => Ok(stats),
            // TODO: stop reading from last_consensus_index after rollout.
            None => {
                let indices = self
                    .tables()?
                    .get_last_consensus_index()
                    .map(|x| x.unwrap_or_default())
                    .map_err(SuiError::from)?;
                Ok(ExecutionIndicesWithStats {
                    index: indices.index,
                    hash: indices.hash,
                    stats: ConsensusStats::default(),
                })
            }
        }
    }

    pub fn get_accumulators_in_checkpoint_range(
        &self,
        from_checkpoint: CheckpointSequenceNumber,
        to_checkpoint: CheckpointSequenceNumber,
    ) -> SuiResult<Vec<(CheckpointSequenceNumber, Accumulator)>> {
        self.tables()?
            .state_hash_by_checkpoint
            .safe_range_iter(from_checkpoint..=to_checkpoint)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Returns future containing the state digest for the given epoch
    /// once available.
    /// TODO: remove once StateAccumulatorV1 is removed
    pub async fn notify_read_checkpoint_state_digests(
        &self,
        checkpoints: Vec<CheckpointSequenceNumber>,
    ) -> SuiResult<Vec<Accumulator>> {
        // We need to register waiters _before_ reading from the database to avoid
        // race conditions
        let registrations = self.checkpoint_state_notify_read.register_all(&checkpoints);
        let accumulators = self
            .tables()?
            .state_hash_by_checkpoint
            .multi_get(checkpoints)?;

        // Zipping together registrations and accumulators ensures returned order is
        // the same as order of digests
        let results =
            accumulators
                .into_iter()
                .zip(registrations.into_iter())
                .map(|(a, r)| match a {
                    // Note that Some() clause also drops registration that is already fulfilled
                    Some(ready) => Either::Left(futures::future::ready(ready)),
                    None => Either::Right(r),
                });

        Ok(join_all(results).await)
    }

    pub async fn notify_read_running_root(
        &self,
        checkpoint: CheckpointSequenceNumber,
    ) -> SuiResult<Accumulator> {
        let registration = self.running_root_notify_read.register_one(&checkpoint);
        let acc = self.tables()?.running_root_accumulators.get(&checkpoint)?;

        let result = match acc {
            Some(ready) => Either::Left(futures::future::ready(ready)),
            None => Either::Right(registration),
        }
        .await;

        Ok(result)
    }

    /// `pending_certificates` table related methods. Should only be used from TransactionManager.

    /// Gets all pending certificates. Used during recovery.
    pub fn all_pending_execution(&self) -> SuiResult<Vec<VerifiedExecutableTransaction>> {
        Ok(self
            .tables()?
            .pending_execution
            .unbounded_iter()
            .map(|(_, cert)| cert.into())
            .collect())
    }

    /// Deletes many pending certificates.
    #[instrument(level = "trace", skip_all)]
    pub fn multi_remove_pending_execution(&self, digests: &[TransactionDigest]) -> SuiResult<()> {
        let tables = match self.tables() {
            Ok(tables) => tables,
            // After Epoch ends, it is no longer necessary to remove pending transactions
            // because the table will not be used anymore and be deleted eventually.
            Err(SuiError::EpochEnded(_)) => return Ok(()),
            Err(e) => return Err(e),
        };
        let mut batch = tables.pending_execution.batch();
        batch.delete_batch(&tables.pending_execution, digests)?;
        batch.write()?;
        Ok(())
    }

    pub fn get_all_pending_consensus_transactions(&self) -> Vec<ConsensusTransaction> {
        self.tables()
            .expect("recovery should not cross epoch boundary")
            .get_all_pending_consensus_transactions()
    }

    #[cfg(test)]
    pub fn get_next_object_version(&self, obj: &ObjectID) -> Option<SequenceNumber> {
        self.tables()
            .expect("test should not cross epoch boundary")
            .next_shared_object_versions
            .get(obj)
            .unwrap()
    }

    pub fn set_shared_object_versions_for_testing(
        &self,
        tx_digest: &TransactionDigest,
        assigned_versions: &Vec<(ObjectID, SequenceNumber)>,
    ) -> SuiResult {
        if self.randomness_state_enabled() {
            self.tables()?
                .assigned_shared_object_versions_v2
                .insert(&TransactionKey::Digest(*tx_digest), assigned_versions)?;
        } else {
            self.tables()?
                .assigned_shared_object_versions
                .insert(tx_digest, assigned_versions)?;
        }
        Ok(())
    }

    pub fn insert_finalized_transactions(
        &self,
        digests: &[TransactionDigest],
        sequence: CheckpointSequenceNumber,
    ) -> SuiResult {
        let mut batch = self.tables()?.executed_transactions_to_checkpoint.batch();
        batch.insert_batch(
            &self.tables()?.executed_transactions_to_checkpoint,
            digests.iter().map(|d| (*d, sequence)),
        )?;
        batch.write()?;
        trace!("Transactions {digests:?} finalized at checkpoint {sequence}");
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

    // For each id in objects_to_init, return the next version for that id as recorded in the
    // next_shared_object_versions table.
    //
    // If any ids are missing, then we need to initialize the table. We first check if a previous
    // version of that object has been written. If so, then the object was written in a previous
    // epoch, and we initialize next_shared_object_versions to that value. If no version of the
    // object has yet been written, we initialize the object to the initial version recorded in the
    // certificate (which is a function of the lamport version computation of the transaction that
    // created the shared object originally - which transaction may not yet have been executed on
    // this node).
    //
    // Because all paths that assign shared locks for a shared object transaction call this
    // function, it is impossible for parent_sync to be updated before this function completes
    // successfully for each affected object id.
    pub(crate) async fn get_or_init_next_object_versions(
        &self,
        objects_to_init: &[(ObjectID, SequenceNumber)],
        cache_reader: &dyn ObjectCacheRead,
    ) -> SuiResult<HashMap<ObjectID, SequenceNumber>> {
        let mut ret: HashMap<_, _>;
        // Since this can be called from consensus task, we must retry forever - the only other
        // option is to panic. It is extremely unlikely that more than 2 retries will be needed, as
        // the only two writers are the consensus task and checkpoint execution.
        retry_transaction_forever!({
            // This code may still be correct without using a transaction snapshot, but I couldn't
            // convince myself of that.
            let tables = self.tables()?;
            let mut db_transaction = tables.next_shared_object_versions.transaction()?;

            let ids: Vec<_> = objects_to_init.iter().map(|(id, _)| *id).collect();

            let next_versions = db_transaction
                .multi_get(&self.tables()?.next_shared_object_versions, ids.clone())?;

            let uninitialized_objects: Vec<(ObjectID, SequenceNumber)> = next_versions
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
                return Ok(izip!(ids, next_versions.into_iter().map(|v| v.unwrap())).collect());
            }

            let versions_to_write: Vec<_> = uninitialized_objects
                .iter()
                .map(|(id, initial_version)| {
                    // Note: we don't actually need to read from the transaction here, as no writer
                    // can update object_store until after get_or_init_next_object_versions
                    // completes.
                    match cache_reader.get_object(id).expect("read cannot fail") {
                        Some(obj) => (*id, obj.version()),
                        None => (*id, *initial_version),
                    }
                })
                .collect();

            ret = izip!(ids.clone(), next_versions.into_iter(),)
                // take all the previously initialized versions
                .filter_map(|(id, next_version)| next_version.map(|v| (id, v)))
                // add all the versions we're going to write
                .chain(versions_to_write.iter().cloned())
                .collect();

            debug!(
                ?versions_to_write,
                "initializing next_shared_object_versions"
            );
            db_transaction.insert_batch(
                &self.tables()?.next_shared_object_versions,
                versions_to_write,
            )?;
            db_transaction.commit()
        })?;

        Ok(ret)
    }

    async fn set_assigned_shared_object_versions_with_db_batch(
        &self,
        versions: AssignedTxAndVersions,
        db_batch: &mut DBBatch,
    ) -> SuiResult {
        debug!("set_assigned_shared_object_versions: {:?}", versions);

        if self.randomness_state_enabled() {
            db_batch.insert_batch(&self.tables()?.assigned_shared_object_versions_v2, versions)?;
        } else {
            db_batch.insert_batch(
                &self.tables()?.assigned_shared_object_versions,
                versions
                    .iter()
                    .map(|(key, versions)| (key.unwrap_digest(), versions)),
            )?;
        }
        Ok(())
    }

    /// Given list of certificates, assign versions for all shared objects used in them.
    /// We start with the current next_shared_object_versions table for each object, and build
    /// up the versions based on the dependencies of each certificate.
    /// However, in the end we do not update the next_shared_object_versions table, which keeps
    /// this function idempotent. We should call this function when we are assigning shared object
    /// versions outside of consensus and do not want to taint the next_shared_object_versions table.
    pub async fn assign_shared_object_versions_idempotent(
        &self,
        cache_reader: &dyn ObjectCacheRead,
        certificates: &[VerifiedExecutableTransaction],
    ) -> SuiResult {
        let mut db_batch = self.tables()?.assigned_shared_object_versions.batch();
        let assigned_versions = SharedObjVerManager::assign_versions_from_consensus(
            self,
            cache_reader,
            certificates,
            None,
            &BTreeMap::new(),
        )
        .await?
        .assigned_versions;
        self.set_assigned_shared_object_versions_with_db_batch(assigned_versions, &mut db_batch)
            .await?;
        db_batch.write()?;
        Ok(())
    }

    fn defer_transactions(
        &self,
        batch: &mut DBBatch,
        key: DeferralKey,
        transactions: Vec<VerifiedSequencedConsensusTransaction>,
    ) -> SuiResult {
        batch.insert_batch(
            &self.tables()?.deferred_transactions,
            std::iter::once((key, transactions)),
        )?;
        Ok(())
    }

    fn load_deferred_transactions_for_randomness(
        &self,
        batch: &mut DBBatch,
    ) -> SuiResult<Vec<(DeferralKey, Vec<VerifiedSequencedConsensusTransaction>)>> {
        let (min, max) = DeferralKey::full_range_for_randomness();
        self.load_deferred_transactions(batch, min, max)
    }

    fn load_and_process_deferred_transactions_for_randomness(
        &self,
        batch: &mut DBBatch,
        previously_deferred_tx_digests: &mut HashMap<TransactionDigest, DeferralKey>,
        sequenced_randomness_transactions: &mut Vec<VerifiedSequencedConsensusTransaction>,
    ) -> SuiResult {
        let deferred_randomness_txs = self.load_deferred_transactions_for_randomness(batch)?;
        previously_deferred_tx_digests.extend(deferred_randomness_txs.iter().flat_map(
            |(deferral_key, txs)| {
                txs.iter().map(|tx| match tx.0.transaction.key() {
                    SequencedConsensusTransactionKey::External(
                        ConsensusTransactionKey::Certificate(digest),
                    ) => (digest, *deferral_key),
                    _ => {
                        panic!("deferred randomness transaction was not a user certificate: {tx:?}")
                    }
                })
            },
        ));
        sequenced_randomness_transactions
            .extend(deferred_randomness_txs.into_iter().flat_map(|(_, txs)| txs));
        Ok(())
    }

    fn load_deferred_transactions_for_up_to_consensus_round(
        &self,
        batch: &mut DBBatch,
        consensus_round: u64,
    ) -> SuiResult<Vec<(DeferralKey, Vec<VerifiedSequencedConsensusTransaction>)>> {
        let (min, max) = DeferralKey::range_for_up_to_consensus_round(consensus_round);
        self.load_deferred_transactions(batch, min, max)
    }

    // factoring of the above
    fn load_deferred_transactions(
        &self,
        batch: &mut DBBatch,
        min: DeferralKey,
        max: DeferralKey,
    ) -> SuiResult<Vec<(DeferralKey, Vec<VerifiedSequencedConsensusTransaction>)>> {
        debug!("Query epoch store to load deferred txn {:?} {:?}", min, max);
        let mut keys = Vec::new();
        let mut txns = Vec::new();
        self.tables()?
            .deferred_transactions
            .safe_iter_with_bounds(Some(min), Some(max))
            .try_for_each(|result| match result {
                Ok((key, txs)) => {
                    debug!(
                        "Loaded {:?} deferred txn with deferral key {:?}",
                        txs.len(),
                        key
                    );
                    keys.push(key);
                    txns.push((key, txs));
                    Ok(())
                }
                Err(err) => Err(err),
            })?;

        // verify that there are no duplicates - should be impossible due to
        // is_consensus_message_processed
        #[cfg(debug_assertions)]
        {
            let mut seen = HashSet::new();
            for deferred_txn_batch in &txns {
                for txn in &deferred_txn_batch.1 {
                    assert!(seen.insert(txn.0.key()));
                }
            }
        }

        // Transactional DBs do not support range deletes, so we have to delete keys one-by-one.
        // This shouldn't be a problem, there should not usually be more than a small handful of
        // keys loaded in each round.
        batch.delete_batch(&self.tables()?.deferred_transactions, keys)?;

        Ok(txns)
    }

    pub fn get_all_deferred_transactions_for_test(
        &self,
    ) -> SuiResult<Vec<(DeferralKey, Vec<VerifiedSequencedConsensusTransaction>)>> {
        Ok(self
            .tables()?
            .deferred_transactions
            .safe_iter()
            .collect::<Result<Vec<_>, _>>()?)
    }

    fn should_defer(
        &self,
        cert: &VerifiedExecutableTransaction,
        commit_round: Round,
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
            let deferred_from_round = previously_deferred_tx_digests
                .get(cert.digest())
                .map(|previous_key| previous_key.deferred_from_round())
                .unwrap_or(commit_round);
            return Some((
                DeferralKey::new_for_randomness(deferred_from_round),
                DeferralReason::RandomnessNotReady,
            ));
        }

        if let Some(max_accumulated_txn_cost_per_object_in_checkpoint) = self
            .protocol_config()
            .max_accumulated_txn_cost_per_object_in_checkpoint_as_option()
        {
            // Defer transaction if it uses shared objects that are congested.
            if let Some((deferral_key, congested_objects)) = shared_object_congestion_tracker
                .should_defer_due_to_object_congestion(
                    cert,
                    max_accumulated_txn_cost_per_object_in_checkpoint,
                    previously_deferred_tx_digests,
                    commit_round,
                )
            {
                Some((
                    deferral_key,
                    DeferralReason::SharedObjectCongestion(congested_objects),
                ))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Lock a sequence number for the shared objects of the input transaction based on the effects
    /// of that transaction.
    /// Used by full nodes who don't listen to consensus, and validators who catch up by state sync.
    // TODO: We should be able to pass in a vector of certs/effects and lock them all at once.
    #[instrument(level = "trace", skip_all)]
    pub async fn acquire_shared_locks_from_effects(
        &self,
        certificate: &VerifiedExecutableTransaction,
        effects: &TransactionEffects,
        cache_reader: &dyn ObjectCacheRead,
    ) -> SuiResult {
        let versions = SharedObjVerManager::assign_versions_from_effects(
            &[(certificate, effects)],
            self,
            cache_reader,
        )
        .await?;
        let mut db_batch = self.tables()?.assigned_shared_object_versions.batch();
        self.set_assigned_shared_object_versions_with_db_batch(versions, &mut db_batch)
            .await?;
        db_batch.write()?;
        Ok(())
    }

    /// When submitting a certificate caller **must** provide a ReconfigState lock guard
    /// and verify that it allows new user certificates
    pub fn insert_pending_consensus_transactions(
        &self,
        transactions: &[ConsensusTransaction],
        lock: Option<&RwLockReadGuard<ReconfigState>>,
    ) -> SuiResult {
        let key_value_pairs = transactions.iter().map(|tx| (tx.key(), tx));
        self.tables()?
            .pending_consensus_transactions
            .multi_insert(key_value_pairs)?;

        // TODO: lock once for all insert() calls.
        for transaction in transactions {
            if let ConsensusTransactionKind::UserTransaction(cert) = &transaction.kind {
                let state = lock.expect("Must pass reconfiguration lock when storing certificate");
                // Caller is responsible for performing graceful check
                assert!(
                    state.should_accept_user_certs(),
                    "Reconfiguration state should allow accepting user transactions"
                );
                self.pending_consensus_certificates
                    .lock()
                    .insert(*cert.digest());
            }
        }
        Ok(())
    }

    pub fn remove_pending_consensus_transactions(
        &self,
        keys: &[ConsensusTransactionKey],
    ) -> SuiResult {
        self.tables()?
            .pending_consensus_transactions
            .multi_remove(keys)?;
        // TODO: lock once for all remove() calls.
        for key in keys {
            if let ConsensusTransactionKey::Certificate(cert) = key {
                self.pending_consensus_certificates.lock().remove(cert);
            }
        }
        Ok(())
    }

    pub fn pending_consensus_certificates_count(&self) -> usize {
        self.pending_consensus_certificates.lock().len()
    }

    pub fn pending_consensus_certificates_empty(&self) -> bool {
        self.pending_consensus_certificates.lock().is_empty()
    }

    pub fn pending_consensus_certificates(&self) -> HashSet<TransactionDigest> {
        self.pending_consensus_certificates.lock().clone()
    }

    pub fn deferred_transactions_empty(&self) -> bool {
        self.tables()
            .expect("deferred transactions should not be read past end of epoch")
            .deferred_transactions
            .is_empty()
    }

    /// Stores a list of pending certificates to be executed.
    pub fn insert_pending_execution(
        &self,
        certs: &[TrustedExecutableTransaction],
    ) -> SuiResult<()> {
        let mut batch = self.tables()?.pending_execution.batch();
        batch.insert_batch(
            &self.tables()?.pending_execution,
            certs
                .iter()
                .map(|cert| (*cert.inner().digest(), cert.clone())),
        )?;
        batch.write()?;
        Ok(())
    }

    /// Check whether certificate was processed by consensus.
    /// For shared lock certificates, if this function returns true means shared locks for this certificate are set
    pub fn is_tx_cert_consensus_message_processed(
        &self,
        certificate: &CertifiedTransaction,
    ) -> SuiResult<bool> {
        self.is_consensus_message_processed(&SequencedConsensusTransactionKey::External(
            ConsensusTransactionKey::Certificate(*certificate.digest()),
        ))
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

    /// Check whether any certificates were processed by consensus.
    /// This handles multiple certificates at once.
    pub fn is_all_tx_certs_consensus_message_processed<'a>(
        &self,
        certificates: impl Iterator<Item = &'a VerifiedCertificate>,
    ) -> SuiResult<bool> {
        let keys = certificates.map(|cert| {
            SequencedConsensusTransactionKey::External(ConsensusTransactionKey::Certificate(
                *cert.digest(),
            ))
        });
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
            .tables()?
            .consensus_message_processed
            .contains_key(key)?)
    }

    pub fn check_consensus_messages_processed(
        &self,
        keys: impl Iterator<Item = SequencedConsensusTransactionKey>,
    ) -> SuiResult<Vec<bool>> {
        Ok(self
            .tables()?
            .consensus_message_processed
            .multi_contains_keys(keys)?)
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

    pub fn has_sent_end_of_publish(&self, authority: &AuthorityName) -> SuiResult<bool> {
        Ok(self
            .end_of_publish
            .try_lock()
            .expect("No contention on end_of_publish lock")
            .contains_key(authority))
    }

    // Converts transaction keys to digests, waiting for digests to become available for any
    // non-digest keys.
    pub async fn notify_read_executed_digests(
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

    /// Note: caller usually need to call consensus_message_processed_notify before this call
    pub fn user_signatures_for_checkpoint(
        &self,
        transactions: &[VerifiedTransaction],
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Vec<GenericSignature>>> {
        assert_eq!(transactions.len(), digests.len());
        let signatures = self
            .tables()?
            .user_signatures_for_checkpoints
            .multi_get(digests)?;
        let mut result = Vec::with_capacity(digests.len());
        for (signatures, transaction) in signatures.into_iter().zip(transactions.iter()) {
            let signatures = if let Some(signatures) = signatures {
                signatures
            } else if matches!(
                transaction.inner().transaction_data().kind(),
                TransactionKind::RandomnessStateUpdate(_)
            ) {
                // RandomnessStateUpdate transactions don't go through consensus, but
                // have system-generated signatures that are guaranteed to be the same,
                // so we can just pull it from the transaction.
                transaction.tx_signatures().to_vec()
            } else {
                return Err(SuiError::from(
                    format!(
                        "Can not find user signature for checkpoint for transaction {:?}",
                        transaction.key()
                    )
                    .as_str(),
                ));
            };
            result.push(signatures);
        }
        Ok(result)
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
    pub fn record_capabilities(&self, capabilities: &AuthorityCapabilities) -> SuiResult {
        info!("received capabilities {:?}", capabilities);
        let authority = &capabilities.authority;

        // Read-compare-write pattern assumes we are only called from the consensus handler task.
        if let Some(cap) = self.tables()?.authority_capabilities.get(authority)? {
            if cap.generation >= capabilities.generation {
                debug!(
                    "ignoring new capabilities {:?} in favor of previous capabilities {:?}",
                    capabilities, cap
                );
                return Ok(());
            }
        }
        self.tables()?
            .authority_capabilities
            .insert(authority, capabilities)?;
        Ok(())
    }

    pub fn get_capabilities(&self) -> SuiResult<Vec<AuthorityCapabilities>> {
        let result: Result<Vec<AuthorityCapabilities>, TypedStoreError> = self
            .tables()?
            .authority_capabilities
            .values()
            .map_into()
            .collect();
        Ok(result?)
    }

    pub fn record_jwk_vote(
        &self,
        batch: &mut DBBatch,
        round: u64,
        authority: AuthorityName,
        id: &JwkId,
        jwk: &JWK,
    ) -> SuiResult {
        info!(
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
            return Ok(());
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
            return Ok(());
        }

        batch.insert_batch(
            &self.tables()?.pending_jwks,
            std::iter::once(((authority, id.clone(), jwk.clone()), ())),
        )?;

        let key = (id.clone(), jwk.clone());
        let previously_active = jwk_aggregator.has_quorum_for_key(&key);
        let insert_result = jwk_aggregator.insert(authority, key.clone());

        if !previously_active && insert_result.is_quorum_reached() {
            info!(epoch = ?self.epoch(), ?round, jwk = ?key, "jwk became active");
            batch.insert_batch(
                &self.tables()?.active_jwks,
                std::iter::once(((round, key), ())),
            )?;
        }

        Ok(())
    }

    pub(crate) fn get_new_jwks(&self, round: u64) -> SuiResult<Vec<ActiveJwk>> {
        let epoch = self.epoch();

        let empty_jwk_id = JwkId::new(String::new(), String::new());
        let empty_jwk = JWK {
            kty: String::new(),
            e: String::new(),
            n: String::new(),
            alg: String::new(),
        };

        let start = (round, (empty_jwk_id.clone(), empty_jwk.clone()));
        let end = (round + 1, (empty_jwk_id, empty_jwk));

        // TODO: use a safe iterator
        Ok(self
            .tables()?
            .active_jwks
            .safe_iter_with_bounds(Some(start), Some(end))
            .map_ok(|((r, (jwk_id, jwk)), _)| {
                debug_assert!(round == r);
                ActiveJwk { jwk_id, jwk, epoch }
            })
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn jwk_active_in_current_epoch(&self, jwk_id: &JwkId, jwk: &JWK) -> bool {
        let jwk_aggregator = self.jwk_aggregator.lock();
        jwk_aggregator.has_quorum_for_key(&(jwk_id.clone(), jwk.clone()))
    }

    /// Record when finished processing a transaction from consensus.
    fn record_consensus_message_processed(
        &self,
        batch: &mut DBBatch,
        key: SequencedConsensusTransactionKey,
    ) -> SuiResult {
        batch.insert_batch(&self.tables()?.consensus_message_processed, [(key, true)])?;
        Ok(())
    }

    /// Record when finished processing a consensus commit.
    fn record_consensus_commit_stats(
        &self,
        batch: &mut DBBatch,
        consensus_stats: &ExecutionIndicesWithStats,
    ) -> SuiResult {
        // TODO: remove writing to last_consensus_index.
        batch.insert_batch(
            &self.tables()?.last_consensus_index,
            [(
                LAST_CONSENSUS_STATS_ADDR,
                ExecutionIndicesWithHash {
                    index: consensus_stats.index,
                    hash: consensus_stats.hash,
                },
            )],
        )?;
        batch.insert_batch(
            &self.tables()?.last_consensus_stats,
            [(LAST_CONSENSUS_STATS_ADDR, consensus_stats)],
        )?;
        Ok(())
    }

    pub fn test_insert_user_signature(
        &self,
        digest: TransactionDigest,
        signatures: Vec<GenericSignature>,
    ) {
        self.tables()
            .expect("test should not cross epoch boundary")
            .user_signatures_for_checkpoints
            .insert(&digest, &signatures)
            .unwrap();
        let key = ConsensusTransactionKey::Certificate(digest);
        let key = SequencedConsensusTransactionKey::External(key);
        self.tables()
            .expect("test should not cross epoch boundary")
            .consensus_message_processed
            .insert(&key, &true)
            .unwrap();
        self.consensus_notify_read.notify(&key, &());
    }

    pub fn finish_consensus_certificate_process_with_batch(
        &self,
        batch: &mut DBBatch,
        certificates: &[VerifiedExecutableTransaction],
    ) -> SuiResult {
        for certificate in certificates {
            batch.insert_batch(
                &self.tables()?.pending_execution,
                [(*certificate.digest(), certificate.clone().serializable())],
            )?;
            // User signatures are written in the same batch as consensus certificate processed flag,
            // which means we won't attempt to insert this twice for the same tx digest
            debug_assert!(!self
                .tables()?
                .user_signatures_for_checkpoints
                .contains_key(certificate.digest())?);
            batch.insert_batch(
                &self.tables()?.user_signatures_for_checkpoints,
                [(*certificate.digest(), certificate.tx_signatures().to_vec())],
            )?;
        }
        Ok(())
    }

    pub fn get_reconfig_state_read_lock_guard(&self) -> RwLockReadGuard<ReconfigState> {
        self.reconfig_state_mem.read()
    }

    pub fn get_reconfig_state_write_lock_guard(&self) -> RwLockWriteGuard<ReconfigState> {
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
        debug!("Epoch terminated - waiting for pending tasks to complete");
        *self.epoch_alive.write().await = false;
        debug!("All pending epoch tasks completed");
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
    fn verify_consensus_transaction(
        &self,
        transaction: SequencedConsensusTransaction,
        skipped_consensus_txns: &IntCounter,
    ) -> Option<VerifiedSequencedConsensusTransaction> {
        let _scope = monitored_scope("VerifyConsensusTransaction");
        if self
            .is_consensus_message_processed(&transaction.transaction.key())
            .expect("Storage error")
        {
            debug!(
                consensus_index=?transaction.consensus_index.transaction_index,
                tracking_id=?transaction.transaction.get_tracking_id(),
                "handle_consensus_transaction UserTransaction [skip]",
            );
            skipped_consensus_txns.inc();
            return None;
        }
        // Signatures are verified as part of the consensus payload verification in SuiTxValidator
        match &transaction.transaction {
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::UserTransaction(_certificate),
                ..
            }) => {}
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::CheckpointSignature(data),
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
                kind: ConsensusTransactionKind::CapabilityNotification(capabilities),
                ..
            }) => {
                if transaction.sender_authority() != capabilities.authority {
                    warn!(
                        "CapabilityNotification authority {} does not match its author from consensus {}",
                        capabilities.authority, transaction.certificate_author_index
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
            SequencedConsensusTransactionKind::System(_) => {}
        }
        Some(VerifiedSequencedConsensusTransaction(transaction))
    }

    fn db_batch(&self) -> SuiResult<DBBatch> {
        Ok(self.tables()?.last_consensus_index.batch())
    }

    #[cfg(test)]
    pub fn db_batch_for_test(&self) -> DBBatch {
        self.db_batch()
            .expect("test should not be write past end of epoch")
    }

    #[instrument(level = "debug", skip_all)]
    pub(crate) async fn process_consensus_transactions_and_commit_boundary<
        'a,
        C: CheckpointServiceNotify,
    >(
        &self,
        transactions: Vec<SequencedConsensusTransaction>,
        consensus_stats: &ExecutionIndicesWithStats,
        checkpoint_service: &Arc<C>,
        cache_reader: &dyn ObjectCacheRead,
        consensus_commit_info: &ConsensusCommitInfo,
        authority_metrics: &Arc<AuthorityMetrics>,
    ) -> SuiResult<Vec<VerifiedExecutableTransaction>> {
        // Split transactions into different types for processing.
        let verified_transactions: Vec<_> = transactions
            .into_iter()
            .filter_map(|transaction| {
                self.verify_consensus_transaction(
                    transaction,
                    &authority_metrics.skipped_consensus_txns,
                )
            })
            .collect();
        let mut system_transactions = Vec::with_capacity(verified_transactions.len());
        let mut current_commit_sequenced_consensus_transactions =
            Vec::with_capacity(verified_transactions.len());
        let mut current_commit_sequenced_randomness_transactions =
            Vec::with_capacity(verified_transactions.len());
        let mut end_of_publish_transactions = Vec::with_capacity(verified_transactions.len());
        for tx in verified_transactions {
            if tx.0.is_end_of_publish() {
                end_of_publish_transactions.push(tx);
            } else if tx.0.is_system() {
                system_transactions.push(tx);
            } else if tx
                .0
                .is_user_tx_with_randomness(self.randomness_state_enabled())
            {
                current_commit_sequenced_randomness_transactions.push(tx);
            } else {
                current_commit_sequenced_consensus_transactions.push(tx);
            }
        }
        let mut batch = self
            .db_batch()
            .expect("Failed to create DBBatch for processing consensus transactions");

        // Load transactions deferred from previous commits.
        let deferred_txs: Vec<(DeferralKey, Vec<VerifiedSequencedConsensusTransaction>)> = self
            .load_deferred_transactions_for_up_to_consensus_round(
                &mut batch,
                consensus_commit_info.round,
            )?
            .into_iter()
            .collect();
        let mut previously_deferred_tx_digests: HashMap<TransactionDigest, DeferralKey> =
            deferred_txs
                .iter()
                .flat_map(|(deferral_key, txs)| {
                    txs.iter().map(|tx| match tx.0.transaction.key() {
                        SequencedConsensusTransactionKey::External(
                            ConsensusTransactionKey::Certificate(digest),
                        ) => (digest, *deferral_key),
                        _ => panic!("deferred transaction was not a user certificate: {tx:?}"),
                    })
                })
                .collect();

        // Sequenced_transactions and sequenced_randomness_transactions store all transactions that will be sent to
        // process_consensus_transactions. We put deferred transactions at the beginning of the list before
        // PostConsensusTxReorder::reorder, so that for transactions with the same gas price, deferred transactions
        // will be placed earlier in the execution queue.
        let mut sequenced_transactions: Vec<VerifiedSequencedConsensusTransaction> =
            Vec::with_capacity(
                current_commit_sequenced_consensus_transactions.len()
                    + previously_deferred_tx_digests.len(),
            );
        let mut sequenced_randomness_transactions: Vec<VerifiedSequencedConsensusTransaction> =
            Vec::with_capacity(
                current_commit_sequenced_randomness_transactions.len()
                    + previously_deferred_tx_digests.len(),
            );

        let mut randomness_manager = self.randomness_manager.get().map(|rm| {
            rm.try_lock()
                .expect("should only ever be called from the commit handler thread")
        });
        let mut dkg_failed = false;
        let randomness_round = if self.randomness_state_enabled() {
            let randomness_manager = randomness_manager
                .as_mut()
                .expect("randomness manager should exist if randomness is enabled");
            match randomness_manager.dkg_status() {
                DkgStatus::Pending => None,
                DkgStatus::Failed => {
                    dkg_failed = true;
                    None
                }
                DkgStatus::Successful => {
                    // Generate randomness for this commit if DKG is successful and we are still
                    // accepting certs.
                    if self
                        // It is ok to just release lock here as functions called by this one are the
                        // only place that transition into RejectAllCerts state, and this function
                        // itself is always executed from consensus task.
                        .get_reconfig_state_read_lock_guard()
                        .should_accept_tx()
                    {
                        randomness_manager
                            .reserve_next_randomness(consensus_commit_info.timestamp, &mut batch)?
                    } else {
                        None
                    }
                }
            }
        } else {
            None
        };

        // We should load any previously-deferred randomness-using tx:
        // - if DKG is failed, so we can ignore them
        // - if randomness is being generated, so we can process them
        if dkg_failed || randomness_round.is_some() {
            self.load_and_process_deferred_transactions_for_randomness(
                &mut batch,
                &mut previously_deferred_tx_digests,
                &mut sequenced_randomness_transactions,
            )?;
        }

        // Add ConsensusRound deferred tx back into the sequence.
        for tx in deferred_txs
            .into_iter()
            .flat_map(|(_, txs)| txs.into_iter())
        {
            if tx
                .0
                .is_user_tx_with_randomness(self.randomness_state_enabled())
            {
                sequenced_randomness_transactions.push(tx);
            } else {
                sequenced_transactions.push(tx);
            }
        }
        sequenced_transactions.extend(current_commit_sequenced_consensus_transactions);
        sequenced_randomness_transactions.extend(current_commit_sequenced_randomness_transactions);

        // Save roots for checkpoint generation. One set for most tx, one for randomness tx.
        let mut roots: BTreeSet<_> = system_transactions
            .iter()
            .chain(sequenced_transactions.iter())
            // no need to include end_of_publish_transactions here because they would be
            // filtered out below by `executable_transaction_digest` anyway
            .filter_map(|transaction| {
                transaction
                    .0
                    .transaction
                    .executable_transaction_digest()
                    .map(TransactionKey::Digest)
            })
            .collect();
        let mut randomness_roots: BTreeSet<_> = sequenced_randomness_transactions
            .iter()
            .filter_map(|transaction| {
                transaction
                    .0
                    .transaction
                    .executable_transaction_digest()
                    .map(TransactionKey::Digest)
            })
            .collect();

        // We always order transactions using randomness last.
        PostConsensusTxReorder::reorder(
            &mut sequenced_transactions,
            self.protocol_config.consensus_transaction_ordering(),
        );
        PostConsensusTxReorder::reorder(
            &mut sequenced_randomness_transactions,
            self.protocol_config.consensus_transaction_ordering(),
        );
        let consensus_transactions: Vec<_> = system_transactions
            .into_iter()
            .chain(sequenced_transactions)
            .chain(sequenced_randomness_transactions)
            .collect();

        let (
            transactions_to_schedule,
            notifications,
            lock,
            final_round,
            consensus_commit_prologue_root,
        ) = self
            .process_consensus_transactions(
                &mut batch,
                &consensus_transactions,
                &end_of_publish_transactions,
                checkpoint_service,
                cache_reader,
                consensus_commit_info,
                &mut roots,
                &mut randomness_roots,
                previously_deferred_tx_digests,
                randomness_manager.as_deref_mut(),
                dkg_failed,
                randomness_round,
                authority_metrics,
            )
            .await?;
        self.finish_consensus_certificate_process_with_batch(
            &mut batch,
            &transactions_to_schedule,
        )?;
        self.record_consensus_commit_stats(&mut batch, consensus_stats)?;

        // Create pending checkpoints if we are still accepting tx.
        let should_accept_tx = if let Some(lock) = &lock {
            lock.should_accept_tx()
        } else {
            // It is ok to just release lock here as functions called by this one are the
            // only place that transition reconfig state, and this function itself is always
            // executed from consensus task. At this point if the lock was not already provided
            // above, we know we won't be transitioning state for this commit.
            self.get_reconfig_state_read_lock_guard().should_accept_tx()
        };
        let make_checkpoint = should_accept_tx || final_round;
        if make_checkpoint {
            // Generate pending checkpoint for regular user tx.
            let checkpoint_height = if self.randomness_state_enabled() {
                consensus_commit_info.round * 2
            } else {
                consensus_commit_info.round
            };

            let mut checkpoint_roots: Vec<TransactionKey> = Vec::with_capacity(roots.len() + 1);

            if let Some(consensus_commit_prologue_root) = consensus_commit_prologue_root {
                if self
                    .protocol_config()
                    .prepend_prologue_tx_in_consensus_commit_in_checkpoints()
                {
                    // Put consensus commit prologue root at the beginning of the checkpoint roots.
                    checkpoint_roots.push(consensus_commit_prologue_root);
                } else {
                    roots.insert(consensus_commit_prologue_root);
                }
            }
            checkpoint_roots.extend(roots.into_iter());
            let pending_checkpoint = PendingCheckpointV2::V2(PendingCheckpointV2Contents {
                roots: checkpoint_roots,
                details: PendingCheckpointInfo {
                    timestamp_ms: consensus_commit_info.timestamp,
                    last_of_epoch: final_round && randomness_round.is_none(),
                    checkpoint_height,
                },
            });
            self.write_pending_checkpoint(&mut batch, &pending_checkpoint)?;

            // Generate pending checkpoint for user tx with randomness.
            // Note if randomness is not generated for this commit, we will skip the
            // checkpoint with the associated height. Therefore checkpoint heights may
            // not be contiguous.
            if let Some(randomness_round) = randomness_round {
                randomness_roots.insert(TransactionKey::RandomnessRound(
                    self.epoch(),
                    randomness_round,
                ));

                let pending_checkpoint = PendingCheckpointV2::V2(PendingCheckpointV2Contents {
                    roots: randomness_roots.into_iter().collect(),
                    details: PendingCheckpointInfo {
                        timestamp_ms: consensus_commit_info.timestamp,
                        last_of_epoch: final_round,
                        checkpoint_height: checkpoint_height + 1,
                    },
                });
                self.write_pending_checkpoint(&mut batch, &pending_checkpoint)?;
            }
        }

        batch.write()?;

        // Only after batch is written, notify checkpoint service to start building any new
        // pending checkpoints.
        if make_checkpoint {
            debug!(
                ?consensus_commit_info.round,
                "Notifying checkpoint service about new pending checkpoint(s)",
            );
            checkpoint_service.notify_checkpoint()?;
        }

        // Once commit processing is recorded, kick off randomness generation.
        if let Some(randomness_round) = randomness_round {
            let epoch = self.epoch();
            randomness_manager
                .as_ref()
                .expect("randomness manager should exist if randomness round is provided")
                .generate_randomness(epoch, randomness_round);
        }

        self.process_notifications(&notifications, &end_of_publish_transactions);

        if final_round {
            info!(
                epoch=?self.epoch(),
                // Accessing lock on purpose so that the compiler ensures
                // the lock is not yet dropped.
                lock=?lock.as_ref(),
                final_round=?final_round,
                "Notified last checkpoint"
            );
            self.record_end_of_message_quorum_time_metric();
        }

        Ok(transactions_to_schedule)
    }

    // Adds the consensus commit prologue transaction to the beginning of input `transactions` to update
    // the system clock used in all transactions in the current consensus commit.
    // Returns the root of the consensus commit prologue transaction if it was added to the input.
    fn add_consensus_commit_prologue_transaction(
        &self,
        batch: &mut DBBatch,
        transactions: &mut VecDeque<VerifiedExecutableTransaction>,
        consensus_commit_info: &ConsensusCommitInfo,
        cancelled_txns: &BTreeMap<TransactionDigest, CancelConsensusCertificateReason>,
    ) -> SuiResult<Option<TransactionKey>> {
        #[cfg(any(test, feature = "test-utils"))]
        {
            if consensus_commit_info.skip_consensus_commit_prologue_in_test() {
                return Ok(None);
            }
        }

        let mut version_assignment: Vec<(TransactionDigest, Vec<(ObjectID, SequenceNumber)>)> =
            Vec::new();

        let mut shared_input_next_version = HashMap::new();
        for txn in transactions.iter() {
            if let Some(CancelConsensusCertificateReason::CongestionOnObjects(_)) =
                cancelled_txns.get(txn.digest())
            {
                let assigned_versions = SharedObjVerManager::assign_versions_for_certificate(
                    txn,
                    &mut shared_input_next_version,
                    cancelled_txns,
                );

                version_assignment.push((*txn.digest(), assigned_versions));
            }
        }

        fail_point_arg!(
            "additional_cancelled_txns_for_tests",
            |additional_cancelled_txns: Vec<(
                TransactionDigest,
                Vec<(ObjectID, SequenceNumber)>
            )>| {
                version_assignment.extend(additional_cancelled_txns);
            }
        );

        let transaction = consensus_commit_info.create_consensus_commit_prologue_transaction(
            self.epoch(),
            self.protocol_config(),
            version_assignment,
        );
        let consensus_commit_prologue_root = match self.process_consensus_system_transaction(&transaction) {
            ConsensusCertificateResult::SuiTransaction(processed_tx) => {
                transactions.push_front(processed_tx.clone());
                Some(processed_tx.key())
            }
            ConsensusCertificateResult::IgnoredSystem => None,
            _ => unreachable!("process_consensus_system_transaction returned unexpected ConsensusCertificateResult."),
        };

        self.record_consensus_message_processed(
            batch,
            SequencedConsensusTransactionKey::System(*transaction.digest()),
        )?;
        Ok(consensus_commit_prologue_root)
    }

    // Assigns shared object versions to transactions and updates the shared object version state.
    // Shared object versions in cancelled transactions are assigned to special versions that will
    // cause the transactions to be cancelled in execution engine.
    async fn process_consensus_transaction_shared_object_versions(
        &self,
        cache_reader: &dyn ObjectCacheRead,
        transactions: &[VerifiedExecutableTransaction],
        randomness_round: Option<RandomnessRound>,
        cancelled_txns: &BTreeMap<TransactionDigest, CancelConsensusCertificateReason>,
        db_batch: &mut DBBatch,
    ) -> SuiResult {
        let ConsensusSharedObjVerAssignment {
            shared_input_next_versions,
            assigned_versions,
        } = SharedObjVerManager::assign_versions_from_consensus(
            self,
            cache_reader,
            transactions,
            randomness_round,
            cancelled_txns,
        )
        .await?;
        self.set_assigned_shared_object_versions_with_db_batch(assigned_versions, db_batch)
            .await?;
        db_batch.insert_batch(
            &self.tables()?.next_shared_object_versions,
            shared_input_next_versions,
        )?;
        Ok(())
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn get_highest_pending_checkpoint_height(&self) -> CheckpointHeight {
        if self.randomness_state_enabled() {
            self.tables()
                .expect("test should not cross epoch boundary")
                .pending_checkpoints_v2
                .unbounded_iter()
                .skip_to_last()
                .next()
                .map(|(key, _)| key)
                .unwrap_or_default()
        } else {
            self.tables()
                .expect("test should not cross epoch boundary")
                .pending_checkpoints
                .unbounded_iter()
                .skip_to_last()
                .next()
                .map(|(key, _)| key)
                .unwrap_or_default()
        }
    }

    // Caller is not required to set ExecutionIndices with the right semantics in
    // VerifiedSequencedConsensusTransaction.
    // Also, ConsensusStats and hash will not be updated in the db with this function, unlike in
    // process_consensus_transactions_and_commit_boundary().
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn process_consensus_transactions_for_tests<C: CheckpointServiceNotify>(
        self: &Arc<Self>,
        transactions: Vec<SequencedConsensusTransaction>,
        checkpoint_service: &Arc<C>,
        cache_reader: &dyn ObjectCacheRead,
        authority_metrics: &Arc<AuthorityMetrics>,
        skip_consensus_commit_prologue_in_test: bool,
    ) -> SuiResult<Vec<VerifiedExecutableTransaction>> {
        self.process_consensus_transactions_and_commit_boundary(
            transactions,
            &ExecutionIndicesWithStats::default(),
            checkpoint_service,
            cache_reader,
            &ConsensusCommitInfo::new_for_test(
                self.get_highest_pending_checkpoint_height() + 1,
                0,
                skip_consensus_commit_prologue_in_test,
            ),
            authority_metrics,
        )
        .await
    }

    fn process_notifications(
        &self,
        notifications: &[SequencedConsensusTransactionKey],
        end_of_publish: &[VerifiedSequencedConsensusTransaction],
    ) {
        for key in notifications
            .iter()
            .cloned()
            .chain(end_of_publish.iter().map(|tx| tx.0.transaction.key()))
        {
            self.consensus_notify_read.notify(&key, &());
        }
    }

    /// Depending on the type of the VerifiedSequencedConsensusTransaction wrappers,
    /// - Verify and initialize the state to execute the certificates.
    ///   Return VerifiedCertificates for each executable certificate
    /// - Or update the state for checkpoint or epoch change protocol.
    #[instrument(level = "debug", skip_all)]
    #[allow(clippy::type_complexity)]
    pub(crate) async fn process_consensus_transactions<C: CheckpointServiceNotify>(
        &self,
        batch: &mut DBBatch,
        transactions: &[VerifiedSequencedConsensusTransaction],
        end_of_publish_transactions: &[VerifiedSequencedConsensusTransaction],
        checkpoint_service: &Arc<C>,
        cache_reader: &dyn ObjectCacheRead,
        consensus_commit_info: &ConsensusCommitInfo,
        roots: &mut BTreeSet<TransactionKey>,
        randomness_roots: &mut BTreeSet<TransactionKey>,
        previously_deferred_tx_digests: HashMap<TransactionDigest, DeferralKey>,
        mut randomness_manager: Option<&mut RandomnessManager>,
        dkg_failed: bool,
        randomness_round: Option<RandomnessRound>,
        authority_metrics: &Arc<AuthorityMetrics>,
    ) -> SuiResult<(
        Vec<VerifiedExecutableTransaction>,    // transactions to schedule
        Vec<SequencedConsensusTransactionKey>, // keys to notify as complete
        Option<RwLockWriteGuard<ReconfigState>>,
        bool,                   // true if final round
        Option<TransactionKey>, // consensus commit prologue root
    )> {
        if randomness_round.is_some() {
            assert!(!dkg_failed); // invariant check
        }

        let mut verified_certificates = VecDeque::with_capacity(transactions.len() + 1);
        let mut notifications = Vec::with_capacity(transactions.len());

        let mut deferred_txns: BTreeMap<DeferralKey, Vec<VerifiedSequencedConsensusTransaction>> =
            BTreeMap::new();
        let mut cancelled_txns: BTreeMap<TransactionDigest, CancelConsensusCertificateReason> =
            BTreeMap::new();

        // We track transaction execution cost separately for regular transactions and transactions using randomness, since
        // they will be in different checkpoints.
        let mut shared_object_congestion_tracker = SharedObjectCongestionTracker::new(
            self.protocol_config().per_object_congestion_control_mode(),
        );
        let mut shared_object_using_randomness_congestion_tracker =
            SharedObjectCongestionTracker::new(
                self.protocol_config().per_object_congestion_control_mode(),
            );

        fail_point_arg!(
            "initial_congestion_tracker",
            |tracker: SharedObjectCongestionTracker| {
                info!(
                    "Initialize shared_object_congestion_tracker to  {:?}",
                    tracker
                );
                shared_object_congestion_tracker = tracker;
            }
        );

        let mut randomness_state_updated = false;
        for tx in transactions {
            let key = tx.0.transaction.key();
            let mut ignored = false;
            let mut filter_roots = false;
            let execution_cost = if tx
                .0
                .is_user_tx_with_randomness(self.randomness_state_enabled())
            {
                &mut shared_object_using_randomness_congestion_tracker
            } else {
                &mut shared_object_congestion_tracker
            };
            match self
                .process_consensus_transaction(
                    batch,
                    tx,
                    checkpoint_service,
                    consensus_commit_info.round,
                    &previously_deferred_tx_digests,
                    randomness_manager.as_deref_mut(),
                    dkg_failed,
                    randomness_round.is_some(),
                    execution_cost,
                    authority_metrics,
                )
                .await?
            {
                ConsensusCertificateResult::SuiTransaction(cert) => {
                    notifications.push(key.clone());
                    verified_certificates.push_back(cert);
                }
                ConsensusCertificateResult::Deferred(deferral_key) => {
                    // Note: record_consensus_message_processed() must be called for this
                    // cert even though we are not processing it now!
                    deferred_txns
                        .entry(deferral_key)
                        .or_default()
                        .push(tx.clone());
                    filter_roots = true;
                    if tx.0.transaction.is_executable_transaction() {
                        // Notify consensus adapter that the consensus handler has received the transaction.
                        notifications.push(key.clone());
                    }
                }
                ConsensusCertificateResult::Cancelled((cert, reason)) => {
                    notifications.push(key.clone());
                    assert!(cancelled_txns.insert(*cert.digest(), reason).is_none());
                    verified_certificates.push_back(cert);
                }
                ConsensusCertificateResult::RandomnessConsensusMessage => {
                    randomness_state_updated = true;
                    notifications.push(key.clone());
                }
                ConsensusCertificateResult::ConsensusMessage => notifications.push(key.clone()),
                ConsensusCertificateResult::IgnoredSystem => {
                    filter_roots = true;
                }
                // Note: ignored external transactions must not be recorded as processed. Otherwise
                // they may not get reverted after restart during epoch change.
                ConsensusCertificateResult::Ignored => {
                    ignored = true;
                    filter_roots = true;
                }
            }
            if !ignored {
                self.record_consensus_message_processed(batch, key.clone())?;
            }
            if filter_roots {
                if let Some(txn_key) =
                    tx.0.transaction
                        .executable_transaction_digest()
                        .map(TransactionKey::Digest)
                {
                    roots.remove(&txn_key);
                    randomness_roots.remove(&txn_key);
                }
            }
        }

        let commit_has_deferred_txns = !deferred_txns.is_empty();
        let mut total_deferred_txns = 0;
        for (key, txns) in deferred_txns.into_iter() {
            total_deferred_txns += txns.len();
            self.defer_transactions(batch, key, txns)?;
        }
        authority_metrics
            .consensus_handler_deferred_transactions
            .inc_by(total_deferred_txns as u64);
        authority_metrics
            .consensus_handler_cancelled_transactions
            .inc_by(cancelled_txns.len() as u64);

        if randomness_state_updated {
            if let Some(randomness_manager) = randomness_manager.as_mut() {
                randomness_manager
                    .advance_dkg(batch, consensus_commit_info.round)
                    .await?;
            }
        }

        // Add the consensus commit prologue transaction to the beginning of `verified_certificates`.
        let consensus_commit_prologue_root = self.add_consensus_commit_prologue_transaction(
            batch,
            &mut verified_certificates,
            consensus_commit_info,
            &cancelled_txns,
        )?;

        let verified_certificates: Vec<_> = verified_certificates.into();

        self.process_consensus_transaction_shared_object_versions(
            cache_reader,
            &verified_certificates,
            randomness_round,
            &cancelled_txns,
            batch,
        )
        .await?;

        let (lock, final_round) = self.process_end_of_publish_transactions_and_reconfig(
            batch,
            end_of_publish_transactions,
            commit_has_deferred_txns,
        )?;

        Ok((
            verified_certificates,
            notifications,
            lock,
            final_round,
            consensus_commit_prologue_root,
        ))
    }

    fn process_end_of_publish_transactions_and_reconfig(
        &self,
        write_batch: &mut DBBatch,
        transactions: &[VerifiedSequencedConsensusTransaction],
        commit_has_deferred_txns: bool,
    ) -> SuiResult<(
        Option<RwLockWriteGuard<ReconfigState>>,
        bool, // true if final round
    )> {
        let mut lock = None;

        for transaction in transactions {
            let VerifiedSequencedConsensusTransaction(SequencedConsensusTransaction {
                transaction,
                ..
            }) = transaction;

            if let SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::EndOfPublish(authority),
                ..
            }) = transaction
            {
                debug!(
                    "Received EndOfPublish for epoch {} from {:?}",
                    self.committee.epoch,
                    authority.concise()
                );

                // It is ok to just release lock here as this function is the only place that transition into RejectAllCerts state
                // And this function itself is always executed from consensus task
                let collected_end_of_publish = if lock.is_none()
                    && self
                        .get_reconfig_state_read_lock_guard()
                        .should_accept_consensus_certs()
                {
                    write_batch.insert_batch(&self.tables()?.end_of_publish, [(authority, ())])?;
                    self.end_of_publish.try_lock()
                        .expect("No contention on Authority::end_of_publish as it is only accessed from consensus handler")
                        .insert_generic(*authority, ()).is_quorum_reached()
                    // end_of_publish lock is released here.
                } else {
                    // If we past the stage where we are accepting consensus certificates we also don't record end of publish messages
                    debug!("Ignoring end of publish message from validator {:?} as we already collected enough end of publish messages", authority.concise());
                    false
                };

                if collected_end_of_publish {
                    assert!(lock.is_none());
                    debug!(
                        "Collected enough end_of_publish messages for epoch {} with last message from validator {:?}",
                        self.committee.epoch,
                        authority.concise(),
                    );
                    let mut l = self.get_reconfig_state_write_lock_guard();
                    l.close_all_certs();
                    self.store_reconfig_state_batch(&l, write_batch)?;
                    // Holding this lock until end of process_consensus_transactions_and_commit_boundary() where we write batch to DB
                    lock = Some(l);
                };
                // Important: we actually rely here on fact that ConsensusHandler panics if its
                // operation returns error. If some day we won't panic in ConsensusHandler on error
                // we need to figure out here how to revert in-memory state of .end_of_publish
                // and .reconfig_state when write fails.
                self.record_consensus_message_processed(write_batch, transaction.key())?;
            } else {
                panic!(
                    "process_end_of_publish_transactions_and_reconfig called with non-end-of-publish transaction"
                );
            }
        }

        // Determine if we're ready to advance reconfig state to RejectAllTx.
        let is_reject_all_certs = if let Some(lock) = &lock {
            lock.is_reject_all_certs()
        } else {
            // It is ok to just release lock here as this function is the only place that
            // transitions into RejectAllTx state, and this function itself is always
            // executed from consensus task.
            self.get_reconfig_state_read_lock_guard()
                .is_reject_all_certs()
        };

        if !is_reject_all_certs || !self.deferred_transactions_empty() || commit_has_deferred_txns {
            // Don't end epoch until all deferred transactions are processed.
            if is_reject_all_certs {
                debug!(
                    "Blocking end of epoch on deferred transactions, from previous commits?={}, from this commit?={commit_has_deferred_txns}",
                    !self.deferred_transactions_empty(),
                );
            }
            return Ok((lock, false));
        }

        // Acquire lock to advance state if we don't already have it.
        let mut lock = lock.unwrap_or_else(|| self.get_reconfig_state_write_lock_guard());
        lock.close_all_tx();
        self.store_reconfig_state_batch(&lock, write_batch)?;
        Ok((Some(lock), true))
    }

    #[instrument(level = "trace", skip_all)]
    async fn process_consensus_transaction<C: CheckpointServiceNotify>(
        &self,
        batch: &mut DBBatch,
        transaction: &VerifiedSequencedConsensusTransaction,
        checkpoint_service: &Arc<C>,
        commit_round: Round,
        previously_deferred_tx_digests: &HashMap<TransactionDigest, DeferralKey>,
        mut randomness_manager: Option<&mut RandomnessManager>,
        dkg_failed: bool,
        generating_randomness: bool,
        shared_object_congestion_tracker: &mut SharedObjectCongestionTracker,
        authority_metrics: &Arc<AuthorityMetrics>,
    ) -> SuiResult<ConsensusCertificateResult> {
        let _scope = monitored_scope("HandleConsensusTransaction");
        let VerifiedSequencedConsensusTransaction(SequencedConsensusTransaction {
            certificate_author_index: _,
            certificate_author,
            consensus_index,
            transaction,
        }) = transaction;
        let tracking_id = transaction.get_tracking_id();

        match &transaction {
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::UserTransaction(certificate),
                ..
            }) => {
                if certificate.epoch() != self.epoch() {
                    // Epoch has changed after this certificate was sequenced, ignore it.
                    debug!(
                        "Certificate epoch ({:?}) doesn't match the current epoch ({:?})",
                        certificate.epoch(),
                        self.epoch()
                    );
                    return Ok(ConsensusCertificateResult::Ignored);
                }
                if self.has_sent_end_of_publish(certificate_author)?
                    && !previously_deferred_tx_digests.contains_key(certificate.digest())
                {
                    // This can not happen with valid authority
                    // With some edge cases consensus might sometimes resend previously seen certificate after EndOfPublish
                    // However this certificate will be filtered out before this line by `consensus_message_processed` call in `verify_consensus_transaction`
                    // If we see some new certificate here it means authority is byzantine and sent certificate after EndOfPublish (or we have some bug in ConsensusAdapter)
                    warn!("[Byzantine authority] Authority {:?} sent a new, previously unseen certificate {:?} after it sent EndOfPublish message to consensus", certificate_author.concise(), certificate.digest());
                    return Ok(ConsensusCertificateResult::Ignored);
                }
                // Safe because signatures are verified when consensus called into SuiTxValidator::validate_batch.
                let certificate = VerifiedCertificate::new_unchecked(*certificate.clone());
                let certificate = VerifiedExecutableTransaction::new_from_certificate(certificate);

                debug!(
                    ?tracking_id,
                    tx_digest = ?certificate.digest(),
                    "handle_consensus_transaction UserTransaction",
                );

                if !self
                    .get_reconfig_state_read_lock_guard()
                    .should_accept_consensus_certs()
                    && !previously_deferred_tx_digests.contains_key(certificate.digest())
                {
                    debug!("Ignoring consensus certificate for transaction {:?} because of end of epoch",
                    certificate.digest());
                    return Ok(ConsensusCertificateResult::Ignored);
                }

                let deferral_info = self.should_defer(
                    &certificate,
                    commit_round,
                    dkg_failed,
                    generating_randomness,
                    previously_deferred_tx_digests,
                    shared_object_congestion_tracker,
                );

                if let Some((deferral_key, deferral_reason)) = deferral_info {
                    debug!(
                        "Deferring consensus certificate for transaction {:?} until {:?}",
                        certificate.digest(),
                        deferral_key
                    );

                    let deferral_result = match deferral_reason {
                        DeferralReason::RandomnessNotReady => {
                            // Always defer transaction due to randomness not ready.
                            ConsensusCertificateResult::Deferred(deferral_key)
                        }
                        DeferralReason::SharedObjectCongestion(congested_objects) => {
                            authority_metrics
                                .consensus_handler_congested_transactions
                                .inc();
                            if transaction_deferral_within_limit(
                                &deferral_key,
                                self.protocol_config()
                                    .max_deferral_rounds_for_congestion_control(),
                            ) {
                                ConsensusCertificateResult::Deferred(deferral_key)
                            } else {
                                // Cancel the transaction that has been deferred for too long.
                                debug!(
                                    "Cancelling consensus certificate for transaction {:?} with deferral key {:?} due to congestion on objects {:?}",
                                    certificate.digest(),
                                    deferral_key,
                                    congested_objects
                                );
                                ConsensusCertificateResult::Cancelled((
                                    certificate,
                                    CancelConsensusCertificateReason::CongestionOnObjects(
                                        congested_objects,
                                    ),
                                ))
                            }
                        }
                    };
                    return Ok(deferral_result);
                }

                if dkg_failed
                    && self.randomness_state_enabled()
                    && certificate.transaction_data().uses_randomness()
                {
                    // TODO: Cancel these immediately instead of waiting until end of epoch.
                    debug!(
                        "Ignoring randomness-using certificate for transaction {:?} because DKG failed",
                        certificate.digest(),
                    );
                    return Ok(ConsensusCertificateResult::Ignored);
                }

                // This certificate will be scheduled. Update object execution cost.
                if certificate.contains_shared_object() {
                    shared_object_congestion_tracker.bump_object_execution_cost(&certificate);
                }

                Ok(ConsensusCertificateResult::SuiTransaction(certificate))
            }
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::CheckpointSignature(info),
                ..
            }) => {
                // We usually call notify_checkpoint_signature in SuiTxValidator, but that step can
                // be skipped when a batch is already part of a certificate, so we must also
                // notify here.
                checkpoint_service.notify_checkpoint_signature(self, info)?;
                Ok(ConsensusCertificateResult::ConsensusMessage)
            }
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::EndOfPublish(_),
                ..
            }) => {
                // these are partitioned earlier
                panic!("process_consensus_transaction called with end-of-publish transaction");
            }
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::CapabilityNotification(capabilities),
                ..
            }) => {
                let authority = capabilities.authority;
                if self
                    .get_reconfig_state_read_lock_guard()
                    .should_accept_consensus_certs()
                {
                    debug!(
                        "Received CapabilityNotification from {:?}",
                        authority.concise()
                    );
                    self.record_capabilities(capabilities)?;
                } else {
                    debug!(
                        "Ignoring CapabilityNotification from {:?} because of end of epoch",
                        authority.concise()
                    );
                }
                Ok(ConsensusCertificateResult::ConsensusMessage)
            }
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::NewJWKFetched(authority, jwk_id, jwk),
                ..
            }) => {
                if self
                    .get_reconfig_state_read_lock_guard()
                    .should_accept_consensus_certs()
                {
                    self.record_jwk_vote(
                        batch,
                        consensus_index.last_committed_round,
                        *authority,
                        jwk_id,
                        jwk,
                    )?;
                } else {
                    debug!(
                        "Ignoring NewJWKFetched from {:?} because of end of epoch",
                        authority.concise()
                    );
                }
                Ok(ConsensusCertificateResult::ConsensusMessage)
            }
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::RandomnessStateUpdate(_, _),
                ..
            }) => {
                // These are always generated as System transactions (handled below).
                panic!("process_consensus_transaction called with external RandomnessStateUpdate");
            }
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::RandomnessDkgMessage(authority, bytes),
                ..
            }) => {
                if self.get_reconfig_state_read_lock_guard().should_accept_tx() {
                    if let Some(randomness_manager) = randomness_manager.as_mut() {
                        debug!(
                            "Received RandomnessDkgMessage from {:?}",
                            authority.concise()
                        );
                        let versioned_dkg_message = match self.protocol_config.dkg_version() {
                            // old message was not an enum
                            0 => bcs::from_bytes(bytes).map(VersionedDkgMessage::V0),
                            _ => bcs::from_bytes(bytes),
                        };
                        match versioned_dkg_message {
                            Ok(message) => randomness_manager.add_message(authority, message)?,
                            Err(e) => {
                                warn!(
                                    "Failed to deserialize RandomnessDkgMessage from {:?}: {e:?}",
                                    authority.concise()
                                );
                            }
                        }
                    } else {
                        debug!(
                            "Ignoring RandomnessDkgMessage from {:?} because randomness is not enabled",
                            authority.concise()
                        );
                    }
                } else {
                    debug!(
                        "Ignoring RandomnessDkgMessage from {:?} because of end of epoch",
                        authority.concise()
                    );
                }
                Ok(ConsensusCertificateResult::RandomnessConsensusMessage)
            }
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::RandomnessDkgConfirmation(authority, bytes),
                ..
            }) => {
                if self.get_reconfig_state_read_lock_guard().should_accept_tx() {
                    if let Some(randomness_manager) = randomness_manager.as_mut() {
                        debug!(
                            "Received RandomnessDkgConfirmation from {:?}",
                            authority.concise()
                        );

                        let versioned_dkg_confirmation = match self.protocol_config.dkg_version() {
                            // old message was not an enum
                            0 => bcs::from_bytes(bytes).map(VersionedDkgConfimation::V0),
                            _ => bcs::from_bytes(bytes),
                        };

                        match versioned_dkg_confirmation {
                            Ok(message) => {
                                randomness_manager.add_confirmation(batch, authority, message)?
                            }
                            Err(e) => {
                                warn!(
                                        "Failed to deserialize RandomnessDkgConfirmation from {:?}: {e:?}",
                                        authority.concise(),
                                    );
                            }
                        }
                    } else {
                        debug!(
                            "Ignoring RandomnessDkgMessage from {:?} because randomness is not enabled",
                            authority.concise()
                        );
                    }
                } else {
                    debug!(
                        "Ignoring RandomnessDkgMessage from {:?} because of end of epoch",
                        authority.concise()
                    );
                }
                Ok(ConsensusCertificateResult::RandomnessConsensusMessage)
            }
            SequencedConsensusTransactionKind::System(system_transaction) => {
                Ok(self.process_consensus_system_transaction(system_transaction))
            }
        }
    }

    fn process_consensus_system_transaction(
        &self,
        system_transaction: &VerifiedExecutableTransaction,
    ) -> ConsensusCertificateResult {
        if !self.get_reconfig_state_read_lock_guard().should_accept_tx() {
            debug!(
                "Ignoring system transaction {:?} because of end of epoch",
                system_transaction.digest()
            );
            return ConsensusCertificateResult::IgnoredSystem;
        }

        // If needed we can support owned object system transactions as well...
        assert!(system_transaction.contains_shared_object());
        ConsensusCertificateResult::SuiTransaction(system_transaction.clone())
    }

    pub(crate) fn write_pending_checkpoint(
        &self,
        batch: &mut DBBatch,
        checkpoint: &PendingCheckpointV2,
    ) -> SuiResult {
        if let Some(pending) = self.get_pending_checkpoint(&checkpoint.height())? {
            if pending.roots() != checkpoint.roots() {
                panic!("Received checkpoint at index {} that contradicts previously stored checkpoint. Old roots: {:?}, new roots: {:?}", checkpoint.height(), pending.roots(), checkpoint.roots());
            }
            debug!(
                checkpoint_commit_height = checkpoint.height(),
                "Ignoring duplicate checkpoint notification",
            );
            return Ok(());
        }
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

        if self.randomness_state_enabled() {
            batch.insert_batch(
                &self.tables()?.pending_checkpoints_v2,
                std::iter::once((checkpoint.height(), checkpoint)),
            )?;
        } else {
            batch.insert_batch(
                &self.tables()?.pending_checkpoints,
                std::iter::once((checkpoint.height(), checkpoint.clone().expect_v1())),
            )?;
        }

        Ok(())
    }

    pub fn get_pending_checkpoints(
        &self,
        last: Option<CheckpointHeight>,
    ) -> SuiResult<Vec<(CheckpointHeight, PendingCheckpointV2)>> {
        let tables = self.tables()?;
        if self.randomness_state_enabled() {
            let mut iter = tables.pending_checkpoints_v2.unbounded_iter();
            if let Some(last_processed_height) = last {
                iter = iter.skip_to(&(last_processed_height + 1))?;
            }
            Ok(iter.collect())
        } else {
            let mut iter = tables.pending_checkpoints.unbounded_iter();
            if let Some(last_processed_height) = last {
                iter = iter.skip_to(&(last_processed_height + 1))?;
            }
            Ok(iter.map(|(height, cp)| (height, cp.into())).collect())
        }
    }

    pub fn get_pending_checkpoint(
        &self,
        index: &CheckpointHeight,
    ) -> SuiResult<Option<PendingCheckpointV2>> {
        if self.randomness_state_enabled() {
            Ok(self.tables()?.pending_checkpoints_v2.get(index)?)
        } else {
            Ok(self
                .tables()?
                .pending_checkpoints
                .get(index)?
                .map(|c| c.into()))
        }
    }

    pub fn process_pending_checkpoint(
        &self,
        commit_height: CheckpointHeight,
        content_info: Vec<(CheckpointSummary, CheckpointContents)>,
    ) -> SuiResult<()> {
        // All created checkpoints are inserted in builder_checkpoint_summary in a single batch.
        // This means that upon restart we can use BuilderCheckpointSummary::commit_height
        // from the last built summary to resume building checkpoints.
        let mut batch = self.tables()?.pending_checkpoints.batch();
        for (position_in_commit, (summary, transactions)) in content_info.into_iter().enumerate() {
            let sequence_number = summary.sequence_number;
            let summary = BuilderCheckpointSummary {
                summary,
                checkpoint_height: Some(commit_height),
                position_in_commit,
            };
            batch.insert_batch(
                &self.tables()?.builder_checkpoint_summary_v2,
                [(&sequence_number, summary)],
            )?;
            batch.insert_batch(
                &self.tables()?.builder_digest_to_checkpoint,
                transactions
                    .iter()
                    .map(|tx| (tx.transaction, sequence_number)),
            )?;
        }

        Ok(batch.write()?)
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
        Ok(self
            .tables()?
            .builder_checkpoint_summary_v2
            .unbounded_iter()
            .skip_to_last()
            .next()
            .map(|(_, s)| s))
    }

    pub fn last_built_checkpoint_summary(
        &self,
    ) -> SuiResult<Option<(CheckpointSequenceNumber, CheckpointSummary)>> {
        Ok(self
            .tables()?
            .builder_checkpoint_summary_v2
            .unbounded_iter()
            .skip_to_last()
            .next()
            .map(|(seq, s)| (seq, s.summary)))
    }

    pub fn get_built_checkpoint_summary(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> SuiResult<Option<CheckpointSummary>> {
        Ok(self
            .tables()?
            .builder_checkpoint_summary_v2
            .get(&sequence)?
            .map(|s| s.summary))
    }

    pub fn builder_included_transactions_in_checkpoint<'a>(
        &self,
        digests: impl Iterator<Item = &'a TransactionDigest>,
    ) -> SuiResult<Vec<bool>> {
        Ok(self
            .tables()?
            .builder_digest_to_checkpoint
            .multi_contains_keys(digests)?)
    }

    pub fn get_last_checkpoint_signature_index(&self) -> SuiResult<u64> {
        Ok(self
            .tables()?
            .pending_checkpoint_signatures
            .unbounded_iter()
            .skip_to_last()
            .next()
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
        self.metrics.current_epoch.set(self.epoch() as i64);
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
}

impl GetSharedLocks for AuthorityPerEpochStore {
    fn get_shared_locks(
        &self,
        key: &TransactionKey,
    ) -> Result<Vec<(ObjectID, SequenceNumber)>, SuiError> {
        if self.randomness_state_enabled() {
            Ok(self
                .tables()?
                .assigned_shared_object_versions_v2
                .get(key)?
                .unwrap_or_default())
        } else {
            Ok(self
                .tables()?
                .assigned_shared_object_versions
                .get(key.unwrap_digest())?
                .unwrap_or_default())
        }
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
        let executor = sui_execution::executor(protocol_config, silent, None)
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
