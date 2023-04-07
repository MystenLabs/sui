// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::system_state_observer::SystemStateObserver;
use crate::util::publish_basics_package;
use crate::workloads::payload::Payload;
use crate::workloads::workload::{
    Workload, WorkloadBuilder, ESTIMATED_COMPUTATION_COST, MAX_GAS_FOR_TESTING,
    STORAGE_COST_PER_COUNTER,
};
use crate::workloads::GasCoinConfig;
use crate::workloads::{Gas, WorkloadBuilderInfo, WorkloadParams};
use crate::{ExecutionEffects, ValidatorProxy};
use async_trait::async_trait;
use futures::future::join_all;
use rand::seq::SliceRandom;
use std::sync::Arc;
use sui_types::crypto::get_key_pair;
use sui_types::{
    base_types::{ObjectDigest, ObjectID, SequenceNumber},
    messages::VerifiedTransaction,
};
use test_utils::messages::{make_counter_create_transaction, make_counter_increment_transaction};
use tracing::{debug, error, info};

#[derive(Debug)]
pub struct SharedCounterTestPayload {
    package_id: ObjectID,
    counter_id: ObjectID,
    counter_initial_shared_version: SequenceNumber,
    gas: Gas,
    system_state_observer: Arc<SystemStateObserver>,
}

impl std::fmt::Display for SharedCounterTestPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "shared_counter")
    }
}

impl Payload for SharedCounterTestPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        if !effects.is_ok() {
            effects.print_gas_summary();
            error!("Shared counter tx failed...");
        }
        self.gas.0 = effects.gas_object().0;
    }
    fn make_transaction(&mut self) -> VerifiedTransaction {
        make_counter_increment_transaction(
            self.gas.0,
            self.package_id,
            self.counter_id,
            self.counter_initial_shared_version,
            self.gas.1,
            &self.gas.2,
            Some(
                self.system_state_observer
                    .state
                    .borrow()
                    .reference_gas_price,
            ),
        )
    }
}

#[derive(Debug)]
pub struct SharedCounterWorkloadBuilder {
    num_counters: u64,
    num_payloads: u64,
}

impl SharedCounterWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        shared_counter_hotness_factor: u32,
    ) -> Option<WorkloadBuilderInfo> {
        let target_qps = (workload_weight * target_qps as f32) as u64;
        let num_workers = (workload_weight * num_workers as f32).ceil() as u64;
        let max_ops = target_qps * in_flight_ratio;
        let shared_counter_ratio =
            1.0 - (std::cmp::min(shared_counter_hotness_factor, 100) as f32 / 100.0);
        let num_shared_counters = (max_ops as f32 * shared_counter_ratio) as u64;
        if num_shared_counters == 0 || num_workers == 0 {
            None
        } else {
            let workload_params = WorkloadParams {
                target_qps,
                num_workers,
                max_ops,
            };
            let workload_builder = Box::<dyn WorkloadBuilder<dyn Payload>>::from(Box::new(
                SharedCounterWorkloadBuilder {
                    num_counters: num_shared_counters,
                    num_payloads: max_ops,
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
impl WorkloadBuilder<dyn Payload> for SharedCounterWorkloadBuilder {
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
        for _i in 0..self.num_counters {
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
        let amount = MAX_GAS_FOR_TESTING
            + ESTIMATED_COMPUTATION_COST
            + STORAGE_COST_PER_COUNTER * self.num_counters;
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
        Box::<dyn Workload<dyn Payload>>::from(Box::new(SharedCounterWorkload {
            basics_package_id: None,
            counters: vec![],
            init_gas,
            payload_gas,
        }))
    }
}

#[derive(Debug)]
pub struct SharedCounterWorkload {
    pub basics_package_id: Option<ObjectID>,
    pub counters: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    pub init_gas: Vec<Gas>,
    pub payload_gas: Vec<Gas>,
}

#[async_trait]
impl Workload<dyn Payload> for SharedCounterWorkload {
    async fn init(
        &mut self,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) {
        if self.basics_package_id.is_some() {
            return;
        }
        let gas_price = system_state_observer.state.borrow().reference_gas_price;
        let (head, tail) = self
            .init_gas
            .split_first()
            .expect("Not enough gas to initialize shared counter workload");

        // Publish basics package
        info!("Publishing basics package");
        self.basics_package_id = Some(
            publish_basics_package(head.0, proxy.clone(), head.1, &head.2, gas_price)
                .await
                .0,
        );
        if !self.counters.is_empty() {
            // We already initialized the workload with some counters
            return;
        }
        let mut futures = vec![];
        for (gas, sender, keypair) in tail.iter() {
            let transaction = make_counter_create_transaction(
                *gas,
                self.basics_package_id.unwrap(),
                *sender,
                keypair,
                Some(gas_price),
            );
            let proxy_ref = proxy.clone();
            futures.push(async move {
                if let Ok(effects) = proxy_ref
                    .execute_transaction_block(transaction.into())
                    .await
                {
                    effects.created()[0].0
                } else {
                    panic!("Failed to create shared counter!");
                }
            });
        }
        self.counters = join_all(futures).await;
    }
    async fn make_test_payloads(
        &self,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        // create counters using gas objects we created above
        info!("Creating shared txn payloads, hang tight..");
        let mut shared_payloads = vec![];
        debug!(
            "num of gas = {:?}, {:?}",
            self.payload_gas.len(),
            self.counters.len()
        );
        for g in self.payload_gas.iter() {
            // pick a random counter from the pool
            let counter_ref = self
                .counters
                .choose(&mut rand::thread_rng())
                .expect("Failed to get a random counter from the pool");
            shared_payloads.push(Box::new(SharedCounterTestPayload {
                package_id: self.basics_package_id.unwrap(),
                counter_id: counter_ref.0,
                counter_initial_shared_version: counter_ref.1,
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
