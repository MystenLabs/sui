// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::drivers::Interval;
use crate::system_state_observer::SystemStateObserver;
use crate::util::publish_basics_package;
use crate::workloads::payload::Payload;
use crate::workloads::workload::{
    ExpectedFailureType, Workload, WorkloadBuilder, ESTIMATED_COMPUTATION_COST, MAX_GAS_FOR_TESTING,
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
use sui_types::crypto::{get_key_pair, AccountKeyPair};
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{CallArg, ObjectArg, Transaction};
use sui_types::{Identifier, SUI_RANDOMNESS_STATE_OBJECT_ID};
use tracing::{error, info};

use super::STORAGE_COST_PER_COUNTER;

pub const MAX_GAS_IN_UNIT: u64 = 1_000_000_000;

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
    randomness_initial_shared_version: SequenceNumber,
    transfer_to: SuiAddress,
    gas: Gas,
    system_state_observer: Arc<SystemStateObserver>,
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
    // Number of pure inputs
    num_pure_input: u64,
    // Number of shared inputs
    num_shared_inputs: u64,
    // Number of move calls
    num_move_calls: u64,
}

fn generate_random_transaction_config(
    num_shared_objects_exist: u64,
) -> RandomizedTransactionConfig {
    let num_shared_inputs = rand::thread_rng().gen_range(0..=num_shared_objects_exist);
    RandomizedTransactionConfig {
        contain_owned_object: rand::thread_rng().gen_bool(0.5),
        num_pure_input: rand::thread_rng().gen_range(0..=5),
        num_shared_inputs,
        num_move_calls: std::cmp::min(rand::thread_rng().gen_range(0..=3), num_shared_inputs),
    }
}

/// Type of move call to generate.
enum MoveCallType {
    ContractCall,
    Randomness,
    NativeCall,
}

/// Choose a random move call type.
fn choose_move_call_type(next_shared_input_index: usize, num_shared_inputs: u64) -> MoveCallType {
    if next_shared_input_index < num_shared_inputs as usize {
        match rand::thread_rng().gen_range(0..=2) {
            0 => MoveCallType::ContractCall,
            1 => MoveCallType::Randomness,
            _ => MoveCallType::NativeCall,
        }
    } else {
        match rand::thread_rng().gen_range(0..=1) {
            0 => MoveCallType::Randomness,
            _ => MoveCallType::NativeCall,
        }
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
                            mutable: true,
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
                                mutable: true,
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
                            mutable: false,
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
                    mutable: false,
                })],
            )
            .unwrap();
    }

    fn make_native_move_call(&mut self, builder: &mut ProgrammableTransactionBuilder) {
        builder
            .pay_sui(
                vec![self.transfer_to],
                vec![rand::thread_rng().gen_range(0..=1)],
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
        self.gas.0 = effects.gas_object().0;

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
    }

    fn make_transaction(&mut self) -> Transaction {
        let rgp = self
            .system_state_observer
            .state
            .borrow()
            .reference_gas_price;

        let config = generate_random_transaction_config(self.shared_objects.len() as u64);

        let mut builder = ProgrammableTransactionBuilder::new();

        // Generate inputs in addition to move calls.
        if config.contain_owned_object {
            builder
                .obj(ObjectArg::ImmOrOwnedObject(self.owned_object))
                .unwrap();
        }
        for i in 0..config.num_shared_inputs {
            builder
                .obj(ObjectArg::SharedObject {
                    id: self.shared_objects[i as usize].0,
                    initial_shared_version: self.shared_objects[i as usize].1,
                    mutable: rand::thread_rng().gen_bool(0.5),
                })
                .unwrap();
        }
        for _i in 0..config.num_pure_input {
            let len = rand::thread_rng().gen_range(0..=3);
            let mut bytes = vec![0u8; len];
            rand::thread_rng().fill(&mut bytes[..]);
            builder.pure_bytes(bytes, false);
        }

        // Generate move calls.
        let mut next_shared_input_index: usize = 0;
        for _i in 0..config.num_move_calls {
            match choose_move_call_type(next_shared_input_index, config.num_shared_inputs) {
                MoveCallType::ContractCall => {
                    self.make_counter_move_call(&mut builder, next_shared_input_index);
                    next_shared_input_index += 1;
                }
                MoveCallType::Randomness => {
                    self.make_randomness_move_call(&mut builder);
                    // TODO: add TransferObject move call after randomness command.
                    break;
                }
                MoveCallType::NativeCall => {
                    self.make_native_move_call(&mut builder);
                }
            }
        }
        let tx = builder.finish();

        tracing::info!("Randomized transaction: {:?}", tx);

        let signed_tx = TestTransactionBuilder::new(self.gas.1, self.gas.0, rgp)
            .programmable(tx)
            .build_and_sign(self.gas.2.as_ref());

        tracing::debug!("Signed transaction digest: {:?}", signed_tx.digest());
        signed_tx
    }

    fn get_failure_type(&self) -> Option<ExpectedFailureType> {
        // We do not expect randomized transaction to fail
        Some(ExpectedFailureType::NoFailure)
    }
}

