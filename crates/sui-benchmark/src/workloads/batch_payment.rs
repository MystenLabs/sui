// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::drivers::Interval;
use crate::in_memory_wallet::InMemoryWallet;
use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::Payload;
use crate::workloads::workload::{ExpectedFailureType, Workload, STORAGE_COST_PER_COIN};
use crate::workloads::workload::{WorkloadBuilder, ESTIMATED_COMPUTATION_COST};
use crate::workloads::{Gas, GasCoinConfig, WorkloadBuilderInfo, WorkloadParams};
use crate::{ExecutionEffects, ValidatorProxy};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use sui_core::test_utils::make_pay_sui_transaction;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::digests::ObjectDigest;
use sui_types::gas_coin::MIST_PER_SUI;
use sui_types::object::Owner;
use sui_types::{
    base_types::{ObjectRef, SuiAddress},
    crypto::get_key_pair,
    transaction::Transaction,
};
use tracing::{debug, error};

/// Value of each address's "primary coin" in mist. The first transaction gives
/// each address a coin worth PRIMARY_COIN_VALUE, and all subsequent transfers
/// send TRANSFER_AMOUNT coins each time
const PRIMARY_COIN_VALUE: u64 = 100 * MIST_PER_SUI;

/// Number of mist sent to each address on each batch transfer
const BATCH_TRANSFER_AMOUNT: u64 = 1;

const DUMMY_GAS: ObjectRef = (ObjectID::ZERO, SequenceNumber::MIN, ObjectDigest::MIN);

#[derive(Debug)]
pub struct BatchPaymentTestPayload {
    state: InMemoryWallet,
    /// total number of payments made, to be used in reporting batch TPS
    num_payments: usize,
    /// address of the first sender. important because in the beginning, only one address has any coins.
    /// after the first tx, any address can send
    first_sender: SuiAddress,
    system_state_observer: Arc<SystemStateObserver>,
}

impl std::fmt::Display for BatchPaymentTestPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "batch_payment")
    }
}

impl Payload for BatchPaymentTestPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        if !effects.is_ok() {
            effects.print_gas_summary();
            error!("Batch payment failed... Status: {:?}", effects.status());
        }

        self.state.update(effects);
        if self.num_payments == 0 {
            for (coin_obj, owner) in effects.created().into_iter().chain(effects.mutated()) {
                if let Owner::AddressOwner(addr) = owner {
                    self.state.account_mut(&addr).unwrap().gas = coin_obj;
                } else {
                    unreachable!("Initial payment should only send to addresses")
                }
            }
        }
        self.num_payments += self.state.num_addresses();
    }

    fn make_transaction(&mut self) -> Transaction {
        let addrs = self.state.addresses().cloned().collect::<Vec<SuiAddress>>();
        let num_recipients = addrs.len();
        let sender = if self.num_payments == 0 {
            // first tx--use the address that has gas
            debug!("First sender sending gas {}...", self.first_sender);
            self.first_sender
        } else {
            // everyone has gas now, round-robin the senders
            let addr = addrs[self.num_payments % num_recipients];
            debug!("New sender sending gas {}...", addr);
            addr
        };
        // we're only using gas objects in this benchmark, so safe to assume everything owned by an address is a gas object
        let gas_obj = self.state.gas(&sender).unwrap();
        debug!("Gas ID being used for tx {gas_obj:#?}");
        let amount = if self.num_payments == 0 {
            PRIMARY_COIN_VALUE
        } else {
            BATCH_TRANSFER_AMOUNT
        };
        debug!("Sending {amount} to {num_recipients} recipients");

        // Have to include not just the coins that are going to be created and sent
        // but the coin being used as gas as well.
        let gas_budget =
            ESTIMATED_COMPUTATION_COST + (STORAGE_COST_PER_COIN * (num_recipients + 1) as u64);
        // pay everything from the gas object, no other coins
        let coins = Vec::new();
        // create a sender -> all transfer, using all of the sender's coins
        // TODO: use a larger amount, fewer input coins?
        make_pay_sui_transaction(
            *gas_obj,
            coins,
            addrs,
            vec![amount; num_recipients],
            sender,
            &self.state.keypair(&sender).unwrap(),
            self.system_state_observer
                .state
                .borrow()
                .reference_gas_price,
            gas_budget,
        )
    }

    fn get_failure_type(&self) -> Option<ExpectedFailureType> {
        None
    }
}

#[derive(Debug)]
pub struct BatchPaymentWorkloadBuilder {
    num_payloads: u64,
    batch_size: u32,
}

impl BatchPaymentWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        batch_size: u32,
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
                BatchPaymentWorkloadBuilder {
                    num_payloads: max_ops,
                    batch_size,
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
impl WorkloadBuilder<dyn Payload> for BatchPaymentWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        vec![]
    }
    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig> {
        // Have to include not just the coins that are going to be created and sent
        // but the coin being used as gas as well.
        let amount = (PRIMARY_COIN_VALUE * (self.batch_size + 1) as u64)
            + ESTIMATED_COMPUTATION_COST
            + (STORAGE_COST_PER_COIN * self.batch_size as u64);
        debug!(
            "Creating gas coins for batch payload {} coin(s) of balance {amount}",
            self.num_payloads
        );
        (0..self.num_payloads)
            .map(|_| {
                let (address, keypair) = get_key_pair();
                GasCoinConfig {
                    amount,
                    address,
                    keypair: Arc::new(keypair),
                }
            })
            .collect()
    }
    async fn build(
        &self,
        _init_gas: Vec<Gas>,
        payload_gas: Vec<Gas>,
    ) -> Box<dyn Workload<dyn Payload>> {
        Box::<dyn Workload<dyn Payload>>::from(Box::new(BatchPaymentWorkload {
            payload_gas,
            batch_size: self.batch_size,
        }))
    }
}

#[derive(Debug, Default)]
pub struct BatchPaymentWorkload {
    payload_gas: Vec<Gas>,
    batch_size: u32,
}

#[async_trait]
impl Workload<dyn Payload> for BatchPaymentWorkload {
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
        let mut gas_by_address: HashMap<SuiAddress, Vec<Gas>> = HashMap::new();
        debug!(
            "Making test payloads with {} payload gas...",
            self.payload_gas.len()
        );
        for gas in self.payload_gas.iter() {
            gas_by_address
                .entry(gas.1)
                .or_insert_with(|| Vec::with_capacity(1))
                .push(gas.clone());
        }
        assert_eq!(
            gas_by_address.len(),
            self.payload_gas.len(),
            "Each sender needs some gas"
        );

        let mut payloads = Vec::new();

        debug!(
            "Creating {} senders & {} recipients per sender...",
            gas_by_address.len(),
            self.batch_size
        );

        for (addr, gas) in gas_by_address {
            let mut state = InMemoryWallet::default();
            let key = gas[0].2.clone();
            let mut objs: Vec<ObjectRef> = gas.into_iter().map(|g| g.0).collect();
            let gas_coin = objs.pop().unwrap();
            state.add_account(addr, key, gas_coin, objs);
            // add empty accounts for `addr` to transfer to
            for _ in 0..self.batch_size - 1 {
                let (a, key) = get_key_pair();
                // we'll replace this after the first send
                let gas = DUMMY_GAS;
                state.add_account(a, Arc::new(key), gas, Vec::new());
            }
            payloads.push(Box::new(BatchPaymentTestPayload {
                state,
                num_payments: 0,
                first_sender: addr,
                system_state_observer: system_state_observer.clone(),
            }));
        }
        payloads
            .into_iter()
            .map(|b| Box::<dyn Payload>::from(b))
            .collect()
    }
}
