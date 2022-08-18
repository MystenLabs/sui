// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use std::collections::HashMap;
use sui_core::{
    authority_aggregator::AuthorityAggregator, authority_client::NetworkAuthorityClient,
};
use sui_quorum_driver::QuorumDriverMetrics;
use sui_types::{
    base_types::{ObjectID, ObjectRef},
    crypto::EmptySignInfo,
    messages::{TransactionEffects, TransactionEnvelope},
    object::{Object, ObjectRead, Owner},
};

use futures::FutureExt;
use sui_quorum_driver::QuorumDriverHandler;
use sui_types::{
    base_types::SuiAddress,
    crypto::AccountKeyPair,
    messages::{
        ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
        Transaction,
    },
};
use test_utils::messages::make_transfer_sui_transaction;
use tracing::log::error;

use rand::{prelude::*, rngs::OsRng};
use rand_distr::WeightedAliasIndex;

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
    client: &AuthorityAggregator<NetworkAuthorityClient>,
) -> Option<UpdatedAndNewlyMinted> {
    let tx = make_transfer_sui_transaction(
        gas.0,
        address,
        Some(value),
        gas.1.get_owner_address().unwrap(),
        keypair,
    );
    let quorum_driver_handler =
        QuorumDriverHandler::new(client.clone(), QuorumDriverMetrics::new_for_tests());
    let qd = quorum_driver_handler.clone_quorum_driver();
    qd.execute_transaction(ExecuteTransactionRequest {
        transaction: tx.clone(),
        request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
    })
    .map(move |res| match res {
        Ok(ExecuteTransactionResponse::EffectsCert(result)) => {
            let (_, effects) = *result;
            let minted = effects.effects().created.get(0).unwrap().0;
            let updated = effects
                .effects()
                .mutated
                .iter()
                .find(|(k, _)| k.0 == gas.0 .0)
                .unwrap()
                .0;
            Some((updated, minted))
        }
        Ok(resp) => {
            error!("Unexpected response while transferring sui: {:?}", resp);
            None
        }
        Err(err) => {
            error!("Error while transferring sui: {:?}", err);
            None
        }
    })
    .await
}

pub async fn get_latest(
    object_id: ObjectID,
    aggregator: &AuthorityAggregator<NetworkAuthorityClient>,
) -> Option<Object> {
    // Return the latest object version
    match aggregator.get_object_info_execute(object_id).await.unwrap() {
        ObjectRead::Exists(_, object, _) => Some(object),
        _ => None,
    }
}

pub async fn submit_transaction(
    transaction: Transaction,
    aggregator: &AuthorityAggregator<NetworkAuthorityClient>,
) -> Option<TransactionEffects> {
    let qd = QuorumDriverHandler::new(aggregator.clone(), QuorumDriverMetrics::new_for_tests());
    if let ExecuteTransactionResponse::EffectsCert(result) = qd
        .clone_quorum_driver()
        .execute_transaction(ExecuteTransactionRequest {
            transaction,
            request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
        })
        .await
        .unwrap()
    {
        let (_, effects) = *result;
        Some(effects.effects().clone())
    } else {
        None
    }
}

pub trait Payload: Send + Sync {
    fn make_new_payload(
        self: Box<Self>,
        new_object: ObjectRef,
        new_gas: ObjectRef,
    ) -> Box<dyn Payload>;
    fn make_transaction(&self) -> TransactionEnvelope<EmptySignInfo>;
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
    fn make_transaction(&self) -> TransactionEnvelope<EmptySignInfo> {
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

#[async_trait]
pub trait Workload<T: Payload + ?Sized>: Send + Sync {
    async fn init(&mut self, aggregator: &AuthorityAggregator<NetworkAuthorityClient>);
    async fn make_test_payloads(
        &self,
        count: u64,
        client: &AuthorityAggregator<NetworkAuthorityClient>,
    ) -> Vec<Box<T>>;
}

type WeightAndPayload = (u32, Box<dyn Workload<dyn Payload>>);
pub struct CombinationWorkload {
    workloads: HashMap<WorkloadType, WeightAndPayload>,
}

#[async_trait]
impl Workload<dyn Payload> for CombinationWorkload {
    async fn init(&mut self, aggregator: &AuthorityAggregator<NetworkAuthorityClient>) {
        for (_, (_, workload)) in self.workloads.iter_mut() {
            workload.init(aggregator).await;
        }
    }
    async fn make_test_payloads(
        &self,
        count: u64,
        aggregator: &AuthorityAggregator<NetworkAuthorityClient>,
    ) -> Vec<Box<dyn Payload>> {
        let mut workloads: HashMap<WorkloadType, (u32, Vec<Box<dyn Payload>>)> = HashMap::new();
        for (workload_type, (weight, workload)) in self.workloads.iter() {
            let payloads: Vec<Box<dyn Payload>> =
                workload.make_test_payloads(count, aggregator).await;
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
