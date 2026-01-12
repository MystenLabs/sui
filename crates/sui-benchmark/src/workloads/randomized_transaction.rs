// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::drivers::Interval;
use crate::system_state_observer::SystemStateObserver;
use crate::util::publish_basics_package;
use crate::workloads::payload::{ConcurrentTransactionResult, Payload};
use crate::workloads::workload::{
    ESTIMATED_COMPUTATION_COST, ExpectedFailureType, MAX_GAS_FOR_TESTING, Workload, WorkloadBuilder,
};
use crate::workloads::{Gas, GasCoinConfig, WorkloadBuilderInfo, WorkloadParams};
use crate::{ExecutionEffects, ValidatorProxy};
use async_trait::async_trait;
use futures::future::join_all;
use rand::Rng;
use std::sync::Arc;
use std::time::Duration;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::crypto::{AccountKeyPair, get_key_pair};
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{CallArg, ObjectArg, SharedObjectMutability, Transaction};
use sui_types::{Identifier, SUI_RANDOMNESS_STATE_OBJECT_ID};
use tracing::{error, info};

use super::STORAGE_COST_PER_COUNTER;

pub const MAX_GAS_IN_UNIT: u64 = 1_000_000_000;

const NUM_COUNTERS: usize = 8;

/// A workload that generates random transactions to test the system under different transaction patterns.
/// The workload can:
/// - Create shared counter objects
/// - Make random move calls to increment/read/delete counters
/// - Make calls to the randomness module
/// - Make native transfers
/// - Mix different numbers of pure and shared object inputs
/// - Generate multiple move calls per transaction
///
/// The exact mix of operations is controlled by random selection within configured bounds.
///
/// Different from adversarial workload, this workload is not designed to test the system under
/// malicious behaviors, but to test the system under different transaction patterns.
#[derive(Debug)]
pub struct RandomizedTransactionPayload {
    package_id: ObjectID,
    shared_objects: Vec<ObjectRef>,
    owned_object: ObjectRef,
    /// Immutable object (frozen) shared across all payloads
    immutable_object: ObjectRef,
    randomness_initial_shared_version: SequenceNumber,
    transfer_to: SuiAddress,
    gas_objects: Vec<Gas>,
    system_state_observer: Arc<SystemStateObserver>,
    /// Number of transactions to submit concurrently (1 = single transaction mode)
    concurrency: u64,
}

impl std::fmt::Display for RandomizedTransactionPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "randomized_transaction")
    }
}

/// Config for a randomized transaction
#[derive(Debug)]
struct RandomizedTransactionConfig {
    // Whether the transaction contains the owned object
    contain_owned_object: bool,
    // Whether the transaction contains the immutable object
    contain_immutable_object: bool,
    // Number of pure inputs
    num_pure_input: u64,
    // Number of shared inputs
    num_shared_inputs: u64,
    // Number of commands per PTB
    num_commands: u64,
}

fn generate_random_transaction_config(
    total_shared_objects: u64,
    concurrency: u64,
) -> RandomizedTransactionConfig {
    let num_shared_inputs = rand::thread_rng().gen_range(0..=total_shared_objects);
    RandomizedTransactionConfig {
        contain_owned_object: if concurrency <= 1 {
            rand::thread_rng().gen_bool(0.5)
        } else {
            rand::thread_rng().gen_bool(0.1)
        },
        contain_immutable_object: rand::thread_rng().gen_bool(0.5),
        num_pure_input: rand::thread_rng().gen_range(0..=5),
        num_shared_inputs,
        num_commands: std::cmp::min(rand::thread_rng().gen_range(0..=3), num_shared_inputs),
    }
}

/// Type of move call to generate.
enum MoveCallType {
    ContractCall,
    Randomness,
}

