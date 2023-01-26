// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::Payload;
use crate::workloads::workload::{Workload, WorkloadType, MAX_GAS_FOR_TESTING};
use crate::workloads::{GasCoinConfig, WorkloadInitGas, WorkloadPayloadGas};
use crate::{ExecutionEffects, ValidatorProxy};
use async_trait::async_trait;
use rand::seq::IteratorRandom;
use std::sync::Arc;
use sui_core::test_utils::make_transfer_sui_transaction;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::crypto::{get_key_pair, AccountKeyPair};
use sui_types::messages::VerifiedTransaction;
use sui_types::SUI_FRAMEWORK_OBJECT_ID;
use test_utils::messages::make_delegation_transaction;

pub struct DelegationTestPayload {
    system_package_ref: ObjectRef,
    coin: Option<ObjectRef>,
    gas: ObjectRef,
    validator: SuiAddress,
    sender: SuiAddress,
    keypair: Arc<AccountKeyPair>,
    system_state_observer: Arc<SystemStateObserver>,
}

impl Payload for DelegationTestPayload {
    /// delegation flow is split into two phases
    /// first `make_transaction` call creates separate coin object for future delegation
    /// followup call creates delegation transaction itself
    fn make_transaction(&self) -> VerifiedTransaction {
        match self.coin {
            Some(coin) => make_delegation_transaction(
                self.gas,
                coin,
                self.system_package_ref,
                self.validator,
                self.sender,
                &self.keypair,
                Some(*self.system_state_observer.reference_gas_price.borrow()),
            ),
            None => make_transfer_sui_transaction(
                self.gas,
                self.sender,
                Some(1),
                self.sender,
                &self.keypair,
                Some(*self.system_state_observer.reference_gas_price.borrow()),
            ),
        }
    }

    fn make_new_payload(
        self: Box<Self>,
        _: ObjectRef,
        new_gas: ObjectRef,
        effects: &ExecutionEffects,
    ) -> Box<dyn Payload> {
        let coin = match self.coin {
            None => Some(effects.created().get(0).unwrap().0),
            Some(_) => None,
        };
        Box::new(DelegationTestPayload {
            system_package_ref: self.system_package_ref,
            coin,
            gas: new_gas,
            validator: self.validator,
            sender: self.sender,
            keypair: self.keypair,
            system_state_observer: self.system_state_observer,
        })
    }

    fn get_object_id(&self) -> ObjectID {
        self.gas.0
    }

    fn get_workload_type(&self) -> WorkloadType {
        WorkloadType::Delegation
    }
}

pub struct DelegationWorkload;

impl DelegationWorkload {
    pub fn new_boxed() -> Box<dyn Workload<dyn Payload>> {
        Box::<dyn Workload<dyn Payload>>::from(Box::new(DelegationWorkload))
    }

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
impl Workload<dyn Payload> for DelegationWorkload {
    async fn init(
        &mut self,
        _: WorkloadInitGas,
        _: Arc<dyn ValidatorProxy + Sync + Send>,
        _system_state_observer: Arc<SystemStateObserver>,
    ) {
    }

    async fn make_test_payloads(
        &self,
        _num_payloads: u64,
        gas_config: WorkloadPayloadGas,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        let system_package = proxy.get_object(SUI_FRAMEWORK_OBJECT_ID).await;
        let system_package_ref = system_package.unwrap().compute_object_reference();
        let validators = proxy
            .get_validators()
            .await
            .expect("failed to fetch validators");

        gas_config
            .delegation_payload_gas
            .into_iter()
            .map(|(gas, owner, keypair)| {
                let validator = *validators.iter().choose(&mut rand::thread_rng()).unwrap();
                Box::new(DelegationTestPayload {
                    system_package_ref,
                    coin: None,
                    gas,
                    validator,
                    sender: owner.get_owner_address().unwrap(),
                    keypair,
                    system_state_observer: system_state_observer.clone(),
                })
            })
            .map(|b| Box::<dyn Payload>::from(b))
            .collect()
    }

    fn get_workload_type(&self) -> WorkloadType {
        WorkloadType::Delegation
    }
}
