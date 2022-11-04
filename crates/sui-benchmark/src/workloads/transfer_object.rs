// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use rand::seq::IteratorRandom;

use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress},
    crypto::{get_key_pair, AccountKeyPair},
    messages::VerifiedTransaction,
    object::Owner,
};

use test_utils::messages::make_transfer_object_transaction;

use crate::ValidatorProxy;

use super::workload::{
    transfer_sui_for_testing, Gas, Payload, Workload, WorkloadType, MAX_GAS_FOR_TESTING,
};

pub struct TransferObjectTestPayload {
    transfer_object: ObjectRef,
    transfer_from: SuiAddress,
    transfer_to: SuiAddress,
    gas: Vec<Gas>,
    keypairs: Arc<HashMap<SuiAddress, AccountKeyPair>>,
}

impl Payload for TransferObjectTestPayload {
    fn make_new_payload(
        self: Box<Self>,
        new_object: ObjectRef,
        new_gas: ObjectRef,
    ) -> Box<dyn Payload> {
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
    fn make_transaction(&self) -> VerifiedTransaction {
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
    fn get_workload_type(&self) -> WorkloadType {
        WorkloadType::TransferObject
    }
}

pub struct TransferObjectWorkload {
    pub test_gas: ObjectID,
    pub test_gas_owner: SuiAddress,
    pub test_gas_keypair: Arc<AccountKeyPair>,
    pub num_accounts: u64,
    pub transfer_keypairs: Arc<HashMap<SuiAddress, AccountKeyPair>>,
}

impl TransferObjectWorkload {
    pub fn new_boxed(
        num_accounts: u64,
        gas: ObjectID,
        owner: SuiAddress,
        keypair: Arc<AccountKeyPair>,
    ) -> Box<dyn Workload<dyn Payload>> {
        // create several accounts to transfer object between
        let keypairs: Arc<HashMap<SuiAddress, AccountKeyPair>> =
            Arc::new((0..num_accounts).map(|_| get_key_pair()).collect());
        Box::new(TransferObjectWorkload {
            test_gas: gas,
            test_gas_owner: owner,
            test_gas_keypair: keypair,
            num_accounts,
            transfer_keypairs: keypairs,
        })
    }
}

#[async_trait]
impl Workload<dyn Payload> for TransferObjectWorkload {
    async fn init(
        &mut self,
        _num_shared_counters: u64,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
    ) {
        return;
    }
    async fn make_test_payloads(
        &self,
        count: u64,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
    ) -> Vec<Box<dyn Payload>> {
        // Read latest test gas object
        let primary_gas = proxy.get_object(self.test_gas).await.unwrap();
        let mut primary_gas_ref = primary_gas.compute_object_reference();
        let owner = *self
            .transfer_keypairs
            .keys()
            .choose(&mut rand::thread_rng())
            .unwrap();
        // create as many gas objects as there are number of transfer objects times number of accounts
        eprintln!("Creating enough gas to transfer objects..");
        let mut transfer_gas: Vec<Vec<Gas>> = vec![];
        for _i in 0..count {
            let mut account_transfer_gas = vec![];
            for (owner, _) in self.transfer_keypairs.iter() {
                if let Some((updated, minted)) = transfer_sui_for_testing(
                    (primary_gas_ref, Owner::AddressOwner(self.test_gas_owner)),
                    &self.test_gas_keypair,
                    MAX_GAS_FOR_TESTING,
                    *owner,
                    proxy.clone(),
                )
                .await
                {
                    primary_gas_ref = updated;
                    account_transfer_gas.push((minted, Owner::AddressOwner(*owner)));
                }
            }
            transfer_gas.push(account_transfer_gas);
        }
        eprintln!("Creating transfer object txns, almost done..");
        // create transfer objects with 1 SUI value each
        let mut transfer_objects: Vec<Gas> = vec![];
        for _i in 0..count {
            if let Some((updated, minted)) = transfer_sui_for_testing(
                (primary_gas_ref, Owner::AddressOwner(self.test_gas_owner)),
                &self.test_gas_keypair,
                1,
                owner,
                proxy.clone(),
            )
            .await
            {
                primary_gas_ref = updated;
                transfer_objects.push((minted, Owner::AddressOwner(owner)));
            }
        }
        let refs: Vec<(Vec<Gas>, ObjectRef)> = transfer_gas
            .into_iter()
            .zip(transfer_objects.iter())
            .map(|(g, t)| (g, t.0))
            .collect();
        refs.iter()
            .map(|(g, t)| {
                let from = owner;
                let (_, to) = *g
                    .iter()
                    .find(|x| x.1.get_owner_address().unwrap() != from)
                    .unwrap();
                Box::new(TransferObjectTestPayload {
                    transfer_object: *t,
                    transfer_from: from,
                    transfer_to: to.get_owner_address().unwrap(),
                    gas: g.clone(),
                    keypairs: self.transfer_keypairs.clone(),
                })
            })
            .map(|b| Box::<dyn Payload>::from(b))
            .collect()
    }
}