/// Choose a random move call type.
fn choose_move_call_type(next_shared_input_index: usize, num_shared_inputs: u64) -> MoveCallType {
    if next_shared_input_index < num_shared_inputs as usize {
        match rand::thread_rng().gen_range(0..=1) {
            0 => MoveCallType::ContractCall,
            _ => MoveCallType::Randomness,
        }
    } else {
        MoveCallType::Randomness
    }
}

impl RandomizedTransactionPayload {
    fn make_counter_move_call(
        &mut self,
        builder: &mut ProgrammableTransactionBuilder,
        next_shared_input_index: usize,
    ) {
        // 33% chance to increment, 33% chance to set value, 33% chance to read value.
        match rand::thread_rng().gen_range(0..=2) {
            0 => {
                builder
                    .move_call(
                        self.package_id,
                        Identifier::new("counter").unwrap(),
                        Identifier::new("increment").unwrap(),
                        vec![],
                        vec![CallArg::Object(ObjectArg::SharedObject {
                            id: self.shared_objects[next_shared_input_index].0,
                            initial_shared_version: self.shared_objects[next_shared_input_index].1,
                            mutability: SharedObjectMutability::Mutable,
                        })],
                    )
                    .unwrap();
            }
            1 => {
                builder
                    .move_call(
                        self.package_id,
                        Identifier::new("counter").unwrap(),
                        Identifier::new("set_value").unwrap(),
                        vec![],
                        vec![
                            CallArg::Object(ObjectArg::SharedObject {
                                id: self.shared_objects[next_shared_input_index].0,
                                initial_shared_version: self.shared_objects
                                    [next_shared_input_index]
                                    .1,
                                mutability: SharedObjectMutability::Mutable,
                            }),
                            CallArg::Pure((10_u64).to_le_bytes().to_vec()),
                        ],
                    )
                    .unwrap();
            }
            _ => {
                builder
                    .move_call(
                        self.package_id,
                        Identifier::new("counter").unwrap(),
                        Identifier::new("value").unwrap(),
                        vec![],
                        vec![CallArg::Object(ObjectArg::SharedObject {
                            id: self.shared_objects[next_shared_input_index].0,
                            initial_shared_version: self.shared_objects[next_shared_input_index].1,
                            mutability: SharedObjectMutability::Immutable,
                        })],
                    )
                    .unwrap();
            }
        }
    }

    fn make_randomness_move_call(&mut self, builder: &mut ProgrammableTransactionBuilder) {
        builder
            .move_call(
                self.package_id,
                Identifier::new("random").unwrap(),
                Identifier::new("new").unwrap(),
                vec![],
                vec![CallArg::Object(ObjectArg::SharedObject {
                    id: SUI_RANDOMNESS_STATE_OBJECT_ID,
                    initial_shared_version: self.randomness_initial_shared_version,
                    mutability: SharedObjectMutability::Immutable,
                })],
            )
            .unwrap();
    }

    fn make_transfer(&mut self, builder: &mut ProgrammableTransactionBuilder) {
        builder
            .pay_sui(
                vec![self.transfer_to],
                vec![rand::thread_rng().gen_range(0..=1)],
            )
            .unwrap();
    }

    fn make_immutable_object_call(&self, builder: &mut ProgrammableTransactionBuilder) {
        builder
            .move_call(
                self.package_id,
                Identifier::new("object_basics").unwrap(),
                Identifier::new("get_value").unwrap(),
                vec![],
                vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(
                    self.immutable_object,
                ))],
            )
            .unwrap();
    }
}

