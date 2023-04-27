// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::{join_all, select, Either};
use futures::FutureExt;
use narwhal_executor::ExecutionIndices;
use narwhal_types::Round;
use parking_lot::RwLock;
use parking_lot::{Mutex, RwLockReadGuard};
use rocksdb::Options;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::iter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sui_config::node::ExpensiveSafetyCheckConfig;
use sui_types::accumulator::Accumulator;
use sui_types::base_types::{AuthorityName, EpochId, ObjectID, SequenceNumber, TransactionDigest};
use sui_types::committee::Committee;
use sui_types::crypto::{AuthoritySignInfo, AuthorityStrongQuorumSignInfo};
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{
    AuthorityCapabilities, CertifiedTransaction, ConsensusTransaction, ConsensusTransactionKey,
    ConsensusTransactionKind, SenderSignedData, SharedInputObject, TransactionData,
    TransactionDataAPI, TrustedExecutableTransaction, VerifiedCertificate,
    VerifiedExecutableTransaction, VerifiedSignedTransaction,
};
use sui_types::signature::GenericSignature;
use tracing::{debug, error, info, trace, warn};
use typed_store::rocks::{
    default_db_options, DBBatch, DBMap, DBOptions, MetricConf, TypedStoreError,
};
use typed_store::traits::{TableSummary, TypedStoreDebug};

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
use crate::signature_verifier::*;
use crate::stake_aggregator::StakeAggregator;
use move_bytecode_utils::module_cache::SyncModuleCache;
use move_vm_runtime::move_vm::MoveVM;
use move_vm_runtime::native_functions::NativeFunctionTable;
use mysten_common::sync::notify_once::NotifyOnce;
use mysten_common::sync::notify_read::NotifyRead;
use mysten_metrics::monitored_scope;
use prometheus::IntCounter;
use std::cmp::Ordering as CmpOrdering;
use sui_adapter::adapter;
use sui_macros::fail_point;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion};
use sui_storage::mutex_table::{MutexGuard, MutexTable};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::message_envelope::TrustedEnvelope;
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointSequenceNumber, CheckpointSignatureMessage, CheckpointSummary,
    CheckpointTimestamp,
};
use sui_types::storage::{transaction_input_object_keys, ObjectKey, ParentSync};
use sui_types::sui_system_state::epoch_start_sui_system_state::{
    EpochStartSystemState, EpochStartSystemStateTrait,
};
use tap::TapOptional;
use tokio::time::Instant;
use typed_store::{retry_transaction_forever, Map};
use typed_store_derive::DBMapUtils;

use super::epoch_start_configuration::EpochStartConfigTrait;

/// The key where the latest consensus index is stored in the database.
// TODO: Make a single table (e.g., called `variables`) storing all our lonely variables in one place.
const LAST_CONSENSUS_INDEX_ADDR: u64 = 0;
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

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionIndicesWithHash {
    pub index: ExecutionIndices,
    pub hash: u64,
}

// Data related to VM and Move execution and type layout
pub struct ExecutionComponents {
    /// Move native functions that are available to invoke
    pub(crate) native_functions: NativeFunctionTable,
    pub(crate) move_vm: Arc<MoveVM>,
    // TODO: use strategies (e.g. LRU?) to constraint memory usage
    pub(crate) module_cache: Arc<SyncModuleCache<ResolverWrapper<AuthorityStore>>>,
    metrics: Arc<ResolverMetrics>,
}

pub struct AuthorityPerEpochStore {
    committee: Arc<Committee>,
    tables: AuthorityEpochTables,

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
    /// Fast cache for EpochFlag::InMemoryCheckpointRoots
    pub(crate) in_memory_checkpoint_roots: bool,
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

    // todo - this table will be deleted after switch to EpochFlag::InMemoryCheckpointRoots
    /// This is an inverse index for consensus_message_processed - it allows to select
    /// all transactions at the specific consensus range
    ///
    /// The consensus position for the transaction is defined as first position at which valid
    /// certificate for this transaction is seen in consensus
    consensus_message_order: DBMap<ExecutionIndices, TransactionDigest>,

    /// The following table is used to store a single value (the corresponding key is a constant). The value
    /// represents the index of the latest consensus message this authority processed. This field is written
    /// by a single process acting as consensus (light) client. It is used to ensure the authority processes
    /// every message output by consensus (and in the right order).
    last_consensus_index: DBMap<u64, ExecutionIndicesWithHash>,

    // todo - this table will be deleted after switch to EpochFlag::InMemoryCheckpointRoots
    /// This table lists all checkpoint boundaries in the consensus sequence
    ///
    /// The key in this table is incremental index and value is corresponding narwhal
    /// consensus output index
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
    ///
    /// The boolean value indicates whether this is the last checkpoint of the epoch.
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
}

fn signed_transactions_table_default_config() -> DBOptions {
    default_db_options()
        .optimize_for_write_throughput()
        .optimize_for_large_values_no_scan()
}

fn pending_execution_table_default_config() -> DBOptions {
    default_db_options()
        .optimize_for_write_throughput()
        .optimize_for_large_values_no_scan()
}

fn pending_consensus_transactions_table_default_config() -> DBOptions {
    default_db_options()
        .optimize_for_write_throughput()
        .optimize_for_large_values_no_scan()
}

fn pending_checkpoints_table_default_config() -> DBOptions {
    default_db_options()
        .optimize_for_write_throughput()
        .optimize_for_large_values_no_scan()
}

impl AuthorityEpochTables {
    pub fn open(epoch: EpochId, parent_path: &Path, db_options: Option<Options>) -> Self {
        Self::open_tables_transactional(
            Self::path(epoch, parent_path),
            MetricConf::with_db_name("epoch"),
            db_options,
            None,
        )
    }

