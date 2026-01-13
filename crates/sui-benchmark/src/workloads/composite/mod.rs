// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod operations;

pub use operations::{
    ALL_OPERATIONS, AddressBalanceDeposit, AddressBalanceWithdraw, ObjectBalanceDeposit,
    ObjectBalanceWithdraw, OperationDescriptor, RandomnessRead, SharedCounterIncrement,
    SharedCounterRead, TestCoinAddressDeposit, TestCoinAddressWithdraw, TestCoinMint,
    TestCoinObjectWithdraw, describe_flags,
};

use crate::drivers::Interval;
use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::Payload;
use crate::workloads::workload::{
    ESTIMATED_COMPUTATION_COST, MAX_GAS_FOR_TESTING, STORAGE_COST_PER_COUNTER, Workload,
    WorkloadBuilder,
};
use crate::workloads::{Gas, GasCoinConfig, WorkloadBuilderInfo, WorkloadParams};
use crate::{ExecutionEffects, ValidatorProxy};
use async_trait::async_trait;
use futures::future::join_all;
use operations::{InitRequirement, Operation, OperationResources, ResourceRequest};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng, thread_rng};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::SUI_RANDOMNESS_STATE_OBJECT_ID;
use sui_types::TypeTag;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::crypto::{AccountKeyPair, get_key_pair};
use sui_types::gas_coin::GAS;
use sui_types::object::Owner;
use sui_types::transaction::{Argument, Command, ObjectArg, SharedObjectMutability, Transaction};
use sui_types::{Identifier, SUI_FRAMEWORK_PACKAGE_ID};
use tokio::sync::RwLock;
use tracing::{error, info};

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

#[derive(Debug, Default)]
pub struct OperationSetStats {
    pub success_count: u64,
    pub failure_count: u64,
    pub cancellation_count: u64,
}

#[derive(Debug, Default)]
pub struct CompositionMetrics {
    stats: std::collections::HashMap<u32, OperationSetStats>,
}

