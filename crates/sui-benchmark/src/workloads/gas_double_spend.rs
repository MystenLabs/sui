// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{fmt, num::NonZeroUsize, sync::Arc};

use async_trait::async_trait;
use tracing::{debug, warn};

use crate::{
    ExecutionEffects, ValidatorProxy,
    drivers::Interval,
    system_state_observer::SystemStateObserver,
    workloads::{
        Gas, GasCoinConfig, Workload, WorkloadBuilderInfo, WorkloadParams,
        payload::{BatchExecutionResults, BatchedTransactionStatus, Payload},
        workload::{ESTIMATED_COMPUTATION_COST, MAX_GAS_FOR_TESTING, WorkloadBuilder},
    },
};
use sui_core::test_utils::make_transfer_sui_transaction;
use sui_types::{
    base_types::SuiAddress,
    crypto::{AccountKeyPair, get_key_pair},
    transaction::Transaction,
};

#[derive(Debug)]
pub struct GasDoubleSpendPayload {
    gas: Gas,
    recipient: SuiAddress,
    copies_per_object: usize,
    reference_gas_price: u64,
}

impl GasDoubleSpendPayload {
    fn make_transaction_at_index(&self, index: usize) -> Transaction {
        let (gas_object, sender, keypair) = &self.gas;
        make_transfer_sui_transaction(
            *gas_object,
            self.recipient,
            Some(1),
            *sender,
            keypair,
            self.reference_gas_price.max(1) + index as u64,
        )
    }

    fn update_gas_from_effects(&mut self, effects: &ExecutionEffects) {
        if let Some(updated_gas) = effects.updated_gas(self.gas.0.0) {
            self.gas.0 = updated_gas;
        }
    }

    fn is_expected_conflict(error: &str) -> bool {
        error.contains("ObjectLockConflict")
            || error.contains("ObjectsDoubleUsed")
            || error.contains("locked objects")
            || error.contains("already locked")
            || error.contains("is not available for consumption")
    }
}

#[async_trait]
impl Payload for GasDoubleSpendPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        self.update_gas_from_effects(effects);
    }

    fn make_transaction(&mut self) -> Transaction {
        self.make_transaction_at_index(0)
    }

    fn is_batched(&self) -> bool {
        true
    }

    fn max_soft_bundles(&self) -> NonZeroUsize {
        NonZeroUsize::new(self.copies_per_object).unwrap()
    }

    fn max_soft_bundle_size(&self) -> NonZeroUsize {
        NonZeroUsize::new(1).unwrap()
    }

    fn use_direct_batch_submission(&self) -> bool {
        true
    }

    async fn make_transaction_batch(&mut self) -> Vec<Transaction> {
        (0..self.copies_per_object)
            .map(|index| self.make_transaction_at_index(index))
            .collect()
    }

    fn handle_batch_results(&mut self, results: &BatchExecutionResults) {
        let mut successful_effects = None;
        let mut expected_conflicts = 0;
        let mut retriable_failures = 0;
        let mut unexpected_failures = 0;

        for result in &results.results {
            match &result.status {
                BatchedTransactionStatus::Success { effects } if effects.is_ok() => {
                    if successful_effects.is_some() {
                        warn!(
                            digest = ?result.digest,
                            "gas double spend workload observed multiple successful transactions"
                        );
                    } else {
                        successful_effects = Some(effects.as_ref());
                    }
                }
                BatchedTransactionStatus::Success { effects } => {
                    if effects.is_cancelled() {
                        expected_conflicts += 1;
                    } else {
                        unexpected_failures += 1;
                        warn!(
                            digest = ?result.digest,
                            status = effects.status(),
                            "gas double spend transaction executed with unexpected failure"
                        );
                    }
                }
                BatchedTransactionStatus::PermanentFailure { error } => {
                    if Self::is_expected_conflict(error) {
                        expected_conflicts += 1;
                    } else {
                        unexpected_failures += 1;
                        warn!(
                            digest = ?result.digest,
                            error,
                            "gas double spend transaction failed unexpectedly"
                        );
                    }
                }
                BatchedTransactionStatus::RetriableFailure { .. }
                | BatchedTransactionStatus::UnknownRejection => {
                    retriable_failures += 1;
                }
            }
        }

        if let Some(effects) = successful_effects {
            self.update_gas_from_effects(effects);
        }

        debug!(
            total = results.results.len(),
            has_success = successful_effects.is_some(),
            expected_conflicts,
            retriable_failures,
            unexpected_failures,
            "gas double spend batch completed"
        );
    }
}

