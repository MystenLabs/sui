// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use std::sync::Arc;
use std::{collections::HashMap, fmt};

use sui_types::{
    base_types::{ObjectID, ObjectRef},
    object::Owner,
};

use futures::FutureExt;
use sui_types::{base_types::SuiAddress, crypto::AccountKeyPair, messages::VerifiedTransaction};
use test_utils::messages::make_transfer_sui_transaction;
use tracing::error;

use rand::{prelude::*, rngs::OsRng};
use rand_distr::WeightedAliasIndex;

use crate::ValidatorProxy;

// This is the maximum gas we will transfer from primary coin into any gas coin
// for running the benchmark
pub const MAX_GAS_FOR_TESTING: u64 = 1_000_000_000;

pub type Gas = (ObjectRef, Owner);

pub type UpdatedAndNewlyMinted = (ObjectRef, ObjectRef);

pub async fn transfer_sui_for_testing(
    gas: Gas,
    keypair: &AccountKeyPair,
    value: u64,
    address: SuiAddress,
    proxy: Arc<dyn ValidatorProxy + Sync + Send>,
) -> Option<UpdatedAndNewlyMinted> {
    let tx = make_transfer_sui_transaction(
        gas.0,
        address,
        Some(value),
        gas.1.get_owner_address().unwrap(),
        keypair,
    );
    proxy
        .execute_transaction(tx.into())
        .map(move |res| match res {
            Ok((_, effects)) => {
                let minted = effects.created().get(0).unwrap().0;
                let updated = effects
                    .mutated()
                    .iter()
                    .find(|(k, _)| k.0 == gas.0 .0)
                    .unwrap()
                    .0;
                Some((updated, minted))
            }
            Err(err) => {
                error!("Error while transferring sui: {:?}", err);
                None
            }
        })
        .await
}

pub trait Payload: Send + Sync {
    fn make_new_payload(
        self: Box<Self>,
        new_object: ObjectRef,
        new_gas: ObjectRef,
    ) -> Box<dyn Payload>;
    fn make_transaction(&self) -> VerifiedTransaction;
    fn get_object_id(&self) -> ObjectID;
    fn get_workload_type(&self) -> WorkloadType;
}

pub struct CombinationPayload {
    payloads: Vec<Box<dyn Payload>>,
    dist: WeightedAliasIndex<u32>,
    curr_index: usize,
    rng: OsRng,
}

impl Payload for CombinationPayload {
    fn make_new_payload(
        self: Box<Self>,
        new_object: ObjectRef,
        new_gas: ObjectRef,
    ) -> Box<dyn Payload> {
        let mut new_payloads = vec![];
        for (pos, e) in self.payloads.into_iter().enumerate() {
            if pos == self.curr_index {
                let updated = e.make_new_payload(new_object, new_gas);
                new_payloads.push(updated);
            } else {
                new_payloads.push(e);
            }
        }
        let mut rng = self.rng;
        let next_index = self.dist.sample(&mut rng);
        Box::new(CombinationPayload {
            payloads: new_payloads,
            dist: self.dist,
            curr_index: next_index,
            rng: self.rng,
        })
    }
    fn make_transaction(&self) -> VerifiedTransaction {
        let curr = self.payloads.get(self.curr_index).unwrap();
        curr.make_transaction()
    }
    fn get_object_id(&self) -> ObjectID {
        let curr = self.payloads.get(self.curr_index).unwrap();
        curr.get_object_id()
    }
    fn get_workload_type(&self) -> WorkloadType {
        self.payloads
            .get(self.curr_index)
            .unwrap()
            .get_workload_type()
    }
}

#[derive(Copy, Clone, Hash, PartialEq, Eq)]
pub enum WorkloadType {
    SharedCounter,
    TransferObject,
}

impl fmt::Display for WorkloadType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            WorkloadType::SharedCounter => write!(f, "shared_counter"),
            WorkloadType::TransferObject => write!(f, "transfer_object"),
        }
    }
}

#[async_trait]
pub trait Workload<T: Payload + ?Sized>: Send + Sync {
    async fn init(
        &mut self,
        num_shared_counters: u64,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
    );
    async fn make_test_payloads(
        &self,
        count: u64,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
    ) -> Vec<Box<T>>;
}

type WeightAndPayload = (u32, Box<dyn Workload<dyn Payload>>);
pub struct CombinationWorkload {
    workloads: HashMap<WorkloadType, WeightAndPayload>,
}

#[async_trait]
impl Workload<dyn Payload> for CombinationWorkload {
    async fn init(
        &mut self,
        num_shared_counters: u64,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
    ) {
        for (_, (_, workload)) in self.workloads.iter_mut() {
            workload.init(num_shared_counters, proxy.clone()).await;
        }
    }
    async fn make_test_payloads(
        &self,
        count: u64,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
    ) -> Vec<Box<dyn Payload>> {
        let mut workloads: HashMap<WorkloadType, (u32, Vec<Box<dyn Payload>>)> = HashMap::new();
        for (workload_type, (weight, workload)) in self.workloads.iter() {
            let payloads: Vec<Box<dyn Payload>> =
                workload.make_test_payloads(count, proxy.clone()).await;
            assert_eq!(payloads.len() as u64, count);
            workloads
                .entry(*workload_type)
                .or_insert_with(|| (*weight, payloads));
        }
        let mut res = vec![];
        for _i in 0..count {
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
}