    pub fn open_readonly(epoch: EpochId, parent_path: &Path) -> AuthorityEpochTablesReadOnly {
        Self::get_read_only_handle(
            Self::path(epoch, parent_path),
            None,
            None,
            MetricConf::with_db_name("epoch"),
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
            .iter()
            .map(|(_k, v)| v)
            .collect()
    }
}

pub(crate) const MUTEX_TABLE_SIZE: usize = 1024;

impl AuthorityPerEpochStore {
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
    ) -> Arc<Self> {
        let current_time = Instant::now();
        let epoch_id = committee.epoch;
        let tables = AuthorityEpochTables::open(epoch_id, parent_path, db_options.clone());
        let end_of_publish =
            StakeAggregator::from_iter(committee.clone(), tables.end_of_publish.iter());
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
        let protocol_config = ProtocolConfig::get_for_version(protocol_version);

        let execution_component = ExecutionComponents::new(
            &protocol_config,
            store,
            cache_metrics,
            expensive_safety_check_config,
        );
        let signature_verifier =
            SignatureVerifier::new(committee.clone(), signature_verifier_metrics);
        let in_memory_checkpoint_roots = epoch_start_configuration
            .flags()
            .contains(&EpochFlag::InMemoryCheckpointRoots);
        info!(
            "in_memory_checkpoint_roots = {}",
            in_memory_checkpoint_roots
        );
        let s = Arc::new(Self {
            committee,
            protocol_config,
            tables,
            parent_path: parent_path.to_path_buf(),
            db_options,
            reconfig_state_mem: RwLock::new(reconfig_state),
            epoch_alive_notify,
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
            in_memory_checkpoint_roots,
        });
        s.update_buffer_stake_metric();
        s
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

    pub fn new_at_next_epoch(
        &self,
        name: AuthorityName,
        new_committee: Committee,
        epoch_start_configuration: EpochStartConfiguration,
        store: Arc<AuthorityStore>,
        expensive_safety_check_config: &ExpensiveSafetyCheckConfig,
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
        Ok(self.tables.state_hash_by_checkpoint.get(checkpoint)?)
    }

    pub fn insert_state_hash_for_checkpoint(
        &self,
        checkpoint: &CheckpointSequenceNumber,
        accumulator: &Accumulator,
    ) -> SuiResult {
        Ok(self
            .tables
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

    pub fn move_vm(&self) -> &Arc<MoveVM> {
        &self.execution_component.move_vm
    }

    pub fn native_functions(&self) -> &NativeFunctionTable {
        &self.execution_component.native_functions
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
        self.tables
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
            &self.tables.reconfig_state,
            [(&RECONFIG_STATE_INDEX, new_state)],
        )?;
        Ok(())
    }

    pub fn insert_signed_transaction(&self, transaction: VerifiedSignedTransaction) -> SuiResult {
        Ok(self
            .tables
            .signed_transactions
            .insert(transaction.digest(), transaction.serializable_ref())?)
    }

    #[cfg(test)]
    pub fn delete_signed_transaction_for_test(&self, transaction: &TransactionDigest) {
        self.tables.signed_transactions.remove(transaction).unwrap();
    }

    pub fn get_signed_transaction(
        &self,
        tx_digest: &TransactionDigest,
    ) -> SuiResult<Option<VerifiedSignedTransaction>> {
        Ok(self
            .tables
            .signed_transactions
            .get(tx_digest)?
            .map(|t| t.into()))
    }

    pub fn insert_tx_cert_and_effects_signature(
        &self,
        tx_digest: &TransactionDigest,
        cert_sig: Option<&AuthorityStrongQuorumSignInfo>,
        effects_signature: Option<&AuthoritySignInfo>,
    ) -> SuiResult {
        let mut batch = self.tables.effects_signatures.batch();
        if let Some(cert_sig) = cert_sig {
            batch.insert_batch(
                &self.tables.transaction_cert_signatures,
                [(tx_digest, cert_sig)],
            )?;
        }
        if let Some(effects_signature) = effects_signature {
            batch.insert_batch(
                &self.tables.effects_signatures,
                [(tx_digest, effects_signature)],
            )?;
        }
        batch.write()?;
        Ok(())
    }

    pub fn get_effects_signature(
        &self,
        tx_digest: &TransactionDigest,
    ) -> SuiResult<Option<AuthoritySignInfo>> {
        Ok(self.tables.effects_signatures.get(tx_digest)?)
    }

    pub fn get_transaction_cert_sig(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<Option<AuthorityStrongQuorumSignInfo>, TypedStoreError> {
        self.tables.transaction_cert_signatures.get(tx_digest)
    }

    pub fn multi_get_next_shared_object_versions<'a>(
        &self,
        ids: impl Iterator<Item = &'a ObjectID>,
    ) -> SuiResult<Vec<Option<SequenceNumber>>> {
        Ok(self.tables.next_shared_object_versions.multi_get(ids)?)
    }

    pub fn get_last_checkpoint_boundary(&self) -> (u64, Option<u64>) {
        match self.tables.checkpoint_boundary.iter().skip_to_last().next() {
            Some((idx, height)) => (idx, Some(height)),
            None => (0, None),
        }
    }

    pub fn get_last_consensus_index(&self) -> SuiResult<ExecutionIndicesWithHash> {
        self.tables
            .last_consensus_index
            .get(&LAST_CONSENSUS_INDEX_ADDR)
            .map(|x| x.unwrap_or_default())
            .map_err(SuiError::from)
    }

    pub fn get_accumulators_in_checkpoint_range(
        &self,
        from_checkpoint: CheckpointSequenceNumber,
        to_checkpoint: CheckpointSequenceNumber,
    ) -> Result<Vec<(CheckpointSequenceNumber, Accumulator)>, TypedStoreError> {
        Ok(self
            .tables
            .state_hash_by_checkpoint
            .iter()
            .skip_to(&from_checkpoint)?
            .take_while(|(checkpoint, _)| *checkpoint <= to_checkpoint)
            .collect())
    }

    pub fn get_transactions_in_checkpoint_range(
        &self,
        from_height_excluded: Option<u64>,
        to_height_included: u64,
    ) -> SuiResult<Vec<TransactionDigest>> {
        let mut iter = self.tables.consensus_message_order.iter();
        if let Some(from_height_excluded) = from_height_excluded {
            let last_previous = ExecutionIndices::end_for_commit(from_height_excluded);
            iter = iter.skip_to(&last_previous)?;
        }
        // skip_to lands to key the last_key or key after it
        // technically here we need to check if first item in stream has a key equal to last_previous
        // however in practice this can not happen because number of batches in certificate is
        // limited and is less then u64::MAX
        let roots: Vec<_> = iter
            .take_while(|(idx, _tx)| idx.last_committed_round <= to_height_included)
            .map(|(_idx, tx)| tx)
            .collect();
        Ok(roots)
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
            .tables
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
            .tables
            .pending_execution
            .iter()
            .map(|(_, cert)| cert.into())
            .collect())
    }

    /// Deletes one pending certificate.
    pub fn remove_pending_execution(&self, digest: &TransactionDigest) -> SuiResult<()> {
        self.tables.pending_execution.remove(digest)?;
        Ok(())
    }

    pub fn get_all_pending_consensus_transactions(&self) -> Vec<ConsensusTransaction> {
        self.tables.get_all_pending_consensus_transactions()
    }

    /// Read shared object locks / versions for a specific transaction.
    pub fn get_shared_locks(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> Result<Vec<(ObjectID, SequenceNumber)>, SuiError> {
        Ok(self
            .tables
            .assigned_shared_object_versions
            .get(transaction_digest)?
            .unwrap_or_default())
    }

    #[cfg(test)]
    pub fn get_next_object_version(&self, obj: &ObjectID) -> Option<SequenceNumber> {
        self.tables.next_shared_object_versions.get(obj).unwrap()
    }

    pub fn set_shared_object_versions_for_testing(
        &self,
        tx_digest: &TransactionDigest,
        assigned_versions: &Vec<(ObjectID, SequenceNumber)>,
    ) -> SuiResult {
        self.tables
            .assigned_shared_object_versions
            .insert(tx_digest, assigned_versions)?;
        Ok(())
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
        tx_data: &TransactionData,
        objects_to_init: impl Iterator<Item = ObjectID> + Clone,
        parent_sync_store: impl ParentSync,
    ) -> SuiResult<Vec<SequenceNumber>> {
        // Since this can be called from consensus task, we must retry forever - the only other
        // option is to panic. It is extremely unlikely that more than 2 retries will be needed, as
        // the only two writers are the consensus task and checkpoint execution.
        retry_transaction_forever!({
            // This code may still be correct without using a transaction snapshot, but I couldn't
            // convince myself of that.
            let mut db_transaction = self.tables.next_shared_object_versions.transaction()?;

            let next_versions = db_transaction.multi_get(
                &self.tables.next_shared_object_versions,
                objects_to_init.clone(),
            )?;

            let uninitialized_objects: Vec<ObjectID> = next_versions
                .iter()
                .zip(objects_to_init.clone())
                .filter_map(|(next_version, id)| match next_version {
                    None => Some(id),
                    Some(_) => None,
                })
                .collect();

            // The common case is that there are no uninitialized versions - this early return will
            // happen every time except the first time an object is used in an epoch.
            if uninitialized_objects.is_empty() {
                // unwrap ok - we already verified that next_versions is not missing any keys.
                return Ok(next_versions.into_iter().map(|v| v.unwrap()).collect());
            }

            // if the object has never been used before (in any epoch) the initial version comes
            // from the cert.
            let initial_versions: HashMap<_, _> = tx_data
                .shared_input_objects()
                .into_iter()
                .map(SharedInputObject::into_id_and_version)
                .collect();

            let versions_to_write = uninitialized_objects.iter().map(|id| {
                // Note: we don't actually need to read from the transaction here, as no writer
                // can update parent_sync_store until after get_or_init_next_object_versions
                // completes.
                match parent_sync_store
                    .get_latest_parent_entry_ref(*id)
                    .expect("read cannot fail")
                {
                    Some(objref) => (*id, objref.1),
                    None => (
                        *id,
                        *initial_versions
                            .get(id)
                            .expect("object cannot be missing from shared_input_objects"),
                    ),
                }
            });

            debug!(
                ?versions_to_write,
                "initializing next_shared_object_versions"
            );
            db_transaction
                .insert_batch(&self.tables.next_shared_object_versions, versions_to_write)?;
            db_transaction.commit()
        })?;

        // this case only occurs when there were uninitialized versions, which is rare, so its much
        // simpler to just re-read all the ids here.
        let next_versions = self
            .tables
            .next_shared_object_versions
            .multi_get(objects_to_init)?
            .into_iter()
            // unwrap ok - we just finished initializing all versions.
            .map(|v| v.unwrap())
            .collect();

        Ok(next_versions)
    }

    async fn set_assigned_shared_object_versions(
        &self,
        certificate: &VerifiedExecutableTransaction,
        assigned_versions: &Vec<(ObjectID, SequenceNumber)>,
        parent_sync_store: impl ParentSync,
    ) -> SuiResult {
        let tx_digest = certificate.digest();

        debug!(
            ?tx_digest,
            ?assigned_versions,
            "set_assigned_shared_object_versions"
        );
        self.get_or_init_next_object_versions(
            certificate.data().transaction_data(),
            assigned_versions.iter().map(|(id, _)| *id),
            parent_sync_store,
        )
        .await?;
        self.tables
            .assigned_shared_object_versions
            .insert(tx_digest, assigned_versions)?;
        Ok(())
    }

    /// Lock a sequence number for the shared objects of the input transaction based on the effects
    /// of that transaction.
    /// Used by full nodes who don't listen to consensus, and validators who catch up by state sync.
    pub async fn acquire_shared_locks_from_effects(
        &self,
        certificate: &VerifiedExecutableTransaction,
        effects: &TransactionEffects,
        parent_sync_store: impl ParentSync,
    ) -> SuiResult {
        self.set_assigned_shared_object_versions(
            certificate,
            &effects
                .shared_objects()
                .iter()
                .map(|(id, version, _)| (*id, *version))
                .collect(),
            parent_sync_store,
        )
        .await
    }

    pub fn insert_checkpoint_boundary(&self, index: u64, height: u64) -> SuiResult {
        self.tables.checkpoint_boundary.insert(&index, &height)?;
        Ok(())
    }

    /// When submitting a certificate caller **must** provide a ReconfigState lock guard
    /// and verify that it allows new user certificates
    pub fn insert_pending_consensus_transactions(
        &self,
        transaction: &ConsensusTransaction,
        lock: Option<&RwLockReadGuard<ReconfigState>>,
    ) -> SuiResult {
        self.tables
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
        self.tables.pending_consensus_transactions.remove(key)?;
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

    /// Stores a list of pending certificates to be executed.
    pub fn insert_pending_execution(
        &self,
        certs: &[TrustedExecutableTransaction],
    ) -> Result<(), TypedStoreError> {
        let mut batch = self.tables.pending_execution.batch();
        batch.insert_batch(
            &self.tables.pending_execution,
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
        Ok(self.tables.consensus_message_processed.contains_key(key)?)
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
            .tables
            .user_signatures_for_checkpoints
            .multi_get(digests)?;
        let mut result = Vec::with_capacity(digests.len());
        for (signatures, digest) in signatures.into_iter().zip(digests.iter()) {
            let Some(signatures) = signatures else {
                return Err(SuiError::from(format!("Can not find user signature for checkpoint for transaction {:?}", digest).as_str()));
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
        self.tables
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
        self.tables.override_protocol_upgrade_buffer_stake.insert(
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
        self.tables
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
        if let Some(cap) = self.tables.authority_capabilities.get(authority)? {
            if cap.generation >= capabilities.generation {
                debug!(
                    "ignoring new capabilities {:?} in favor of previous capabilities {:?}",
                    capabilities, cap
                );
                return Ok(());
            }
        }
        self.tables
            .authority_capabilities
            .insert(authority, capabilities)?;
        Ok(())
    }

    pub fn get_capabilities(&self) -> Result<Vec<AuthorityCapabilities>, TypedStoreError> {
        self.tables.authority_capabilities.values().collect()
    }

    /// Returns Ok(true) if 2f+1 end of publish messages were recorded at this point
    pub fn record_end_of_publish(
        &self,
        authority: AuthorityName,
        key: SequencedConsensusTransactionKey,
        consensus_index: ExecutionIndicesWithHash,
    ) -> SuiResult {
        let mut write_batch = self.tables.last_consensus_index.batch();
        // It is ok to just release lock here as this function is the only place that transition into RejectAllCerts state
        // And this function itself is always executed from consensus task
        let collected_end_of_publish = if self
            .get_reconfig_state_read_lock_guard()
            .should_accept_consensus_certs()
        {
            write_batch.insert_batch(&self.tables.end_of_publish, [(authority, ())])?;
            self.end_of_publish.try_lock()
                .expect("No contention on Authority::end_of_publish as it is only accessed from consensus handler")
                .insert_generic(authority, ()).is_quorum_reached()
        } else {
            // If we past the stage where we are accepting consensus certificates we also don't record end of publish messages
            debug!("Ignoring end of publish message from validator {:?} as we already collected enough end of publish messages", authority.concise());
            false
        };
        let _lock = if collected_end_of_publish {
            debug!(
                "Collected enough end_of_publish messages with last message from validator {:?}",
                authority.concise()
            );
            let mut lock = self.get_reconfig_state_write_lock_guard();
            lock.close_all_certs();
            // We store reconfig_state and end_of_publish in same batch to avoid dealing with inconsistency here on restart
            self.store_reconfig_state_batch(&lock, &mut write_batch)?;
            write_batch.insert_batch(
                &self.tables.final_epoch_checkpoint,
                [(
                    &FINAL_EPOCH_CHECKPOINT_INDEX,
                    &consensus_index.index.last_committed_round,
                )],
            )?;
            // Holding this lock until end of this function where we write batch to DB
            Some(lock)
        } else {
            None
        };
        // Important: we actually rely here on fact that ConsensusHandler panics if it's operation returns error
        // If some day we won't panic in ConsensusHandler on error we need to figure out here how
        // to revert in-memory state of .end_of_publish and .reconfig_state when write fails
        self.finish_consensus_transaction_process_with_batch(write_batch, key, consensus_index)
    }

    /// Caller is responsible to call consensus_message_processed before this method
    pub async fn record_owned_object_cert_from_consensus(
        &self,
        transaction: &SequencedConsensusTransactionKind,
        certificate: &VerifiedExecutableTransaction,
        consensus_index: ExecutionIndicesWithHash,
    ) -> Result<(), SuiError> {
        let key = transaction.key();
        self.finish_consensus_certificate_process(key, certificate, consensus_index)
    }

    /// Locks a sequence number for the shared objects of the input transaction. Also updates the
    /// last consensus index, consensus_message_processed and pending_certificates tables.
    /// This function must only be called from the consensus task (i.e. from handle_consensus_transaction).
    ///
    /// Caller is responsible to call consensus_message_processed before this method
    pub async fn record_shared_object_cert_from_consensus(
        &self,
        transaction: &SequencedConsensusTransactionKind,
        certificate: &VerifiedExecutableTransaction,
        consensus_index: ExecutionIndicesWithHash,
        parent_sync_store: impl ParentSync,
    ) -> Result<(), SuiError> {
        // Make an iterator to save the certificate.
        let transaction_digest = *certificate.digest();

        // Make an iterator to update the locks of the transaction's shared objects.
        let shared_input_objects: Vec<_> = certificate.shared_input_objects().collect();
        let ids: Vec<_> = shared_input_objects
            .iter()
            .map(SharedInputObject::id)
            .collect();

        let versions = self
            .get_or_init_next_object_versions(
                certificate.data().transaction_data(),
                ids.iter().copied(),
                &parent_sync_store,
            )
            .await?;

        let mut input_object_keys = transaction_input_object_keys(certificate)?;
        let mut assigned_versions = Vec::with_capacity(shared_input_objects.len());
        let mut is_mutable_input = Vec::with_capacity(shared_input_objects.len());
        for (SharedInputObject { id, mutable, .. }, version) in
            shared_input_objects.iter().zip(versions.into_iter())
        {
            assigned_versions.push((*id, version));
            input_object_keys.push(ObjectKey(*id, version));
            is_mutable_input.push(*mutable);
        }

        let next_version =
            SequenceNumber::lamport_increment(input_object_keys.iter().map(|obj| obj.1));
        let next_versions: Vec<_> = assigned_versions
            .iter()
            .zip(is_mutable_input.into_iter())
            .filter_map(|((id, _), mutable)| {
                if mutable {
                    Some((*id, next_version))
                } else {
                    None
                }
            })
            .collect();

        trace!(tx_digest = ?transaction_digest,
               ?assigned_versions, ?next_version,
               "locking shared objects");

        self.finish_assign_shared_object_versions(
            transaction.key(),
            certificate,
            consensus_index,
            assigned_versions,
            next_versions,
        )
    }

    pub fn record_consensus_transaction_processed(
        &self,
        transaction: &SequencedConsensusTransactionKind,
        consensus_index: ExecutionIndicesWithHash,
    ) -> Result<(), SuiError> {
        // executable transactions need to use record_(shared|owned)_object_cert_from_consensus
        assert!(!transaction.is_executable_transaction());
        let key = transaction.key();
        let write_batch = self.tables.last_consensus_index.batch();
        self.finish_consensus_transaction_process_with_batch(write_batch, key, consensus_index)
    }

    pub fn finish_consensus_certificate_process(
        &self,
        key: SequencedConsensusTransactionKey,
        certificate: &VerifiedExecutableTransaction,
        consensus_index: ExecutionIndicesWithHash,
    ) -> SuiResult {
        let write_batch = self.tables.last_consensus_index.batch();
        self.finish_consensus_certificate_process_with_batch(
            write_batch,
            key,
            certificate,
            consensus_index,
        )
    }

    fn finish_assign_shared_object_versions(
        &self,
        key: SequencedConsensusTransactionKey,
        certificate: &VerifiedExecutableTransaction,
        consensus_index: ExecutionIndicesWithHash,
        assigned_versions: Vec<(ObjectID, SequenceNumber)>,
        next_versions: Vec<(ObjectID, SequenceNumber)>,
    ) -> SuiResult {
        // Atomically store all elements.
        let mut write_batch = self.tables.assigned_shared_object_versions.batch();

        let tx_digest = *certificate.digest();

        debug!(
            ?tx_digest,
            ?assigned_versions,
            "finish_assign_shared_object_versions"
        );
        write_batch.insert_batch(
            &self.tables.assigned_shared_object_versions,
            iter::once((tx_digest, assigned_versions)),
        )?;

        write_batch.insert_batch(&self.tables.next_shared_object_versions, next_versions)?;

        self.finish_consensus_certificate_process_with_batch(
            write_batch,
            key,
            certificate,
            consensus_index,
        )
    }

    /// When we finish processing certificate from consensus we record this information.
    /// Tables updated:
    ///  * consensus_message_processed - indicate that this certificate was processed by consensus
    ///  * last_consensus_index - records last processed position in consensus stream
    ///  * consensus_message_order - records at what position this transaction was first seen in consensus
    /// Self::consensus_message_processed returns true after this call for given certificate
    fn finish_consensus_transaction_process_with_batch(
        &self,
        mut batch: DBBatch,
        key: SequencedConsensusTransactionKey,
        consensus_index: ExecutionIndicesWithHash,
    ) -> SuiResult {
        batch.insert_batch(
            &self.tables.last_consensus_index,
            [(LAST_CONSENSUS_INDEX_ADDR, consensus_index)],
        )?;
        batch.insert_batch(&self.tables.consensus_message_processed, [(key, true)])?;
        batch.write()?;
        self.consensus_notify_read.notify(&key, &());
        Ok(())
    }

    pub fn test_insert_user_signature(
        &self,
        digest: TransactionDigest,
        signatures: Vec<GenericSignature>,
    ) {
        self.tables
            .user_signatures_for_checkpoints
            .insert(&digest, &signatures)
            .unwrap();
        let key = ConsensusTransactionKey::Certificate(digest);
        let key = SequencedConsensusTransactionKey::External(key);
        self.tables
            .consensus_message_processed
            .insert(&key, &true)
            .unwrap();
        self.consensus_notify_read.notify(&key, &());
    }

    fn finish_consensus_certificate_process_with_batch(
        &self,
        mut batch: DBBatch,
        key: SequencedConsensusTransactionKey,
        certificate: &VerifiedExecutableTransaction,
        consensus_index: ExecutionIndicesWithHash,
    ) -> SuiResult {
        if !self.in_memory_checkpoint_roots {
            let transaction_digest = *certificate.digest();
            batch.insert_batch(
                &self.tables.consensus_message_order,
                [(consensus_index.index, transaction_digest)],
            )?;
        }
        batch.insert_batch(
            &self.tables.pending_execution,
            [(*certificate.digest(), certificate.clone().serializable())],
        )?;
        // User signatures are written in the same batch as consensus certificate processed flag,
        // which means we won't attempt to insert this twice for the same tx digest
        debug_assert!(!self
            .tables
            .user_signatures_for_checkpoints
            .contains_key(certificate.digest())?);
        batch.insert_batch(
            &self.tables.user_signatures_for_checkpoints,
            [(*certificate.digest(), certificate.tx_signatures().to_vec())],
        )?;
        self.finish_consensus_transaction_process_with_batch(batch, key, consensus_index)
    }

    pub fn final_epoch_checkpoint(&self) -> SuiResult<Option<u64>> {
        Ok(self
            .tables
            .final_epoch_checkpoint
            .get(&FINAL_EPOCH_CHECKPOINT_INDEX)?)
    }

    /// Returns transaction digests from consensus_message_order table in the "checkpoint range".
    ///
    /// Checkpoint range is defined from the last seen checkpoint(excluded) to the provided
    /// to_height (included)
    pub fn last_checkpoint(
        &self,
        to_height_included: u64,
    ) -> SuiResult<Option<(u64, Vec<TransactionDigest>)>> {
        let (index, from_height_excluded) = self.get_last_checkpoint_boundary();

        if let Some(from_height_excluded) = from_height_excluded {
            if from_height_excluded >= to_height_included {
                // Due to crash recovery we might enter this function twice for same boundary
                debug!("Not returning last checkpoint - already processed");
                return Ok(None);
            }
        }

        let roots =
            self.get_transactions_in_checkpoint_range(from_height_excluded, to_height_included)?;

        debug!(
            "Selected {} roots between narwhal commit rounds {:?} and {}",
            roots.len(),
            from_height_excluded,
            to_height_included
        );

        Ok(Some((index, roots)))
    }
    pub fn record_checkpoint_boundary(&self, commit_round: u64) -> SuiResult {
        let (index, height) = self.get_last_checkpoint_boundary();

        if let Some(height) = height {
            if height >= commit_round {
                // Due to crash recovery we might see same boundary twice
                debug!("Not recording checkpoint boundary - already updated");
                return Ok(());
            }
        }

        let index = index + 1;
        debug!(
            "Recording checkpoint boundary {} at {}",
            index, commit_round
        );
        self.insert_checkpoint_boundary(index, commit_round)?;
        Ok(())
    }

    pub fn get_reconfig_state_read_lock_guard(
        &self,
    ) -> parking_lot::RwLockReadGuard<ReconfigState> {
        self.reconfig_state_mem.read()
    }

    pub fn get_reconfig_state_write_lock_guard(
        &self,
    ) -> parking_lot::RwLockWriteGuard<ReconfigState> {
        self.reconfig_state_mem.write()
    }

    pub fn close_user_certs(
        &self,
        mut lock_guard: parking_lot::RwLockWriteGuard<'_, ReconfigState>,
    ) {
        lock_guard.close_user_certs();
        self.store_reconfig_state(&lock_guard)
            .expect("Updating reconfig state cannot fail");

        // Set epoch_close_time for metric purpose.
        let mut epoch_close_time = self.epoch_close_time.write();
        if epoch_close_time.is_none() {
            // Only update it the first time epoch is closed.
            *epoch_close_time = Some(Instant::now());
        }
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

    /// Verifies transaction signatures and other data
    /// Important: This function can potentially be called in parallel and you can not rely on order of transactions to perform verification
    /// If this function return an error, transaction is skipped and is not passed to handle_consensus_transaction
    /// This function returns unit error and is responsible for emitting log messages for internal errors
    pub(crate) fn verify_consensus_transaction(
        &self,
        transaction: SequencedConsensusTransaction,
        skipped_consensus_txns: &IntCounter,
    ) -> Result<VerifiedSequencedConsensusTransaction, ()> {
        let _scope = monitored_scope("VerifyConsensusTransaction");
        if self
            .is_consensus_message_processed(&transaction.transaction.key())
            .expect("Storage error")
        {
            debug!(
                consensus_index=?transaction.consensus_index.index.transaction_index,
                tracking_id=?transaction.transaction.get_tracking_id(),
                "handle_consensus_transaction UserTransaction [skip]",
            );
            skipped_consensus_txns.inc();
            return Err(());
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
                    warn!("CheckpointSignature authority {} does not match narwhal certificate source {}", data.summary.auth_sig().authority, transaction.certificate.origin() );
                    return Err(());
                }
            }
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::EndOfPublish(authority),
                ..
            }) => {
                if &transaction.sender_authority() != authority {
                    warn!(
                        "EndOfPublish authority {} does not match narwhal certificate source {}",
                        authority,
                        transaction.certificate.origin()
                    );
                    return Err(());
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
                        transaction.certificate.origin()
                    );
                    return Err(());
                }
            }
            SequencedConsensusTransactionKind::System(_) => {}
        }
        Ok(VerifiedSequencedConsensusTransaction(transaction))
    }

    /// Depending on the type of the VerifiedSequencedConsensusTransaction wrappers,
    /// - Verify and initialize the state to execute the certificates.
    ///   Return VerifiedCertificates for each executable certificate
    /// - Or update the state for checkpoint or epoch change protocol.
    pub(crate) async fn process_consensus_transactions<C: CheckpointServiceNotify>(
        &self,
        transactions: Vec<VerifiedSequencedConsensusTransaction>,
        checkpoint_service: &Arc<C>,
        parent_sync_store: impl ParentSync,
    ) -> SuiResult<Vec<VerifiedExecutableTransaction>> {
        let mut verified_certificates = Vec::new();
        for tx in transactions {
            if let Some(cert) = self
                .process_consensus_transaction(tx, checkpoint_service, &parent_sync_store)
                .await?
            {
                verified_certificates.push(cert);
            }
        }
        Ok(verified_certificates)
    }

    async fn process_consensus_transaction<C: CheckpointServiceNotify>(
        &self,
        transaction: VerifiedSequencedConsensusTransaction,
        checkpoint_service: &Arc<C>,
        parent_sync_store: impl ParentSync,
    ) -> SuiResult<Option<VerifiedExecutableTransaction>> {
        let _scope = monitored_scope("HandleConsensusTransaction");
        let VerifiedSequencedConsensusTransaction(SequencedConsensusTransaction {
            certificate: _consensus_output,
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
                    return Ok(None);
                }
                if self.has_sent_end_of_publish(&certificate_author)? {
                    // This can not happen with valid authority
                    // With some edge cases narwhal might sometimes resend previously seen certificate after EndOfPublish
                    // However this certificate will be filtered out before this line by `consensus_message_processed` call in `verify_consensus_transaction`
                    // If we see some new certificate here it means authority is byzantine and sent certificate after EndOfPublish (or we have some bug in ConsensusAdapter)
                    warn!("[Byzantine authority] Authority {:?} sent a new, previously unseen certificate {:?} after it sent EndOfPublish message to consensus", certificate_author.concise(), certificate.digest());
                    return Ok(None);
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
                    return Ok(None);
                }

                if certificate.contains_shared_object() {
                    self.record_shared_object_cert_from_consensus(
                        &transaction,
                        &certificate,
                        consensus_index,
                        parent_sync_store,
                    )
                    .await?;
                } else {
                    self.record_owned_object_cert_from_consensus(
                        &transaction,
                        &certificate,
                        consensus_index,
                    )
                    .await?;
                }

                Ok(Some(certificate))
            }
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::CheckpointSignature(info),
                ..
            }) => {
                checkpoint_service.notify_checkpoint_signature(self, info)?;
                self.record_consensus_transaction_processed(&transaction, consensus_index)?;
                Ok(None)
            }
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::EndOfPublish(authority),
                ..
            }) => {
                debug!("Received EndOfPublish from {:?}", authority.concise());
                self.record_end_of_publish(*authority, transaction.key(), consensus_index)?;
                Ok(None)
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
                self.record_consensus_transaction_processed(&transaction, consensus_index)?;
                Ok(None)
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
                    return Ok(None);
                }

