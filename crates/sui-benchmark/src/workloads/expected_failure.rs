// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::drivers::Interval;
use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::Payload;
use crate::workloads::workload::{
    ExpectedFailureType, WorkloadBuilder, ESTIMATED_COMPUTATION_COST, MAX_GAS_FOR_TESTING,
    STORAGE_COST_PER_COIN,
};
use crate::workloads::{Gas, GasCoinConfig, Workload, WorkloadBuilderInfo, WorkloadParams};
use crate::ExecutionEffects;
use crate::ValidatorProxy;
use async_trait::async_trait;
use rand::seq::IteratorRandom;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use sui_core::test_utils::make_transfer_object_transaction;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{AccountKeyPair, Ed25519SuiSignature};
use sui_types::signature::GenericSignature;
use sui_types::{base_types::ObjectRef, crypto::get_key_pair, transaction::Transaction};
use tracing::debug;

#[derive(Debug, Clone)]
pub struct ExpectedFailurePayload {
    failure_type: ExpectedFailureType,
    transfer_object: ObjectRef,
    transfer_from: SuiAddress,
    transfer_to: SuiAddress,
    gas: Vec<Gas>,
    system_state_observer: Arc<SystemStateObserver>,
}

#[derive(Debug, Clone)]
pub struct ExpectedFailurePayloadCfg {
    pub failure_type: ExpectedFailureType,
}

impl Copy for ExpectedFailurePayloadCfg {}

impl ExpectedFailurePayload {
    fn create_failing_transaction(&self, mut tx: Transaction) -> Transaction {
        match self.failure_type {
            ExpectedFailureType::InvalidSignature => {
                let signatures = tx.tx_signatures_mut_for_testing();
                signatures.pop();
                signatures.push(GenericSignature::Signature(
                    sui_types::crypto::Signature::Ed25519SuiSignature(
                        Ed25519SuiSignature::default(),
                    ),
                ));
                tx
            }
            ExpectedFailureType::Random => unreachable!(),
            ExpectedFailureType::NoFailure => unreachable!(),
        }
    }
}

impl Payload for ExpectedFailurePayload {
    fn make_new_payload(&mut self, _effects: &ExecutionEffects) {
        // This should never be called, as an expected failure payload
        // should fail (thereby having no effects) and be retried. Note
        // that since these are failures rather than Move level errors,
        // no gas should be consumed, nor any objects mutated.
        unreachable!()
    }

    fn make_transaction(&mut self) -> Transaction {
        let (gas_obj, _, keypair) = self.gas.iter().find(|x| x.1 == self.transfer_from).unwrap();
        let tx = make_transfer_object_transaction(
            self.transfer_object,
            *gas_obj,
            self.transfer_from,
            keypair,
            self.transfer_to,
            self.system_state_observer
                .state
                .borrow()
                .reference_gas_price,
        );
        self.create_failing_transaction(tx)
    }

    fn get_failure_type(&self) -> Option<ExpectedFailureType> {
        Some(self.failure_type)
    }
}

impl fmt::Display for ExpectedFailurePayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ExpectedFailurePayload({:?})", self.failure_type)
    }
}

#[derive(Debug)]
pub struct ExpectedFailureWorkloadBuilder {
    expected_failure_cfg: ExpectedFailurePayloadCfg,
    num_transfer_accounts: u64,
    num_payloads: u64,
}

impl ExpectedFailureWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        num_transfer_accounts: u64,
        expected_failure_cfg: ExpectedFailurePayloadCfg,
        duration: Interval,
        group: u32,
    ) -> Option<WorkloadBuilderInfo> {
        let target_qps = (workload_weight * target_qps as f32).ceil() as u64;
        let num_workers = (workload_weight * num_workers as f32).ceil() as u64;
        let max_ops = target_qps * in_flight_ratio;
        if max_ops == 0 || num_workers == 0 {
            None
        } else {
            let workload_params = WorkloadParams {
                target_qps,
                num_workers,
                max_ops,
                duration,
                group,
            };
            let workload_builder = Box::<dyn WorkloadBuilder<dyn Payload>>::from(Box::new(
                ExpectedFailureWorkloadBuilder {
                    expected_failure_cfg,
                    num_payloads: max_ops,
                    num_transfer_accounts,
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
impl WorkloadBuilder<dyn Payload> for ExpectedFailureWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        vec![]
    }
    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig> {
        let mut address_map = HashMap::new();
        // Have to include not just the coins that are going to be created and sent
        // but the coin being used as gas as well.
        let amount = MAX_GAS_FOR_TESTING
            + ESTIMATED_COMPUTATION_COST
            + STORAGE_COST_PER_COIN * (self.num_transfer_accounts + 1);
        // gas for payloads
        let mut payload_configs = vec![];
        for _i in 0..self.num_transfer_accounts {
            let (address, keypair) = get_key_pair();
            let cloned_keypair: Arc<AccountKeyPair> = Arc::new(keypair);
            address_map.insert(address, cloned_keypair.clone());
            for _j in 0..self.num_payloads {
                payload_configs.push(GasCoinConfig {
                    amount,
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
        debug!(
            "Using `{:?}` expected failure workloads",
            self.expected_failure_cfg.failure_type,
        );

        Box::<dyn Workload<dyn Payload>>::from(Box::new(ExpectedFailureWorkload {
            num_tokens: self.num_payloads,
            payload_gas,
            expected_failure_cfg: self.expected_failure_cfg,
        }))
    }
}

#[derive(Debug)]
pub struct ExpectedFailureWorkload {
    num_tokens: u64,
    payload_gas: Vec<Gas>,
    expected_failure_cfg: ExpectedFailurePayloadCfg,
}

#[async_trait]
impl Workload<dyn Payload> for ExpectedFailureWorkload {
    async fn init(
        &mut self,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        _system_state_observer: Arc<SystemStateObserver>,
    ) {
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
                Box::new(ExpectedFailurePayload {
                    failure_type: self.expected_failure_cfg.failure_type,
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
