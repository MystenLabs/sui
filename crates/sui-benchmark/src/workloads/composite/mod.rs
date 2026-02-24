// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod operations;

use derive_more::Add;
use mysten_common::random::get_rng;
use mysten_common::{assert_reachable, assert_sometimes, debug_fatal};
pub use operations::{
    ALIAS_ADD_FLAG, ALIAS_REMOVE_FLAG, ALIAS_TX_FLAG, ALL_OPERATIONS, AddressBalanceDeposit,
    AddressBalanceOverdraw, AddressBalanceWithdraw, INVALID_ALIAS_TX_FLAG, ObjectBalanceDeposit,
    ObjectBalanceWithdraw, OperationDescriptor, RandomnessRead, SharedCounterIncrement,
    SharedCounterRead, TestCoinAddressDeposit, TestCoinAddressWithdraw, TestCoinMint,
    TestCoinObjectWithdraw, describe_flags,
};
use rand::seq::SliceRandom;

use crate::drivers::Interval;
use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::{BatchExecutionResults, BatchedTransactionStatus, Payload};
use crate::workloads::workload::{
    ESTIMATED_COMPUTATION_COST, MAX_GAS_FOR_TESTING, STORAGE_COST_PER_COUNTER, Workload,
    WorkloadBuilder,
};
use crate::workloads::{Gas, GasCoinConfig, WorkloadBuilderInfo, WorkloadParams, gas_to_multi_gas};
use crate::{ExecutionEffects, ValidatorProxy};
use async_trait::async_trait;
use futures::future::join_all;
use operations::{InitRequirement, Operation, OperationResources, ResourceRequest};
use rand::Rng;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::TypeTag;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::crypto::{AccountKeyPair, get_key_pair};
use sui_types::digests::TransactionDigest;
use sui_types::gas_coin::GAS;
use sui_types::object::Owner;
use sui_types::transaction::{
    Argument, CallArg, Command, ObjectArg, SharedObjectMutability, Transaction,
};
use sui_types::{Identifier, SUI_FRAMEWORK_PACKAGE_ID};
use sui_types::{SUI_ADDRESS_ALIAS_STATE_OBJECT_ID, SUI_RANDOMNESS_STATE_OBJECT_ID};
use tracing::{debug, info, trace};

use super::MultiGas;

const MAX_BATCH_SIZE: usize = 4;

fn address_balance_disabled(protocol_config: Option<&ProtocolConfig>) -> bool {
    protocol_config
        .map(|cfg| !cfg.enable_address_balance_gas_payments())
        .unwrap_or(false)
}

fn address_alias_disabled(protocol_config: Option<&ProtocolConfig>) -> bool {
    protocol_config
        .map(|cfg| !cfg.address_aliases())
        .unwrap_or(false)
}

macro_rules! update_gas {
    ($gas:expr, $effects:expr) => {{
        let new_gas_ref = $effects.gas_object().0;
        if new_gas_ref.0 == ObjectID::ZERO {
            info!("No gas object, skipping update");
            return;
        }
        assert_eq!($gas.0, new_gas_ref.0, "ObjectIDs must match");
        info!(
            "Updating gas object from {:?} to {:?} for tx {:?}",
            $gas,
            new_gas_ref,
            $effects.digest()
        );
        *$gas = new_gas_ref;
    }};
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationSet(u32);

impl OperationSet {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn with(mut self, flag: u32) -> Self {
        self.0 |= flag;
        self
    }

    pub fn contains(&self, flag: u32) -> bool {
        (self.0 & flag) != 0
    }

    pub fn raw(&self) -> u32 {
        self.0
    }

    pub fn contains_shared_object_op(&self) -> bool {
        self.contains(SharedCounterIncrement::FLAG) || self.contains(SharedCounterRead::FLAG)
    }

    pub fn contains_contentious_shared_object_op(&self) -> bool {
        self.contains(SharedCounterIncrement::FLAG)
    }

    pub fn describe(&self) -> String {
        describe_flags(self.0)
    }
}

impl Default for OperationSet {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default, Add, Copy, Clone)]
pub struct OperationSetStats {
    pub signed_and_sent_count: u64,
    pub success_count: u64,
    pub abort_count: u64,
    pub permanent_failure_count: u64,
    pub retriable_failure_count: u64,
    pub unknown_rejection_count: u64,
    pub cancellation_count: u64,
    pub insufficient_funds_count: u64,
}

#[derive(Debug, Default)]
pub struct CompositionMetrics {
    stats: std::collections::HashMap<u32, OperationSetStats>,
}

impl CompositionMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sum_all(&self) -> OperationSetStats {
        let mut stats = OperationSetStats::default();
        for stat in self.stats.values() {
            stats = stats + *stat;
        }
        stats
    }

    pub fn record_signed_and_sent(&mut self, op_set: OperationSet) {
        self.stats
            .entry(op_set.raw())
            .or_default()
            .signed_and_sent_count += 1;
    }

    pub fn record_success(&mut self, op_set: OperationSet) {
        self.stats.entry(op_set.raw()).or_default().success_count += 1;
    }

    pub fn record_abort(&mut self, op_set: OperationSet) {
        self.stats.entry(op_set.raw()).or_default().abort_count += 1;
    }

    pub fn record_permanent_failure(&mut self, op_set: OperationSet) {
        self.stats
            .entry(op_set.raw())
            .or_default()
            .permanent_failure_count += 1;
    }

    pub fn record_retriable_failure(&mut self, op_set: OperationSet) {
        self.stats
            .entry(op_set.raw())
            .or_default()
            .retriable_failure_count += 1;
    }

    pub fn record_unknown_rejection(&mut self, op_set: OperationSet) {
        self.stats
            .entry(op_set.raw())
            .or_default()
            .unknown_rejection_count += 1;
    }

    pub fn record_cancellation(&mut self, op_set: OperationSet) {
        self.stats
            .entry(op_set.raw())
            .or_default()
            .cancellation_count += 1;
    }

    pub fn record_insufficient_funds(&mut self, op_set: OperationSet) {
        self.stats
            .entry(op_set.raw())
            .or_default()
            .insufficient_funds_count += 1;
    }

    pub fn get_stats(&self, op_set: OperationSet) -> Option<&OperationSetStats> {
        self.stats.get(&op_set.raw())
    }

    pub fn cancellation_rate(&self, op_set: OperationSet) -> Option<f64> {
        self.stats.get(&op_set.raw()).map(|s| {
            let total = s.success_count + s.abort_count + s.cancellation_count;
            if total == 0 {
                0.0
            } else {
                s.cancellation_count as f64 / total as f64
            }
        })
    }

    pub fn total_transactions(&self, op_set: OperationSet) -> u64 {
        self.stats
            .get(&op_set.raw())
            .map(|s| s.success_count + s.abort_count + s.cancellation_count)
            .unwrap_or(0)
    }

    pub fn total_transactions_all(&self) -> u64 {
        self.stats
            .values()
            .map(|s| s.success_count + s.abort_count + s.cancellation_count)
            .sum()
    }

    pub fn total_successes_all(&self) -> u64 {
        self.stats.values().map(|s| s.success_count).sum()
    }

    pub fn total_failures_all(&self) -> u64 {
        self.stats.values().map(|s| s.abort_count).sum()
    }

    pub fn total_cancellations_all(&self) -> u64 {
        self.stats.values().map(|s| s.cancellation_count).sum()
    }

    pub fn overall_cancellation_rate(&self) -> f64 {
        let total = self.total_transactions_all();
        if total == 0 {
            0.0
        } else {
            self.total_cancellations_all() as f64 / total as f64
        }
    }

    pub fn distinct_operation_sets_count(&self) -> usize {
        self.stats.len()
    }

    pub fn iter_stats(&self) -> impl Iterator<Item = (OperationSet, &OperationSetStats)> {
        self.stats
            .iter()
            .map(|(raw, stats)| (OperationSet(*raw), stats))
    }
}

