// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::bank::BenchmarkBank;
use crate::options::{Opts, RunSpec};
use crate::system_state_observer::SystemStateObserver;
use crate::workloads::workload::{WorkloadInitParameter, WorkloadType, WORKLOAD_REGISTRY};
use crate::workloads::{WorkloadBuilderInfo, WorkloadInfo, WorkloadParams};
use anyhow::Result;
use std::sync::Arc;

pub struct WorkloadConfiguration;

impl WorkloadConfiguration {
    pub async fn configure(
        bank: BenchmarkBank,
        opts: Opts,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Result<Vec<WorkloadInfo>> {
        match opts.run_spec {
            RunSpec::Bench {
                target_qps,
                num_workers,
                in_flight_ratio,
                shared_counter,
                transfer_object,
                delegation,
                batch_payment,
                batch_payment_size,
                shared_counter_hotness_factor,
                mut weights,
                mut workload_parameters,
                ..
            } => {
                // TODO: temporary code for backward compatibility. Remove it
                if shared_counter > 0 {
                    weights.push((WorkloadType::SharedCounter, shared_counter));
                    workload_parameters.push((
                        WorkloadInitParameter::SharedCounterHotnessFactor,
                        shared_counter_hotness_factor,
                    ));
                }
                if delegation > 0 {
                    weights.push((WorkloadType::Delegation, delegation));
                }
                if batch_payment > 0 {
                    weights.push((WorkloadType::BatchPayment, batch_payment));
                    workload_parameters
                        .push((WorkloadInitParameter::BatchPaymentSize, batch_payment_size));
                }
                if transfer_object > 0 {
                    weights.push((WorkloadType::TransferObject, transfer_object));
                    workload_parameters.push((
                        WorkloadInitParameter::NumTransferAccounts,
                        opts.num_transfer_accounts as u32,
                    ));
                }

                Self::build_workloads(
                    num_workers,
                    target_qps,
                    in_flight_ratio,
                    bank,
                    system_state_observer,
                    opts.gas_request_chunk_size,
                    weights,
                    workload_parameters,
                )
                .await
            }
        }
    }

    pub async fn build_workloads(
        num_workers: u64,
        target_qps: u64,
        in_flight_ratio: u64,
        mut bank: BenchmarkBank,
        system_state_observer: Arc<SystemStateObserver>,
        chunk_size: u64,
        weights: Vec<(WorkloadType, u32)>,
        workload_init_parameters: Vec<(WorkloadInitParameter, u32)>,
    ) -> Result<Vec<WorkloadInfo>> {
        let total_weight = weights.iter().map(|pair| pair.1).sum::<u32>() as f32;
        let mut workload_builders = vec![];
        let parameters = workload_init_parameters.into_iter().collect();
        for (workload_type, weight) in weights {
            let workload_weight = weight as f32 / total_weight;
            let target_qps = (workload_weight * target_qps as f32) as u64;
            let num_workers = (workload_weight * num_workers as f32).ceil() as u64;
            let max_ops = target_qps * in_flight_ratio;
            if max_ops == 0 || num_workers == 0 {
                continue;
            }
            let workload_params = WorkloadParams {
                target_qps,
                num_workers,
                max_ops,
            };
            let workload_builder = WORKLOAD_REGISTRY[&workload_type](max_ops, &parameters);
            workload_builders.push(WorkloadBuilderInfo {
                workload_params,
                workload_builder,
            });
        }

        let (workload_params, workload_builders): (Vec<_>, Vec<_>) = workload_builders
            .into_iter()
            .map(|x| (x.workload_params, x.workload_builder))
            .unzip();
        let reference_gas_price = *system_state_observer.reference_gas_price.borrow();
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
