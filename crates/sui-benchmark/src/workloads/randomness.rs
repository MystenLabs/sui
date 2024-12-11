// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::drivers::Interval;
use crate::system_state_observer::SystemStateObserver;
use crate::util::publish_basics_package;
use crate::workloads::payload::Payload;
use crate::workloads::workload::{
    ExpectedFailureType, Workload, WorkloadBuilder, ESTIMATED_COMPUTATION_COST, MAX_GAS_FOR_TESTING,
};
use crate::workloads::GasCoinConfig;
use crate::workloads::{Gas, WorkloadBuilderInfo, WorkloadParams};
use crate::{ExecutionEffects, ValidatorProxy};
use async_trait::async_trait;
use std::sync::Arc;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::crypto::get_key_pair;
use sui_types::object::Owner;
use sui_types::SUI_RANDOMNESS_STATE_OBJECT_ID;
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    transaction::Transaction,
};
use tracing::{error, info};

/// The max amount of gas units needed for a payload.
pub const MAX_GAS_IN_UNIT: u64 = 1_000_000_000;

#[derive(Debug)]
pub struct RandomnessTestPayload {
    package_id: ObjectID,
    randomness_initial_shared_version: SequenceNumber,
    gas: Gas,
    system_state_observer: Arc<SystemStateObserver>,
}

impl std::fmt::Display for RandomnessTestPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "randomness")
    }
}

impl Payload for RandomnessTestPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        if !effects.is_ok() {
            effects.print_gas_summary();
            error!("Randomness tx failed... Status: {:?}", effects.status());
        }
        self.gas.0 = effects.gas_object().0;
    }
    fn make_transaction(&mut self) -> Transaction {
        let rgp = self
            .system_state_observer
            .state
            .borrow()
            .reference_gas_price;
        TestTransactionBuilder::new(self.gas.1, self.gas.0, rgp)
            .call_emit_random(self.package_id, self.randomness_initial_shared_version)
            .build_and_sign(self.gas.2.as_ref())
    }
    fn get_failure_type(&self) -> Option<ExpectedFailureType> {
        None
    }
}

#[derive(Debug)]
pub struct RandomnessWorkloadBuilder {
    num_payloads: u64,
    rgp: u64,
}

impl RandomnessWorkloadBuilder {
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
                RandomnessWorkloadBuilder {
                    num_payloads: max_ops,
                    rgp: reference_gas_price,
                },
            ));
            let builder_info = WorkloadBuilderInfo {
                workload_params,
                workload_builder,
            };
            Some(builder_info)
        }
    }
}

#[async_trait]
impl WorkloadBuilder<dyn Payload> for RandomnessWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        let (address, keypair) = get_key_pair();
        vec![GasCoinConfig {
            amount: MAX_GAS_FOR_TESTING,
            address,
            keypair: Arc::new(keypair),
        }]
    }
    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig> {
        let mut configs = vec![];
        let amount = MAX_GAS_IN_UNIT * self.rgp + ESTIMATED_COMPUTATION_COST;
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
        Box::<dyn Workload<dyn Payload>>::from(Box::new(RandomnessWorkload {
            basics_package_id: None,
            randomness_initial_shared_version: None,
            init_gas,
            payload_gas,
        }))
    }
}

#[derive(Debug)]
pub struct RandomnessWorkload {
    pub basics_package_id: Option<ObjectID>,
    pub randomness_initial_shared_version: Option<SequenceNumber>,
    pub init_gas: Vec<Gas>,
    pub payload_gas: Vec<Gas>,
}

#[async_trait]
impl Workload<dyn Payload> for RandomnessWorkload {
    async fn init(
        &mut self,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) {
        if self.basics_package_id.is_some() {
            return;
        }
        let gas_price = system_state_observer.state.borrow().reference_gas_price;
        let gas = self
            .init_gas
            .first()
            .expect("Not enough gas to initialize randomness workload");

        // Publish basics package
        if self.basics_package_id.is_none() {
            info!("Publishing basics package");
            self.basics_package_id = Some(
                publish_basics_package(gas.0, proxy.clone(), gas.1, &gas.2, gas_price)
                    .await
                    .0,
            );
            info!("Basics package id {:?}", self.basics_package_id);
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
    }
    async fn make_test_payloads(
        &self,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        let mut shared_payloads = vec![];
        for g in self.payload_gas.iter() {
            shared_payloads.push(Box::new(RandomnessTestPayload {
                package_id: self.basics_package_id.unwrap(),
                randomness_initial_shared_version: self.randomness_initial_shared_version.unwrap(),
                gas: g.clone(),
                system_state_observer: system_state_observer.clone(),
            }));
        }
        let payloads: Vec<Box<dyn Payload>> = shared_payloads
            .into_iter()
            .map(|b| Box::<dyn Payload>::from(b))
            .collect();
        payloads
    }
}