const MAX_GAS_IN_UNIT: u64 = 1_000_000_000;

#[derive(Debug, Clone)]
pub struct CompositeWorkloadConfig {
    pub probabilities: HashMap<u32, f32>,
    pub num_shared_counters: u64,
    pub shared_counter_hotness: f32,
    pub address_balance_amount: u64,
    pub address_balance_gas_probability: f32,
    pub conflicting_transaction_probability: f32,
    pub alias_tx_probability: f32,
    pub alias_txs_before_revoke: u32,
    pub metrics: Option<Arc<Mutex<CompositionMetrics>>>,
}

impl CompositeWorkloadConfig {
    pub fn balanced() -> Self {
        let mut probabilities = HashMap::new();
        probabilities.insert(SharedCounterIncrement::FLAG, 0.3);
        probabilities.insert(SharedCounterRead::FLAG, 0.3);
        probabilities.insert(RandomnessRead::FLAG, 0.2);
        probabilities.insert(AddressBalanceDeposit::FLAG, 0.2);
        probabilities.insert(AddressBalanceWithdraw::FLAG, 0.2);
        probabilities.insert(ObjectBalanceDeposit::FLAG, 0.2);
        probabilities.insert(ObjectBalanceWithdraw::FLAG, 0.2);
        probabilities.insert(TestCoinMint::FLAG, 0.1);
        probabilities.insert(TestCoinAddressDeposit::FLAG, 0.1);
        probabilities.insert(TestCoinAddressWithdraw::FLAG, 0.1);
        probabilities.insert(TestCoinObjectWithdraw::FLAG, 0.1);
        probabilities.insert(AddressBalanceOverdraw::FLAG, 0.1);
        Self {
            probabilities,
            alias_tx_probability: 0.3,
            ..Default::default()
        }
    }

    pub fn with_probability(mut self, flag: u32, prob: f32) -> Self {
        self.probabilities.insert(flag, prob);
        self
    }

    pub fn probability_for(&self, desc: &OperationDescriptor) -> f32 {
        self.probabilities.get(&desc.flag).copied().unwrap_or(0.0)
    }

    pub fn sample_operations(&self) -> Vec<Box<dyn Operation>> {
        let mut rng = get_rng();
        let mut ops: Vec<Box<dyn Operation>> = ALL_OPERATIONS
            .iter()
            .filter(|desc| rng.gen_bool(self.probability_for(desc) as f64))
            .map(|desc| (desc.factory)())
            .collect();

        if ops.is_empty()
            && let Some(desc) = ALL_OPERATIONS.first()
        {
            ops.push((desc.factory)());
        }

        ops.sort_by_key(|op| op.constraints().must_be_last_shared_access as u8);
        ops
    }

    pub fn collect_init_requirements(
        &self,
        protocol_config: Option<&ProtocolConfig>,
    ) -> std::collections::HashSet<InitRequirement> {
        let mut requirements: std::collections::HashSet<InitRequirement> = ALL_OPERATIONS
            .iter()
            .filter(|desc| self.probability_for(desc) > 0.0)
            .flat_map(|desc| (desc.factory)().init_requirements())
            .collect();

        if self.address_balance_gas_probability > 0.0 {
            requirements.insert(InitRequirement::SeedAddressBalance);
        }

        if self.alias_tx_probability > 0.0 && !address_alias_disabled(protocol_config) {
            requirements.insert(InitRequirement::EnableAddressAlias);
        }

        requirements
    }
}

impl Default for CompositeWorkloadConfig {
    fn default() -> Self {
        Self {
            probabilities: HashMap::new(),
            num_shared_counters: 10,
            shared_counter_hotness: 0.5,
            address_balance_amount: 1000,
            address_balance_gas_probability: 0.5,
            conflicting_transaction_probability: 0.1,
            alias_tx_probability: 0.0,
            alias_txs_before_revoke: 3,
            metrics: None,
        }
    }
}

#[derive(Debug)]
pub struct OperationPool {
    pub shared_counters: Vec<(ObjectID, SequenceNumber)>,
    pub package_id: ObjectID,
    pub randomness_initial_shared_version: SequenceNumber,
    pub hotness: f32,
    pub balance_pool: Option<(ObjectID, SequenceNumber)>,
    pub test_coin_cap: Option<(ObjectID, SequenceNumber)>,
    pub test_coin_type: Option<TypeTag>,
    pub chain_identifier: sui_types::digests::ChainIdentifier,
}

impl OperationPool {
    pub fn select_counter(&self) -> (ObjectID, SequenceNumber) {
        let mut rng = get_rng();
        if self.shared_counters.is_empty() {
            panic!("No shared counters available");
        }
        if rng.gen_range(0.0..1.0) < self.hotness {
            self.shared_counters[0]
        } else {
            let idx = rng.gen_range(0..self.shared_counters.len());
            self.shared_counters[idx]
        }
    }
}

pub struct CompositePayload {
    config: Arc<CompositeWorkloadConfig>,
    fullnode_proxies: Vec<Arc<dyn ValidatorProxy + Sync + Send>>,
    pool: Arc<OperationPool>,
    gas: Mutex<MultiGas>,
    current_batch_num_conflicting_transactions: usize,
    current_batch_txs: Vec<BatchTxInfo>,
    system_state_observer: Arc<SystemStateObserver>,
    metrics: Arc<Mutex<CompositionMetrics>>,
    nonce_counter: AtomicU32,
    alias_state: Option<AliasState>,
}

/// Tracks the lifecycle of an alias revoke-and-re-add cycle for a single payload.
#[derive(Debug, Clone)]
enum AliasRevokeCycleState {
    /// The alias needs to be (re-)added. This is the beginning of the cycle.
    NeedAdd,
    /// An add-alias tx has been sent; waiting for its result and checkpoint confirmation.
    /// Phase 1 (`None`): tx sent, waiting for effects from `handle_batch_results`.
    /// Phase 2 (`Some(digest)`): effects received, polling for checkpoint inclusion.
    AddPending {
        tx_digest: Option<TransactionDigest>,
    },
    /// Alias is active; alias-signed txs are being injected. Once `successful_alias_txs`
    /// reaches the configured threshold, a remove-alias tx is sent.
    Active { successful_alias_txs: u32 },
    /// A remove-alias tx has been sent; waiting for its result and checkpoint confirmation.
    /// Phase 1 (`None`): tx sent, waiting for effects from `handle_batch_results`.
    /// Phase 2 (`Some(digest)`): effects received, polling for checkpoint inclusion.
    RemovePending {
        tx_digest: Option<TransactionDigest>,
    },
    /// Alias was successfully removed. Next batch will inject an invalid post-revocation tx.
    Revoked,
    /// An invalid post-revocation tx has been sent; waiting for its (expected) failure.
    InvalidPostRevocationTxPending,
}

/// Per-payload alias setup data produced during init:
/// (alias_address, alias_keypair, address_aliases_object_id, address_aliases_initial_shared_version).
type AliasInitInfo = (SuiAddress, Arc<AccountKeyPair>, ObjectID, SequenceNumber);

