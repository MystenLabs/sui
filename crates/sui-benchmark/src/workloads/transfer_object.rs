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
    object::Owner,
};

use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::Payload;
use crate::workloads::{Gas, GasCoinConfig, WorkloadInitGas, WorkloadPayloadGas};
use crate::{ExecutionEffects, ValidatorProxy};
use sui_core::test_utils::make_transfer_object_transaction;

use super::workload::{Workload, WorkloadType, MAX_GAS_FOR_TESTING};

#[derive(Debug)]
pub struct TransferObjectTestPayload {
    transfer_object: ObjectRef,
    transfer_from: SuiAddress,
    transfer_to: SuiAddress,
    gas: Vec<Gas>,
    system_state_observer: Arc<SystemStateObserver>,
}

impl Payload for TransferObjectTestPayload {
    fn make_new_payload(self: Box<Self>, effects: &ExecutionEffects) -> Box<dyn Payload> {
        let recipient = self
            .gas
            .iter()
            .find(|x| x.1.get_owner_address().unwrap() != self.transfer_to)
            .unwrap()
            .1;
        let updated_gas: Vec<Gas> = self
            .gas
            .into_iter()
            .map(|x| {
                if x.1.get_owner_address().unwrap() == self.transfer_from {
                    (
                        effects.gas_object().0,
                        Owner::AddressOwner(self.transfer_from),
                        x.2,
                    )
                } else {
                    x
                }
            })
            .collect();
        Box::new(TransferObjectTestPayload {
            transfer_object: effects
                .mutated()
                .iter()
                .find(|(object_ref, _)| object_ref.0 == self.transfer_object.0)
                .map(|x| x.0)
                .unwrap(),
            transfer_from: self.transfer_to,
            transfer_to: recipient.get_owner_address().unwrap(),
            gas: updated_gas,
            system_state_observer: self.system_state_observer,
        })
    }
    fn make_transaction(&self) -> VerifiedTransaction {
        let (gas_obj, _, keypair) = self
            .gas
            .iter()
            .find(|x| x.1.get_owner_address().unwrap() == self.transfer_from)
            .unwrap();
        make_transfer_object_transaction(
            self.transfer_object,
            *gas_obj,
            self.transfer_from,
            keypair,
            self.transfer_to,
            Some(*self.system_state_observer.reference_gas_price.borrow()),
        )
    }
    fn get_workload_type(&self) -> WorkloadType {
        WorkloadType::TransferObject
    }
}

#[derive(Debug)]
pub struct TransferObjectWorkload {
    pub transfer_keypairs: Arc<HashMap<SuiAddress, AccountKeyPair>>,
}

impl TransferObjectWorkload {
    pub fn new_boxed(num_accounts: u64) -> Box<dyn Workload<dyn Payload>> {
        // create several accounts to transfer object between
        let keypairs: Arc<HashMap<SuiAddress, AccountKeyPair>> =
            Arc::new((0..num_accounts).map(|_| get_key_pair()).collect());
        Box::new(TransferObjectWorkload {
            transfer_keypairs: keypairs,
        })
    }
    pub fn generate_coin_config_for_payloads(
        num_tokens: u64,
        num_transfer_accounts: u64,
        num_payloads: u64,
    ) -> (Vec<GasCoinConfig>, Vec<GasCoinConfig>) {
        let mut address_map = HashMap::new();

        // gas for payloads
        let mut payload_configs = vec![];
        for _i in 0..num_transfer_accounts {
            let (address, keypair) = get_key_pair();
            let cloned_keypair: Arc<AccountKeyPair> = Arc::new(keypair);
            address_map.insert(address, cloned_keypair.clone());
            for _j in 0..num_payloads {
                payload_configs.push(GasCoinConfig {
                    amount: MAX_GAS_FOR_TESTING,
                    address,
                    keypair: cloned_keypair.clone(),
                });
            }
        }

        let owner = *address_map.keys().choose(&mut rand::thread_rng()).unwrap();

        // transfer tokens
        let mut token_configs = vec![];
        for _i in 0..num_tokens {
            let (address, keypair) = (owner, address_map.get(&owner).unwrap().clone());
            token_configs.push(GasCoinConfig {
                amount: MAX_GAS_FOR_TESTING,
                address,
                keypair: keypair.clone(),
            });
        }

        (token_configs, payload_configs)
    }
}

#[async_trait]
impl Workload<dyn Payload> for TransferObjectWorkload {
    async fn init(
        &mut self,
        _init_config: WorkloadInitGas,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        _system_state_observer: Arc<SystemStateObserver>,
    ) {
        return;
    }
    async fn make_test_payloads(
        &self,
        num_payloads: u64,
        payload_config: WorkloadPayloadGas,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        let mut gas_by_address: HashMap<SuiAddress, Vec<Gas>> = HashMap::new();
        for gas in payload_config.transfer_object_payload_gas.iter() {
            gas_by_address
                .entry(gas.1.get_owner_address().unwrap())
                .or_insert_with(|| Vec::with_capacity(1))
                .push(gas.clone());
        }

        let addresses: Vec<SuiAddress> = gas_by_address.keys().cloned().collect();
        let mut transfer_gas: Vec<Vec<Gas>> = vec![];
        for i in 0..num_payloads {
            let mut account_transfer_gas = vec![];
            for address in addresses.iter() {
                account_transfer_gas.push(gas_by_address[address][i as usize].clone());
            }
            transfer_gas.push(account_transfer_gas);
        }
        let refs: Vec<(Vec<Gas>, Gas)> = transfer_gas
            .into_iter()
            .zip(payload_config.transfer_tokens.into_iter())
            .map(|(g, t)| (g, t))
            .collect();
        refs.iter()
            .map(|(g, t)| {
                let from = t.1;
                let to = g.iter().find(|x| x.1 != from).unwrap().1;
                Box::new(TransferObjectTestPayload {
                    transfer_object: t.0,
                    transfer_from: from.get_owner_address().unwrap(),
                    transfer_to: to.get_owner_address().unwrap(),
                    gas: g.to_vec(),
                    system_state_observer: system_state_observer.clone(),
                })
            })
            .map(|b| Box::<dyn Payload>::from(b))
            .collect()
    }
    fn get_workload_type(&self) -> WorkloadType {
        WorkloadType::TransferObject
    }

    fn debug(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self as &TransferObjectWorkload)
    }
}
