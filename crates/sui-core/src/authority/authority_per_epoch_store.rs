// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use arc_swap::ArcSwapOption;
use enum_dispatch::enum_dispatch;
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
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::future::Future;
use std::iter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sui_config::node::ExpensiveSafetyCheckConfig;
use sui_types::accumulator::Accumulator;
use sui_types::authenticator_state::{get_authenticator_state, ActiveJwk};
use sui_types::base_types::ConciseableName;
use sui_types::base_types::{AuthorityName, EpochId, ObjectID, SequenceNumber, TransactionDigest};
use sui_types::committee::Committee;
use sui_types::committee::CommitteeTrait;
use sui_types::crypto::{AuthoritySignInfo, AuthorityStrongQuorumSignInfo};
use sui_types::digests::ChainIdentifier;
use sui_types::error::{SuiError, SuiResult};
use sui_types::signature::GenericSignature;
use sui_types::transaction::{
    AuthenticatorStateUpdate, CertifiedTransaction, SenderSignedData, SharedInputObject,
    Transaction, TransactionDataAPI, TransactionKind, VerifiedCertificate,
    VerifiedSignedTransaction, VerifiedTransaction,
};
use sui_types::SUI_RANDOMNESS_STATE_OBJECT_ID;
use tracing::{debug, error, info, instrument, trace, warn};
use typed_store::{
    rocks::{default_db_options, DBBatch, DBMap, DBOptions, MetricConf},
    traits::{TableSummary, TypedStoreDebug},
    TypedStoreError,
};

use super::epoch_start_configuration::EpochStartConfigTrait;
use crate::authority::epoch_start_configuration::{EpochFlag, EpochStartConfiguration};
use crate::authority::{AuthorityStore, ResolverWrapper};
use crate::checkpoints::{
    BuilderCheckpointSummary, CheckpointCommitHeight, CheckpointServiceNotify, EpochStats,
    PendingCheckpoint, PendingCheckpointInfo,
};
use crate::consensus_handler::{
    SequencedConsensusTransaction, SequencedConsensusTransactionKey,
    SequencedConsensusTransactionKind, VerifiedSequencedConsensusTransaction,
};
use crate::epoch::epoch_metrics::EpochMetrics;
use crate::epoch::reconfiguration::ReconfigState;
use crate::module_cache_metrics::ResolverMetrics;
use crate::post_consensus_tx_reorder::PostConsensusTxReorder;
use crate::signature_verifier::*;
use crate::stake_aggregator::{GenericMultiStakeAggregator, StakeAggregator};
use move_bytecode_utils::module_cache::SyncModuleCache;
use mysten_common::sync::notify_once::NotifyOnce;
use mysten_common::sync::notify_read::NotifyRead;
use mysten_metrics::monitored_scope;
use narwhal_types::{RandomnessRound, Round, TimestampMs};
use prometheus::IntCounter;
use std::str::FromStr;
use sui_execution::{self, Executor};
use sui_macros::fail_point;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_storage::mutex_table::{MutexGuard, MutexTable};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
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
use sui_types::storage::{
    transaction_input_object_keys, transaction_receiving_object_keys, GetSharedLocks, ObjectKey,
    ObjectStore,
};
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
const FINAL_EPOCH_CHECKPOINT_INDEX: u64 = 0;
const OVERRIDE_PROTOCOL_UPGRADE_BUFFER_STAKE_INDEX: u64 = 0;
pub const EPOCH_DB_PREFIX: &str = "epoch_";

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

pub enum ConsensusCertificateResult {
    /// The consensus message was ignored (e.g. because it has already been processed).
    Ignored,
    /// An executable transaction (can be a user tx or a system tx)
    SuiTransaction(VerifiedExecutableTransaction),
    /// The transaction should be re-processed at a future commit, specified by the DeferralKey
    Defered(DeferralKey),
    /// Everything else, e.g. AuthorityCapabilities, CheckpointSignatures, etc.
    ConsensusMessage,
    /// A system message in consensus was ignored (e.g. because of end of epoch).
    IgnoredSystem,
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

// Data related to VM and Move execution and type layout
pub struct ExecutionComponents {
    pub(crate) executor: Arc<dyn Executor + Send + Sync>,
    // TODO: use strategies (e.g. LRU?) to constraint memory usage
    pub(crate) module_cache: Arc<SyncModuleCache<ResolverWrapper<AuthorityStore>>>,
    metrics: Arc<ResolverMetrics>,
}

pub struct AuthorityPerEpochStore {
    /// Committee of validators for the current epoch.
    committee: Arc<Committee>,

    /// Holds the underlying per-epoch typed store tables.
    /// This is an ArcSwapOption because it needs to be used concurrently,
    /// and it nees to be cleared at the end of the epoch.
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
    /// Pending certificates that we are waiting to be sequenced by consensus.
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
    epoch_open_time: Instant,

    /// The moment when epoch is closed. We don't care much about crash recovery because it's
    /// a metric that doesn't have to be available for each epoch, and it's only used during
    /// the last few seconds of an epoch.
    epoch_close_time: RwLock<Option<Instant>>,
    metrics: Arc<EpochMetrics>,
    epoch_start_configuration: Arc<EpochStartConfiguration>,

    /// Execution state that has to restart at each epoch change
    execution_component: ExecutionComponents,

    /// Chain identifier
    chain_identifier: ChainIdentifier,

    /// aggregator for JWK votes
    jwk_aggregator: Mutex<JwkAggregator>,
}

/// AuthorityEpochTables contains tables that contain data that is only valid within an epoch.
#[derive(DBMapUtils)]
pub struct AuthorityEpochTables {
    /// This is map between the transaction digest and transactions found in the `transaction_lock`.
    #[default_options_override_fn = "signed_transactions_table_default_config"]
    signed_transactions:
        DBMap<TransactionDigest, TrustedEnvelope<SenderSignedData, AuthoritySignInfo>>,