/// Per-payload alias state used at runtime to drive alias-signed transactions and
/// the revoke/re-add cycle.
#[derive(Debug)]
struct AliasState {
    alias_address: SuiAddress,
    alias_keypair: Arc<AccountKeyPair>,
    address_aliases_id: ObjectID,
    address_aliases_initial_shared_version: SequenceNumber,
    cycle_state: AliasRevokeCycleState,
}

#[derive(Clone, Copy)]
enum BatchTxKind {
    /// Standard composite PTB.
    Normal,
    /// Transaction signed by alias keypair.
    AliasSigned,
    /// Remove alias Move call.
    AliasRemove,
    /// Add alias Move call.
    AliasAdd,
    /// Alias-signed transfer after revocation (expected to fail).
    InvalidPostRevocation,
}

#[derive(Clone, Copy)]
struct BatchTxInfo {
    gas_idx: usize,
    op_set: OperationSet,
    kind: BatchTxKind,
}

impl std::fmt::Debug for CompositePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositePayload")
            .field("config", &self.config)
            .finish()
    }
}

impl std::fmt::Display for CompositePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "composite")
    }
}

impl CompositePayload {
    fn resolve_resources_for_op(
        op: &dyn Operation,
        pool: &OperationPool,
        config: &CompositeWorkloadConfig,
    ) -> OperationResources {
        let mut counter = None;
        let mut randomness = None;
        let mut balance_pool = None;
        let mut test_coin_cap = None;

        for req in op.resource_requests() {
            match req {
                ResourceRequest::SharedCounter => {
                    counter = Some(pool.select_counter());
                }
                ResourceRequest::Randomness => {
                    randomness = Some(pool.randomness_initial_shared_version);
                }
                ResourceRequest::AddressBalance => {}
                ResourceRequest::ObjectBalance => {
                    balance_pool = pool.balance_pool;
                }
                ResourceRequest::TestCoinCap => {
                    test_coin_cap = pool.test_coin_cap;
                }
            }
        }

        OperationResources {
            counter,
            randomness,
            package_id: pool.package_id,
            address_balance_amount: config.address_balance_amount,
            balance_pool,
            test_coin_cap,
            test_coin_type: pool.test_coin_type.clone(),
        }
    }

    fn sample_operations(&self) -> Vec<Box<dyn Operation>> {
        let protocol_config = self
            .system_state_observer
            .state
            .borrow()
            .protocol_config
            .clone();
        let filter_address_balance = address_balance_disabled(protocol_config.as_ref());

        loop {
            let mut ops = self.config.sample_operations();
            if filter_address_balance {
                ops.retain(|op| {
                    !op.resource_requests().iter().any(|r| {
                        matches!(
                            r,
                            ResourceRequest::AddressBalance | ResourceRequest::ObjectBalance
                        )
                    })
                });
            }
            if !ops.is_empty() {
                return ops;
            }
        }
    }

    fn generate_transaction(
        &mut self,
        mut tx_builder: TestTransactionBuilder,
        account_state: &AccountState,
        keypair: &AccountKeyPair,
    ) -> (Transaction, OperationSet) {
        let ops = self.sample_operations();

        let mut op_set = OperationSet::new();
        for op in &ops {
            op_set = op_set.with(op.operation_flag());
        }
        self.metrics.lock().unwrap().record_signed_and_sent(op_set);

        let op_names: Vec<&str> = ops.iter().map(|op| op.name()).collect();

        tracing::debug!(
            "Building composite transaction with operations: {:?}",
            op_names
        );

        {
            let builder = tx_builder.ptb_builder_mut();
            for op in &ops {
                let resources =
                    Self::resolve_resources_for_op(op.as_ref(), &self.pool, &self.config);
                op.apply(builder, &resources, account_state);
            }
        }

        (tx_builder.build_and_sign(keypair), op_set)
    }

    fn generate_alias_transaction(
        &mut self,
        sender: SuiAddress,
        gas: ObjectRef,
        gas_idx: usize,
        rgp: u64,
        keypair: &AccountKeyPair,
    ) -> (Transaction, BatchTxInfo) {
        let alias_state = self.alias_state.as_mut().unwrap();

        let (tx, op_set, kind) = match &alias_state.cycle_state {
            AliasRevokeCycleState::Active {
                successful_alias_txs,
            } if *successful_alias_txs >= self.config.alias_txs_before_revoke => {
                let tx = TestTransactionBuilder::new(sender, gas, rgp)
                    .move_call(
                        SUI_FRAMEWORK_PACKAGE_ID,
                        "address_alias",
                        "remove",
                        vec![
                            CallArg::Object(ObjectArg::SharedObject {
                                id: alias_state.address_aliases_id,
                                initial_shared_version: alias_state
                                    .address_aliases_initial_shared_version,
                                mutability: SharedObjectMutability::Mutable,
                            }),
                            CallArg::Pure(bcs::to_bytes(&alias_state.alias_address).unwrap()),
                        ],
                    )
                    .build_and_sign(keypair);
                alias_state.cycle_state = AliasRevokeCycleState::RemovePending { tx_digest: None };
                (
                    tx,
                    OperationSet::new().with(ALIAS_REMOVE_FLAG),
                    BatchTxKind::AliasRemove,
                )
            }
            AliasRevokeCycleState::Active { .. } => {
                let data = TestTransactionBuilder::new(sender, gas, rgp)
                    .transfer_sui(None, sender)
                    .build();
                let tx = Transaction::from_data_and_signer(
                    data,
                    vec![alias_state.alias_keypair.as_ref()],
                );
                (
                    tx,
                    OperationSet::new().with(ALIAS_TX_FLAG),
                    BatchTxKind::AliasSigned,
                )
            }
            AliasRevokeCycleState::Revoked => {
                let data = TestTransactionBuilder::new(sender, gas, rgp)
                    .transfer_sui(None, sender)
                    .build();
                let tx = Transaction::from_data_and_signer(
                    data,
                    vec![alias_state.alias_keypair.as_ref()],
                );
                alias_state.cycle_state = AliasRevokeCycleState::InvalidPostRevocationTxPending;
                (
                    tx,
                    OperationSet::new().with(INVALID_ALIAS_TX_FLAG),
                    BatchTxKind::InvalidPostRevocation,
                )
            }
            AliasRevokeCycleState::NeedAdd => {
                let tx = TestTransactionBuilder::new(sender, gas, rgp)
                    .move_call(
                        SUI_FRAMEWORK_PACKAGE_ID,
                        "address_alias",
                        "add",
                        vec![
                            CallArg::Object(ObjectArg::SharedObject {
                                id: alias_state.address_aliases_id,
                                initial_shared_version: alias_state
                                    .address_aliases_initial_shared_version,
                                mutability: SharedObjectMutability::Mutable,
                            }),
                            CallArg::Pure(bcs::to_bytes(&alias_state.alias_address).unwrap()),
                        ],
                    )
                    .build_and_sign(keypair);
                alias_state.cycle_state = AliasRevokeCycleState::AddPending { tx_digest: None };
                (
                    tx,
                    OperationSet::new().with(ALIAS_ADD_FLAG),
                    BatchTxKind::AliasAdd,
                )
            }
            _ => unreachable!("should not generate alias tx in pending state"),
        };

        self.metrics.lock().unwrap().record_signed_and_sent(op_set);
        (
            tx,
            BatchTxInfo {
                gas_idx,
                op_set,
                kind,
            },
        )
    }