#[derive(Debug)]
pub struct RandomizedTransactionWorkloadBuilder {
    num_payloads: u64,
    rgp: u64,
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
    ) -> Option<WorkloadBuilderInfo> {
        let target_qps = (workload_weight * target_qps as f32).ceil() as u64;
        let num_workers = (workload_weight * num_workers as f32).ceil() as u64;
        let max_ops = target_qps * in_flight_ratio;

        if max_ops == 0 || num_workers == 0 {
            None
        } else {
            let workload_params = WorkloadParams {
                group,
                target_qps,
                num_workers,
                max_ops,
                duration,
            };
            let workload_builder = Box::<dyn WorkloadBuilder<dyn Payload>>::from(Box::new(
                RandomizedTransactionWorkloadBuilder {
                    num_payloads: max_ops,
                    rgp: reference_gas_price,
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
        for _i in 0..self.num_payloads {
            let (address, keypair) = get_key_pair();
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
        let amount = MAX_GAS_IN_UNIT * (self.rgp)
            + ESTIMATED_COMPUTATION_COST
            + STORAGE_COST_PER_COUNTER * self.num_payloads
            + MAX_GAS_FOR_TESTING;
        // Gas coins for running workload
        for _i in 0..self.num_payloads {
            let (address, keypair) = get_key_pair();
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
        Box::<dyn Workload<dyn Payload>>::from(Box::new(RandomizedTransactionWorkload {
            basics_package_id: None,
            shared_objects: vec![],
            owned_objects: vec![],
            transfer_to: None,
            init_gas,
            payload_gas,
            randomness_initial_shared_version: None,
        }))
    }
}

#[derive(Debug)]
pub struct RandomizedTransactionWorkload {
    pub basics_package_id: Option<ObjectID>,
    pub shared_objects: Vec<ObjectRef>,
    pub owned_objects: Vec<ObjectRef>,
    pub transfer_to: Option<SuiAddress>,
    pub init_gas: Vec<Gas>,
    pub payload_gas: Vec<Gas>,
    pub randomness_initial_shared_version: Option<SequenceNumber>,
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
            for (gas, sender, keypair) in tail.iter() {
                let transaction = TestTransactionBuilder::new(*sender, *gas, gas_price)
                    .call_counter_create(self.basics_package_id.unwrap())
                    .build_and_sign(keypair.as_ref());
                let proxy_ref = proxy.clone();
                futures.push(async move {
                    proxy_ref
                        .execute_transaction_block(transaction)
                        .await
                        .unwrap()
                        .created()[0]
                        .0
                });
            }
            self.shared_objects = join_all(futures).await;
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
                    let execution_result = proxy_ref
                        .execute_transaction_block(transaction)
                        .await
                        .unwrap();
                    let created_owned = execution_result.created()[0].0;
                    let updated_gas = execution_result.gas_object().0;
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

        for (i, g) in self.payload_gas.iter().enumerate() {
            payloads.push(Box::new(RandomizedTransactionPayload {
                package_id: self.basics_package_id.unwrap(),
                shared_objects: self.shared_objects.clone(),
                owned_object: self.owned_objects[i],
                randomness_initial_shared_version: self.randomness_initial_shared_version.unwrap(),
                transfer_to: self.transfer_to.unwrap(),
                gas: g.clone(),
                system_state_observer: system_state_observer.clone(),
            }));
        }

        payloads
            .into_iter()
            .map(|b| Box::<dyn Payload>::from(b))
            .collect()
    }
}