    /// Signatures over transaction effects that were executed in the current epoch.
    /// Store this to avoid re-signing the same effects twice.
    effects_signatures: DBMap<TransactionDigest, AuthoritySignInfo>,

    /// Signatures of transaction certificates that are executed locally.
    pub(crate) transaction_cert_signatures: DBMap<TransactionDigest, AuthorityStrongQuorumSignInfo>,

    /// The two tables below manage shared object locks / versions. There are two ways they can be
    /// updated:
    /// 1. (validators only): Upon receiving a certified transaction from consensus, the authority
    /// assigns the next version to each shared object of the transaction. The next versions of
    /// the shared objects are updated as well.
    /// 2. (fullnodes + validators): Upon receiving a certified effect from state sync, or
    /// transaction orchestrator fast execution path, the node assigns the shared object
    /// versions from the transaction effect. Next object versions are not updated.
    ///
    /// REQUIRED: all authorities must assign the same shared object versions for each transaction.
    assigned_shared_object_versions: DBMap<TransactionDigest, Vec<(ObjectID, SequenceNumber)>>,
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
    pending_execution: DBMap<TransactionDigest, TrustedExecutableTransaction>,

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

    // todo - if we move processing of entire nw commit into single DB batch,
    // we can potentially get rid of this table
    /// Records narwhal consensus output index of the final checkpoint in epoch
    /// This is a single entry table with key FINAL_EPOCH_CHECKPOINT_INDEX
    final_epoch_checkpoint: DBMap<u64, u64>,

    /// This table has information for the checkpoints for which we constructed all the data
    /// from consensus, but not yet constructed actual checkpoint.
    ///
    /// Key in this table is the narwhal commit height and not a checkpoint sequence number.
    ///
    /// Non-empty list of transactions here might result in empty list when we are forming checkpoint.
    /// Because we don't want to create checkpoints with empty content(see CheckpointBuilder::write_checkpoint),
    /// the sequence number of checkpoint does not match height here.
    #[default_options_override_fn = "pending_checkpoints_table_default_config"]
    pending_checkpoints: DBMap<CheckpointCommitHeight, PendingCheckpoint>,

    /// Checkpoint builder maintains internal list of transactions it included in checkpoints here
    builder_digest_to_checkpoint: DBMap<TransactionDigest, CheckpointSequenceNumber>,

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

    /// Records the round numbers for which we have written randomness.
    randomness_rounds_written: DBMap<RandomnessRound, ()>,
}

// DeferralKey requires both the round to which the tx should be deferred (so that we can
// efficiently load all txns that are now ready), and the round from which it has been deferred (so
// that multiple rounds can efficiently defer to the same future round).
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DeferralKey {
    RandomnessRound {
        future_round: RandomnessRound,
        deferred_from_round: Round,
    },

    ConsensusRound {
        future_round: Round,
        deferred_from_round: Round,
    },
}

impl DeferralKey {
    fn new_for_randomness_round(future_round: RandomnessRound, deferred_from_round: Round) -> Self {
        Self::RandomnessRound {
            future_round,
            deferred_from_round,
        }
    }

    fn new_for_consensus_round(future_round: Round, deferred_from_round: Round) -> Self {
        Self::ConsensusRound {
            future_round,
            deferred_from_round,
        }
    }

    fn range_for_randomness_round(future_round: RandomnessRound) -> (Self, Self) {
        (
            Self::RandomnessRound {
                future_round,
                deferred_from_round: 0,
            },
            Self::RandomnessRound {
                future_round: future_round.checked_add(1).unwrap(),
                deferred_from_round: 0,
            },
        )
    }

    fn range_for_consensus_round(future_round: Round) -> (Self, Self) {
        (
            Self::ConsensusRound {
                future_round,
                deferred_from_round: 0,
            },
            Self::ConsensusRound {
                future_round: future_round.checked_add(1).unwrap(),
                deferred_from_round: 0,
            },
        )
    }
}

#[tokio::test]
async fn test_deferral_key_sort_order() {
    use rand::prelude::*;

    #[derive(DBMapUtils)]
    struct TestDB {
        deferred_certs: DBMap<DeferralKey, ()>,
    }

    // get a tempdir
    let tempdir = tempfile::tempdir().unwrap();

    let db = TestDB::open_tables_read_write(
        tempdir.path().to_owned(),
        MetricConf::new("test_db"),
        None,
        None,
    );

    for _ in 0..10000 {
        let future_round = rand::thread_rng().gen_range(0..u64::MAX);
        let current_round = rand::thread_rng().gen_range(0..u64::MAX);

        let key = if rand::thread_rng().gen() {
            DeferralKey::new_for_randomness_round(RandomnessRound(future_round), current_round)
        } else {
            DeferralKey::new_for_consensus_round(future_round, current_round)
        };

        db.deferred_certs.insert(&key, &()).unwrap();
    }

    // verify that all random round keys are sorted before all consensus round keys
    let mut first_consensus_round_seen = false;
    let mut previous_future_round = 0;
    for (key, _) in db.deferred_certs.unbounded_iter() {
        match key {
            DeferralKey::ConsensusRound { future_round, .. } => {
                if !first_consensus_round_seen {
                    first_consensus_round_seen = true;
                    previous_future_round = 0;
                }
                assert!(previous_future_round <= future_round);
                previous_future_round = future_round;
            }
            DeferralKey::RandomnessRound { future_round, .. } => {
                assert!(!first_consensus_round_seen);
                assert!(previous_future_round <= future_round.0);
                previous_future_round = future_round.0;
            }
        }
    }
}

fn signed_transactions_table_default_config() -> DBOptions {
    default_db_options()
        .optimize_for_write_throughput()
        .optimize_for_large_values_no_scan(1 << 10)
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
        store: Arc<AuthorityStore>,
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
            store.clone(),
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
        );