    /// Polls for pending alias checkpoint confirmations and returns true if
    /// an alias tx should be generated this batch.
    async fn advance_alias_state(&mut self) -> bool {
        let Some(ref mut alias_state) = self.alias_state else {
            return false;
        };

        match &alias_state.cycle_state {
            AliasRevokeCycleState::AddPending {
                tx_digest: Some(digest),
            } => {
                let checkpointed = self.fullnode_proxies[0]
                    .is_transaction_checkpointed(digest)
                    .await
                    .unwrap_or(false);
                if checkpointed {
                    info!("Add alias tx {digest} checkpoint confirmed");
                    alias_state.cycle_state = AliasRevokeCycleState::Active {
                        successful_alias_txs: 0,
                    };
                }
            }
            AliasRevokeCycleState::RemovePending {
                tx_digest: Some(digest),
            } => {
                let checkpointed = self.fullnode_proxies[0]
                    .is_transaction_checkpointed(digest)
                    .await
                    .unwrap_or(false);
                if checkpointed {
                    info!("Remove alias tx {digest} checkpoint confirmed");
                    alias_state.cycle_state = AliasRevokeCycleState::Revoked;
                }
            }
            _ => {}
        }

        let mut rng = get_rng();
        match &alias_state.cycle_state {
            AliasRevokeCycleState::Active {
                successful_alias_txs,
            } if *successful_alias_txs >= self.config.alias_txs_before_revoke => true,
            AliasRevokeCycleState::Active { .. } => {
                rng.gen_bool(self.config.alias_tx_probability as f64)
            }
            AliasRevokeCycleState::Revoked | AliasRevokeCycleState::NeedAdd => true,
            _ => false,
        }
    }

    /// Updates alias cycle state based on a transaction result.
    /// Returns true if this was an expected alias failure (should not count toward
    /// the conflicting transaction failure assertion).
    fn handle_alias_tx_result(
        alias_state: &mut AliasState,
        kind: BatchTxKind,
        status: &BatchedTransactionStatus,
        alias_txs_before_revoke: u32,
        digest: TransactionDigest,
    ) -> bool {
        match (kind, status) {
            (BatchTxKind::InvalidPostRevocation, BatchedTransactionStatus::Success { .. }) => {
                debug_fatal!("Invalid post-revocation alias tx unexpectedly succeeded: {digest:?}");
                alias_state.cycle_state = AliasRevokeCycleState::NeedAdd;
                false
            }
            (
                BatchTxKind::InvalidPostRevocation,
                BatchedTransactionStatus::PermanentFailure { .. }
                | BatchedTransactionStatus::UnknownRejection,
            ) => {
                info!("Invalid post-revocation alias tx correctly rejected: {digest:?}");
                alias_state.cycle_state = AliasRevokeCycleState::NeedAdd;
                true
            }
            (
                BatchTxKind::InvalidPostRevocation,
                BatchedTransactionStatus::RetriableFailure { .. },
            ) => {
                info!(
                    "Invalid post-revocation alias tx had retriable failure, will retry: {digest:?}",
                );
                alias_state.cycle_state = AliasRevokeCycleState::Revoked;
                false
            }

            (BatchTxKind::AliasRemove, BatchedTransactionStatus::Success { effects }) => {
                if effects.is_ok() {
                    info!("Remove alias tx succeeded, waiting for checkpoint: {digest:?}");
                    alias_state.cycle_state = AliasRevokeCycleState::RemovePending {
                        tx_digest: Some(digest),
                    };
                } else {
                    info!("Remove alias tx aborted: {digest:?}");
                    alias_state.cycle_state = AliasRevokeCycleState::Active {
                        successful_alias_txs: alias_txs_before_revoke,
                    };
                }
                false
            }
            (BatchTxKind::AliasRemove, _) => {
                info!("Remove alias tx failed, retrying: {digest:?}");
                alias_state.cycle_state = AliasRevokeCycleState::Active {
                    successful_alias_txs: alias_txs_before_revoke,
                };
                false
            }

            (BatchTxKind::AliasAdd, BatchedTransactionStatus::Success { effects }) => {
                if effects.is_ok() {
                    info!("Add alias tx succeeded, waiting for checkpoint: {digest:?}");
                    alias_state.cycle_state = AliasRevokeCycleState::AddPending {
                        tx_digest: Some(digest),
                    };
                } else {
                    info!("Add alias tx aborted: {digest:?}");
                    alias_state.cycle_state = AliasRevokeCycleState::NeedAdd;
                }
                false
            }
            (BatchTxKind::AliasAdd, _) => {
                info!("Add alias tx failed, retrying: {digest:?}");
                alias_state.cycle_state = AliasRevokeCycleState::NeedAdd;
                false
            }

            (BatchTxKind::AliasSigned, BatchedTransactionStatus::Success { effects }) => {
                if effects.is_ok()
                    && let AliasRevokeCycleState::Active {
                        ref mut successful_alias_txs,
                    } = alias_state.cycle_state
                {
                    *successful_alias_txs += 1;
                }
                false
            }
            (BatchTxKind::AliasSigned, _) => false,

            (BatchTxKind::Normal, _) => unreachable!("Normal txs should not reach this function"),
        }
    }
}

#[async_trait]
impl Payload for CompositePayload {
    fn make_new_payload(&mut self, _: &ExecutionEffects) {
        unimplemented!();
    }

    fn make_transaction(&mut self) -> Transaction {
        unimplemented!()
    }

    fn is_batched(&self) -> bool {
        true
    }