                // If needed we can support owned object system transactions as well...
                assert!(system_transaction.contains_shared_object());
                self.record_shared_object_cert_from_consensus(
                    &transaction,
                    system_transaction,
                    consensus_index,
                    parent_sync_store,
                )
                .await?;

                Ok(Some(system_transaction.clone()))
            }
        }
    }

    pub fn handle_commit_boundary<C: CheckpointServiceNotify>(
        &self,
        round: Round,
        timestamp_ms: CheckpointTimestamp,
        checkpoint_service: &Arc<C>,
    ) -> SuiResult {
        debug!("Commit boundary at {}", round);
        // This exchange is restart safe because of following:
        //
        // We try to read last checkpoint content and send it to the checkpoint service
        // CheckpointService::notify_checkpoint is idempotent in case you send same last checkpoint multiple times
        //
        // Only after CheckpointService::notify_checkpoint stores checkpoint in it's store we update checkpoint boundary
        if let Some((index, roots)) = self.last_checkpoint(round)? {
            let final_checkpoint_round = self.final_epoch_checkpoint()?;
            let final_checkpoint = match final_checkpoint_round.map(|r| r.cmp(&round)) {
                Some(CmpOrdering::Less) => {
                    debug!(
                        "Not forming checkpoint for round {} above final checkpoint round {:?}",
                        round, final_checkpoint_round
                    );
                    return Ok(());
                }
                Some(CmpOrdering::Equal) => true,
                Some(CmpOrdering::Greater) => false,
                None => false,
            };
            let checkpoint = PendingCheckpoint {
                roots,
                details: PendingCheckpointInfo {
                    timestamp_ms,
                    last_of_epoch: final_checkpoint,
                    commit_height: index,
                },
            };
            checkpoint_service.notify_checkpoint(self, checkpoint)?;
            if final_checkpoint {
                info!(epoch=?self.epoch(), "Received 2f+1 EndOfPublish messages, notifying last checkpoint");
                self.record_end_of_message_quorum_time_metric();
            }
        }
        self.record_checkpoint_boundary(round)
    }

    pub fn get_pending_checkpoints(
        &self,
        last: Option<CheckpointCommitHeight>,
    ) -> Vec<(CheckpointCommitHeight, PendingCheckpoint)> {
        let mut iter = self.tables.pending_checkpoints.iter();
        if let Some(last_processed_height) = last {
            iter = iter
                .skip_to(&(last_processed_height + 1))
                .expect("Unexpected storage error");
        }
        iter.collect()
    }

    pub fn get_pending_checkpoint(
        &self,
        index: &CheckpointCommitHeight,
    ) -> Result<Option<PendingCheckpoint>, TypedStoreError> {
        self.tables.pending_checkpoints.get(index)
    }

    pub fn insert_pending_checkpoint(
        &self,
        index: &CheckpointCommitHeight,
        checkpoint: &PendingCheckpoint,
    ) -> Result<(), TypedStoreError> {
        self.tables.pending_checkpoints.insert(index, checkpoint)
    }

    pub fn process_pending_checkpoint(
        &self,
        commit_height: CheckpointCommitHeight,
        content_info: Vec<(CheckpointSummary, CheckpointContents)>,
    ) -> Result<(), TypedStoreError> {
        // All created checkpoints are inserted in builder_checkpoint_summary in a single batch.
        // This means that upon restart we can use BuilderCheckpointSummary::commit_height
        // from the last built summary to resume building checkpoints.
        let mut batch = self.tables.pending_checkpoints.batch();
        for (position_in_commit, (summary, transactions)) in content_info.into_iter().enumerate() {
            let sequence_number = summary.sequence_number;
            let summary = BuilderCheckpointSummary {
                summary,
                commit_height: Some(commit_height),
                position_in_commit,
            };
            batch.insert_batch(
                &self.tables.builder_checkpoint_summary_v2,
                [(&sequence_number, summary)],
            )?;
            batch.insert_batch(
                &self.tables.builder_digest_to_checkpoint,
                transactions
                    .iter()
                    .map(|tx| (tx.transaction, sequence_number)),
            )?;
        }

        batch.write()
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
            self.tables
                .builder_digest_to_checkpoint
                .insert(&digest, &sequence)?;
        }
        let builder_summary = BuilderCheckpointSummary {
            summary: summary.clone(),
            commit_height: None,
            position_in_commit: 0,
        };
        self.tables
            .builder_checkpoint_summary_v2
            .insert(summary.sequence_number(), &builder_summary)?;
        Ok(())
    }

    pub fn last_built_checkpoint_commit_height(&self) -> Option<CheckpointCommitHeight> {
        self.tables
            .builder_checkpoint_summary_v2
            .iter()
            .skip_to_last()
            .next()
            .and_then(|(_, b)| b.commit_height)
    }

    pub fn last_built_checkpoint_summary(
        &self,
    ) -> SuiResult<Option<(CheckpointSequenceNumber, CheckpointSummary)>> {
        Ok(self
            .tables
            .builder_checkpoint_summary_v2
            .iter()
            .skip_to_last()
            .next()
            .map(|(seq, s)| (seq, s.summary)))
    }

    pub fn get_built_checkpoint_summary(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> SuiResult<Option<CheckpointSummary>> {
        Ok(self
            .tables
            .builder_checkpoint_summary_v2
            .get(&sequence)?
            .map(|s| s.summary))
    }

    pub fn builder_included_transaction_in_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> Result<bool, TypedStoreError> {
        self.tables
            .builder_digest_to_checkpoint
            .contains_key(digest)
    }

    pub fn get_pending_checkpoint_signatures_iter(
        &self,
        checkpoint_seq: CheckpointSequenceNumber,
        starting_index: u64,
    ) -> Result<
        impl Iterator<Item = ((CheckpointSequenceNumber, u64), CheckpointSignatureMessage)> + '_,
        TypedStoreError,
    > {
        let key = (checkpoint_seq, starting_index);
        debug!("Scanning pending checkpoint signatures from {:?}", key);
        self.tables
            .pending_checkpoint_signatures
            .iter()
            .skip_to(&key)
    }

    pub fn get_last_checkpoint_signature_index(&self) -> u64 {
        self.tables
            .pending_checkpoint_signatures
            .iter()
            .skip_to_last()
            .next()
            .map(|((_, index), _)| index)
            .unwrap_or_default()
    }

    pub fn insert_checkpoint_signature(
        &self,
        checkpoint_seq: CheckpointSequenceNumber,
        index: u64,
        info: &CheckpointSignatureMessage,
    ) -> Result<(), TypedStoreError> {
        self.tables
            .pending_checkpoint_signatures
            .insert(&(checkpoint_seq, index), info)
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
}

impl ExecutionComponents {
    fn new(
        protocol_config: &ProtocolConfig,
        store: Arc<AuthorityStore>,
        metrics: Arc<ResolverMetrics>,
        expensive_safety_check_config: &ExpensiveSafetyCheckConfig,
    ) -> Self {
        let native_functions = sui_move_natives::all_natives(/* silent */ true);
        let move_vm = Arc::new(
            adapter::new_move_vm(
                native_functions.clone(),
                protocol_config,
                expensive_safety_check_config.enable_move_vm_paranoid_checks(),
            )
            .expect("We defined natives to not fail here"),
        );
        let module_cache = Arc::new(SyncModuleCache::new(ResolverWrapper::new(
            store,
            metrics.clone(),
        )));
        Self {
            native_functions,
            move_vm,
            module_cache,
            metrics,
        }
    }

    pub(crate) fn metrics(&self) -> Arc<ResolverMetrics> {
        self.metrics.clone()
    }
}
