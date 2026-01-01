// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Conflicting Transfer Workload (Soft Bundle)
//!
//! This workload tests conflict detection by submitting multiple conflicting transactions
//! as a soft bundle. The soft bundle ensures deterministic ordering: the first transaction
//! in the bundle succeeds, and subsequent ones fail with ObjectLockConflict.
//!
//! This validates that post-consensus object lock conflict detection works correctly.

use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, info};

use crate::ValidatorProxy;
use crate::drivers::Interval;
use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::{Payload, SoftBundleExecutionResults};
use crate::workloads::workload::WorkloadBuilder;
use crate::workloads::workload::{
    ESTIMATED_COMPUTATION_COST, MAX_GAS_FOR_TESTING, STORAGE_COST_PER_COIN, Workload,
};
use crate::workloads::{Gas, GasCoinConfig, WorkloadBuilderInfo, WorkloadParams};
use sui_core::test_utils::make_transfer_object_transaction;
use sui_types::{
    base_types::{ObjectRef, SuiAddress},
    crypto::{AccountKeyPair, get_key_pair},
    transaction::Transaction,
};

/// Number of conflicting transactions per soft bundle.
/// All transactions try to transfer the same object.
const TRANSACTIONS_PER_BUNDLE: u64 = 2;

/// Payload that submits conflicting transactions as a soft bundle.
/// The first transaction should succeed, and others should fail with ObjectLockConflict.
#[derive(Debug)]
pub struct SoftBundleConflictingTransferPayload {
    /// The object being contested
    transfer_object: ObjectRef,
    /// Owner of the transfer object
    owner: SuiAddress,
    /// Keypair for signing
    keypair: Arc<AccountKeyPair>,
    /// Gas objects for each transaction in the bundle
    gas_objects: Vec<ObjectRef>,
    /// Recipient for transfers
    recipient: SuiAddress,
    /// Reference gas price
    reference_gas_price: u64,
}

impl Payload for SoftBundleConflictingTransferPayload {
    fn make_new_payload(&mut self, _effects: &crate::ExecutionEffects) {
        // State updates are handled in handle_soft_bundle_results()
    }

    fn make_transaction(&mut self) -> Transaction {
        // This is called as a fallback but shouldn't be used for soft bundles.
        // Return a single transfer transaction.
        self.create_transfer_transaction(0)
    }

    fn is_soft_bundle(&self) -> bool {
        true
    }

    fn make_soft_bundle_transactions(&mut self) -> Vec<Transaction> {
        // Create N transactions all trying to transfer the same object
        let transactions: Vec<Transaction> = (0..self.gas_objects.len())
            .map(|i| self.create_transfer_transaction(i))
            .collect();

        debug!(
            "Creating {} conflicting transactions as soft bundle for object {:?}",
            transactions.len(),
            self.transfer_object.0
        );

        transactions
    }

    fn handle_soft_bundle_results(&mut self, results: &SoftBundleExecutionResults) {
        let mut success_count = 0;
        let mut conflict_count = 0;

        for (i, result) in results.results.iter().enumerate() {
            if result.success {
                success_count += 1;
                debug!("Transaction {} executed successfully", i);

                // Update object refs from the successful transaction
                if let Some(effects) = &result.effects {
                    // Update gas object ref
                    let gas_ref = effects.gas_object().0;
                    self.gas_objects[i] = gas_ref;

                    // Update transfer object ref if this was the successful transfer
                    if let Some((obj_ref, _)) = effects
                        .mutated()
                        .iter()
                        .find(|(obj_ref, _)| obj_ref.0 == self.transfer_object.0)
                    {
                        self.transfer_object = *obj_ref;
                    }
                }
            } else {
                // Check if it's an ObjectLockConflict
                let is_lock_conflict = result
                    .error
                    .as_ref()
                    .map(|e| e.contains("ObjectLockConflict"))
                    .unwrap_or(false);

                if is_lock_conflict {
                    conflict_count += 1;
                    debug!(
                        "Transaction {} rejected with ObjectLockConflict (expected)",
                        i
                    );
                } else {
                    debug!("Transaction {} rejected with error: {:?}", i, result.error);
                }
            }
        }

        // Validate: exactly one should succeed, rest should be conflicts.
        // With soft bundles, the ordering is deterministic so this should always hold.
        let expected_conflicts = self.gas_objects.len() - 1;
        assert_eq!(
            success_count, 1,
            "Expected exactly 1 successful transaction in soft bundle, got {}",
            success_count
        );
        assert_eq!(
            conflict_count, expected_conflicts,
            "Expected {} ObjectLockConflict rejections, got {}",
            expected_conflicts, conflict_count
        );
        debug!(
            "Soft bundle validation passed: 1 success, {} conflicts",
            conflict_count
        );
    }
}

impl SoftBundleConflictingTransferPayload {
    /// Create a transfer transaction using the gas object at the given index
    fn create_transfer_transaction(&self, gas_index: usize) -> Transaction {
        make_transfer_object_transaction(
            self.transfer_object,
            self.gas_objects[gas_index],
            self.owner,
            &self.keypair,
            self.recipient,
            self.reference_gas_price,
        )
    }
}

impl std::fmt::Display for SoftBundleConflictingTransferPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "soft_bundle_conflicting_transfer")
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
            + STORAGE_COST_PER_COIN * (TRANSACTIONS_PER_BUNDLE + 1);

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

            // Create gas coins for each transaction in the bundle
            for _ in 0..TRANSACTIONS_PER_BUNDLE {
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
        let coins_per_group = (TRANSACTIONS_PER_BUNDLE + 1) as usize;
        let mut payloads: Vec<Box<dyn Payload>> = vec![];

        // Create a recipient for transfers
        let (recipient, _): (SuiAddress, AccountKeyPair) = get_key_pair();

        // Get reference gas price
        let reference_gas_price = system_state_observer.state.borrow().reference_gas_price;

        for group_idx in 0..self.num_contested_objects as usize {
            let group_start = group_idx * coins_per_group;
            let group_coins = &self.payload_gas[group_start..group_start + coins_per_group];

            // First coin in group is the contested object
            let contested_object = group_coins[0].clone();
            let owner = contested_object.1;
            let keypair = contested_object.2.clone();

            // Remaining coins are gas for each transaction in the bundle
            let gas_objects: Vec<ObjectRef> = group_coins[1..].iter().map(|gas| gas.0).collect();

            payloads.push(Box::new(SoftBundleConflictingTransferPayload {
                transfer_object: contested_object.0,
                owner,
                keypair,
                gas_objects,
                recipient,
                reference_gas_price,
            }));
        }

        info!(
            "Created {} soft bundle conflicting transfer payloads for {} contested objects ({} transactions per bundle)",
            payloads.len(),
            self.num_contested_objects,
            TRANSACTIONS_PER_BUNDLE
        );

        payloads
    }

    fn name(&self) -> &str {
        "SoftBundleConflictingTransfer"
    }
}
