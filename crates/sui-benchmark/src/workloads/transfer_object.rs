// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use rand::seq::IteratorRandom;

use std::collections::HashMap;
use std::sync::Arc;

use sui_types::{
    base_types::{ObjectRef, SuiAddress},
    crypto::{get_key_pair, AccountKeyPair},
    messages::VerifiedTransaction,
};

use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::Payload;
use crate::workloads::workload::{WorkloadBuilder, WorkloadInitParameter, WorkloadType};
use crate::workloads::{Gas, GasCoinConfig};
use crate::{ExecutionEffects, ValidatorProxy};
use sui_core::test_utils::make_transfer_object_transaction;

use super::workload::{Workload, MAX_GAS_FOR_TESTING};

#[derive(Debug)]
pub struct TransferObjectTestPayload {
    transfer_object: ObjectRef,
    transfer_from: SuiAddress,
    transfer_to: SuiAddress,
    gas: Vec<Gas>,
    system_state_observer: Arc<SystemStateObserver>,
}

impl Payload for TransferObjectTestPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        let recipient = self.gas.iter().find(|x| x.1 != self.transfer_to).unwrap().1;
        let updated_gas: Vec<Gas> = self
            .gas
            .iter()
            .map(|x| {
                if x.1 == self.transfer_from {
                    (effects.gas_object().0, self.transfer_from, x.2.clone())
                } else {
                    x.clone()
                }
            })
            .collect();
        self.transfer_object = effects
            .mutated()
            .iter()
            .find(|(object_ref, _)| object_ref.0 == self.transfer_object.0)
            .map(|x| x.0)
            .unwrap();
        self.transfer_from = self.transfer_to;
        self.transfer_to = recipient;
        self.gas = updated_gas;
    }
    fn make_transaction(&mut self) -> VerifiedTransaction {
        let (gas_obj, _, keypair) = self.gas.iter().find(|x| x.1 == self.transfer_from).unwrap();
        make_transfer_object_transaction(
            self.transfer_object,
            *gas_obj,
            self.transfer_from,
            keypair,
            self.transfer_to,
            Some(*self.system_state_observer.reference_gas_price.borrow()),
        )
    }
    fn workload_type(&self) -> WorkloadType {
        WorkloadType::TransferObject
    }
}

#[derive(Debug)]
pub struct TransferObjectWorkloadBuilder {
    num_transfer_accounts: u64,
    num_payloads: u64,
}

pub fn transfer_object_initializer(
    max_ops: u64,
    parameters: &HashMap<WorkloadInitParameter, u32>,
) -> Box<dyn WorkloadBuilder<dyn Payload>> {
    let num_transfer_accounts = *parameters
        .get(&WorkloadInitParameter::NumTransferAccounts)
        .unwrap_or(&2);
    Box::new(TransferObjectWorkloadBuilder {
        num_transfer_accounts: num_transfer_accounts.into(),
        num_payloads: max_ops,
    })
}

#[async_trait]
impl WorkloadBuilder<dyn Payload> for TransferObjectWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        vec![]
    }
    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig> {
        let mut address_map = HashMap::new();

        // gas for payloads
        let mut payload_configs = vec![];
        for _i in 0..self.num_transfer_accounts {
            let (address, keypair) = get_key_pair();
            let cloned_keypair: Arc<AccountKeyPair> = Arc::new(keypair);
            address_map.insert(address, cloned_keypair.clone());
            for _j in 0..self.num_payloads {
                payload_configs.push(GasCoinConfig {
                    amount: MAX_GAS_FOR_TESTING,
                    address,
                    keypair: cloned_keypair.clone(),
                });
            }
        }

        let owner = *address_map.keys().choose(&mut rand::thread_rng()).unwrap();

        // transfer tokens
        let mut gas_configs = vec![];
        for _i in 0..self.num_payloads {
            let (address, keypair) = (owner, address_map.get(&owner).unwrap().clone());
            gas_configs.push(GasCoinConfig {
                amount: MAX_GAS_FOR_TESTING,
                address,
                keypair: keypair.clone(),
            });
        }

        gas_configs.extend(payload_configs);
        gas_configs
    }
    async fn build(
        &self,
        _init_gas: Vec<Gas>,
        payload_gas: Vec<Gas>,
    ) -> Box<dyn Workload<dyn Payload>> {
        Box::<dyn Workload<dyn Payload>>::from(Box::new(TransferObjectWorkload {
            num_tokens: self.num_payloads,
            payload_gas,
        }))
    }
}

#[derive(Debug)]
pub struct TransferObjectWorkload {
    num_tokens: u64,
    payload_gas: Vec<Gas>,
}

#[async_trait]
impl Workload<dyn Payload> for TransferObjectWorkload {
    async fn init(
        &mut self,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        _system_state_observer: Arc<SystemStateObserver>,
    ) {
        return;
    }
    async fn make_test_payloads(
        &self,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        let (transfer_tokens, payload_gas) = self.payload_gas.split_at(self.num_tokens as usize);
        let mut gas_by_address: HashMap<SuiAddress, Vec<Gas>> = HashMap::new();
        for gas in payload_gas.iter() {
            gas_by_address
                .entry(gas.1)
                .or_insert_with(|| Vec::with_capacity(1))
                .push(gas.clone());
        }

        let addresses: Vec<SuiAddress> = gas_by_address.keys().cloned().collect();
        let mut transfer_gas: Vec<Vec<Gas>> = vec![];
        for i in 0..self.num_tokens {
            let mut account_transfer_gas = vec![];
            for address in addresses.iter() {
                account_transfer_gas.push(gas_by_address[address][i as usize].clone());
            }
            transfer_gas.push(account_transfer_gas);
        }
        let refs: Vec<(Vec<Gas>, Gas)> = transfer_gas
            .into_iter()
            .zip(transfer_tokens.iter())
            .map(|(g, t)| (g, t.clone()))
            .collect();
        refs.iter()
            .map(|(g, t)| {
                let from = t.1;
                let to = g.iter().find(|x| x.1 != from).unwrap().1;
                Box::new(TransferObjectTestPayload {
                    transfer_object: t.0,
                    transfer_from: from,
                    transfer_to: to,
                    gas: g.to_vec(),
                    system_state_observer: system_state_observer.clone(),
                })
            })
            .map(|b| Box::<dyn Payload>::from(b))
            .collect()
    }
}