impl Payload for RandomizedTransactionPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        if !effects.is_ok() {
            effects.print_gas_summary();
            error!(
                "Randomized transaction failed... Status: {:?}",
                effects.status()
            );
        }
        self.gas_objects[0].0 = effects.gas_object().0;

        // Update owned object if it's mutated in this transaction
        if let Some(owned_in_effects) = effects
            .mutated()
            .iter()
            .find(|(object_ref, _)| object_ref.0 == self.owned_object.0)
            .map(|x| x.0)
        {
            tracing::debug!("Owned object mutated: {:?}", owned_in_effects);
            self.owned_object = owned_in_effects;
        }

        // Verify immutable object is NOT in changed objects
        let immutable_obj_id = self.immutable_object.0;
        assert!(
            !effects
                .mutated()
                .iter()
                .any(|(obj_ref, _)| obj_ref.0 == immutable_obj_id),
            "Immutable object should not be in mutated objects"
        );
        assert!(
            !effects
                .created()
                .iter()
                .any(|(obj_ref, _)| obj_ref.0 == immutable_obj_id),
            "Immutable object should not be in created objects"
        );
        assert!(
            !effects
                .deleted()
                .iter()
                .any(|obj_ref| obj_ref.0 == immutable_obj_id),
            "Immutable object should not be in deleted objects"
        );
    }

    fn make_transaction(&mut self) -> Transaction {
        unimplemented!("Randomized transaction should not be executed as a single transaction");
    }

    fn get_failure_type(&self) -> Option<ExpectedFailureType> {
        // We do not expect randomized transaction to fail
        Some(ExpectedFailureType::NoFailure)
    }

    fn is_concurrent_batch(&self) -> bool {
        true
    }

    fn make_concurrent_transactions(&mut self) -> Vec<Transaction> {
        let rgp = self
            .system_state_observer
            .state
            .borrow()
            .reference_gas_price;

        // Decide the shape of this batch of concurrent transactions.
        let config =
            generate_random_transaction_config(self.shared_objects.len() as u64, self.concurrency);

        let mut transactions = Vec::with_capacity(self.concurrency as usize);
        for i in 0..self.concurrency as usize {
            let mut tx_builder =
                TestTransactionBuilder::new(self.gas_objects[i].1, self.gas_objects[i].0, rgp);
            {
                let builder = tx_builder.ptb_builder_mut();

                // Include owned object based on random config.
                if config.contain_owned_object {
                    builder
                        .obj(ObjectArg::ImmOrOwnedObject(self.owned_object))
                        .unwrap();
                    self.make_transfer(builder);
                }

                // Add immutable object call to test immutable object handling.
                if config.contain_immutable_object {
                    self.make_immutable_object_call(builder);
                }

                // Add pure inputs
                for _j in 0..config.num_pure_input {
                    let len = rand::thread_rng().gen_range(0..=3);
                    let mut bytes = vec![0u8; len];
                    rand::thread_rng().fill(&mut bytes[..]);
                    builder.pure_bytes(bytes, false);
                }

                // Generate move calls
                let mut next_shared_input_index: usize = 0;
                for _j in 0..config.num_commands {
                    match choose_move_call_type(next_shared_input_index, config.num_shared_inputs) {
                        MoveCallType::ContractCall => {
                            self.make_counter_move_call(builder, next_shared_input_index);
                            next_shared_input_index += 1;
                        }
                        MoveCallType::Randomness => {
                            self.make_randomness_move_call(builder);
                            break;
                        }
                    }
                }
            }

            let signed_tx = tx_builder.build_and_sign(self.gas_objects[i].2.as_ref());
            transactions.push(signed_tx);
        }

        tracing::debug!(
            "Created {} concurrent transactions (config: {:?})",
            transactions.len(),
            config,
        );
        transactions
    }

    fn handle_concurrent_results(&mut self, results: &[ConcurrentTransactionResult]) {
        let mut success_count = 0;
        let mut lock_conflict_count = 0;

        for (i, result) in results.iter().enumerate() {
            match result {
                ConcurrentTransactionResult::Success { effects } => {
                    success_count += 1;
                    // Update gas object ref
                    self.gas_objects[i].0 = effects.gas_object().0;

                    // Update owned object if it was mutated by this transaction
                    if let Some(owned_in_effects) = effects
                        .mutated()
                        .iter()
                        .find(|(obj_ref, _)| obj_ref.0 == self.owned_object.0)
                        .map(|x| x.0)
                    {
                        self.owned_object = owned_in_effects;
                    }

                    // Verify immutable object is NOT in changed objects
                    let immutable_obj_id = self.immutable_object.0;
                    assert!(
                        !effects
                            .mutated()
                            .iter()
                            .any(|(obj_ref, _)| obj_ref.0 == immutable_obj_id),
                        "Immutable object should not be in mutated objects"
                    );
                }
                ConcurrentTransactionResult::Failure { error } => {
                    // Check if it's an ObjectLockConflict (expected when owned object is included)
                    if error.contains("ObjectLockConflict") {
                        lock_conflict_count += 1;
                        tracing::debug!(
                            "Transaction {} rejected with ObjectLockConflict (expected)",
                            i
                        );
                    } else {
                        tracing::debug!("Transaction {} failed with error: {:?}", i, error);
                    }
                }
            }
        }

        tracing::debug!(
            "Concurrent batch results: {} success, {} lock conflicts out of {} transactions",
            success_count,
            lock_conflict_count,
            results.len()
        );
    }
}

