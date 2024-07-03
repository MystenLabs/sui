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
use crate::workloads::{GroupID, WorkloadBuilderInfo, WorkloadInfo};
use anyhow::Result;
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;
use tracing::info;

use super::adversarial::{AdversarialPayloadCfg, AdversarialWorkloadBuilder};
use super::shared_object_deletion::SharedCounterDeletionWorkloadBuilder;

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
                shared_counter_hotness_factor,
                num_shared_counters,
                shared_counter_max_tip,
                batch_payment_size,
                adversarial_cfg,
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
                    let builders = Self::create_workload_builders(
                        workload_group,
                        num_workers[i],
                        opts.num_transfer_accounts,
                        shared_counter[i],
                        transfer_object[i],
                        delegation[i],
                        batch_payment[i],
                        shared_deletion[i],
                        adversarial[i],
                        AdversarialPayloadCfg::from_str(&adversarial_cfg[i]).unwrap(),
                        batch_payment_size[i],
                        shared_counter_hotness_factor[i],
                        num_shared_counters.as_ref().map(|n| n[i]),
                        shared_counter_max_tip[i],
                        target_qps[i],
                        in_flight_ratio[i],
                        duration[i],
                        system_state_observer.clone(),
                    )
                    .await;
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
        workload_group: u32,
        num_workers: u64,
        num_transfer_accounts: u64,
        shared_counter_weight: u32,
        transfer_object_weight: u32,
        delegation_weight: u32,
        batch_payment_weight: u32,
        shared_deletion_weight: u32,
        adversarial_weight: u32,
        adversarial_cfg: AdversarialPayloadCfg,
        batch_payment_size: u32,
        shared_counter_hotness_factor: u32,
        num_shared_counters: Option<u64>,
        shared_counter_max_tip: u64,
        target_qps: u64,
        in_flight_ratio: u64,
        duration: Interval,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Option<WorkloadBuilderInfo>> {
        let total_weight = shared_counter_weight
            + shared_deletion_weight
            + transfer_object_weight
            + delegation_weight
            + batch_payment_weight
            + adversarial_weight;
        let reference_gas_price = system_state_observer.state.borrow().reference_gas_price;
        let mut workload_builders = vec![];
        let shared_workload = SharedCounterWorkloadBuilder::from(
            shared_counter_weight as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            shared_counter_hotness_factor,
            num_shared_counters,
            shared_counter_max_tip,
            reference_gas_price,
            duration,
            workload_group,
        );
        workload_builders.push(shared_workload);
        let shared_deletion_workload = SharedCounterDeletionWorkloadBuilder::from(
            shared_deletion_weight as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            shared_counter_hotness_factor,
            shared_counter_max_tip,
            reference_gas_price,
            duration,
            workload_group,
        );
        workload_builders.push(shared_deletion_workload);
        let transfer_workload = TransferObjectWorkloadBuilder::from(
            transfer_object_weight as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            num_transfer_accounts,
            duration,
            workload_group,
        );
        workload_builders.push(transfer_workload);
        let delegation_workload = DelegationWorkloadBuilder::from(
            delegation_weight as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            duration,
            workload_group,
        );
        workload_builders.push(delegation_workload);
        let batch_payment_workload = BatchPaymentWorkloadBuilder::from(
            batch_payment_weight as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            batch_payment_size,
            duration,
            workload_group,
        );
        workload_builders.push(batch_payment_workload);
        let adversarial_workload = AdversarialWorkloadBuilder::from(
            adversarial_weight as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            adversarial_cfg,
            duration,
            workload_group,
        );
        workload_builders.push(adversarial_workload);

        workload_builders
    }
}