    async fn make_transaction_batch(&mut self) -> Vec<Transaction> {
        let alias_tx_needed = self.advance_alias_state().await;
        let batch_size = {
            let mut rng = get_rng();
            let max_normal_batch = if alias_tx_needed {
                MAX_BATCH_SIZE - 1
            } else {
                MAX_BATCH_SIZE
            };
            rng.gen_range(1..=max_normal_batch)
        };

        let system_state = self.system_state_observer.state.borrow().clone();
        let rgp = system_state.reference_gas_price;
        let current_epoch = system_state.epoch;
        let address_balance_gas_disabled =
            address_balance_disabled(system_state.protocol_config.as_ref());

        let (current_batch_gas, sender, keypair) = {
            let gas = self.gas.lock().unwrap();
            assert!(
                gas.0.len() >= batch_size,
                "Not enough gas coins available in pool"
            );
            (gas.0.clone(), gas.1, gas.2.clone())
        };

        self.current_batch_txs.clear();
        self.current_batch_num_conflicting_transactions = 0;
        let mut transactions = Vec::with_capacity(batch_size + 1);

        let account_state = AccountState::new(sender, &self.fullnode_proxies).await;

        let mut used_gas = vec![];

        let mut rng = get_rng();

        for (i, gas) in current_batch_gas.iter().take(batch_size).enumerate() {
            let builder = if !address_balance_gas_disabled
                && rng.gen_bool(self.config.address_balance_gas_probability as f64)
            {
                let nonce = self.nonce_counter.fetch_add(1, Ordering::Relaxed);
                TestTransactionBuilder::new_with_address_balance_gas(
                    sender,
                    rgp,
                    self.pool.chain_identifier,
                    current_epoch,
                    nonce,
                )
            } else {
                used_gas.push(i);
                TestTransactionBuilder::new(sender, *gas, rgp)
            };

            let (tx, op_set) = self.generate_transaction(builder, &account_state, &keypair);
            self.current_batch_txs.push(BatchTxInfo {
                gas_idx: i,
                op_set,
                kind: BatchTxKind::Normal,
            });
            transactions.push(tx);
        }

        self.current_batch_num_conflicting_transactions = if rng
            .gen_bool(self.config.conflicting_transaction_probability as f64)
            && !used_gas.is_empty()
        {
            let num_conflicting_transactions = rng.gen_range(1..=used_gas.len());
            for gas_idx in used_gas.iter().take(num_conflicting_transactions) {
                let gas = current_batch_gas[*gas_idx];

                // use rgp + 1 to ensure we never make a duplicate transaction here
                let builder = TestTransactionBuilder::new(sender, gas, rgp + 1);
                let (tx, op_set) = self.generate_transaction(builder, &account_state, &keypair);
                self.current_batch_txs.push(BatchTxInfo {
                    gas_idx: *gas_idx,
                    op_set,
                    kind: BatchTxKind::Normal,
                });
                transactions.push(tx);
            }
            num_conflicting_transactions
        } else {
            0
        };

        if alias_tx_needed {
            let alias_gas_idx = batch_size;
            if alias_gas_idx < current_batch_gas.len() {
                let gas = current_batch_gas[alias_gas_idx];
                let (tx, info) = self.generate_alias_transaction(
                    sender,
                    gas,
                    alias_gas_idx,
                    rgp,
                    keypair.as_ref(),
                );
                self.current_batch_txs.push(info);
                transactions.push(tx);
            }
        }

        debug!(
            num_conflicting_transactions = self.current_batch_num_conflicting_transactions,
            "built batch: {:?}",
            transactions
                .iter()
                .map(|tx| tx.digest())
                .collect::<Vec<_>>(),
        );
        transactions
    }

    fn handle_batch_results(&mut self, results: &BatchExecutionResults) {
        debug!(
            "Handling batch results: {:?}",
            results.results.iter().map(|r| r.digest).collect::<Vec<_>>()
        );
        let mut metrics = self.metrics.lock().unwrap();

        let mut permanent_failure_count = 0;
        let mut expected_alias_failure_count = 0;

        let mut gas = self.gas.lock().unwrap();
        for (i, result) in results.results.iter().enumerate() {
            let tx_info = self.current_batch_txs[i];
            trace!("result: {}", result.description());
            assert!(
                tx_info.gas_idx < gas.0.len(),
                "result should correspond to a gas coin"
            );

            if !matches!(tx_info.kind, BatchTxKind::Normal)
                && let Some(ref mut alias_state) = self.alias_state
                && Self::handle_alias_tx_result(
                    alias_state,
                    tx_info.kind,
                    &result.status,
                    self.config.alias_txs_before_revoke,
                    result.digest,
                )
            {
                expected_alias_failure_count += 1;
            }

            match &result.status {
                BatchedTransactionStatus::Success { effects } => {
                    if effects.is_cancelled() {
                        metrics.record_cancellation(tx_info.op_set);
                    } else if effects.is_insufficient_funds() {
                        metrics.record_insufficient_funds(tx_info.op_set);
                    } else if effects.is_ok() {
                        metrics.record_success(tx_info.op_set);
                    } else {
                        metrics.record_abort(tx_info.op_set);
                    }
                    update_gas!(&mut gas.0[tx_info.gas_idx], effects);
                }
                BatchedTransactionStatus::PermanentFailure { error } => {
                    permanent_failure_count += 1;
                    metrics.record_permanent_failure(tx_info.op_set);
                    tracing::debug!(
                        "Transaction {} ({}) rejected with error: {:?}",
                        i,
                        result.digest,
                        error
                    );
                }
                BatchedTransactionStatus::RetriableFailure { error } => {
                    metrics.record_retriable_failure(tx_info.op_set);
                    tracing::debug!(
                        "Transaction {} ({}) had retriable failure: {:?}",
                        i,
                        result.digest,
                        error
                    );
                }
                BatchedTransactionStatus::UnknownRejection => {
                    metrics.record_unknown_rejection(tx_info.op_set);
                    tracing::debug!(
                        "Transaction {} ({}) had unknown rejection",
                        i,
                        result.digest,
                    );
                }
            }
        }
        assert_sometimes!(
            permanent_failure_count
                >= self.current_batch_num_conflicting_transactions + expected_alias_failure_count,
            "failure count should sometimes be greater than or equal to the number of conflicting transactions"
        );
        self.current_batch_txs.clear();
    }
}

pub struct AccountState {
    pub sender: SuiAddress,
    pub sui_balance: u64,
}

impl AccountState {
    pub async fn new(
        sender: SuiAddress,
        fullnode_proxies: &Vec<Arc<dyn ValidatorProxy + Sync + Send>>,
    ) -> Self {
        let mut retries = 0;
        while retries < 3 {
            let proxy = fullnode_proxies.choose(&mut get_rng()).unwrap();
            let Ok(sui_balance) = proxy.get_sui_address_balance(sender).await else {
                info!("Failed to get sui balance for address {sender}");
                retries += 1;
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            };
            assert_reachable!("successfully got sui balance for address");
            return Self {
                sender,
                sui_balance,
            };
        }
        Self {
            sender,
            sui_balance: 0,
        }
    }
}

pub struct CompositeWorkloadBuilder {
    config: CompositeWorkloadConfig,
    num_payloads: u64,
    rgp: u64,
    metrics: Arc<Mutex<CompositionMetrics>>,
}

impl std::fmt::Debug for CompositeWorkloadBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeWorkloadBuilder")
            .field("config", &self.config)
            .field("num_payloads", &self.num_payloads)
            .field("rgp", &self.rgp)
            .finish()
    }
}

impl CompositeWorkloadBuilder {
    pub fn from(
        weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        config: CompositeWorkloadConfig,
        reference_gas_price: u64,
        duration: Interval,
        group: u32,
    ) -> Option<WorkloadBuilderInfo> {
        let target_qps = (target_qps as f32 * weight) as u64;
        if target_qps == 0 || num_workers == 0 {
            return None;
        }
        Self::new_with_config(
            config,
            target_qps,
            num_workers,
            in_flight_ratio,
            reference_gas_price,
            duration,
            group,
        )
    }

    pub fn new_with_config(
        config: CompositeWorkloadConfig,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        reference_gas_price: u64,
        duration: Interval,
        group: u32,
    ) -> Option<WorkloadBuilderInfo> {
        let max_ops = target_qps * in_flight_ratio;
        if max_ops == 0 || num_workers == 0 {
            return None;
        }

        let workload_params = WorkloadParams {
            group,
            target_qps,
            num_workers,
            max_ops,
            duration,
        };

        let metrics = config
            .metrics
            .clone()
            .unwrap_or_else(|| Arc::new(Mutex::new(CompositionMetrics::new())));

        let workload_builder =
            Box::<dyn WorkloadBuilder<dyn Payload>>::from(Box::new(CompositeWorkloadBuilder {
                config,
                num_payloads: max_ops,
                rgp: reference_gas_price,
                metrics,
            }));

        Some(WorkloadBuilderInfo {
            workload_params,
            workload_builder,
        })
    }
}