impl fmt::Display for GasDoubleSpendPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "gas_double_spend")
    }
}

#[derive(Debug)]
pub struct GasDoubleSpendWorkloadBuilder {
    num_payloads: u64,
    copies_per_object: usize,
    reference_gas_price: u64,
}

impl GasDoubleSpendWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        copies_per_object: usize,
        reference_gas_price: u64,
        duration: Interval,
        group: u32,
    ) -> Option<WorkloadBuilderInfo> {
        let copies_per_object = copies_per_object.max(2);
        let target_qps =
            (workload_weight * target_qps as f32 / copies_per_object as f32).ceil() as u64;
        let num_workers = (workload_weight * num_workers as f32).ceil() as u64;
        let max_ops = target_qps * in_flight_ratio;
        if max_ops == 0 || num_workers == 0 {
            None
        } else {
            Some(WorkloadBuilderInfo {
                workload_params: WorkloadParams {
                    group,
                    target_qps,
                    num_workers,
                    max_ops,
                    duration,
                },
                workload_builder: Box::<dyn WorkloadBuilder<dyn Payload>>::from(Box::new(
                    GasDoubleSpendWorkloadBuilder {
                        num_payloads: max_ops,
                        copies_per_object,
                        reference_gas_price,
                    },
                )),
            })
        }
    }
}

#[async_trait]
impl WorkloadBuilder<dyn Payload> for GasDoubleSpendWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        vec![]
    }

    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig> {
        let amount =
            MAX_GAS_FOR_TESTING + ESTIMATED_COMPUTATION_COST * self.copies_per_object as u64;
        let mut configs = vec![];
        for _ in 0..self.num_payloads {
            let (address, keypair): (SuiAddress, AccountKeyPair) = get_key_pair();
            configs.push(GasCoinConfig {
                amount,
                address,
                keypair: Arc::new(keypair),
            });
        }
        configs
    }

    async fn build(
        &self,
        _init_gas: Vec<Gas>,
        payload_gas: Vec<Gas>,
    ) -> Box<dyn Workload<dyn Payload>> {
        Box::new(GasDoubleSpendWorkload {
            payload_gas,
            copies_per_object: self.copies_per_object,
            reference_gas_price: self.reference_gas_price,
        })
    }
}

#[derive(Debug)]
pub struct GasDoubleSpendWorkload {
    payload_gas: Vec<Gas>,
    copies_per_object: usize,
    reference_gas_price: u64,
}

#[async_trait]
impl Workload<dyn Payload> for GasDoubleSpendWorkload {
    async fn init(
        &mut self,
        _execution_proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        _fullnode_proxies: Vec<Arc<dyn ValidatorProxy + Sync + Send>>,
        _system_state_observer: Arc<SystemStateObserver>,
    ) {
    }

    async fn make_test_payloads(
        &self,
        _execution_proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        _fullnode_proxies: Vec<Arc<dyn ValidatorProxy + Sync + Send>>,
        _system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        self.payload_gas
            .iter()
            .cloned()
            .map(|gas| {
                Box::<dyn Payload>::from(Box::new(GasDoubleSpendPayload {
                    gas,
                    recipient: SuiAddress::random_for_testing_only(),
                    copies_per_object: self.copies_per_object,
                    reference_gas_price: self.reference_gas_price,
                }))
            })
            .collect()
    }

    fn name(&self) -> &str {
        "GasDoubleSpend"
    }
}
