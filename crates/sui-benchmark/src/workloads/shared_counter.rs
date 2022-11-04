// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::workload::{Gas, Payload, Workload, WorkloadType};
use crate::{
    workloads::workload::{transfer_sui_for_testing, MAX_GAS_FOR_TESTING},
    ValidatorProxy,
};
use async_trait::async_trait;
use futures::future::join_all;
use rand::seq::SliceRandom;
use std::{path::PathBuf, sync::Arc};
use sui_types::{
    base_types::{ObjectDigest, ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    crypto::{get_key_pair, AccountKeyPair},
    messages::VerifiedTransaction,
    object::Owner,
};
use test_utils::messages::{make_counter_create_transaction, make_counter_increment_transaction};
use test_utils::{
    messages::create_publish_move_package_transaction, transaction::parse_package_ref,
};

pub struct SharedCounterTestPayload {
    package_ref: ObjectRef,
    counter_id: ObjectID,
    counter_initial_shared_version: SequenceNumber,
    gas: Gas,
    sender: SuiAddress,
    keypair: Arc<AccountKeyPair>,
}

impl Payload for SharedCounterTestPayload {
    fn make_new_payload(self: Box<Self>, _: ObjectRef, new_gas: ObjectRef) -> Box<dyn Payload> {
        Box::new(SharedCounterTestPayload {
            package_ref: self.package_ref,
            counter_id: self.counter_id,
            counter_initial_shared_version: self.counter_initial_shared_version,
            gas: (new_gas, self.gas.1),
            sender: self.sender,
            keypair: self.keypair,
        })
    }
    fn make_transaction(&self) -> VerifiedTransaction {
        make_counter_increment_transaction(
            self.gas.0,
            self.package_ref,
            self.counter_id,
            self.counter_initial_shared_version,
            self.sender,
            &self.keypair,
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
    pub test_gas: ObjectID,
    pub test_gas_owner: SuiAddress,
    pub test_gas_keypair: Arc<AccountKeyPair>,
    pub basics_package_ref: Option<ObjectRef>,
    pub counters: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
}

impl SharedCounterWorkload {
    pub fn new_boxed(
        gas: ObjectID,
        owner: SuiAddress,
        keypair: Arc<AccountKeyPair>,
        basics_package_ref: Option<ObjectRef>,
        counters: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) -> Box<dyn Workload<dyn Payload>> {
        Box::<dyn Workload<dyn Payload>>::from(Box::new(SharedCounterWorkload {
            test_gas: gas,
            test_gas_owner: owner,
            test_gas_keypair: keypair,
            basics_package_ref,
            counters,
        }))
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
    async fn init(&mut self, num_counters: u64, proxy: Arc<dyn ValidatorProxy + Sync + Send>) {
        if self.basics_package_ref.is_some() {
            return;
        }
        // publish basics package
        let primary_gas = proxy.get_object(self.test_gas).await.unwrap();
        let mut primary_gas_ref = primary_gas.compute_object_reference();
        let mut publish_module_gas_ref = None;
        let (address, keypair) = get_key_pair();
        if let Some((updated, minted)) = transfer_sui_for_testing(
            (primary_gas_ref, Owner::AddressOwner(self.test_gas_owner)),
            &self.test_gas_keypair,
            MAX_GAS_FOR_TESTING,
            address,
            proxy.clone(),
        )
        .await
        {
            publish_module_gas_ref = Some((address, keypair, minted));
            primary_gas_ref = updated;
        }
        // Publish basics package
        eprintln!("Publishing basics package");
        let publish_module_gas = publish_module_gas_ref.unwrap();
        self.basics_package_ref = Some(
            publish_basics_package(
                publish_module_gas.2,
                proxy.clone(),
                publish_module_gas.0,
                &publish_module_gas.1,
            )
            .await,
        );
        if !self.counters.is_empty() {
            // We already initialized the workload with some counters
            return;
        }
        // create counters
        let num_counters = std::cmp::max(num_counters as usize, 1);
        eprintln!(
            "Creating {:?} shared counters, this may take a while..",
            num_counters
        );
        // Make as many gas objects as the number of actual unique counters
        // This gas is used for creating the counters
        let mut counters_gas = vec![];
        for _ in 0..num_counters {
            let (address, keypair) = get_key_pair();
            if let Some((updated, minted)) = transfer_sui_for_testing(
                (primary_gas_ref, Owner::AddressOwner(self.test_gas_owner)),
                &self.test_gas_keypair,
                MAX_GAS_FOR_TESTING,
                address,
                proxy.clone(),
            )
            .await
            {
                primary_gas_ref = updated;
                counters_gas.push((address, keypair, minted));
            }
        }
        let mut futures = vec![];
        for (sender, keypair, gas) in counters_gas.iter() {
            let transaction = make_counter_create_transaction(
                *gas,
                self.basics_package_ref.unwrap(),
                *sender,
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
        count: u64,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
    ) -> Vec<Box<dyn Payload>> {
        // Read latest test gas object
        let primary_gas = proxy.get_object(self.test_gas).await.unwrap();
        let mut primary_gas_ref = primary_gas.compute_object_reference();
        // Make as many gas objects as the number of payloads
        let mut counters_gas = vec![];
        for _ in 0..count {
            let (address, keypair) = get_key_pair::<AccountKeyPair>();
            if let Some((updated, minted)) = transfer_sui_for_testing(
                (primary_gas_ref, Owner::AddressOwner(self.test_gas_owner)),
                &self.test_gas_keypair,
                MAX_GAS_FOR_TESTING,
                address,
                proxy.clone(),
            )
            .await
            {
                primary_gas_ref = updated;
                counters_gas.push((address, Arc::new(keypair), minted));
            }
        }
        // create counters using gas objects we created above
        eprintln!("Creating shared txn payloads, hang tight..");
        let mut shared_payloads = vec![];
        for i in 0..count {
            let (sender, keypair, gas) = &counters_gas[i as usize];
            // pick a random counter from the pool
            let counter_ref = self
                .counters
                .choose(&mut rand::thread_rng())
                .expect("Failed to get a random counter from the pool");
            shared_payloads.push(Box::new(SharedCounterTestPayload {
                package_ref: self.basics_package_ref.unwrap(),
                counter_id: counter_ref.0,
                counter_initial_shared_version: counter_ref.1,
                gas: (*gas, Owner::AddressOwner(*sender)),
                sender: *sender,
                keypair: keypair.clone(),
            }));
        }
        let payloads: Vec<Box<dyn Payload>> = shared_payloads
            .into_iter()
            .map(|b| Box::<dyn Payload>::from(b))
            .collect();
        payloads
    }
}
