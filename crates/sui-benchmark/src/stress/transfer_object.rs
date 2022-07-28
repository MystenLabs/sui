// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use rand::seq::IteratorRandom;
use sui_config::NetworkConfig;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress},
    crypto::{get_key_pair, AccountKeyPair, EmptySignInfo},
    messages::TransactionEnvelope,
    object::{Object, Owner},
};

use test_utils::{
    messages::make_transfer_object_transaction, objects::generate_gas_objects_with_owner,
};

use super::context::{Gas, Payload, StressTestCtx};

pub struct TransferObjectTestPayload {
    transfer_object: ObjectRef,
    transfer_from: SuiAddress,
    transfer_to: SuiAddress,
    gas: Vec<Gas>,
    keypairs: Arc<HashMap<SuiAddress, AccountKeyPair>>,
}

impl Payload for TransferObjectTestPayload {
    fn make_new_payload(&self, new_object: ObjectRef, new_gas: ObjectRef) -> Box<dyn Payload> {
        let updated_gas: Vec<Gas> = self
            .gas
            .iter()
            .map(|x| {
                if x.1.get_owner_address().unwrap() == self.transfer_from {
                    (new_gas, Owner::AddressOwner(self.transfer_from))
                } else {
                    *x
                }
            })
            .collect();
        let (_, recipient) = self
            .gas
            .iter()
            .find(|x| x.1.get_owner_address().unwrap() != self.transfer_to)
            .unwrap();
        Box::new(TransferObjectTestPayload {
            transfer_object: new_object,
            transfer_from: self.transfer_to,
            transfer_to: recipient.get_owner_address().unwrap(),
            gas: updated_gas,
            keypairs: self.keypairs.clone(),
        })
    }
    fn make_transaction(&self) -> TransactionEnvelope<EmptySignInfo> {
        let (gas_obj, _) = self
            .gas
            .iter()
            .find(|x| x.1.get_owner_address().unwrap() == self.transfer_from)
            .unwrap();
        make_transfer_object_transaction(
            self.transfer_object,
            *gas_obj,
            self.transfer_from,
            self.keypairs.get(&self.transfer_from).unwrap(),
            self.transfer_to,
        )
    }
    fn get_object_id(&self) -> ObjectID {
        self.transfer_object.0
    }
}

pub struct TransferObjectTestCtx {
    transfer_gas: Vec<Vec<Object>>,
    transfer_objects: Vec<Object>,
    transfer_objects_owner: SuiAddress,
    keypairs: Arc<HashMap<SuiAddress, AccountKeyPair>>,
}

impl TransferObjectTestCtx {
    pub fn make_ctx(
        count: u64,
        num_accounts: u64,
        _configs: &NetworkConfig,
    ) -> Box<dyn StressTestCtx<dyn Payload>> {
        // create several accounts to transfer object between
        let keypairs: Arc<HashMap<SuiAddress, AccountKeyPair>> =
            Arc::new((0..num_accounts).map(|_| get_key_pair()).collect());
        // create enough gas to do those transfers
        let gas: Vec<Vec<Object>> = (0..count)
            .map(|_| {
                keypairs
                    .iter()
                    .map(|(owner, _)| generate_gas_objects_with_owner(1, *owner).pop().unwrap())
                    .collect()
            })
            .collect();
        // choose a random owner to be the owner of transfer objects
        let owner = *keypairs.keys().choose(&mut rand::thread_rng()).unwrap();
        // create transfer objects
        let transfer_objects = generate_gas_objects_with_owner(count as usize, owner);
        Box::new(TransferObjectTestCtx {
            transfer_gas: gas,
            transfer_objects,
            transfer_objects_owner: owner,
            keypairs,
        })
    }
}

#[async_trait]
impl StressTestCtx<dyn Payload> for TransferObjectTestCtx {
    fn get_gas_objects(&mut self) -> Vec<Object> {
        let mut gas: Vec<Object> = self.transfer_gas.clone().into_iter().flatten().collect();
        gas.append(&mut self.transfer_objects.clone());
        gas
    }
    async fn make_test_payloads(&self, _configs: &NetworkConfig) -> Vec<Box<dyn Payload>> {
        let refs: Vec<(Vec<Gas>, ObjectRef)> = self
            .transfer_gas
            .iter()
            .zip(self.transfer_objects.iter())
            .map(|(g, t)| {
                (
                    g.iter()
                        .map(|x| (x.compute_object_reference(), x.owner))
                        .collect(),
                    t.compute_object_reference(),
                )
            })
            .collect();
        refs.iter()
            .map(|(g, t)| {
                let from = self.transfer_objects_owner;
                let (_, to) = *g
                    .iter()
                    .find(|x| x.1.get_owner_address().unwrap() != from)
                    .unwrap();
                Box::new(TransferObjectTestPayload {
                    transfer_object: *t,
                    transfer_from: from,
                    transfer_to: to.get_owner_address().unwrap(),
                    gas: g.clone(),
                    keypairs: self.keypairs.clone(),
                })
            })
            .map(|b| Box::<dyn Payload>::from(b))
            .collect()
    }
}