        let authenticator_state_exists = epoch_start_configuration
            .authenticator_obj_initial_shared_version()
            .is_some();
        let authenticator_state_enabled =
            authenticator_state_exists && protocol_config.enable_jwk_consensus_updates();

        if authenticator_state_enabled {
            info!("authenticator_state enabled");
            let authenticator_state = get_authenticator_state(&store)
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

        let is_validator = committee.authority_index(&name).is_some();
        if is_validator {
            assert!(epoch_start_configuration
                .flags()
                .contains(&EpochFlag::InMemoryCheckpointRoots));
        }

        let mut jwk_aggregator = JwkAggregator::new(committee.clone());

        for ((authority, id, jwk), _) in tables.pending_jwks.unbounded_iter().seek_to_first() {
            jwk_aggregator.insert(authority, (id, jwk));
        }

        let jwk_aggregator = Mutex::new(jwk_aggregator);

        let s = Arc::new(Self {
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
        });
        s.update_buffer_stake_metric();
        s
    }

    pub fn tables(&self) -> SuiResult<Arc<AuthorityEpochTables>> {
        match self.tables.load_full() {
            Some(tables) => Ok(tables),
            None => Err(SuiError::EpochEnded),
        }
    }

    pub fn release_db_handles(&self) {
        // When force releasing DB handle is no longer needed, it will still be useful
        // to make sure AuthorityPerEpochStore is not used after the next epoch starts.
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

    pub fn bridge_exists(&self) -> bool {
        self.epoch_start_configuration
            .bridge_obj_initial_shared_version()
            .is_some()
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
        self.chain_identifier
    }

    pub fn new_at_next_epoch(
        &self,
        name: AuthorityName,
        new_committee: Committee,
        epoch_start_configuration: EpochStartConfiguration,
        store: Arc<AuthorityStore>,
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
            store,
            self.execution_component.metrics(),
            self.signature_verifier.metrics.clone(),
            expensive_safety_check_config,
            chain_identifier,
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

    pub fn reference_gas_price(&self) -> u64 {
        self.epoch_start_state().reference_gas_price()
    }

    pub fn protocol_version(&self) -> ProtocolVersion {
        self.epoch_start_state().protocol_version()
    }

    pub fn module_cache(&self) -> &Arc<SyncModuleCache<ResolverWrapper<AuthorityStore>>> {
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
        batch.write()?;
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

    pub fn multi_get_next_shared_object_versions<'a>(
        &self,
        ids: impl Iterator<Item = &'a ObjectID>,
    ) -> SuiResult<Vec<Option<SequenceNumber>>> {
        Ok(self.tables()?.next_shared_object_versions.multi_get(ids)?)
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
            .map_err(SuiError::StorageError)
    }

    /// Returns future containing the state digest for the given epoch
    /// once available
    pub async fn notify_read_checkpoint_state_digests(
        &self,
        checkpoints: Vec<CheckpointSequenceNumber>,
    ) -> SuiResult<Vec<Accumulator>> {
        // We need to register waiters _before_ reading from the database to avoid
        // race conditions
        let registrations = self
            .checkpoint_state_notify_read
            .register_all(checkpoints.clone());
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

    /// Deletes one pending certificate.
    pub fn remove_pending_execution(&self, digest: &TransactionDigest) -> SuiResult<()> {
        self.tables()?.pending_execution.remove(digest)?;
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
        self.tables()?
            .assigned_shared_object_versions
            .insert(tx_digest, assigned_versions)?;
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

    pub fn per_epoch_finalized_txns_enabled(&self) -> bool {
        self.epoch_start_configuration
            .flags()
            .contains(&EpochFlag::PerEpochFinalizedTransactions)
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
    async fn get_or_init_next_object_versions(
        &self,
        objects_to_init: impl Iterator<Item = (ObjectID, SequenceNumber)> + Clone,
        object_store: impl ObjectStore,
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

            let ids = objects_to_init.clone().map(|(id, _)| id);

            let next_versions = db_transaction
                .multi_get(&self.tables()?.next_shared_object_versions, ids.clone())?;

            let uninitialized_objects: Vec<(ObjectID, SequenceNumber)> = next_versions
                .iter()
                .zip(objects_to_init.clone())
                .filter_map(|(next_version, id_and_version)| match next_version {
                    None => Some(id_and_version),
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
                    match object_store.get_object(id).expect("read cannot fail") {
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

    async fn set_assigned_shared_object_versions(
        &self,
        certificate: &VerifiedExecutableTransaction,
        assigned_versions: &Vec<(ObjectID, SequenceNumber)>,
        object_store: impl ObjectStore,
    ) -> SuiResult {
        let tx_digest = certificate.digest();

        debug!(
            ?tx_digest,
            ?assigned_versions,
            "set_assigned_shared_object_versions"
        );

        #[allow(clippy::needless_collect)]
        let shared_input_objects: Vec<_> = certificate
            .data()
            .transaction_data()
            .kind()
            .shared_input_objects()
            .map(SharedInputObject::into_id_and_version)
            .collect();

        self.get_or_init_next_object_versions(shared_input_objects.into_iter(), object_store)
            .await?;
        self.tables()?
            .assigned_shared_object_versions
            .insert(tx_digest, assigned_versions)?;
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

    fn load_deferred_transactions_for_randomness_round(
        &self,
        batch: &mut DBBatch,
        randomness_round: RandomnessRound,
    ) -> SuiResult<Vec<VerifiedSequencedConsensusTransaction>> {
        let (min, max) = DeferralKey::range_for_randomness_round(randomness_round);
        self.load_deferred_transactions(batch, min, max)
    }

    fn load_deferred_transactions_for_consensus_round(
        &self,
        batch: &mut DBBatch,
        consensus_round: u64,
    ) -> SuiResult<Vec<VerifiedSequencedConsensusTransaction>> {
        let (min, max) = DeferralKey::range_for_consensus_round(consensus_round);
        self.load_deferred_transactions(batch, min, max)
    }

    // factoring of the above
    fn load_deferred_transactions(
        &self,
        batch: &mut DBBatch,
        min: DeferralKey,
        max: DeferralKey,
    ) -> SuiResult<Vec<VerifiedSequencedConsensusTransaction>> {
        let mut keys = Vec::new();
        let txns: Vec<_> = self
            .tables()?
            .deferred_transactions
            .iter_with_bounds(Some(min), Some(max))
            .flat_map(|(key, txns)| {
                keys.push(key);
                txns
            })
            .collect();

        // verify that there are no duplicates - should be impossible due to
        // is_consensus_message_processed
        #[cfg(debug_assertions)]
        {
            let mut seen = HashSet::new();
            for txn in &txns {
                assert!(seen.insert(txn.0.key()));
            }
        }

        // Transactional DBs do not support range deletes, so we have to delete keys one-by-one.
        // This shouldn't be a problem, there should not usually be more than a small handful of
        // keys loaded in each round.
        batch.delete_batch(&self.tables()?.deferred_transactions, keys)?;

        Ok(txns)
    }

    fn should_defer(
        &self,
        cert: &VerifiedExecutableTransaction,
        commit_round: Round,
        previously_deferred_tx_digests: &HashSet<TransactionDigest>,
        last_randomness_round: RandomnessRound,
    ) -> Option<DeferralKey> {
        // Defer transaction if it depends on Random object.
        if cert
            .shared_input_objects()
            .any(|obj| obj.id() == SUI_RANDOMNESS_STATE_OBJECT_ID)
        {
            // Don't re-defer randomness-using tx.
            if previously_deferred_tx_digests.contains(cert.digest()) {
                return None;
            }
            return Some(DeferralKey::new_for_randomness_round(
                // Deferral by two rounds guarantees that the transaction will not depend on
                // randomness that was revealed but not yet sequenced at the time the transaction
                // was sequenced.
                last_randomness_round + 2,
                commit_round,
            ));
        }

        // placeholder construction to silence lints
        let _ = DeferralKey::new_for_consensus_round(0, 0);

        None
    }

    /// Lock a sequence number for the shared objects of the input transaction based on the effects
    /// of that transaction.
    /// Used by full nodes who don't listen to consensus, and validators who catch up by state sync.
    #[instrument(level = "trace", skip_all)]
    pub async fn acquire_shared_locks_from_effects(
        &self,
        certificate: &VerifiedExecutableTransaction,
        effects: &TransactionEffects,
        object_store: impl ObjectStore,
    ) -> SuiResult {
        self.set_assigned_shared_object_versions(
            certificate,
            &effects
                .input_shared_objects()
                .into_iter()
                .map(|iso| iso.id_and_version())
                .collect(),
            object_store,
        )
        .await
    }

    /// When submitting a certificate caller **must** provide a ReconfigState lock guard
    /// and verify that it allows new user certificates
    pub fn insert_pending_consensus_transactions(
        &self,
        transaction: &ConsensusTransaction,
        lock: Option<&RwLockReadGuard<ReconfigState>>,
    ) -> SuiResult {
        self.tables()?
            .pending_consensus_transactions
            .insert(&transaction.key(), transaction)?;
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
        Ok(())
    }

    pub fn remove_pending_consensus_transaction(&self, key: &ConsensusTransactionKey) -> SuiResult {
        self.tables()?.pending_consensus_transactions.remove(key)?;
        if let ConsensusTransactionKey::Certificate(cert) = key {
            self.pending_consensus_certificates.lock().remove(cert);
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

    pub fn is_consensus_message_processed(
        &self,
        key: &SequencedConsensusTransactionKey,
    ) -> SuiResult<bool> {
        Ok(self
            .tables()?
            .consensus_message_processed
            .contains_key(key)?)
    }

    pub async fn consensus_message_processed_notify(
        &self,
        key: SequencedConsensusTransactionKey,
    ) -> Result<(), SuiError> {
        let registration = self.consensus_notify_read.register_one(&key);
        if self.is_consensus_message_processed(&key)? {
            return Ok(());
        }
        registration.await;
        Ok(())
    }

    pub fn check_consensus_messages_processed<'a>(
        &self,
        keys: impl Iterator<Item = &'a SequencedConsensusTransactionKey>,
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
        let registrations = self.consensus_notify_read.register_all(keys.clone());

        let unprocessed_keys_registrations = registrations
            .into_iter()
            .zip(self.check_consensus_messages_processed(keys.iter())?)
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

    /// Note: caller usually need to call consensus_message_processed_notify before this call
    pub fn user_signatures_for_checkpoint(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Vec<GenericSignature>>> {
        let signatures = self
            .tables()?
            .user_signatures_for_checkpoints
            .multi_get(digests)?;
        let mut result = Vec::with_capacity(digests.len());
        for (signatures, digest) in signatures.into_iter().zip(digests.iter()) {
            let Some(signatures) = signatures else {
                return Err(SuiError::from(
                    format!(
                        "Can not find user signature for checkpoint for transaction {:?}",
                        digest
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
            info!("jwk {:?} became active at round {:?}", key, round);
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
            .iter_with_bounds(Some(start), Some(end))
            .map(|((r, (jwk_id, jwk)), _)| {
                debug_assert!(round == r);
                ActiveJwk { jwk_id, jwk, epoch }
            })
            .collect())
    }

    pub fn jwk_active_in_current_epoch(&self, jwk_id: &JwkId, jwk: &JWK) -> bool {
        let jwk_aggregator = self.jwk_aggregator.lock();
        jwk_aggregator.has_quorum_for_key(&(jwk_id.clone(), jwk.clone()))
    }

    /// Caller is responsible to call consensus_message_processed before this method
    pub async fn record_owned_object_cert_from_consensus(
        &self,
        batch: &mut DBBatch,
        certificate: &VerifiedExecutableTransaction,
    ) -> Result<(), SuiError> {
        self.finish_consensus_certificate_process_with_batch(batch, certificate)
    }

    /// Locks a sequence number for the shared objects of the input transaction. Also updates the
    /// last consensus index, consensus_message_processed and pending_certificates tables.
    /// This function must only be called from the consensus task (i.e. from handle_consensus_transaction).
    ///
    /// Caller is responsible to call consensus_message_processed before this method
    pub async fn record_shared_object_cert_from_consensus(
        &self,
        batch: &mut DBBatch,
        shared_input_next_versions: &mut HashMap<ObjectID, SequenceNumber>,
        certificate: &VerifiedExecutableTransaction,
    ) -> Result<(), SuiError> {
        // Make an iterator to save the certificate.
        let transaction_digest = *certificate.digest();

        // Make an iterator to update the locks of the transaction's shared objects.
        let shared_input_objects: Vec<_> = certificate.shared_input_objects().collect();

        let mut input_object_keys = transaction_input_object_keys(certificate)?;
        let mut assigned_versions = Vec::with_capacity(shared_input_objects.len());
        let mut is_mutable_input = Vec::with_capacity(shared_input_objects.len());
        // Record receiving object versions towards the shared version computation.
        let receiving_object_keys = transaction_receiving_object_keys(certificate);
        input_object_keys.extend(receiving_object_keys);

        for (SharedInputObject { id, mutable, .. }, version) in shared_input_objects
            .iter()
            .map(|obj| (obj, *shared_input_next_versions.get(&obj.id()).unwrap()))
        {
            assigned_versions.push((*id, version));
            input_object_keys.push(ObjectKey(*id, version));
            is_mutable_input.push(*mutable);
        }

        let next_version =
            SequenceNumber::lamport_increment(input_object_keys.iter().map(|obj| obj.1));

        // Update the next version for the shared objects.
        assigned_versions
            .iter()
            .zip(is_mutable_input.into_iter())
            .filter_map(|((id, _), mutable)| {
                if mutable {
                    Some((*id, next_version))
                } else {
                    None
                }
            })
            .for_each(|(id, version)| {
                shared_input_next_versions.insert(id, version);
            });

        trace!(tx_digest = ?transaction_digest,
               ?assigned_versions, ?next_version,
               "locking shared objects");

        self.finish_assign_shared_object_versions(batch, certificate, assigned_versions)
    }

    fn finish_assign_shared_object_versions(
        &self,
        write_batch: &mut DBBatch,
        certificate: &VerifiedExecutableTransaction,
        assigned_versions: Vec<(ObjectID, SequenceNumber)>,
    ) -> SuiResult {
        let tx_digest = *certificate.digest();

        debug!(
            ?tx_digest,
            ?assigned_versions,
            "finish_assign_shared_object_versions"
        );
        write_batch.insert_batch(
            &self.tables()?.assigned_shared_object_versions,
            iter::once((tx_digest, assigned_versions)),
        )?;

        self.finish_consensus_certificate_process_with_batch(write_batch, certificate)?;
        Ok(())
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
        certificate: &VerifiedExecutableTransaction,
    ) -> SuiResult {
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
        Ok(())
    }

    pub fn final_epoch_checkpoint(&self) -> SuiResult<Option<u64>> {
        Ok(self
            .tables()?
            .final_epoch_checkpoint
            .get(&FINAL_EPOCH_CHECKPOINT_INDEX)?)
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
        // Signatures are verified as part of narwhal payload verification in SuiTxValidator
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
                    warn!("CheckpointSignature authority {} does not match narwhal certificate source {}", data.summary.auth_sig().authority, transaction.certificate_author_index );
                    return None;
                }
            }
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::EndOfPublish(authority),
                ..
            }) => {
                if &transaction.sender_authority() != authority {
                    warn!(
                        "EndOfPublish authority {} does not match narwhal certificate source {}",
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
                        "CapabilityNotification authority {} does not match narwhal certificate source {}",
                        capabilities.authority,
                        transaction.certificate_author_index
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
                        "NewJWKFetched authority {} does not match narwhal certificate source {}",
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
        object_store: impl ObjectStore,
        commit_round: Round,
        commit_timestamp: TimestampMs,
        skipped_consensus_txns: &IntCounter,
    ) -> SuiResult<Vec<VerifiedExecutableTransaction>> {
        let verified_transactions: Vec<_> = transactions
            .into_iter()
            .filter_map(|transaction| {
                self.verify_consensus_transaction(transaction, skipped_consensus_txns)
            })
            .collect();
        let roots: BTreeSet<_> = verified_transactions
            .iter()
            .filter_map(|transaction| transaction.0.transaction.executable_transaction_digest())
            .collect();
        let mut system_transactions = Vec::with_capacity(verified_transactions.len());
        let mut sequenced_transactions = Vec::with_capacity(verified_transactions.len());
        let mut end_of_publish_transactions = Vec::with_capacity(verified_transactions.len());
        for tx in verified_transactions {
            if tx.0.is_end_of_publish() {
                end_of_publish_transactions.push(tx);
            } else if tx.0.is_system() {
                system_transactions.push(tx);
            } else {
                sequenced_transactions.push(tx);
            }
        }
        let mut batch = self
            .db_batch()
            .expect("Consensus should not be processed past end of epoch");

        // Pre-process transactions to find the most recent randomness round included in the commit.
        let mut last_randomness_round_written = self.last_randomness_round_written()?;
        // There must be at most one RandomnessStateUpdate per commit.
        let mut randomness_state_update_found = None;
        for tx in system_transactions.iter() {
            let SequencedConsensusTransactionKind::System(tx) = &tx.0.transaction else {
                unreachable!("system_transactions vector should only contain system transactions")
            };
            if let TransactionKind::RandomnessStateUpdate(rsu) =
                tx.data().intent_message().value.kind()
            {
                assert!(
                    randomness_state_update_found.is_none(),
                    "found multiple RandomnessStateUpdates in one commit: {:?}, {rsu:?}",
                    randomness_state_update_found.unwrap(),
                );
                randomness_state_update_found = Some(rsu);
                last_randomness_round_written = std::cmp::max(
                    last_randomness_round_written,
                    RandomnessRound(rsu.randomness_round),
                );
            }
        }

        // Load transactions deferred from prevous commits.
        // We do this after updating the last_randomness_round_written above so that every deferred
        // transaction that can be run with this commit is loaded.
        let deferred_tx: Vec<VerifiedSequencedConsensusTransaction> = self
            .load_deferred_transactions_for_consensus_round(&mut batch, commit_round)?
            .into_iter()
            .chain(self.load_deferred_transactions_for_randomness_round(
                &mut batch,
                last_randomness_round_written,
            )?)
            .collect();
        let previously_deferred_tx_digests: HashSet<_> = deferred_tx
            .iter()
            .map(|tx| match tx.0.transaction.key() {
                SequencedConsensusTransactionKey::External(
                    ConsensusTransactionKey::Certificate(digest),
                ) => digest,
                _ => panic!("deferred transaction was not a user certificate: {tx:?}"),
            })
            .collect();
        sequenced_transactions.extend(deferred_tx.into_iter());

        PostConsensusTxReorder::reorder(
            &mut sequenced_transactions,
            self.protocol_config.consensus_transaction_ordering(),
        );
        let consensus_transactions: Vec<_> = system_transactions
            .into_iter()
            .chain(sequenced_transactions)
            .collect();

        let (transactions_to_schedule, notifications, lock_and_final_round) = self
            .process_consensus_transactions(
                &mut batch,
                &consensus_transactions,
                &end_of_publish_transactions,
                checkpoint_service,
                object_store,
                commit_round,
                previously_deferred_tx_digests,
                last_randomness_round_written,
            )
            .await?;
        self.record_consensus_commit_stats(&mut batch, consensus_stats)?;

        // The last block in this function notifies about new checkpoint if needed
        // It's important that we use as_ref() here to make sure we are not dropping the lock.
        // The lock needs to be held until the end of this function.
        let final_checkpoint_round = lock_and_final_round.as_ref().map(|(_, r)| *r);
        let final_checkpoint = match final_checkpoint_round.map(|r| r.cmp(&commit_round)) {
            Some(Ordering::Less) => {
                debug!(
                    "Not forming checkpoint for round {} above final checkpoint round {:?}",
                    commit_round, final_checkpoint_round
                );
                return Ok(vec![]);
            }
            Some(Ordering::Equal) => true,
            Some(Ordering::Greater) => false,
            None => false,
        };
        let pending_checkpoint = PendingCheckpoint {
            roots: roots.into_iter().collect(),
            details: PendingCheckpointInfo {
                timestamp_ms: commit_timestamp,
                last_of_epoch: final_checkpoint,
                commit_height: commit_round,
            },
        };

        self.write_pending_checkpoint(&mut batch, &pending_checkpoint)?;

        batch.write()?;

        self.process_notifications(&notifications, &end_of_publish_transactions);

        checkpoint_service.notify_checkpoint(&pending_checkpoint)?;

        if final_checkpoint {
            info!(
                epoch=?self.epoch(),
                // Accessing lock_and_final_round on purpose so that the compiler ensures
                // the lock is not yet dropped.
                last_checkpoint_round=?lock_and_final_round.as_ref().map(|(_, r)| *r),
                "Received 2f+1 EndOfPublish messages, notifying last checkpoint"
            );
            self.record_end_of_message_quorum_time_metric();
        }

        Ok(transactions_to_schedule)
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn get_highest_pending_checkpoint_height(&self) -> CheckpointCommitHeight {
        self.tables()
            .expect("test should not cross epoch boundary")
            .pending_checkpoints
            .unbounded_iter()
            .skip_to_last()
            .next()
            .map(|(key, _)| key)
            .unwrap_or_default()
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
        object_store: impl ObjectStore,
        skipped_consensus_txns: &IntCounter,
    ) -> SuiResult<Vec<VerifiedExecutableTransaction>> {
        self.process_consensus_transactions_and_commit_boundary(
            transactions,
            &ExecutionIndicesWithStats::default(),
            checkpoint_service,
            object_store,
            self.get_highest_pending_checkpoint_height() + 1,
            0,
            skipped_consensus_txns,
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
        object_store: impl ObjectStore,
        commit_round: Round,
        previously_deferred_tx_digests: HashSet<TransactionDigest>,
        last_randomness_round: RandomnessRound,
    ) -> SuiResult<(
        Vec<VerifiedExecutableTransaction>,
        Vec<SequencedConsensusTransactionKey>, // keys to notify as complete
        Option<(RwLockWriteGuard<ReconfigState>, u64)>,
    )> {
        let mut verified_certificates = Vec::with_capacity(transactions.len());
        let mut notifications = Vec::with_capacity(transactions.len());

        // get the current next versions for each shared object in transactions
        let mut shared_input_next_versions = {
            let unique_shared_input_objects = {
                let mut shared_input_objects: Vec<_> = transactions
                    .iter()
                    .filter_map(|tx| tx.0.as_shared_object_txn())
                    .flat_map(|tx| {
                        tx.transaction_data()
                            .shared_input_objects()
                            .into_iter()
                            .map(|so| so.into_id_and_version())
                    })
                    .collect();

                shared_input_objects.sort();
                shared_input_objects.dedup();
                shared_input_objects
            };

            self.get_or_init_next_object_versions(
                unique_shared_input_objects.into_iter(),
                &object_store,
            )
            .await?
        };

        let mut deferred_txns: BTreeMap<DeferralKey, Vec<VerifiedSequencedConsensusTransaction>> =
            BTreeMap::new();

        for tx in transactions {
            let key = tx.0.transaction.key();
            let mut ignored = false;
            match self
                .process_consensus_transaction(
                    batch,
                    &mut shared_input_next_versions,
                    tx,
                    checkpoint_service,
                    commit_round,
                    &previously_deferred_tx_digests,
                    last_randomness_round,
                )
                .await?
            {
                ConsensusCertificateResult::SuiTransaction(cert) => {
                    notifications.push(key.clone());
                    verified_certificates.push(cert);
                }
                ConsensusCertificateResult::Defered(deferral_key) => {
                    // Note: record_consensus_message_processed() must be called for this
                    // cert even though we are not processing it now!
                    deferred_txns
                        .entry(deferral_key)
                        .or_default()
                        .push(tx.clone());
                }
                ConsensusCertificateResult::ConsensusMessage => notifications.push(key.clone()),
                ConsensusCertificateResult::IgnoredSystem => (),
                // Note: ignored external transactions must not be recorded as processed. Otherwise
                // they may not get reverted after restart during epoch change.
                ConsensusCertificateResult::Ignored => ignored = true,
            }
            if !ignored {
                self.record_consensus_message_processed(batch, key.clone())?;
            }
        }

        for (key, txns) in deferred_txns.into_iter() {
            self.defer_transactions(batch, key, txns)?;
        }

        batch.insert_batch(
            &self.tables()?.next_shared_object_versions,
            shared_input_next_versions.into_iter(),
        )?;

        let lock_and_final_round =
            self.process_end_of_publish_transactions(batch, end_of_publish_transactions)?;

        Ok((verified_certificates, notifications, lock_and_final_round))
    }

    fn process_end_of_publish_transactions(
        &self,
        write_batch: &mut DBBatch,
        transactions: &[VerifiedSequencedConsensusTransaction],
    ) -> SuiResult<
        Option<(
            RwLockWriteGuard<ReconfigState>,
            u64, /* final checkpoint round */
        )>,
    > {
        let mut ret = None;

        for transaction in transactions {
            let VerifiedSequencedConsensusTransaction(SequencedConsensusTransaction {
                consensus_index,
                transaction,
                ..
            }) = transaction;

            if let SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::EndOfPublish(authority),
                ..
            }) = transaction
            {
                debug!("Received EndOfPublish from {:?}", authority.concise());

                // It is ok to just release lock here as this function is the only place that transition into RejectAllCerts state
                // And this function itself is always executed from consensus task
                let collected_end_of_publish = if ret.is_none()
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
                    assert!(ret.is_none());
                    debug!(
                        "Collected enough end_of_publish messages with last message from validator {:?}",
                        authority.concise()
                    );
                    let mut lock = self.get_reconfig_state_write_lock_guard();
                    lock.close_all_certs();
                    // We store reconfig_state and end_of_publish in same batch to avoid dealing with inconsistency here on restart
                    self.store_reconfig_state_batch(&lock, write_batch)?;
                    write_batch.insert_batch(
                        &self.tables()?.final_epoch_checkpoint,
                        [(
                            &FINAL_EPOCH_CHECKPOINT_INDEX,
                            &consensus_index.last_committed_round,
                        )],
                    )?;
                    // Holding this lock until end of process_consensus_transactions_and_commit_boundary() where we write batch to DB
                    ret = Some((lock, consensus_index.last_committed_round));
                };
                // Important: we actually rely here on fact that ConsensusHandler panics if it's
                // operation returns error. If some day we won't panic in ConsensusHandler on error
                // we need to figure out here how to revert in-memory state of .end_of_publish
                // and .reconfig_state when write fails.
                self.record_consensus_message_processed(write_batch, transaction.key())?;
            } else {
                panic!(
                    "process_end_of_publish_transaction called with non-end-of-publish transaction"
                );
            }
        }
        Ok(ret)
    }

    #[instrument(level = "trace", skip_all)]
    async fn process_consensus_transaction<C: CheckpointServiceNotify>(
        &self,
        batch: &mut DBBatch,
        shared_input_next_versions: &mut HashMap<ObjectID, SequenceNumber>,
        transaction: &VerifiedSequencedConsensusTransaction,
        checkpoint_service: &Arc<C>,
        commit_round: Round,
        previously_deferred_tx_digests: &HashSet<TransactionDigest>,
        last_randomness_round: RandomnessRound,
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
                if self.has_sent_end_of_publish(certificate_author)? {
                    // This can not happen with valid authority
                    // With some edge cases narwhal might sometimes resend previously seen certificate after EndOfPublish
                    // However this certificate will be filtered out before this line by `consensus_message_processed` call in `verify_consensus_transaction`
                    // If we see some new certificate here it means authority is byzantine and sent certificate after EndOfPublish (or we have some bug in ConsensusAdapter)
                    warn!("[Byzantine authority] Authority {:?} sent a new, previously unseen certificate {:?} after it sent EndOfPublish message to consensus", certificate_author.concise(), certificate.digest());
                    return Ok(ConsensusCertificateResult::Ignored);
                }
                // Safe because signatures are verified when VerifiedSequencedConsensusTransaction
                // is constructed.
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
                {
                    debug!("Ignoring consensus certificate for transaction {:?} because of end of epoch",
                    certificate.digest());
                    return Ok(ConsensusCertificateResult::Ignored);
                }

                if let Some(deferral_key) = self.should_defer(
                    &certificate,
                    commit_round,
                    previously_deferred_tx_digests,
                    last_randomness_round,
                ) {
                    debug!(
                        "Deferring consensus certificate for transaction {:?} until {deferral_key:?}",
                        certificate.digest(),
                    );
                    return Ok(ConsensusCertificateResult::Defered(deferral_key));
                }

                if certificate.contains_shared_object() {
                    self.record_shared_object_cert_from_consensus(
                        batch,
                        shared_input_next_versions,
                        &certificate,
                    )
                    .await?;
                } else {
                    self.record_owned_object_cert_from_consensus(batch, &certificate)
                        .await?;
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
            SequencedConsensusTransactionKind::System(system_transaction) => {
                if !self
                    .get_reconfig_state_read_lock_guard()
                    .should_accept_consensus_certs()
                {
                    debug!(
                        "Ignoring system transaction {:?} because of end of epoch",
                        system_transaction.digest()
                    );
                    return Ok(ConsensusCertificateResult::IgnoredSystem);
                }

                if let TransactionKind::RandomnessStateUpdate(rsu) =
                    &system_transaction.data().intent_message().value.kind()
                {
                    batch.insert_batch(
                        &self.tables()?.randomness_rounds_written,
                        [(RandomnessRound(rsu.randomness_round), ())],
                    )?;
                }

                // If needed we can support owned object system transactions as well...
                assert!(system_transaction.contains_shared_object());
                self.record_shared_object_cert_from_consensus(
                    batch,
                    shared_input_next_versions,
                    system_transaction,
                )
                .await?;

                Ok(ConsensusCertificateResult::SuiTransaction(
                    system_transaction.clone(),
                ))
            }
        }
    }

    pub(crate) fn write_pending_checkpoint(
        &self,
        batch: &mut DBBatch,
        checkpoint: &PendingCheckpoint,
    ) -> SuiResult {
        if let Some(pending) = self.get_pending_checkpoint(&checkpoint.height())? {
            if pending.roots != checkpoint.roots {
                panic!("Received checkpoint at index {} that contradicts previously stored checkpoint. Old digests: {:?}, new digests: {:?}", checkpoint.height(), pending.roots, checkpoint.roots);
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
            checkpoint.roots.len(),
        );
        trace!(
            checkpoint_commit_height = checkpoint.height(),
            "Transaction roots for pending checkpoint: {:?}",
            checkpoint.roots
        );

        batch.insert_batch(
            &self.tables()?.pending_checkpoints,
            std::iter::once((checkpoint.height(), checkpoint)),
        )?;

        Ok(())
    }

    pub fn get_pending_checkpoints(
        &self,
        last: Option<CheckpointCommitHeight>,
    ) -> SuiResult<Vec<(CheckpointCommitHeight, PendingCheckpoint)>> {
        let tables = self.tables()?;
        let mut iter = tables.pending_checkpoints.unbounded_iter();
        if let Some(last_processed_height) = last {
            iter = iter.skip_to(&(last_processed_height + 1))?;
        }
        Ok(iter.collect())
    }

    pub fn get_pending_checkpoint(
        &self,
        index: &CheckpointCommitHeight,
    ) -> SuiResult<Option<PendingCheckpoint>> {
        Ok(self.tables()?.pending_checkpoints.get(index)?)
    }

    pub fn process_pending_checkpoint(
        &self,
        commit_height: CheckpointCommitHeight,
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
                commit_height: Some(commit_height),
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
            commit_height: None,
            position_in_commit: 0,
        };
        self.tables()?
            .builder_checkpoint_summary_v2
            .insert(summary.sequence_number(), &builder_summary)?;
        Ok(())
    }

    pub fn last_built_checkpoint_commit_height(&self) -> SuiResult<Option<CheckpointCommitHeight>> {
        Ok(self
            .tables()?
            .builder_checkpoint_summary_v2
            .unbounded_iter()
            .skip_to_last()
            .next()
            .and_then(|(_, b)| b.commit_height))
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
            .epoch_first_checkpoint_ready_time_since_epoch_begin_ms
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

    pub fn last_randomness_round_written(&self) -> SuiResult<RandomnessRound> {
        Ok(self
            .tables()?
            .randomness_rounds_written
            .unbounded_iter()
            .skip_to_last()
            .next()
            .map_or(RandomnessRound(0), |(round, _)| round))
    }

    pub fn clear_signature_cache(&self) {
        self.signature_verifier.clear_signature_cache();
    }
}

impl GetSharedLocks for AuthorityPerEpochStore {
    fn get_shared_locks(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> Result<Vec<(ObjectID, SequenceNumber)>, SuiError> {
        Ok(self
            .tables()?
            .assigned_shared_object_versions
            .get(transaction_digest)?
            .unwrap_or_default())
    }
}

impl ExecutionComponents {
    fn new(
        protocol_config: &ProtocolConfig,
        store: Arc<AuthorityStore>,
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
