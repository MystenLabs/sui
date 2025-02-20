// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::bank::BenchmarkBank;
use crate::drivers::Interval;
use crate::options::{Opts, RunSpec};
use crate::system_state_observer::SystemStateObserver;
use crate::workloads::batch_payment::BatchPaymentWorkloadBuilder;
use crate::workloads::delegation::DelegationWorkloadBuilder;
use crate::workloads::shared_counter::SharedCounterWorkloadBuilder;
use crate::workloads::transfer_object::TransferObjectWorkloadBuilder;
use crate::workloads::{ExpectedFailureType, GroupID, WorkloadBuilderInfo, WorkloadInfo};
use anyhow::Result;
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;
use tracing::info;

use super::adversarial::{AdversarialPayloadCfg, AdversarialWorkloadBuilder};
use super::expected_failure::{ExpectedFailurePayloadCfg, ExpectedFailureWorkloadBuilder};
use super::randomized_transaction::RandomizedTransactionWorkloadBuilder;
use super::randomness::RandomnessWorkloadBuilder;
use super::shared_object_deletion::SharedCounterDeletionWorkloadBuilder;

#[derive(Debug)]
pub struct WorkloadWeights {
    pub shared_counter: u32,
    pub transfer_object: u32,
    pub delegation: u32,
    pub batch_payment: u32,
    pub shared_deletion: u32,
    pub adversarial: u32,
    pub expected_failure: u32,
    pub randomness: u32,
    pub randomized_transaction: u32,
}

pub struct WorkloadConfig {
    pub group: u32,
    pub num_workers: u64,
    pub num_transfer_accounts: u64,
    pub weights: WorkloadWeights,
    pub adversarial_cfg: AdversarialPayloadCfg,
    pub expected_failure_cfg: ExpectedFailurePayloadCfg,
    pub batch_payment_size: u32,
    pub shared_counter_hotness_factor: u32,
    pub num_shared_counters: Option<u64>,
    pub shared_counter_max_tip: u64,
    pub target_qps: u64,
    pub in_flight_ratio: u64,
    pub duration: Interval,
}
pub struct WorkloadConfiguration;

