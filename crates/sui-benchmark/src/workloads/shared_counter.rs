// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::workload::{Workload, WorkloadType};
use crate::workloads::Gas;

use crate::workloads::payload::Payload;
use crate::workloads::workload::MAX_GAS_FOR_TESTING;
use crate::workloads::{GasCoinConfig, WorkloadInitGas, WorkloadPayloadGas};
use crate::{ExecutionEffects, ValidatorProxy};
use async_trait::async_trait;
use futures::future::join_all;
use rand::seq::SliceRandom;
use std::{path::PathBuf, sync::Arc};
use sui_types::crypto::get_key_pair;
use sui_types::{
    base_types::{ObjectDigest, ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    crypto::AccountKeyPair,
    messages::VerifiedTransaction,
};
use test_utils::messages::{make_counter_create_transaction, make_counter_increment_transaction};
use test_utils::{
    messages::create_publish_move_package_transaction, transaction::parse_package_ref,
};
use tracing::info;

pub struct SharedCounterTestPayload {
    package_id: ObjectID,
    counter_id: ObjectID,
    counter_initial_shared_version: SequenceNumber,
    gas: Gas,
}

impl Payload for SharedCounterTestPayload {
    fn make_new_payload(
        self: Box<Self>,
        _: ObjectRef,
        new_gas: ObjectRef,
        _: &ExecutionEffects,
    ) -> Box<dyn Payload> {
        Box::new(SharedCounterTestPayload {
            package_id: self.package_id,
            counter_id: self.counter_id,
            counter_initial_shared_version: self.counter_initial_shared_version,
            gas: (new_gas, self.gas.1, self.gas.2),
        })
    }
    fn make_transaction(&self) -> VerifiedTransaction {
        make_counter_increment_transaction(
            self.gas.0,
            self.package_id,
            self.counter_id,
            self.counter_initial_shared_version,
            self.gas
                .1
                .get_owner_address()
                .expect("Cannot convert owner to address"),
            &self.gas.2,
        )
    }
    fn get_object_id(&self) -> ObjectID {
        self.counter_id
    }
    fn get_workload_type(&self) -> WorkloadType {
        WorkloadType::SharedCounter
    }
}

pub struct SharedCounterWorkload {
    pub basics_package_id: Option<ObjectID>,
    pub counters: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
}

impl SharedCounterWorkload {
    pub fn new_boxed(
        basics_package_id: Option<ObjectID>,
        counters: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) -> Box<dyn Workload<dyn Payload>> {
        Box::<dyn Workload<dyn Payload>>::from(Box::new(SharedCounterWorkload {
            basics_package_id,
            counters,
        }))
    }
    pub fn generate_coin_config_for_init(num_counters: u64) -> Vec<GasCoinConfig> {
        let mut configs = vec![];

        // Gas coin for publishing package
        let (address, keypair) = get_key_pair();
        configs.push(GasCoinConfig {
            amount: MAX_GAS_FOR_TESTING,
            address,
            keypair: Arc::new(keypair),
        });

        // Gas coins for creating counters
        for _i in 0..num_counters {
            let (address, keypair) = get_key_pair();
            configs.push(GasCoinConfig {
                amount: MAX_GAS_FOR_TESTING,
                address,
                keypair: Arc::new(keypair),
            });
        }
        configs
    }
    pub fn generate_coin_config_for_payloads(num_payloads: u64) -> Vec<GasCoinConfig> {
        let mut configs = vec![];
        // Gas coins for running workload
        for _i in 0..num_payloads {
            let (address, keypair) = get_key_pair();
            configs.push(GasCoinConfig {
                amount: MAX_GAS_FOR_TESTING,
                address,
                keypair: Arc::new(keypair),
            });
        }
        configs
    }
}

pub async fn publish_basics_package(
    gas: ObjectRef,
    proxy: Arc<dyn ValidatorProxy + Sync + Send>,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
) -> ObjectRef {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../sui_programmability/examples/basics");
    let transaction = create_publish_move_package_transaction(gas, path, sender, keypair);
    let (_, effects) = proxy.execute_transaction(transaction.into()).await.unwrap();
    parse_package_ref(&effects.created()).unwrap()
}

#[async_trait]
impl Workload<dyn Payload> for SharedCounterWorkload {
    async fn init(
        &mut self,
        init_config: WorkloadInitGas,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
    ) {
        if self.basics_package_id.is_some() {
            return;
        }
        let (head, tail) = init_config
            .shared_counter_init_gas
            .split_first()
            .expect("Not enough gas to initialize shared counter workload");

        // Publish basics package
        info!("Publishing basics package");
        self.basics_package_id = Some(
            publish_basics_package(
                head.0,
                proxy.clone(),
                head.1
                    .get_owner_address()
                    .expect("Could not get sui address from owner"),
                &head.2,
            )
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
                (*sender)
                    .get_owner_address()
                    .expect("Could not get sui address from owner"),
                keypair,
            );
            let proxy_ref = proxy.clone();
            futures.push(async move {
                if let Ok((_, effects)) = proxy_ref.execute_transaction(transaction.into()).await {
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
        _num_payloads: u64,
        payload_config: WorkloadPayloadGas,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
    ) -> Vec<Box<dyn Payload>> {
        // create counters using gas objects we created above
        info!("Creating shared txn payloads, hang tight..");
        let mut shared_payloads = vec![];
        for g in payload_config.shared_counter_payload_gas.into_iter() {
            // pick a random counter from the pool
            let counter_ref = self
                .counters
                .choose(&mut rand::thread_rng())
                .expect("Failed to get a random counter from the pool");
            shared_payloads.push(Box::new(SharedCounterTestPayload {
                package_id: self.basics_package_id.unwrap(),
                counter_id: counter_ref.0,
                counter_initial_shared_version: counter_ref.1,
                gas: g,
            }));
        }
        let payloads: Vec<Box<dyn Payload>> = shared_payloads
            .into_iter()
            .map(|b| Box::<dyn Payload>::from(b))
            .collect();
        payloads
    }
    fn get_workload_type(&self) -> WorkloadType {
        WorkloadType::SharedCounter
    }
}