impl CompositionMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_success(&mut self, op_set: OperationSet) {
        self.stats.entry(op_set.raw()).or_default().success_count += 1;
    }

    pub fn record_failure(&mut self, op_set: OperationSet) {
        self.stats.entry(op_set.raw()).or_default().failure_count += 1;
    }

    pub fn record_cancellation(&mut self, op_set: OperationSet) {
        self.stats
            .entry(op_set.raw())
            .or_default()
            .cancellation_count += 1;
    }

    pub fn get_stats(&self, op_set: OperationSet) -> Option<&OperationSetStats> {
        self.stats.get(&op_set.raw())
    }

    pub fn cancellation_rate(&self, op_set: OperationSet) -> Option<f64> {
        self.stats.get(&op_set.raw()).map(|s| {
            let total = s.success_count + s.failure_count + s.cancellation_count;
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
            .map(|s| s.success_count + s.failure_count + s.cancellation_count)
            .unwrap_or(0)
    }

    pub fn total_transactions_all(&self) -> u64 {
        self.stats
            .values()
            .map(|s| s.success_count + s.failure_count + s.cancellation_count)
            .sum()
    }

    pub fn total_successes_all(&self) -> u64 {
        self.stats.values().map(|s| s.success_count).sum()
    }

    pub fn total_failures_all(&self) -> u64 {
        self.stats.values().map(|s| s.failure_count).sum()
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
        Self {
            probabilities,
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

    pub fn sample_operations(&self, rng: &mut impl Rng) -> Vec<Box<dyn Operation>> {
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

    pub fn collect_init_requirements(&self) -> std::collections::HashSet<InitRequirement> {
        let mut requirements: std::collections::HashSet<InitRequirement> = ALL_OPERATIONS
            .iter()
            .filter(|desc| self.probability_for(desc) > 0.0)
            .flat_map(|desc| (desc.factory)().init_requirements())
            .collect();

        if self.address_balance_gas_probability > 0.0 {
            requirements.insert(InitRequirement::SeedAddressBalance);
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
            address_balance_gas_probability: 0.0,
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
    pub fn select_counter(&self, rng: &mut impl Rng) -> (ObjectID, SequenceNumber) {
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
    pool: Arc<RwLock<OperationPool>>,
    gas: Gas,
    rng: SmallRng,
    system_state_observer: Arc<SystemStateObserver>,
    metrics: Arc<Mutex<CompositionMetrics>>,
    current_op_set: OperationSet,
    nonce_counter: AtomicU32,
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
        rng: &mut SmallRng,
    ) -> OperationResources {
        let mut counter = None;
        let mut randomness = None;
        let mut balance_pool = None;
        let mut test_coin_cap = None;

        for req in op.resource_requests() {
            match req {
                ResourceRequest::SharedCounter => {
                    counter = Some(pool.select_counter(rng));
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

    fn sample_operations(&mut self) -> Vec<Box<dyn Operation>> {
        self.config.sample_operations(&mut self.rng)
    }
}

impl Payload for CompositePayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        let mut metrics = self.metrics.lock().unwrap();
        if effects.is_cancelled() {
            metrics.record_cancellation(self.current_op_set);
        } else if effects.is_ok() {
            metrics.record_success(self.current_op_set);
        } else {
            metrics.record_failure(self.current_op_set);
            effects.print_gas_summary();
            error!("Composite tx failed... Status: {:?}", effects.status());
        }
        drop(metrics);
        self.gas.0 = effects.gas_object().0;
    }

    fn make_transaction(&mut self) -> Transaction {
        let system_state = self.system_state_observer.state.borrow().clone();
        let rgp = system_state.reference_gas_price;
        let current_epoch = system_state.epoch;

        let ops = self.sample_operations();

        self.current_op_set = OperationSet::new();
        for op in &ops {
            self.current_op_set = self.current_op_set.with(op.operation_flag());
        }

        let op_names: Vec<&str> = ops.iter().map(|op| op.name()).collect();
        tracing::trace!(
            "Building composite transaction with operations: {:?}",
            op_names
        );

        let pool = self.pool.blocking_read();

        let use_address_balance_gas = self
            .rng
            .gen_bool(self.config.address_balance_gas_probability as f64);

        let mut tx_builder = TestTransactionBuilder::new(self.gas.1, self.gas.0, rgp);
        {
            let builder = tx_builder.ptb_builder_mut();
            for op in &ops {
                let resources =
                    Self::resolve_resources_for_op(op.as_ref(), &pool, &self.config, &mut self.rng);
                op.apply(builder, &resources, &mut self.rng);
            }
        }

        if use_address_balance_gas {
            let nonce = self.nonce_counter.fetch_add(1, Ordering::Relaxed);
            tx_builder =
                tx_builder.with_address_balance_gas(pool.chain_identifier, current_epoch, nonce);
        }
        tx_builder.build_and_sign(self.gas.2.as_ref())
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
            payload_gas,
            metrics: self.metrics.clone(),
            chain_identifier: None,
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
    payload_gas: Vec<Gas>,
    metrics: Arc<Mutex<CompositionMetrics>>,
    chain_identifier: Option<sui_types::digests::ChainIdentifier>,
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
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) {
        if self.package_id.is_some() {
            return;
        }

        self.chain_identifier = Some(proxy.get_chain_identifier());
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
        let (_, execution_result) = proxy.execute_transaction_block(transaction).await;
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
            if let Ok(obj) = proxy.get_object(obj_ref.0).await {
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
            let proxy_ref = proxy.clone();
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

        let obj = proxy
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

        let init_requirements = self.config.collect_init_requirements();
        info!("Init requirements: {:?}", init_requirements);

        if init_requirements.contains(&InitRequirement::SeedAddressBalance) {
            let seed_amount = self.config.address_balance_amount * 100;
            info!(
                "Seeding address balances with {} MIST for {} addresses",
                seed_amount,
                self.payload_gas.len()
            );

            let mut futures = vec![];
            for (idx, (gas, sender, keypair)) in self.payload_gas.iter().enumerate() {
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

                let proxy_ref = proxy.clone();
                futures.push(async move {
                    let (_, execution_result) = proxy_ref.execute_transaction_block(tx).await;
                    let effects = execution_result.expect("Seed deposit should succeed");
                    (idx, effects.gas_object().0)
                });
            }

            let results = join_all(futures).await;
            for (idx, new_gas_ref) in results {
                self.payload_gas[idx].0 = new_gas_ref;
            }
            info!("Seeded {} address balances", self.payload_gas.len());
        }

        if init_requirements.contains(&InitRequirement::CreateBalancePool) {
            info!("Creating balance pool for object balance operations");
            let (gas, sender, keypair) = self.payload_gas.first().unwrap();
            let tx = TestTransactionBuilder::new(*sender, *gas, gas_price)
                .move_call(self.package_id.unwrap(), "balance_pool", "create", vec![])
                .build_and_sign(keypair.as_ref());

            let (_, execution_result) = proxy.execute_transaction_block(tx).await;
            let effects = execution_result.expect("Balance pool creation should succeed");

            self.payload_gas[0].0 = effects.gas_object().0;

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
            info!(
                "Seeding balance pool with {} MIST",
                seed_amount * self.payload_gas.len() as u64
            );

            let (gas, sender, keypair) = self.payload_gas.first().unwrap();
            let (pool_id, pool_version) = self.balance_pool.unwrap();
            let mut tx_builder = TestTransactionBuilder::new(*sender, *gas, gas_price);
            {
                let builder = tx_builder.ptb_builder_mut();
                let amount_arg = builder
                    .pure(seed_amount * self.payload_gas.len() as u64)
                    .unwrap();
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

            let (_, execution_result) = proxy.execute_transaction_block(tx).await;
            let effects = execution_result.expect("Balance pool seed should succeed");
            self.payload_gas[0].0 = effects.gas_object().0;
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

                let (_, execution_result) = proxy.execute_transaction_block(tx).await;
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

                for (idx, (gas, sender, keypair)) in self.payload_gas.clone().iter().enumerate() {
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

                    let (_, execution_result) = proxy.execute_transaction_block(tx).await;
                    let effects = execution_result.expect("TEST_COIN seed deposit should succeed");
                    self.payload_gas[idx].0 = effects.gas_object().0;
                }
                info!(
                    "Seeded {} TEST_COIN address balances",
                    self.payload_gas.len()
                );
            } else {
                info!("TestCoinCap not available - skipping TEST_COIN address balance seeding");
            }
        }
    }

    async fn make_test_payloads(
        &self,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
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

        let pool = Arc::new(RwLock::new(OperationPool {
            shared_counters: self.shared_counters.clone(),
            package_id: self.package_id.unwrap(),
            randomness_initial_shared_version: self.randomness_initial_shared_version.unwrap(),
            hotness: self.config.shared_counter_hotness,
            balance_pool: self.balance_pool,
            test_coin_cap: self.test_coin_cap,
            test_coin_type,
            chain_identifier: self.chain_identifier.unwrap(),
        }));

        let config = Arc::new(self.config.clone());
        let base_seed: u64 = thread_rng().r#gen();

        let mut payloads: Vec<Box<dyn Payload>> = vec![];
        for (i, gas) in self.payload_gas.iter().enumerate() {
            payloads.push(Box::new(CompositePayload {
                config: config.clone(),
                pool: pool.clone(),
                gas: gas.clone(),
                rng: SmallRng::seed_from_u64(base_seed.wrapping_add(i as u64)),
                system_state_observer: system_state_observer.clone(),
                metrics: self.metrics.clone(),
                current_op_set: OperationSet::new(),
                nonce_counter: AtomicU32::new(0),
            }));
        }

        info!("Created {} composite payloads", payloads.len());
        payloads
    }

    fn name(&self) -> &str {
        "Composite"
    }
}
