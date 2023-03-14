// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use sui_types::base_types::ObjectID;
use sui_types::object::Owner;

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use sui_types::{
    base_types::{ObjectRef, SuiAddress},
    crypto::get_key_pair,
    messages::VerifiedTransaction,
};

use crate::in_memory_wallet::InMemoryWallet;
use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::Payload;
use crate::workloads::{Gas, GasCoinConfig, WorkloadInitGas, WorkloadPayloadGas};
use crate::{ExecutionEffects, ValidatorProxy};
use sui_core::test_utils::make_pay_sui_transaction;

use super::workload::{Workload, WorkloadType, MAX_GAS_FOR_TESTING};

/// Value of each address's "primary coin" in mist. The first transaction gives
/// each address a coin worth PRIMARY_COIN_VALUE, and all subsequent transfers
/// send TRANSFER_AMOUNT coins each time
const PRIMARY_COIN_VALUE: u64 = 10_000_000;

/// Number of mist sent to each address on each batch transfer
const TRANSFER_AMOUNT: u64 = 1;

// TODO: make this configurable via CLI
/// Number of payments in each batch
const BATCH_SIZE: u64 = 15;

#[derive(Debug)]
pub struct BatchPaymentTestPayload {
    state: InMemoryWallet,
    // largest value coins owned by each address
    primary_coins: BTreeMap<SuiAddress, ObjectID>,
    /// total number of payments made, to be used in reporting batch TPS
    num_payments: usize,
    /// address of the first sender. important because in the beginning, only one address has any coins.
    /// after the first tx, any address can send
    first_sender: SuiAddress,
    system_state_observer: Arc<SystemStateObserver>,
}

impl Payload for BatchPaymentTestPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        self.state.update(effects);
        if self.num_payments == 0 {
            for (coin_obj, owner) in effects.created().into_iter().chain(effects.mutated()) {
                if let Owner::AddressOwner(addr) = owner {
                    self.primary_coins.insert(addr, coin_obj.0);
                } else {
                    unreachable!("Initial payment should only send to addresses")
                }
            }
        }
        self.num_payments += self.state.num_addresses();
    }

    fn make_transaction(&mut self) -> VerifiedTransaction {
        let addrs = self.state.addresses().cloned().collect::<Vec<SuiAddress>>();
        let num_recipients = addrs.len();
        let sender = if self.num_payments == 0 {
            // first tx--use the address that has gas
            self.first_sender
        } else {
            // everyone has gas now, round-robin the senders
            addrs[self.num_payments % num_recipients]
        };
        // we're only using gas objects in this benchmark, so safe to assume everything owned by an address is a gas object
        let gas_obj = self
            .state
            .owned_object(&sender, self.primary_coins.get(&sender).unwrap())
            .unwrap();
        let amount = if self.num_payments == 0 {
            PRIMARY_COIN_VALUE
        } else {
            TRANSFER_AMOUNT
        };
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
            Some(*self.system_state_observer.reference_gas_price.borrow()),
        )
    }

    fn get_workload_type(&self) -> WorkloadType {
        WorkloadType::BatchPayment
    }
}

#[derive(Debug, Default)]
pub struct BatchPaymentWorkload {}

impl BatchPaymentWorkload {
    pub fn generate_gas_config_for_payloads(count: u64) -> Vec<GasCoinConfig> {
        (0..count)
            .map(|_| {
                let (address, keypair) = get_key_pair();
                GasCoinConfig {
                    amount: MAX_GAS_FOR_TESTING,
                    address,
                    keypair: Arc::new(keypair),
                }
            })
            .collect()
    }
}

#[async_trait]
impl Workload<dyn Payload> for BatchPaymentWorkload {
    async fn init(
        &mut self,
        _init_config: WorkloadInitGas,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        _system_state_observer: Arc<SystemStateObserver>,
    ) {
    }

    async fn make_test_payloads(
        &self,
        num_payloads: u64,
        payload_config: WorkloadPayloadGas,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        let mut gas_by_address: HashMap<SuiAddress, Vec<Gas>> = HashMap::new();
        for gas in payload_config.batch_payment_payload_gas.into_iter() {
            gas_by_address
                .entry(gas.1.get_owner_address().unwrap())
                .or_insert_with(|| Vec::with_capacity(1))
                .push(gas);
        }
        assert!(
            gas_by_address.len() as u64 == num_payloads,
            "Each sender needs some gas"
        );

        let mut payloads = Vec::new();
        for (addr, gas) in gas_by_address {
            let mut state = InMemoryWallet::default();
            let key = gas[0].2.clone();
            let gas_objs: Vec<ObjectRef> = gas.into_iter().map(|g| g.0).collect();
            let primary_coin = gas_objs[0].0;
            state.add_account(addr, key, gas_objs);
            let mut primary_coins = BTreeMap::new();
            primary_coins.insert(addr, primary_coin);
            // add empty accounts for `addr` to transfer to
            for _ in 0..BATCH_SIZE - 1 {
                let (a, key) = get_key_pair();
                state.add_account(a, Arc::new(key), Vec::new());
            }
            payloads.push(Box::new(BatchPaymentTestPayload {
                state,
                num_payments: 0,
                first_sender: addr,
                primary_coins,
                system_state_observer: system_state_observer.clone(),
            }));
        }
        payloads
            .into_iter()
            .map(|b| Box::<dyn Payload>::from(b))
            .collect()
    }

    fn get_workload_type(&self) -> WorkloadType {
        WorkloadType::BatchPayment
    }
}
