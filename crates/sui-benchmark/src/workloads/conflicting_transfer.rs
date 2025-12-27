// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Conflicting Transfer Workload
//!
//! This workload creates contention by having multiple payloads share the same transfer object.
//! When multiple workers try to transfer the same object concurrently, one will succeed and
//! others will fail with ObjectLockConflict errors.
//!
//! This is useful for stress testing the system's conflict detection and resolution mechanisms.

use async_trait::async_trait;
use std::sync::Arc;
use tracing::info;

use crate::drivers::Interval;
use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::Payload;
use crate::workloads::workload::WorkloadBuilder;
use crate::workloads::workload::{
    ESTIMATED_COMPUTATION_COST, ExpectedFailureType, MAX_GAS_FOR_TESTING, STORAGE_COST_PER_COIN,
    Workload,
};
use crate::workloads::{Gas, GasCoinConfig, WorkloadBuilderInfo, WorkloadParams};
use crate::{ExecutionEffects, ValidatorProxy};
use sui_core::test_utils::make_transfer_object_transaction;
use sui_types::{
    base_types::{ObjectRef, SuiAddress},
    crypto::{AccountKeyPair, get_key_pair},
    transaction::Transaction,
};

/// Number of payloads that share each contested object.
/// Higher values create more contention.
const PAYLOADS_PER_CONTESTED_OBJECT: u64 = 2;

#[derive(Debug)]
pub struct ConflictingTransferPayload {
    /// The object being contested by multiple payloads
    transfer_object: ObjectRef,
    transfer_from: SuiAddress,
    transfer_to: SuiAddress,
    gas: Gas,
    system_state_observer: Arc<SystemStateObserver>,
    /// Whether this payload expects to potentially fail due to conflict
    expects_conflict: bool,
}

impl Payload for ConflictingTransferPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        if !effects.is_ok() {
            // This is expected for conflicting transactions
            info!(
                "Conflicting transfer tx result: {:?} (expected conflict: {})",
                effects.status(),
                self.expects_conflict
            );
        }

        // Update gas object reference
        self.gas.0 = effects.gas_object().0;

        // Update transfer object if we were the successful transaction
        if effects.is_ok()
            && let Some((obj_ref, _)) = effects
                .mutated()
                .iter()
                .find(|(obj_ref, _)| obj_ref.0 == self.transfer_object.0)
        {
            self.transfer_object = *obj_ref;
        }
        // Note: If we failed due to conflict, we keep the old object ref
        // and will likely fail again (which is expected behavior for this workload)
    }

    fn make_transaction(&mut self) -> Transaction {
        make_transfer_object_transaction(
            self.transfer_object,
            self.gas.0,
            self.transfer_from,
            &self.gas.2,
            self.transfer_to,
            self.system_state_observer
                .state
                .borrow()
                .reference_gas_price,
        )
    }

    fn get_failure_type(&self) -> Option<ExpectedFailureType> {
        if self.expects_conflict {
            Some(ExpectedFailureType::ObjectLockConflict)
        } else {
            None
        }
    }
}

impl std::fmt::Display for ConflictingTransferPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "conflicting_transfer")
    }
}

#[derive(Debug)]
pub struct ConflictingTransferWorkloadBuilder {
    num_contested_objects: u64,
}

impl ConflictingTransferWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        num_contested_objects: u64,
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
                ConflictingTransferWorkloadBuilder {
                    num_contested_objects,
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
impl WorkloadBuilder<dyn Payload> for ConflictingTransferWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        vec![]
    }

    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig> {
        let amount = MAX_GAS_FOR_TESTING
            + ESTIMATED_COMPUTATION_COST
            + STORAGE_COST_PER_COIN * (PAYLOADS_PER_CONTESTED_OBJECT + 1);

        let mut configs = vec![];

        // Create accounts for each contested object group
        for _ in 0..self.num_contested_objects {
            let (address, keypair): (SuiAddress, AccountKeyPair) = get_key_pair();
            let keypair = Arc::new(keypair);

            // Create the contested object (transfer token)
            configs.push(GasCoinConfig {
                amount,
                address,
                keypair: keypair.clone(),
            });

            // Create gas coins for each payload that will contest this object
            for _ in 0..PAYLOADS_PER_CONTESTED_OBJECT {
                configs.push(GasCoinConfig {
                    amount,
                    address,
                    keypair: keypair.clone(),
                });
            }
        }

        configs
    }

    async fn build(
        &self,
        _init_gas: Vec<Gas>,
        payload_gas: Vec<Gas>,
    ) -> Box<dyn Workload<dyn Payload>> {
        Box::<dyn Workload<dyn Payload>>::from(Box::new(ConflictingTransferWorkload {
            num_contested_objects: self.num_contested_objects,
            payload_gas,
        }))
    }
}

#[derive(Debug)]
pub struct ConflictingTransferWorkload {
    num_contested_objects: u64,
    payload_gas: Vec<Gas>,
}

#[async_trait]
impl Workload<dyn Payload> for ConflictingTransferWorkload {
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
        let coins_per_group = (PAYLOADS_PER_CONTESTED_OBJECT + 1) as usize;
        let mut payloads: Vec<Box<dyn Payload>> = vec![];

        // Create a recipient for transfers
        let (recipient, _): (SuiAddress, AccountKeyPair) = get_key_pair();

        for group_idx in 0..self.num_contested_objects as usize {
            let group_start = group_idx * coins_per_group;
            let group_coins = &self.payload_gas[group_start..group_start + coins_per_group];

            // First coin in group is the contested object
            let contested_object = group_coins[0].clone();
            let owner = contested_object.1;

            // Remaining coins are gas for each payload
            for (i, gas) in group_coins[1..].iter().enumerate() {
                payloads.push(Box::new(ConflictingTransferPayload {
                    transfer_object: contested_object.0,
                    transfer_from: owner,
                    transfer_to: recipient,
                    gas: gas.clone(),
                    system_state_observer: system_state_observer.clone(),
                    // First payload in each group doesn't expect conflict,
                    // others do (though which one succeeds is non-deterministic)
                    expects_conflict: i > 0,
                }));
            }
        }

        info!(
            "Created {} conflicting transfer payloads for {} contested objects",
            payloads.len(),
            self.num_contested_objects
        );

        payloads
    }

    fn name(&self) -> &str {
        "ConflictingTransfer"
    }
}
