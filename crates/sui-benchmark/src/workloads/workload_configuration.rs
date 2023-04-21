// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::bank::BenchmarkBank;
use crate::options::{Opts, RunSpec};
use crate::system_state_observer::SystemStateObserver;
use crate::workloads::batch_payment::BatchPaymentWorkloadBuilder;
use crate::workloads::delegation::DelegationWorkloadBuilder;
use crate::workloads::shared_counter::SharedCounterWorkloadBuilder;
use crate::workloads::transfer_object::TransferObjectWorkloadBuilder;
use crate::workloads::WorkloadInfo;
use anyhow::Result;
use std::str::FromStr;
use std::sync::Arc;

use super::adversarial::{AdversarialPayloadCfg, AdversarialWorkloadBuilder};

pub struct WorkloadConfiguration;

impl WorkloadConfiguration {
    pub async fn configure(
        bank: BenchmarkBank,
        opts: &Opts,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Result<Vec<WorkloadInfo>> {
        match opts.run_spec.clone() {
            RunSpec::Bench {
                target_qps,
                num_workers,
                in_flight_ratio,
                shared_counter,
                transfer_object,
                delegation,
                batch_payment,
                adversarial,
                adversarial_cfg,
                batch_payment_size,
                shared_counter_hotness_factor,
                ..
            } => {
                let health_check_enabled = match opts.run_spec {
                    RunSpec::Bench { health_check, .. } => health_check,
                };
                Self::build_workloads(
                    num_workers,
                    opts.num_transfer_accounts,
                    shared_counter,
                    transfer_object,
                    delegation,
                    batch_payment,
                    adversarial,
                    AdversarialPayloadCfg::from_str(&adversarial_cfg).unwrap(),
                    batch_payment_size,
                    shared_counter_hotness_factor,
                    target_qps,
                    in_flight_ratio,
                    bank,
                    system_state_observer,
                    opts.gas_request_chunk_size,
                    health_check_enabled,
                )
                .await
            }
        }
    }

    pub async fn build_workloads(
        num_workers: u64,
        num_transfer_accounts: u64,
        shared_counter_weight: u32,
        transfer_object_weight: u32,
        delegation_weight: u32,
        batch_payment_weight: u32,
        adversarial_weight: u32,
        adversarial_cfg: AdversarialPayloadCfg,
        batch_payment_size: u32,
        shared_counter_hotness_factor: u32,
        target_qps: u64,
        in_flight_ratio: u64,
        mut bank: BenchmarkBank,
        system_state_observer: Arc<SystemStateObserver>,
        chunk_size: u64,
        health_check_enabled: bool,
    ) -> Result<Vec<WorkloadInfo>> {
        let total_weight = shared_counter_weight
            + transfer_object_weight
            + delegation_weight
            + batch_payment_weight
            + adversarial_weight;
        let mut workload_builders = vec![];
        let health_check_acccount = if health_check_enabled {
            Some((bank.primary_coin.1, bank.primary_coin.2.clone()))
        } else {
            None
        };
        let shared_workload = SharedCounterWorkloadBuilder::from(
            shared_counter_weight as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            shared_counter_hotness_factor,
        );
        workload_builders.push(shared_workload);
        let transfer_workload = TransferObjectWorkloadBuilder::from(
            transfer_object_weight as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            num_transfer_accounts,
            health_check_acccount,
            health_check_enabled,
        );
        workload_builders.push(transfer_workload);
        let delegation_workload = DelegationWorkloadBuilder::from(
            delegation_weight as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
        );
        workload_builders.push(delegation_workload);
        let batch_payment_workload = BatchPaymentWorkloadBuilder::from(
            batch_payment_weight as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            batch_payment_size,
        );
        workload_builders.push(batch_payment_workload);
        let adversarial_workload = AdversarialWorkloadBuilder::from(
            adversarial_weight as f32 / total_weight as f32,
            target_qps,
            num_workers,
            in_flight_ratio,
            adversarial_cfg,
        );
        workload_builders.push(adversarial_workload);
        let (workload_params, workload_builders): (Vec<_>, Vec<_>) = workload_builders
            .into_iter()
            .flatten()
            .map(|x| (x.workload_params, x.workload_builder))
            .unzip();
        let reference_gas_price = system_state_observer.state.borrow().reference_gas_price;
        let mut workloads = bank
            .generate(workload_builders, reference_gas_price, chunk_size)
            .await?;
        for workload in workloads.iter_mut() {
            workload
                .init(bank.proxy.clone(), system_state_observer.clone())
                .await;
        }
        Ok(workloads
            .into_iter()
            .zip(workload_params)
            .map(|(workload, workload_params)| WorkloadInfo {
                workload_params,
                workload,
            })
            .collect())
    }
}