#[derive(Debug)]
pub struct RandomizedTransactionWorkloadBuilder {
    num_payloads: u64,
    rgp: u64,
    concurrency: u64,
}

impl RandomizedTransactionWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        reference_gas_price: u64,
        duration: Interval,
        group: u32,
        concurrency: u64,
    ) -> Option<WorkloadBuilderInfo> {
        let worker_target_qps =
            (workload_weight * target_qps as f32 / concurrency as f32).ceil() as u64;
        let num_workers = (workload_weight * num_workers as f32).ceil() as u64;
        let max_ops = worker_target_qps * in_flight_ratio;

        if max_ops == 0 || num_workers == 0 {
            None
        } else {
            let workload_params = WorkloadParams {
                group,
                target_qps: worker_target_qps,
                num_workers,
                max_ops,
                duration,
            };
            let workload_builder = Box::<dyn WorkloadBuilder<dyn Payload>>::from(Box::new(
                RandomizedTransactionWorkloadBuilder {
                    num_payloads: max_ops,
                    rgp: reference_gas_price,
                    concurrency,
                },
            ));
            Some(WorkloadBuilderInfo {
                workload_params,
                workload_builder,
            })
        }
    }
}

#[async_trait]
impl WorkloadBuilder<dyn Payload> for RandomizedTransactionWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        let mut configs = vec![];

        // Gas coin for publishing package
        let (address, keypair) = get_key_pair();
        configs.push(GasCoinConfig {
            amount: MAX_GAS_FOR_TESTING,
            address,
            keypair: Arc::new(keypair),
        });

        // Gas coins for creating counters
        for _i in 0..NUM_COUNTERS {
            let (address, keypair) = get_key_pair();
            configs.push(GasCoinConfig {
                amount: MAX_GAS_FOR_TESTING,
                address,
                keypair: Arc::new(keypair),
            });
        }

        // Gas coin for creating and freezing immutable object
        let (address, keypair) = get_key_pair();
        configs.push(GasCoinConfig {
            amount: MAX_GAS_FOR_TESTING,
            address,
            keypair: Arc::new(keypair),
        });

        configs
    }

    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig> {
        let mut configs = vec![];
        let amount = MAX_GAS_IN_UNIT * (self.rgp)
            + ESTIMATED_COMPUTATION_COST
            + STORAGE_COST_PER_COUNTER * self.num_payloads
            + MAX_GAS_FOR_TESTING;
        // Gas coins for running workload
        // Each payload needs `concurrency` gas objects for concurrent transactions
        for _i in 0..self.num_payloads {
            let (address, keypair): (SuiAddress, AccountKeyPair) = get_key_pair();
            let keypair = Arc::new(keypair);
            for _j in 0..self.concurrency {
                configs.push(GasCoinConfig {
                    amount,
                    address,
                    keypair: keypair.clone(),
                });
            }
        }
        configs
    }

    async fn build(
        &self,
        init_gas: Vec<Gas>,
        payload_gas: Vec<Gas>,
    ) -> Box<dyn Workload<dyn Payload>> {
        Box::<dyn Workload<dyn Payload>>::from(Box::new(RandomizedTransactionWorkload {
            basics_package_id: None,
            shared_objects: vec![],
            owned_objects: vec![],
            immutable_object: None,
            transfer_to: None,
            init_gas,
            payload_gas,
            randomness_initial_shared_version: None,
            concurrency: self.concurrency,
        }))
    }
}

