// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;

use sui_config::NetworkConfig;
use sui_types::{
    base_types::{ObjectID, ObjectRef},
    crypto::EmptySignInfo,
    messages::TransactionEnvelope,
    object::{Object, Owner},
};

pub type Gas = (ObjectRef, Owner);

pub trait Payload: Send + Sync {
    fn make_new_payload(&self, new_object: ObjectRef, new_gas: ObjectRef) -> Box<dyn Payload>;
    fn make_transaction(&self) -> TransactionEnvelope<EmptySignInfo>;
    fn get_object_id(&self) -> ObjectID;
}

#[async_trait]
pub trait StressTestCtx<T: Payload + ?Sized> {
    fn get_gas_objects(&mut self) -> Vec<Object>;
    async fn make_test_payloads(&self, configs: &NetworkConfig) -> Vec<Box<T>>;
}