#[async_trait]
impl WorkloadBuilder<dyn Payload> for CompositeWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        let mut configs = vec![];

        let (address, keypair) = get_key_pair::<AccountKeyPair>();
        configs.push(GasCoinConfig {
            amount: MAX_GAS_FOR_TESTING,
            address,
            keypair: Arc::new(keypair),
        });

        for _ in 0..self.config.num_shared_counters {
            let (address, keypair) = get_key_pair::<AccountKeyPair>();
            configs.push(GasCoinConfig {
                amount: MAX_GAS_FOR_TESTING,
                address,
                keypair: Arc::new(keypair),
            });
        }

        configs
    }

    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig> {
        let mut configs = vec![];
        let amount = MAX_GAS_IN_UNIT * self.rgp
            + ESTIMATED_COMPUTATION_COST
            + STORAGE_COST_PER_COUNTER * self.config.num_shared_counters;

        for _ in 0..self.num_payloads {
            let (address, keypair) = get_key_pair::<AccountKeyPair>();
            configs.push(GasCoinConfig {
                amount,
                address,
                keypair: Arc::new(keypair),
            });
        }

        configs
    }

    async fn build(
        &self,
        init_gas: Vec<Gas>,
        payload_gas: Vec<Gas>,
    ) -> Box<dyn Workload<dyn Payload>> {
        Box::<dyn Workload<dyn Payload>>::from(Box::new(CompositeWorkload {
            config: self.config.clone(),
            package_id: None,
            shared_counters: vec![],
            randomness_initial_shared_version: None,
            balance_pool: None,
            test_coin_cap: None,
            init_gas,
            payload_gas: payload_gas.into_iter().map(gas_to_multi_gas).collect(),
            num_payloads: self.num_payloads,
            metrics: self.metrics.clone(),
            chain_identifier: None,
            alias_infos: vec![],
        }))
    }
}

#[derive(Debug)]
pub struct CompositeWorkload {
    config: CompositeWorkloadConfig,
    package_id: Option<ObjectID>,
    shared_counters: Vec<(ObjectID, SequenceNumber)>,
    randomness_initial_shared_version: Option<SequenceNumber>,
    balance_pool: Option<(ObjectID, SequenceNumber)>,
    test_coin_cap: Option<(ObjectID, SequenceNumber)>,
    init_gas: Vec<Gas>,
    payload_gas: Vec<MultiGas>,
    num_payloads: u64,
    metrics: Arc<Mutex<CompositionMetrics>>,
    chain_identifier: Option<sui_types::digests::ChainIdentifier>,
    alias_infos: Vec<Option<AliasInitInfo>>,
}

impl CompositeWorkload {
    pub fn metrics(&self) -> Arc<Mutex<CompositionMetrics>> {
        self.metrics.clone()
    }
}

#[async_trait]
impl Workload<dyn Payload> for CompositeWorkload {
    async fn init(
        &mut self,
        execution_proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        _fullnode_proxies: Vec<Arc<dyn ValidatorProxy + Sync + Send>>,
        system_state_observer: Arc<SystemStateObserver>,
    ) {
        if self.package_id.is_some() {
            return;
        }

        self.chain_identifier = Some(execution_proxy.get_chain_identifier());
        info!("Chain identifier: {:?}", self.chain_identifier);

        let gas_price = system_state_observer.state.borrow().reference_gas_price;
        let (head, tail) = self
            .init_gas
            .split_first()
            .expect("Not enough gas to initialize composite workload");

        info!("Publishing composite package for composite workload");
        let mut path = crate::workloads::benchmark_move_base_dir();
        path.push("src/workloads/data/composite");
        let transaction = TestTransactionBuilder::new(head.1, head.0, gas_price)
            .publish_async(path)
            .await
            .build_and_sign(head.2.as_ref());
        let (_, execution_result) = execution_proxy.execute_transaction_block(transaction).await;
        let effects = execution_result.expect("Package publish should succeed");

        let mut treasury_cap_ref = None;
        let mut owned_refs = vec![];
        for (obj_ref, owner) in effects.created() {
            match owner {
                Owner::Immutable => {
                    self.package_id = Some(obj_ref.0);
                    info!("Composite package id {:?}", self.package_id);
                }
                Owner::AddressOwner(_) => {
                    owned_refs.push(obj_ref);
                }
                _ => {}
            }
        }
        for obj_ref in owned_refs {
            if let Ok(obj) = execution_proxy.get_object(obj_ref.0).await {
                let obj_type = obj.type_().map(|t| t.to_string()).unwrap_or_default();
                if obj_type.contains("TreasuryCap") {
                    treasury_cap_ref = Some(obj.compute_object_reference());
                    info!("Found TreasuryCap {:?}", treasury_cap_ref);
                    break;
                }
            }
        }
        let updated_gas = effects.gas_object().0;

        let mut futures = vec![];
        for (gas, sender, keypair) in tail.iter() {
            let transaction = TestTransactionBuilder::new(*sender, *gas, gas_price)
                .call_counter_create(self.package_id.unwrap())
                .build_and_sign(keypair.as_ref());
            let proxy_ref = execution_proxy.clone();
            futures.push(async move {
                let (_, execution_result) = proxy_ref.execute_transaction_block(transaction).await;
                let (obj_ref, owner) = execution_result.unwrap().created()[0].clone();
                let initial_shared_version = match owner {
                    Owner::Shared {
                        initial_shared_version,
                    } => initial_shared_version,
                    _ => panic!("Counter should be shared"),
                };
                (obj_ref.0, initial_shared_version)
            });
        }
        self.shared_counters = join_all(futures).await;
        info!("Created {} shared counters", self.shared_counters.len());

        let obj = execution_proxy
            .get_object(SUI_RANDOMNESS_STATE_OBJECT_ID)
            .await
            .expect("Failed to get randomness object");
        let Owner::Shared {
            initial_shared_version,
        } = obj.owner()
        else {
            panic!("Randomness object must be shared");
        };
        self.randomness_initial_shared_version = Some(*initial_shared_version);
        info!(
            "Randomness initial shared version: {:?}",
            self.randomness_initial_shared_version
        );

        let protocol_config = system_state_observer.state.borrow().protocol_config.clone();
        let init_requirements = self
            .config
            .collect_init_requirements(protocol_config.as_ref());
        info!("Init requirements: {:?}", init_requirements);

        if init_requirements.contains(&InitRequirement::SeedAddressBalance) {
            let seed_amount = self.config.address_balance_amount * 100;
            info!(
                "Seeding address balances with {} MIST for {} addresses",
                seed_amount,
                self.payload_gas.len()
            );

            let mut futures = vec![];
            for (idx, (gas_coins, sender, keypair)) in self.payload_gas.iter().enumerate() {
                assert_eq!(gas_coins.len(), 1);
                let gas = gas_coins[0];

                let mut tx_builder = TestTransactionBuilder::new(*sender, gas, gas_price);
                {
                    let builder = tx_builder.ptb_builder_mut();
                    let amount_arg = builder.pure(seed_amount).unwrap();
                    let coin =
                        builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount_arg]));
                    let Argument::Result(coin_idx) = coin else {
                        panic!("SplitCoins should return Result");
                    };
                    let coin = Argument::NestedResult(coin_idx, 0);
                    let coin_balance = builder.programmable_move_call(
                        SUI_FRAMEWORK_PACKAGE_ID,
                        Identifier::new("coin").unwrap(),
                        Identifier::new("into_balance").unwrap(),
                        vec![GAS::type_tag()],
                        vec![coin],
                    );
                    let recipient_arg = builder.pure(*sender).unwrap();
                    builder.programmable_move_call(
                        SUI_FRAMEWORK_PACKAGE_ID,
                        Identifier::new("balance").unwrap(),
                        Identifier::new("send_funds").unwrap(),
                        vec![GAS::type_tag()],
                        vec![coin_balance, recipient_arg],
                    );
                }
                let tx = tx_builder.build_and_sign(keypair.as_ref());