impl WorkloadConfiguration {
    pub async fn configure(
        bank: BenchmarkBank,
        opts: &Opts,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Result<BTreeMap<GroupID, Vec<WorkloadInfo>>> {
        let mut workload_builders = vec![];

        // Create the workload builders for each Run spec
        match opts.run_spec.clone() {
            RunSpec::Bench {
                num_of_benchmark_groups,
                shared_counter,
                shared_deletion,
                transfer_object,
                delegation,
                batch_payment,
                adversarial,
                expected_failure,
                randomness,
                randomized_transaction,
                shared_counter_hotness_factor,
                num_shared_counters,
                shared_counter_max_tip,
                batch_payment_size,
                adversarial_cfg,
                expected_failure_type,
                target_qps,
                num_workers,
                in_flight_ratio,
                duration,
            } => {
                info!(
                    "Number of benchmark groups to run: {}",
                    num_of_benchmark_groups
                );

                // Creating the workload builders for each benchmark group. The workloads for each
                // benchmark group will run in the same time for the same duration.
                for workload_group in 0..num_of_benchmark_groups {
                    let i = workload_group as usize;
                    let config = WorkloadConfig {
                        group: workload_group,
                        num_workers: num_workers[i],
                        num_transfer_accounts: opts.num_transfer_accounts,
                        weights: WorkloadWeights {
                            shared_counter: shared_counter[i],
                            transfer_object: transfer_object[i],
                            delegation: delegation[i],
                            batch_payment: batch_payment[i],
                            shared_deletion: shared_deletion[i],
                            adversarial: adversarial[i],
                            expected_failure: expected_failure[i],
                            randomness: randomness[i],
                            randomized_transaction: randomized_transaction[i],
                        },
                        adversarial_cfg: AdversarialPayloadCfg::from_str(&adversarial_cfg[i])
                            .unwrap(),
                        expected_failure_cfg: ExpectedFailurePayloadCfg {
                            failure_type: ExpectedFailureType::try_from(expected_failure_type[i])
                                .unwrap(),
                        },
                        batch_payment_size: batch_payment_size[i],
                        shared_counter_hotness_factor: shared_counter_hotness_factor[i],
                        num_shared_counters: num_shared_counters.as_ref().map(|n| n[i]),
                        shared_counter_max_tip: shared_counter_max_tip[i],
                        target_qps: target_qps[i],
                        in_flight_ratio: in_flight_ratio[i],
                        duration: duration[i],
                    };
                    let builders =
                        Self::create_workload_builders(config, system_state_observer.clone()).await;
                    workload_builders.extend(builders);
                }

                Self::build(
                    workload_builders,
                    bank,
                    system_state_observer,
                    opts.gas_request_chunk_size,
                )
                .await
            }
        }
    }

    pub async fn build(
        workload_builders: Vec<Option<WorkloadBuilderInfo>>,
        mut bank: BenchmarkBank,
        system_state_observer: Arc<SystemStateObserver>,
        gas_request_chunk_size: u64,
    ) -> Result<BTreeMap<GroupID, Vec<WorkloadInfo>>> {
        // Generate the workloads and init them
        let reference_gas_price = system_state_observer.state.borrow().reference_gas_price;
        let (workload_params, workload_builders): (Vec<_>, Vec<_>) = workload_builders
            .into_iter()
            .flatten()
            .map(|x| (x.workload_params, x.workload_builder))
            .unzip();
        let mut workloads = bank
            .generate(
                workload_builders,
                reference_gas_price,
                gas_request_chunk_size,
            )
            .await?;
        for workload in workloads.iter_mut() {
            workload
                .init(bank.proxy.clone(), system_state_observer.clone())
                .await;
        }

        let all_workloads = workloads.into_iter().zip(workload_params).fold(
            BTreeMap::<GroupID, Vec<WorkloadInfo>>::new(),
            |mut acc, (workload, workload_params)| {
                let w = WorkloadInfo {
                    workload,
                    workload_params,
                };

                acc.entry(w.workload_params.group).or_default().push(w);
                acc
            },
        );

        Ok(all_workloads)
    }

    pub async fn create_workload_builders(
        WorkloadConfig {
            group,
            num_workers,
            num_transfer_accounts,
            weights,
            adversarial_cfg,
            expected_failure_cfg,
            batch_payment_size,
            shared_counter_hotness_factor,
            num_shared_counters,
            shared_counter_max_tip,
            target_qps,
            in_flight_ratio,
            duration,
        }: WorkloadConfig,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Option<WorkloadBuilderInfo>> {
        tracing::info!(
            "Workload Configuration weights {:?} target_qps: {:?} num_workers: {:?} duration: {:?}",
            weights,
            target_qps,
            num_workers,
            duration
        );
        let total_weight = weights.shared_counter
            + weights.shared_deletion
            + weights.transfer_object
            + weights.delegation
            + weights.batch_payment
            + weights.adversarial
            + weights.randomness
            + weights.expected_failure
            + weights.randomized_transaction;
        let reference_gas_price = system_state_observer.state.borrow().reference_gas_price;
        let mut workload_builders = vec![];
        let shared_workload = SharedCounterWorkloadBuilder::from(
            weights.shared_counter as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            shared_counter_hotness_factor,
            num_shared_counters,
            shared_counter_max_tip,
            reference_gas_price,
            duration,
            group,
        );
        workload_builders.push(shared_workload);
        let shared_deletion_workload = SharedCounterDeletionWorkloadBuilder::from(
            weights.shared_deletion as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            shared_counter_hotness_factor,
            shared_counter_max_tip,
            reference_gas_price,
            duration,
            group,
        );
        workload_builders.push(shared_deletion_workload);
        let transfer_workload = TransferObjectWorkloadBuilder::from(
            weights.transfer_object as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            num_transfer_accounts,
            duration,
            group,
        );
        workload_builders.push(transfer_workload);
        let delegation_workload = DelegationWorkloadBuilder::from(
            weights.delegation as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            duration,
            group,
        );
        workload_builders.push(delegation_workload);
        let batch_payment_workload = BatchPaymentWorkloadBuilder::from(
            weights.batch_payment as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            batch_payment_size,
            duration,
            group,
        );
        workload_builders.push(batch_payment_workload);
        let adversarial_workload = AdversarialWorkloadBuilder::from(
            weights.adversarial as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            adversarial_cfg,
            duration,
            group,
        );
        workload_builders.push(adversarial_workload);
        let randomness_workload = RandomnessWorkloadBuilder::from(
            weights.randomness as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            reference_gas_price,
            duration,
            group,
        );
        workload_builders.push(randomness_workload);
        let expected_failure_workload = ExpectedFailureWorkloadBuilder::from(
            weights.expected_failure as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            num_transfer_accounts,
            expected_failure_cfg,
            duration,
            group,
        );
        workload_builders.push(expected_failure_workload);
        let randomized_transaction_workload = RandomizedTransactionWorkloadBuilder::from(
            weights.randomized_transaction as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            reference_gas_price,
            duration,
            group,
        );
        workload_builders.push(randomized_transaction_workload);

        workload_builders
    }
}
