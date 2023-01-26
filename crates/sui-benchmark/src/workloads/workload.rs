// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use std::sync::Arc;
use std::{collections::HashMap, fmt};

use crate::system_state_observer::SystemStateObserver;
use crate::workloads::{WorkloadInitGas, WorkloadPayloadGas};
use rand::rngs::OsRng;
use rand_distr::WeightedAliasIndex;

use crate::workloads::payload::{CombinationPayload, Payload};
use crate::ValidatorProxy;

// This is the maximum gas we will transfer from primary coin into any gas coin
// for running the benchmark
pub const MAX_GAS_FOR_TESTING: u64 = 1_000_000_000;

#[derive(Copy, Clone, Hash, PartialEq, Eq)]
pub enum WorkloadType {
    SharedCounter,
    TransferObject,
    Combination,
    Delegation,
}

impl fmt::Display for WorkloadType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            WorkloadType::SharedCounter => write!(f, "shared_counter"),
            WorkloadType::TransferObject => write!(f, "transfer_object"),
            WorkloadType::Combination => write!(f, "combination"),
            WorkloadType::Delegation => write!(f, "delegation"),
        }
    }
}

#[async_trait]
pub trait Workload<T: Payload + ?Sized>: Send + Sync {
    async fn init(
        &mut self,
        init_config: WorkloadInitGas,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    );
    async fn make_test_payloads(
        &self,
        num_payloads: u64,
        payload_config: WorkloadPayloadGas,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<T>>;
    fn get_workload_type(&self) -> WorkloadType;
}

type WeightAndPayload = (u32, Box<dyn Workload<dyn Payload>>);
pub struct CombinationWorkload {
    workloads: HashMap<WorkloadType, WeightAndPayload>,
}

#[async_trait]
impl Workload<dyn Payload> for CombinationWorkload {
    async fn init(
        &mut self,
        init_config: WorkloadInitGas,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) {
        for (_, (_, workload)) in self.workloads.iter_mut() {
            workload
                .init(
                    init_config.clone(),
                    proxy.clone(),
                    system_state_observer.clone(),
                )
                .await;
        }
    }
    async fn make_test_payloads(
        &self,
        num_payloads: u64,
        payload_config: WorkloadPayloadGas,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        let mut workloads: HashMap<WorkloadType, (u32, Vec<Box<dyn Payload>>)> = HashMap::new();
        for (workload_type, (weight, workload)) in self.workloads.iter() {
            let payloads: Vec<Box<dyn Payload>> = workload
                .make_test_payloads(
                    num_payloads,
                    payload_config.clone(),
                    proxy.clone(),
                    system_state_observer.clone(),
                )
                .await;
            assert_eq!(payloads.len() as u64, num_payloads);
            workloads
                .entry(*workload_type)
                .or_insert_with(|| (*weight, payloads));
        }
        let mut res = vec![];
        for _i in 0..num_payloads {
            let mut all_payloads: Vec<Box<dyn Payload>> = vec![];
            let mut dist = vec![];
            for (_type, (weight, payloads)) in workloads.iter_mut() {
                all_payloads.push(payloads.pop().unwrap());
                dist.push(*weight);
            }
            res.push(Box::new(CombinationPayload {
                payloads: all_payloads,
                dist: WeightedAliasIndex::new(dist).unwrap(),
                curr_index: 0,
                rng: OsRng::default(),
            }));
        }
        res.into_iter()
            .map(|b| Box::<dyn Payload>::from(b))
            .collect()
    }
    fn get_workload_type(&self) -> WorkloadType {
        WorkloadType::Combination
    }
}

impl CombinationWorkload {
    pub fn new_boxed(
        workloads: HashMap<WorkloadType, WeightAndPayload>,
    ) -> Box<dyn Workload<dyn Payload>> {
        Box::new(CombinationWorkload { workloads })
    }
}

pub struct WorkloadInfo {
    pub target_qps: u64,
    pub num_workers: u64,
    pub max_in_flight_ops: u64,
    pub workload: Box<dyn Workload<dyn Payload>>,
    pub payload_config: WorkloadPayloadGas,
}
