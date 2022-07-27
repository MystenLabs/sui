// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use futures::future::join_all;
use sui_config::NetworkConfig;
use sui_quorum_driver::QuorumDriverHandler;
use sui_types::{
    base_types::{ObjectID, ObjectRef},
    crypto::EmptySignInfo,
    messages::{
        ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
        TransactionEnvelope,
    },
    object::Object,
};
use test_utils::{
    authority::test_authority_aggregator,
    messages::{make_counter_create_transaction, make_counter_increment_transaction},
    objects::{generate_gas_object, generate_gas_objects_for_testing},
    test_account_keys,
    transaction::publish_counter_package,
};

use super::context::{Gas, Payload, StressTestCtx};

pub struct SharedCounterTestPayload {
    package_ref: ObjectRef,
    counter_id: ObjectID,
    gas: Gas,
}

impl Payload for SharedCounterTestPayload {
    fn make_new_payload(&self, _: ObjectRef, new_gas: ObjectRef) -> Box<dyn Payload> {
        Box::new(SharedCounterTestPayload {
            package_ref: self.package_ref,
            counter_id: self.counter_id,
            gas: (new_gas, self.gas.1),
        })
    }
    fn make_transaction(&self) -> TransactionEnvelope<EmptySignInfo> {
        let (sender, keypair) = test_account_keys().pop().unwrap();
        make_counter_increment_transaction(
            self.gas.0,
            self.package_ref,
            self.counter_id,
            sender,
            &keypair,
        )
    }
    fn get_object_id(&self) -> ObjectID {
        self.counter_id
    }
}

pub struct SharedCounterTestCtx {
    counter_gas: Vec<Object>,
    publish_module_gas: Object,
}

impl SharedCounterTestCtx {
    pub fn make_ctx(count: u64, _configs: &NetworkConfig) -> Box<dyn StressTestCtx<dyn Payload>> {
        // create enough gas to increment shared counters and publish module
        let counter_gas = generate_gas_objects_for_testing(count as usize);
        let publish_module_gas = generate_gas_object();
        Box::<dyn StressTestCtx<dyn Payload>>::from(Box::new(SharedCounterTestCtx {
            counter_gas,
            publish_module_gas,
        }))
    }
}
#[async_trait]
impl StressTestCtx<dyn Payload> for SharedCounterTestCtx {
    fn get_gas_objects(&mut self) -> Vec<Object> {
        let mut gas = vec![];
        gas.append(&mut self.counter_gas.clone());
        gas.push(self.publish_module_gas.clone());
        gas
    }
    async fn make_test_payloads(&self, configs: &NetworkConfig) -> Vec<Box<dyn Payload>> {
        let clients = test_authority_aggregator(configs);
        let quorum_driver_handler = QuorumDriverHandler::new(clients.clone());
        // Publish basics package
        eprintln!("Publishing basics package");
        let package_ref =
            publish_counter_package(self.publish_module_gas.clone(), configs.validator_set()).await;
        let qd_and_gas = self
            .counter_gas
            .clone()
            .into_iter()
            .map(|g| (quorum_driver_handler.clone_quorum_driver(), g));
        // create counters
        let futures = qd_and_gas.map(|(qd, gas_object)| async move {
            let (sender, keypair) = test_account_keys().pop().unwrap();
            let tx = make_counter_create_transaction(
                gas_object.compute_object_reference(),
                package_ref,
                sender,
                &keypair,
            );
            if let ExecuteTransactionResponse::EffectsCert(result) = qd
                .execute_transaction(ExecuteTransactionRequest {
                    transaction: tx,
                    request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
                })
                .await
                .unwrap()
            {
                let (_, effects) = *result;
                Box::new(SharedCounterTestPayload {
                    package_ref,
                    counter_id: effects.effects.created[0].0 .0,
                    gas: effects.effects.gas_object,
                })
            } else {
                panic!("Failed to create shared counter!");
            }
        });
        eprintln!("Creating shared counters, this may take a while..");
        join_all(futures)
            .await
            .into_iter()
            .map(|b| Box::<dyn Payload>::from(b))
            .collect()
    }
}
