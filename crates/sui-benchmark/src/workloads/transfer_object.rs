// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use rand::seq::IteratorRandom;
use tracing::error;

use std::collections::HashMap;
use std::sync::Arc;

use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::Payload;
use crate::workloads::workload::WorkloadBuilder;
use crate::workloads::workload::{
    Workload, ESTIMATED_COMPUTATION_COST, MAX_GAS_FOR_TESTING, STORAGE_COST_PER_COIN,
};
use crate::workloads::{Gas, GasCoinConfig, WorkloadBuilderInfo, WorkloadParams};
use crate::{ExecutionEffects, ValidatorProxy};
use sui_core::test_utils::make_transfer_object_transaction;
use sui_types::{
    base_types::{ObjectRef, SuiAddress},
    crypto::{get_key_pair, AccountKeyPair},
    messages::VerifiedTransaction,
};

/// TODO: This should be the amount that is being transferred instead of MAX_GAS.
/// Number of mist sent to each address on each batch transfer
const _TRANSFER_AMOUNT: u64 = 1;

#[derive(Debug)]
pub struct TransferObjectTestPayload {
    transfer_object: ObjectRef,
    transfer_from: SuiAddress,
    transfer_to: SuiAddress,
    gas: Vec<Gas>,
    system_state_observer: Arc<SystemStateObserver>,
    health_check_enabled: bool,
}

impl Payload for TransferObjectTestPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        if !effects.is_ok() {
            effects.print_gas_summary();
            error!("Transfer tx failed...");
        }

        let recipient = self
            .gas
            .iter()
            .find(|x| self.health_check_enabled || x.1 != self.transfer_to)
            .unwrap()
            .1;
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
            self.system_state_observer
                .state
                .borrow()
                .reference_gas_price,
        )
    }
}

impl std::fmt::Display for TransferObjectTestPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "transfer_object")
    }
}

#[derive(Debug)]
pub struct TransferObjectWorkloadBuilder {
    num_transfer_accounts: u64,
    num_payloads: u64,
    account: Option<(SuiAddress, Arc<AccountKeyPair>)>,
    health_check_enabled: bool,
}

impl TransferObjectWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        num_transfer_accounts: u64,
        account: Option<(SuiAddress, Arc<AccountKeyPair>)>,
        health_check_enabled: bool,
    ) -> Option<WorkloadBuilderInfo> {
        let target_qps = (workload_weight * target_qps as f32) as u64;
        let num_workers = (workload_weight * num_workers as f32).ceil() as u64;
        let max_ops = target_qps * in_flight_ratio;
        if max_ops == 0 || num_workers == 0 {
            None
        } else {
            let workload_params = WorkloadParams {
                target_qps,
                num_workers,
                max_ops,
            };
            let workload_builder = Box::<dyn WorkloadBuilder<dyn Payload>>::from(Box::new(
                TransferObjectWorkloadBuilder {
                    num_transfer_accounts,
                    num_payloads: max_ops,
                    account,
                    health_check_enabled,
                },
            ));
            let builder_info = WorkloadBuilderInfo {
                workload_params,
                workload_builder,
            };
            Some(builder_info)
        }
    }
}

#[async_trait]
impl WorkloadBuilder<dyn Payload> for TransferObjectWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        vec![]
    }
    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig> {
        let mut address_map = HashMap::new();
        // Have to include not just the coins that are going to be created and sent
        // but the coin being used as gas as well.
        let mut amount =
            ESTIMATED_COMPUTATION_COST + STORAGE_COST_PER_COIN * (self.num_transfer_accounts + 1);
        if !self.health_check_enabled {
            amount = amount + MAX_GAS_FOR_TESTING;
        }
        // gas for payloads
        let mut payload_configs = vec![];

        // Sending to a specific account used for health checks where we are
        // okay sending the coin to ourselves.
        let account = if self.health_check_enabled && self.account.is_some() {
            Some(self.account.clone().unwrap())
        } else {
            None
        };

        for _ in 0..self.num_transfer_accounts {
            let (address, keypair) = account.as_ref().map_or_else(
                || {
                    let (address, keypair) = get_key_pair();
                    (address, Arc::new(keypair))
                },
                |(a, k)| (a.clone(), k.clone()),
            );
            address_map.insert(address, keypair.clone());

            for _ in 0..self.num_payloads {
                payload_configs.push(GasCoinConfig {
                    amount,
                    address,
                    keypair: keypair.clone(),
                });
            }
        }

        let owner = *address_map.keys().choose(&mut rand::thread_rng()).unwrap();

        // transfer tokens
        let mut gas_configs = vec![];
        for _i in 0..self.num_payloads {
            let (address, keypair) = (owner, address_map.get(&owner).unwrap().clone());
            gas_configs.push(GasCoinConfig {
                amount,
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
            health_check_enabled: self.health_check_enabled,
        }))
    }
}

#[derive(Debug)]
pub struct TransferObjectWorkload {
    num_tokens: u64,
    payload_gas: Vec<Gas>,
    health_check_enabled: bool,
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
                let to = g
                    .iter()
                    .find(|x| self.health_check_enabled || x.1 != from)
                    .unwrap()
                    .1;
                Box::new(TransferObjectTestPayload {
                    transfer_object: t.0,
                    transfer_from: from,
                    transfer_to: to,
                    gas: g.to_vec(),
                    system_state_observer: system_state_observer.clone(),
                    health_check_enabled: self.health_check_enabled,
                })
            })
            .map(|b| Box::<dyn Payload>::from(b))
            .collect()
    }
}
