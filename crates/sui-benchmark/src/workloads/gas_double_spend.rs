// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{fmt, num::NonZeroUsize, str::FromStr, sync::Arc};

use anyhow::anyhow;
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

/// How the conflicting copies of a double-spend op are submitted to validators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GasDoubleSpendSubmission {
    /// Submit each copy independently through the normal transaction path so they race
    /// against the gas-object lock as separate submissions. This is the default and the
    /// cleanest way to observe per-copy conflicts.
    #[default]
    Direct,
    /// Pack all copies into a single soft bundle. Soft bundles are not atomic, so the copies
    /// still contend at execution; this exercises the case where conflicting transactions are
    /// co-submitted in one bundle.
    SoftBundle,
}

impl FromStr for GasDoubleSpendSubmission {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "direct" => Ok(GasDoubleSpendSubmission::Direct),
            "soft-bundle" | "soft_bundle" | "softbundle" => {
                Ok(GasDoubleSpendSubmission::SoftBundle)
            }
            other => Err(anyhow!(
                "invalid gas double spend submission mode '{other}'; expected 'direct' or 'soft-bundle'"
            )),
        }
    }
}

impl fmt::Display for GasDoubleSpendSubmission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GasDoubleSpendSubmission::Direct => write!(f, "direct"),
            GasDoubleSpendSubmission::SoftBundle => write!(f, "soft-bundle"),
        }
    }
}

#[derive(Debug)]
pub struct GasDoubleSpendPayload {
    gas: Gas,
    recipient: SuiAddress,
    copies_per_object: usize,
    reference_gas_price: u64,
    submission: GasDoubleSpendSubmission,
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
        match self.submission {
            // Direct submission ignores bundling; each copy is submitted on its own.
            GasDoubleSpendSubmission::Direct => NonZeroUsize::new(self.copies_per_object).unwrap(),
            // Keep every copy in a single bundle so they contend within one soft bundle.
            GasDoubleSpendSubmission::SoftBundle => NonZeroUsize::new(1).unwrap(),
        }
    }

    fn max_soft_bundle_size(&self) -> NonZeroUsize {
        match self.submission {
            GasDoubleSpendSubmission::Direct => NonZeroUsize::new(1).unwrap(),
            // The bundle must be large enough to hold all copies. `copies_per_object` should stay
            // within the protocol's soft bundle size limit, otherwise the whole bundle is rejected.
            GasDoubleSpendSubmission::SoftBundle => {
                NonZeroUsize::new(self.copies_per_object).unwrap()
            }
        }
    }

    fn use_direct_batch_submission(&self) -> bool {
        matches!(self.submission, GasDoubleSpendSubmission::Direct)
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

/// Builds the gas double-spend workload. Each payload owns a single gas coin and
/// submits `copies_per_object` conflicting transactions that all try to spend that
/// coin, so at most one can succeed and the rest are expected to hit lock conflicts.
#[derive(Debug)]
pub struct GasDoubleSpendWorkloadBuilder {
    num_payloads: u64,
    copies_per_object: usize,
    reference_gas_price: u64,
    submission: GasDoubleSpendSubmission,
}

impl GasDoubleSpendWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        copies_per_object: usize,
        reference_gas_price: u64,
        submission: GasDoubleSpendSubmission,
        duration: Interval,
        group: u32,
    ) -> Option<WorkloadBuilderInfo> {
        // At least two copies are required for a spend to actually conflict.
        let copies_per_object = copies_per_object.max(2);
        // Each op emits `copies_per_object` transactions, so scale the target down by
        // that factor to keep the total submitted tx rate aligned with `target_qps`.
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
                        submission,
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
        // Fund each coin to cover the successful transfer plus the computation cost of
        // every conflicting copy, since all copies are charged before conflicts resolve.
        let amount =
            MAX_GAS_FOR_TESTING + ESTIMATED_COMPUTATION_COST * self.copies_per_object as u64;
        // One dedicated coin per payload, each with its own fresh keypair, so that a
        // payload's copies contend only with each other and never across payloads.
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
            submission: self.submission,
        })
    }
}

#[derive(Debug)]
pub struct GasDoubleSpendWorkload {
    payload_gas: Vec<Gas>,
    copies_per_object: usize,
    reference_gas_price: u64,
    submission: GasDoubleSpendSubmission,
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
                    submission: self.submission,
                }))
            })
            .collect()
    }

    fn name(&self) -> &str {
        "GasDoubleSpend"
    }
}