#[derive(Debug)]
pub struct RandomizedTransactionWorkload {
    pub basics_package_id: Option<ObjectID>,
    pub shared_objects: Vec<ObjectRef>,
    pub owned_objects: Vec<ObjectRef>,
    pub immutable_object: Option<ObjectRef>,
    pub transfer_to: Option<SuiAddress>,
    pub init_gas: Vec<Gas>,
    pub payload_gas: Vec<Gas>,
    pub randomness_initial_shared_version: Option<SequenceNumber>,
    pub concurrency: u64,
}

#[async_trait]
impl Workload<dyn Payload> for RandomizedTransactionWorkload {
    async fn init(
        &mut self,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) {
        // We observed that randomness may need a few seconds until DKG completion, and this may
        // causing randomness transaction fail with `TooOldTransactionPendingOnObject` error.
        // Therefore, wait a few seconds at the beginning to give DKG some time.
        tokio::time::sleep(Duration::from_secs(5)).await;
        if self.basics_package_id.is_some() {
            return;
        }
        let gas_price = system_state_observer.state.borrow().reference_gas_price;

        let (head, tail) = self
            .init_gas
            .split_first()
            .expect("Not enough gas to initialize randomized transaction workload");

        // Split tail: first NUM_COUNTERS for counters, last one for immutable object
        let (counter_gas, immutable_gas) = tail.split_at(NUM_COUNTERS);
        let immutable_gas = immutable_gas
            .first()
            .expect("Not enough gas for immutable object");

        // Publish basics package
        info!("Publishing basics package");
        self.basics_package_id = Some(
            publish_basics_package(head.0, proxy.clone(), head.1, &head.2, gas_price)
                .await
                .0,
        );

        // Create a transfer address
        self.transfer_to = Some(get_key_pair::<AccountKeyPair>().0);

        // Create shared objects
        {
            let mut futures = vec![];
            for (gas, sender, keypair) in counter_gas.iter() {
                let transaction = TestTransactionBuilder::new(*sender, *gas, gas_price)
                    .call_counter_create(self.basics_package_id.unwrap())
                    .build_and_sign(keypair.as_ref());
                let proxy_ref = proxy.clone();
                futures.push(async move {
                    let (_, execution_result) =
                        proxy_ref.execute_transaction_block(transaction).await;
                    execution_result.unwrap().created()[0].0
                });
            }
            self.shared_objects = join_all(futures).await;
        }

        // Create and freeze immutable object
        {
            let (gas, sender, keypair) = immutable_gas;
            let mut current_gas = *gas;

            // Create object
            let transaction = TestTransactionBuilder::new(*sender, current_gas, gas_price)
                .move_call(
                    self.basics_package_id.unwrap(),
                    "object_basics",
                    "create",
                    vec![
                        CallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap()),
                        CallArg::Pure(bcs::to_bytes(&sender).unwrap()),
                    ],
                )
                .build_and_sign(keypair.as_ref());
            let (_, execution_result) = proxy.execute_transaction_block(transaction).await;
            let effects = execution_result.expect("Failed to create immutable object");
            let created_obj = effects.created()[0].0;
            current_gas = effects.gas_object().0;
            info!("Created object for freezing: {:?}", created_obj);

            // Freeze object
            let transaction = TestTransactionBuilder::new(*sender, current_gas, gas_price)
                .move_call(
                    self.basics_package_id.unwrap(),
                    "object_basics",
                    "freeze_object",
                    vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(created_obj))],
                )
                .build_and_sign(keypair.as_ref());
            let (_, execution_result) = proxy.execute_transaction_block(transaction).await;
            let effects = execution_result.expect("Failed to freeze object");

            // After freezing, the object becomes immutable - find it in mutated objects
            let frozen_obj = effects
                .mutated()
                .iter()
                .find(|(obj_ref, owner)| {
                    obj_ref.0 == created_obj.0 && matches!(owner, Owner::Immutable)
                })
                .map(|(obj_ref, _)| *obj_ref)
                .expect("Frozen object not found");

            info!("Frozen immutable object: {:?}", frozen_obj);
            self.immutable_object = Some(frozen_obj);
        }

        // create owned objects
        {
            let mut futures = vec![];
            for (gas, sender, keypair) in self.payload_gas.iter() {
                let transaction = TestTransactionBuilder::new(*sender, *gas, gas_price)
                    .move_call(
                        self.basics_package_id.unwrap(),
                        "object_basics",
                        "create",
                        vec![
                            CallArg::Pure(bcs::to_bytes(&(16_u64)).unwrap()),
                            CallArg::Pure(bcs::to_bytes(&sender).unwrap()),
                        ],
                    )
                    .build_and_sign(keypair.as_ref());
                let proxy_ref = proxy.clone();
                futures.push(async move {
                    let (_, execution_result) =
                        proxy_ref.execute_transaction_block(transaction).await;
                    let effects = execution_result.unwrap();

                    let created_owned = effects.created()[0].0;
                    let updated_gas = effects.gas_object().0;
                    (created_owned, updated_gas)
                });
            }
            let results = join_all(futures).await;
            self.owned_objects = results.iter().map(|x| x.0).collect();

            // Update gas object in payload gas
            for (payload_gas, result) in self.payload_gas.iter_mut().zip(results.iter()) {
                payload_gas.0 = result.1;
            }
        }

        // Get randomness shared object initial version
        if self.randomness_initial_shared_version.is_none() {
            let obj = proxy
                .get_object(SUI_RANDOMNESS_STATE_OBJECT_ID)
                .await
                .expect("Failed to get randomness object");
            let Owner::Shared {
                initial_shared_version,
            } = obj.owner()
            else {
                panic!("randomness object must be shared");
            };
            self.randomness_initial_shared_version = Some(*initial_shared_version);
        }

        info!(
            "Basics package id {:?}. Total shared objects created {:?}",
            self.basics_package_id,
            self.shared_objects.len()
        );
    }

    async fn make_test_payloads(
        &self,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        info!("Creating randomized transaction payloads...");
        let mut payloads = vec![];

        let immutable_object = self
            .immutable_object
            .expect("immutable_object not initialized");

        // Each payload gets `concurrency` gas objects
        let concurrency = self.concurrency as usize;
        for (i, gas_chunk) in self.payload_gas.chunks(concurrency).enumerate() {
            payloads.push(Box::new(RandomizedTransactionPayload {
                package_id: self.basics_package_id.unwrap(),
                shared_objects: self.shared_objects.clone(),
                owned_object: self.owned_objects[i],
                immutable_object,
                randomness_initial_shared_version: self.randomness_initial_shared_version.unwrap(),
                transfer_to: self.transfer_to.unwrap(),
                gas_objects: gas_chunk.to_vec(),
                system_state_observer: system_state_observer.clone(),
                concurrency: self.concurrency,
            }));
        }

        payloads
            .into_iter()
            .map(|b| Box::<dyn Payload>::from(b))
            .collect()
    }

    fn name(&self) -> &str {
        "RandomizedTransaction"
    }
}
