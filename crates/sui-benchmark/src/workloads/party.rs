// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    workload::{Workload, WorkloadBuilder, MAX_GAS_FOR_TESTING},
    WorkloadBuilderInfo, WorkloadParams,
};
use crate::drivers::Interval;
use crate::in_memory_wallet::InMemoryWallet;
use crate::system_state_observer::{SystemState, SystemStateObserver};
use crate::workloads::benchmark_move_base_dir;
use crate::workloads::payload::Payload;
use crate::workloads::{workload::ExpectedFailureType, Gas, GasCoinConfig};
use crate::ProgrammableTransactionBuilder;
use crate::{ExecutionEffects, ValidatorProxy};
use async_trait::async_trait;
use move_core_types::identifier::Identifier;
use rand::seq::IteratorRandom;
use std::sync::{Arc, Mutex};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{base_types::FullObjectRef, object::Owner};
use sui_types::{base_types::SuiAddress, crypto::get_key_pair, transaction::Transaction};
use sui_types::{
    base_types::{FullObjectID, ObjectID},
    transaction::ObjectArg,
};
use tracing::info;

#[derive(Debug)]
pub struct PartyTestPayload {
    /// ID of the Move package with party utility functions
    package_id: ObjectID,

    /// Current ID and owner of object under test.
    object_ref: FullObjectRef,
    sender: SuiAddress,

    state: Arc<Mutex<InMemoryWallet>>,
    system_state_observer: Arc<SystemStateObserver>,
}

impl std::fmt::Display for PartyTestPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "party")
    }
}

impl Payload for PartyTestPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        assert!(
            effects.is_ok(),
            "Party transactions should never abort: {effects:?}",
        );

        for (obj, owner) in effects.mutated().into_iter().chain(effects.created()) {
            if obj.0 != self.object_ref.0.id() {
                continue;
            }
            match owner {
                Owner::AddressOwner(addr) => {
                    self.object_ref = FullObjectRef::from_fastpath_ref(obj);
                    self.sender = addr;
                }
                Owner::ConsensusAddressOwner {
                    start_version,
                    owner,
                } => {
                    self.object_ref = FullObjectRef(
                        FullObjectID::Consensus((obj.0, start_version)),
                        obj.1,
                        obj.2,
                    );
                    self.sender = owner;
                }
                _ => unreachable!("party payload never transfers objects to other owners"),
            }
            break;
        }
        self.state.lock().unwrap().update(effects);
    }

    fn make_transaction(&mut self) -> Transaction {
        self.create_transaction()
    }

    fn get_failure_type(&self) -> Option<ExpectedFailureType> {
        None
    }
}

impl PartyTestPayload {
    fn create_transaction(&self) -> Transaction {
        let gas_price = self
            .system_state_observer
            .state
            .borrow()
            .reference_gas_price;

        // TODO: try sending to other recipients; this complicates gas management so is left out
        // of the current version.
        let next_recipient = self.sender;

        let mut builder = ProgrammableTransactionBuilder::new();
        let args = vec![
            builder
                .obj(match &self.object_ref.0 {
                    FullObjectID::Fastpath(_) => {
                        ObjectArg::ImmOrOwnedObject(self.object_ref.as_object_ref())
                    }
                    FullObjectID::Consensus((id, initial_shared_version)) => {
                        ObjectArg::SharedObject {
                            id: *id,
                            initial_shared_version: *initial_shared_version,
                            mutable: true,
                        }
                    }
                })
                .unwrap(),
            builder.pure(next_recipient).unwrap(),
        ];
        builder.programmable_move_call(
            self.package_id,
            Identifier::new("party").unwrap(),
            // Randomly transfer object to either ConsensusV2 or AddressOwner.
            [
                Identifier::new("transfer_party").unwrap(),
                Identifier::new("transfer_fastpath").unwrap(),
            ]
            .into_iter()
            .choose(&mut rand::thread_rng())
            .unwrap(),
            vec![],
            args,
        );

        let state = self.state.lock().unwrap();
        let account = state.account(&self.sender).unwrap();
        TestTransactionBuilder::new(self.sender, account.gas, gas_price)
            .programmable(builder.finish())
            .build_and_sign(account.key())
    }
}

#[derive(Debug)]
pub struct PartyWorkloadBuilder {
    num_payloads: u64,
}

#[async_trait]
impl WorkloadBuilder<dyn Payload> for PartyWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        // Gas coin for publishing package
        let (address, keypair) = get_key_pair();
        vec![GasCoinConfig {
            amount: MAX_GAS_FOR_TESTING,
            address,
            keypair: Arc::new(keypair),
        }]
    }

    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig> {
        let mut configs = vec![];
        // Gas coins for running workload
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

    async fn build(
        &self,
        init_gas: Vec<Gas>,
        payload_gas: Vec<Gas>,
    ) -> Box<dyn Workload<dyn Payload>> {
        Box::<dyn Workload<dyn Payload>>::from(Box::new(PartyWorkload {
            package_id: ObjectID::ZERO,
            init_gas,
            payload_gas,
        }))
    }
}