                let proxy_ref = execution_proxy.clone();
                futures.push(async move {
                    let (_, execution_result) = proxy_ref.execute_transaction_block(tx).await;
                    let effects = execution_result.expect("Seed deposit should succeed");
                    (idx, effects)
                });
            }

            let results = join_all(futures).await;
            for (idx, effects) in results {
                update_gas!(&mut self.payload_gas[idx].0[0], effects);
            }
            info!("Seeded {} address balances", self.payload_gas.len());
        }

        if init_requirements.contains(&InitRequirement::CreateBalancePool) {
            info!("Creating balance pool for object balance operations");
            let (multi_gas, sender, keypair) = &mut self.payload_gas[0];
            let gas = &mut multi_gas[0];
            let tx = TestTransactionBuilder::new(*sender, *gas, gas_price)
                .move_call(self.package_id.unwrap(), "balance_pool", "create", vec![])
                .build_and_sign(keypair.as_ref());

            let (_, execution_result) = execution_proxy.execute_transaction_block(tx).await;
            let effects = execution_result.expect("Balance pool creation should succeed");

            update_gas!(gas, effects);

            let (obj_ref, owner) = effects.created()[0].clone();
            let initial_shared_version = match owner {
                Owner::Shared {
                    initial_shared_version,
                } => initial_shared_version,
                _ => panic!("Balance pool should be shared"),
            };
            self.balance_pool = Some((obj_ref.0, initial_shared_version));
            info!("Created balance pool {:?}", self.balance_pool);
        }

        if init_requirements.contains(&InitRequirement::SeedBalancePool) {
            let seed_amount = self.config.address_balance_amount * 100;
            info!("Seeding balance pool with {} MIST", seed_amount);

            let (multi_gas, sender, keypair) = &mut self.payload_gas[0];
            let gas = &mut multi_gas[0];
            let (pool_id, pool_version) = self.balance_pool.unwrap();
            let mut tx_builder = TestTransactionBuilder::new(*sender, *gas, gas_price);
            {
                let builder = tx_builder.ptb_builder_mut();
                let amount_arg = builder.pure(seed_amount).unwrap();
                let coin =
                    builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount_arg]));
                let Argument::Result(coin_idx) = coin else {
                    panic!("SplitCoins should return Result");
                };
                let coin = Argument::NestedResult(coin_idx, 0);
                let coin_balance = builder.programmable_move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    Identifier::new("coin").unwrap(),
                    Identifier::new("into_balance").unwrap(),
                    vec![GAS::type_tag()],
                    vec![coin],
                );
                let pool_arg = builder
                    .obj(ObjectArg::SharedObject {
                        id: pool_id,
                        initial_shared_version: pool_version,
                        mutability: SharedObjectMutability::Immutable,
                    })
                    .unwrap();
                builder.programmable_move_call(
                    self.package_id.unwrap(),
                    Identifier::new("balance_pool").unwrap(),
                    Identifier::new("deposit").unwrap(),
                    vec![GAS::type_tag()],
                    vec![pool_arg, coin_balance],
                );
            }
            let tx = tx_builder.build_and_sign(keypair.as_ref());

            let (_, execution_result) = execution_proxy.execute_transaction_block(tx).await;
            let effects = execution_result.expect("Balance pool seed should succeed");
            update_gas!(gas, effects);
            info!("Seeded balance pool");
        }

        if init_requirements.contains(&InitRequirement::CreateTestCoinCap) {
            if let Some(cap_ref) = treasury_cap_ref {
                info!("Creating TestCoinCap for multi-currency operations");
                let (_, sender, keypair) = &self.init_gas[0];
                let tx = TestTransactionBuilder::new(*sender, updated_gas, gas_price)
                    .move_call(
                        self.package_id.unwrap(),
                        "test_coin",
                        "create_cap",
                        vec![cap_ref.into()],
                    )
                    .build_and_sign(keypair.as_ref());

                let (_, execution_result) = execution_proxy.execute_transaction_block(tx).await;
                let effects = execution_result.expect("TestCoinCap creation should succeed");

                self.init_gas[0].0 = effects.gas_object().0;

                let (obj_ref, owner) = effects.created()[0].clone();
                let initial_shared_version = match owner {
                    Owner::Shared {
                        initial_shared_version,
                    } => initial_shared_version,
                    _ => panic!("TestCoinCap should be shared"),
                };
                self.test_coin_cap = Some((obj_ref.0, initial_shared_version));
                info!("Created TestCoinCap {:?}", self.test_coin_cap);
            } else {
                info!("TreasuryCap not found in publish effects - test_coin operations disabled");
            }
        }

        if init_requirements.contains(&InitRequirement::SeedTestCoinAddressBalance) {
            if let Some((cap_id, cap_version)) = self.test_coin_cap {
                let seed_amount = self.config.address_balance_amount * 100;
                info!(
                    "Seeding TEST_COIN address balances with {} for {} addresses",
                    seed_amount,
                    self.payload_gas.len()
                );

                let test_coin_type =
                    TypeTag::Struct(Box::new(move_core_types::language_storage::StructTag {
                        address: self.package_id.unwrap().into(),
                        module: Identifier::new("test_coin").unwrap(),
                        name: Identifier::new("TEST_COIN").unwrap(),
                        type_params: vec![],
                    }));

                for (gas, sender, keypair) in self.payload_gas.iter_mut() {
                    let gas = &mut gas[0];
                    let mut tx_builder = TestTransactionBuilder::new(*sender, *gas, gas_price);
                    {
                        let builder = tx_builder.ptb_builder_mut();
                        let cap_arg = builder
                            .obj(ObjectArg::SharedObject {
                                id: cap_id,
                                initial_shared_version: cap_version,
                                mutability: SharedObjectMutability::Mutable,
                            })
                            .unwrap();
                        let amount_arg = builder.pure(seed_amount).unwrap();
                        let balance = builder.programmable_move_call(
                            self.package_id.unwrap(),
                            Identifier::new("test_coin").unwrap(),
                            Identifier::new("mint_balance").unwrap(),
                            vec![],
                            vec![cap_arg, amount_arg],
                        );
                        let recipient_arg = builder.pure(*sender).unwrap();
                        builder.programmable_move_call(
                            SUI_FRAMEWORK_PACKAGE_ID,
                            Identifier::new("balance").unwrap(),
                            Identifier::new("send_funds").unwrap(),
                            vec![test_coin_type.clone()],
                            vec![balance, recipient_arg],
                        );
                    }
                    let tx = tx_builder.build_and_sign(keypair.as_ref());

                    let (_, execution_result) = execution_proxy.execute_transaction_block(tx).await;
                    let effects = execution_result.expect("TEST_COIN seed deposit should succeed");
                    update_gas!(gas, effects);
                }
                info!(
                    "Seeded {} TEST_COIN address balances",
                    self.payload_gas.len()
                );
            } else {
                info!("TestCoinCap not available - skipping TEST_COIN address balance seeding");
            }
        }

        if init_requirements.contains(&InitRequirement::EnableAddressAlias) {
            let alias_state_obj = execution_proxy
                .get_object(SUI_ADDRESS_ALIAS_STATE_OBJECT_ID)
                .await
                .expect("Failed to get AddressAliasState object");
            let Owner::Shared {
                initial_shared_version: alias_state_isv,
            } = alias_state_obj.owner()
            else {
                panic!("AddressAliasState must be shared");
            };
            let alias_state_isv = *alias_state_isv;
            info!(
                "AddressAliasState initial shared version: {:?}",
                alias_state_isv
            );

            // For each payload, call address_alias::enable to create the AddressAliases
            // object. The alias keypair is generated but not added yet. The cycle starts
            // in NeedAdd state and the first add happens at runtime.
            for (gas_coins, sender, keypair) in self.payload_gas.iter_mut() {
                let gas = &mut gas_coins[0];

                let (alias_address, alias_kp): (_, AccountKeyPair) = get_key_pair();
                let alias_kp = Arc::new(alias_kp);

                let enable_tx = TestTransactionBuilder::new(*sender, *gas, gas_price)
                    .move_call(
                        SUI_FRAMEWORK_PACKAGE_ID,
                        "address_alias",
                        "enable",
                        vec![CallArg::Object(ObjectArg::SharedObject {
                            id: SUI_ADDRESS_ALIAS_STATE_OBJECT_ID,
                            initial_shared_version: alias_state_isv,
                            mutability: SharedObjectMutability::Mutable,
                        })],
                    )
                    .build_and_sign(keypair.as_ref());

                let (_, execution_result) =
                    execution_proxy.execute_transaction_block(enable_tx).await;
                let effects = execution_result.expect("Address alias enable should succeed");
                update_gas!(gas, effects);

                let (aliases_ref, aliases_isv) = effects
                    .created()
                    .iter()
                    .find_map(|(obj_ref, owner)| match owner {
                        Owner::ConsensusAddressOwner { start_version, .. } => {
                            Some((*obj_ref, *start_version))
                        }
                        _ => None,
                    })
                    .expect("AddressAliases object should be created");

                self.alias_infos
                    .push(Some((alias_address, alias_kp, aliases_ref.0, aliases_isv)));
                info!(
                    "Enabled address alias for sender {sender:?}, alias {alias_address:?}, aliases_id {:?}",
                    aliases_ref.0
                );
            }
            info!(
                "Initialized address aliases for {} payloads",
                self.alias_infos.len()
            );
        }

        // split remaining gas coins into 4 equal parts
        {
            let mut futures = vec![];
            for (idx, (gas_coins, sender, keypair)) in self.payload_gas.iter().enumerate() {
                assert_eq!(gas_coins.len(), 1);
                let gas = gas_coins[0];
                let gas_obj = execution_proxy
                    .get_object(gas.0)
                    .await
                    .expect("Gas object should exist");
                // take original gas coin, split the remaining balance into 4 equal parts
                let gas_balance = gas_obj.as_coin_maybe().unwrap().balance.value();
                let split_amount = gas_balance / MAX_BATCH_SIZE as u64;

                let mut tx_builder = TestTransactionBuilder::new(*sender, gas, gas_price);
                {
                    let builder = tx_builder.ptb_builder_mut();
                    let split_amount_arg = builder.pure(split_amount).unwrap();
                    let coin = builder.command(Command::SplitCoins(
                        Argument::GasCoin,
                        vec![split_amount_arg, split_amount_arg, split_amount_arg],
                    ));
                    let Argument::Result(coin_idx) = coin else {
                        panic!("SplitCoins should return Result");
                    };
                    let new_coin_args = (0..MAX_BATCH_SIZE - 1)
                        .map(|i| Argument::NestedResult(coin_idx, i as u16))
                        .collect();
                    builder.transfer_args(*sender, new_coin_args);
                }
                let tx = tx_builder.build_and_sign(keypair.as_ref());
                let proxy_ref = execution_proxy.clone();
                futures.push(async move {
                    let (_, execution_result) = proxy_ref.execute_transaction_block(tx).await;
                    let effects = execution_result.expect("Seed deposit should succeed");
                    (idx, effects)
                });
            }

            let results = join_all(futures).await;

            for (idx, effects) in results {
                let cur_gas = &mut self.payload_gas[idx];
                assert_eq!(cur_gas.0.len(), 1);
                update_gas!(&mut cur_gas.0[0], effects);
                for (new_coin_ref, new_coin_owner) in effects.created().into_iter() {
                    if let Owner::AddressOwner(new_owner_address) = new_coin_owner {
                        assert_eq!(new_owner_address, cur_gas.1);
                        cur_gas.0.push(new_coin_ref);
                    } else {
                        panic!("unexpected owner type: {:?}", new_coin_owner);
                    }
                }
            }
            for multi_gas in self.payload_gas.iter_mut() {
                assert_eq!(multi_gas.0.len(), MAX_BATCH_SIZE);
            }
            info!(
                "Split remaining gas coins into {} equal parts",
                MAX_BATCH_SIZE
            );
        }
    }

    async fn make_test_payloads(
        &self,
        _execution_proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        fullnode_proxies: Vec<Arc<dyn ValidatorProxy + Sync + Send>>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        info!("Creating composite workload payloads...");

        let test_coin_type = self.package_id.map(|pkg| {
            TypeTag::Struct(Box::new(move_core_types::language_storage::StructTag {
                address: pkg.into(),
                module: Identifier::new("test_coin").unwrap(),
                name: Identifier::new("TEST_COIN").unwrap(),
                type_params: vec![],
            }))
        });

        let operation_pool = Arc::new(OperationPool {
            shared_counters: self.shared_counters.clone(),
            package_id: self.package_id.unwrap(),
            randomness_initial_shared_version: self.randomness_initial_shared_version.unwrap(),
            hotness: self.config.shared_counter_hotness,
            balance_pool: self.balance_pool,
            test_coin_cap: self.test_coin_cap,
            test_coin_type,
            chain_identifier: self.chain_identifier.unwrap(),
        });

        let config = Arc::new(self.config.clone());

        if config.address_balance_gas_probability > 0.0 && config.address_balance_amount == 0 {
            panic!("Address balance gas probability is set to 0 but address balance amount is 0");
        }

        let mut payloads: Vec<Box<dyn Payload>> = vec![];
        for i in 0..self.num_payloads {
            let gas = self.payload_gas[i as usize].clone();
            let alias_state = self
                .alias_infos
                .get(i as usize)
                .and_then(|info| info.as_ref())
                .map(
                    |(alias_address, alias_keypair, aliases_id, aliases_isv)| AliasState {
                        alias_address: *alias_address,
                        alias_keypair: alias_keypair.clone(),
                        address_aliases_id: *aliases_id,
                        address_aliases_initial_shared_version: *aliases_isv,
                        cycle_state: AliasRevokeCycleState::NeedAdd,
                    },
                );
            payloads.push(Box::new(CompositePayload {
                config: config.clone(),
                fullnode_proxies: fullnode_proxies.clone(),
                pool: operation_pool.clone(),
                gas: Mutex::new(gas),
                current_batch_num_conflicting_transactions: 0,
                current_batch_txs: vec![],
                system_state_observer: system_state_observer.clone(),
                metrics: self.metrics.clone(),
                nonce_counter: AtomicU32::new(0),
                alias_state,
            }));
        }

        info!("Created {} composite payloads", payloads.len());
        payloads
    }

    fn name(&self) -> &str {
        "Composite"
    }
}
