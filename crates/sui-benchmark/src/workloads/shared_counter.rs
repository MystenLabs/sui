// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::drivers::Interval;
use crate::system_state_observer::SystemStateObserver;
use crate::util::publish_basics_package;
use crate::workloads::payload::Payload;
use crate::workloads::workload::{
    ExpectedFailureType, Workload, WorkloadBuilder, ESTIMATED_COMPUTATION_COST,
    MAX_GAS_FOR_TESTING, STORAGE_COST_PER_COUNTER,
};
use crate::workloads::GasCoinConfig;
use crate::workloads::{Gas, WorkloadBuilderInfo, WorkloadParams};
use crate::{ExecutionEffects, ValidatorProxy};
use async_trait::async_trait;
use futures::future::join_all;
use rand::seq::SliceRandom;
use rand::Rng;
use std::sync::Arc;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::crypto::get_key_pair;
use sui_types::{
    base_types::{ObjectDigest, ObjectID, SequenceNumber},
    transaction::Transaction,
};
use tracing::{debug, error, info};

/// The max amount of gas units needed for a payload.
pub const MAX_GAS_IN_UNIT: u64 = 1_000_000_000;

#[derive(Debug)]
pub struct SharedCounterTestPayload {
    package_id: ObjectID,
    counter_id: ObjectID,
    counter_initial_shared_version: SequenceNumber,
    gas: Gas,
    max_tip_amount: u64,
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
            error!("Shared counter tx failed... Status: {:?}", effects.status());
        }
        self.gas.0 = effects.gas_object().0;
    }
    fn make_transaction(&mut self) -> Transaction {
        let rgp = self
            .system_state_observer
            .state
            .borrow()
            .reference_gas_price;
        let gas_price_increment = if self.max_tip_amount == 0 {
            0
        } else {
            rand::thread_rng().gen_range(0..self.max_tip_amount)
        };
        let gas_price = rgp + gas_price_increment;
        TestTransactionBuilder::new(self.gas.1, self.gas.0, gas_price)
            .call_counter_increment(
                self.package_id,
                self.counter_id,
                self.counter_initial_shared_version,
            )
            .build_and_sign(self.gas.2.as_ref())
    }
    fn get_failure_type(&self) -> Option<ExpectedFailureType> {
        None
    }
}

#[derive(Debug)]
pub struct SharedCounterWorkloadBuilder {
    num_counters: u64,
    num_payloads: u64,
    max_tip_amount: u64,
    rgp: u64,
}

impl SharedCounterWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        shared_counter_hotness_factor: u32,
        num_shared_counters: Option<u64>,
        shared_counter_max_tip_amount: u64,
        reference_gas_price: u64,
        duration: Interval,
        group: u32,
    ) -> Option<WorkloadBuilderInfo> {
        let target_qps = (workload_weight * target_qps as f32).ceil() as u64;
        let num_workers = (workload_weight * num_workers as f32).ceil() as u64;
        let max_ops = target_qps * in_flight_ratio;
        let shared_counter_ratio =
            1.0 - (std::cmp::min(shared_counter_hotness_factor, 100) as f32 / 100.0);
        let num_shared_counters = num_shared_counters.unwrap_or(std::cmp::max(
            1,
            (max_ops as f32 * shared_counter_ratio) as u64,
        ));
        if max_ops == 0 || num_shared_counters == 0 || num_workers == 0 {
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
                SharedCounterWorkloadBuilder {
                    num_counters: num_shared_counters,
                    num_payloads: max_ops,
                    max_tip_amount: shared_counter_max_tip_amount,
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
        let amount = MAX_GAS_IN_UNIT * (self.rgp + self.max_tip_amount)
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
            max_tip_amount: self.max_tip_amount,
        }))
    }
}

#[derive(Debug)]
pub struct SharedCounterWorkload {
    pub basics_package_id: Option<ObjectID>,
    pub counters: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    pub init_gas: Vec<Gas>,
    pub payload_gas: Vec<Gas>,
    pub max_tip_amount: u64,
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
        info!("Basics package id {:?}", self.basics_package_id);
        if !self.counters.is_empty() {
            // We already initialized the workload with some counters
            return;
        }
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
                max_tip_amount: self.max_tip_amount,
            }));
        }
        let payloads: Vec<Box<dyn Payload>> = shared_payloads
            .into_iter()
            .map(|b| Box::<dyn Payload>::from(b))
            .collect();
        payloads
    }
}