impl PartyWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
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
                target_qps,
                num_workers,
                max_ops,
                duration,
                group,
            };
            let workload_builder =
                Box::<dyn WorkloadBuilder<dyn Payload>>::from(Box::new(PartyWorkloadBuilder {
                    num_payloads: max_ops,
                }));
            let builder_info = WorkloadBuilderInfo {
                workload_params,
                workload_builder,
            };
            Some(builder_info)
        }
    }
}

#[derive(Debug)]
pub struct PartyWorkload {
    /// ID of the Move package with party functions
    package_id: ObjectID,

    pub init_gas: Vec<Gas>,
    pub payload_gas: Vec<Gas>,
}

#[async_trait]
impl Workload<dyn Payload> for PartyWorkload {
    async fn init(
        &mut self,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) {
        let (first_gas, _) = self
            .init_gas
            .split_first()
            .expect("Not enough gas to initialize party workload");

        let mut path = benchmark_move_base_dir();
        path.push("src/workloads/data/party");
        let SystemState {
            reference_gas_price,
            protocol_config: _,
        } = system_state_observer.state.borrow().clone();
        let transaction =
            TestTransactionBuilder::new(first_gas.1, first_gas.0, reference_gas_price)
                .publish(path)
                .build_and_sign(first_gas.2.as_ref());
        let (_, execution_result) = proxy.execute_transaction_block(transaction).await;
        let effects = execution_result.unwrap();
        assert!(effects.is_ok(), "Failed to publish party package");
        let created = effects.created();
        let package_obj = created
            .iter()
            .find(|o| matches!(o.1, Owner::Immutable))
            .unwrap();
        self.package_id = package_obj.0 .0;
        info!("Party package id {:?}", self.package_id);
    }

    async fn make_test_payloads(
        &self,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        let SystemState {
            reference_gas_price,
            protocol_config: _,
        } = system_state_observer.state.borrow().clone();

        let state = Arc::new(Mutex::new(InMemoryWallet::new_empty()));

        let mut futures = vec![];
        for (gas, sender, keypair) in &self.payload_gas[..self.payload_gas.len() / 2] {
            let transaction = TestTransactionBuilder::new(*sender, *gas, reference_gas_price)
                .move_call(self.package_id, "party", "create_party", vec![])
                .build_and_sign(keypair.as_ref());
            let state = state.clone();
            let system_state_observer = system_state_observer.clone();
            let proxy = proxy.clone();
            futures.push(async move {
                let (_, execution_result) = proxy.execute_transaction_block(transaction).await;
                let effects = execution_result.unwrap();
                let (
                    obj_ref,
                    Owner::ConsensusAddressOwner {
                        start_version,
                        owner,
                    },
                ) = &effects.created()[0]
                else {
                    panic!("create_party should always create a ConsensusAddressOwner object");
                };
                let (gas_object, _) = effects.gas_object();
                {
                    let mut state = state.lock().unwrap();
                    // TODO: track owned objects per account in state as well once InMemoryWallet supports
                    // ConsensusAddressOwner objects.
                    state.add_account(*sender, keypair.clone(), gas_object, vec![]);
                }
                let full_ref = FullObjectRef(
                    FullObjectID::Consensus((obj_ref.0, *start_version)),
                    obj_ref.1,
                    obj_ref.2,
                );
                let sender = *owner;
                PartyTestPayload {
                    package_id: self.package_id,
                    object_ref: full_ref,
                    sender,
                    state: state.clone(),
                    system_state_observer: system_state_observer.clone(),
                }
            });
        }
        let mut payloads = futures::future::join_all(futures).await;

        let mut futures = vec![];
        for (gas, sender, keypair) in &self.payload_gas[self.payload_gas.len() / 2..] {
            let transaction = TestTransactionBuilder::new(*sender, *gas, reference_gas_price)
                .move_call(self.package_id, "party", "create_fastpath", vec![])
                .build_and_sign(keypair.as_ref());
            let state = state.clone();
            let system_state_observer = system_state_observer.clone();
            let proxy = proxy.clone();
            futures.push(async move {
                let (_, execution_result) = proxy.execute_transaction_block(transaction).await;
                let effects = execution_result.unwrap();
                let (obj_ref, Owner::AddressOwner(owner)) = effects.created()[0] else {
                    panic!("create_fastpath should always create an AddressOwner object");
                };
                let (gas_object, _) = effects.gas_object();
                {
                    let mut state = state.lock().unwrap();
                    // TODO: track owned objects per account in state as well once InMemoryWallet supports
                    // ConsensusAddressOwner objects.
                    state.add_account(*sender, keypair.clone(), gas_object, vec![]);
                }
                let full_ref = FullObjectRef::from_fastpath_ref(obj_ref);
                PartyTestPayload {
                    package_id: self.package_id,
                    object_ref: full_ref,
                    sender: owner,
                    state: state.clone(),
                    system_state_observer: system_state_observer.clone(),
                }
            });
        }
        payloads.extend(futures::future::join_all(futures).await);

        payloads
            .into_iter()
            .map(|b| Box::<dyn Payload>::from(Box::new(b)))
            .collect()
    }
}
