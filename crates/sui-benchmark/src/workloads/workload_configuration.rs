// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::Result;

use move_core_types::language_storage::TypeTag;

use std::sync::Arc;

use crate::options::{Opts, RunSpec};
use crate::system_state_observer::SystemStateObserver;
use crate::util::generate_all_gas_for_test;
use crate::workloads::shared_counter::SharedCounterWorkload;
use crate::workloads::transfer_object::TransferObjectWorkload;

use crate::workloads::delegation::DelegationWorkload;
use crate::workloads::workload::WorkloadInfo;
use crate::workloads::{
    make_combination_workload, make_delegation_workload, make_shared_counter_workload,
    make_transfer_object_workload, Gas, WorkloadGasConfig, WorkloadInitGas, WorkloadPayloadGas,
};
use crate::ValidatorProxy;

pub enum WorkloadConfiguration {
    // Each worker runs all workloads with similar configuration. Backpressure for one workload impact others
    Combined,
    // Different workers run with different workloads configuration, backpressure on one does not impact others
    Disjoint,
}

impl WorkloadConfiguration {
    pub async fn configure(
        &self,
        gas: Gas,
        pay_coin: Gas,
        pay_coin_type_tag: TypeTag,
        proxy: Arc<dyn ValidatorProxy + Send + Sync>,
        opts: &Opts,
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
                shared_counter_hotness_factor,
                ..
            } => match self {
                WorkloadConfiguration::Combined => {
                    self.configure_combined_mode(
                        num_workers,
                        opts.num_transfer_accounts,
                        shared_counter,
                        transfer_object,
                        delegation,
                        shared_counter_hotness_factor,
                        target_qps,
                        in_flight_ratio,
                        gas,
                        pay_coin,
                        pay_coin_type_tag,
                        proxy,
                        system_state_observer,
                        opts.gas_request_chunk_size,
                    )
                    .await
                }
                WorkloadConfiguration::Disjoint => {
                    self.configure_disjoint_mode(
                        num_workers,
                        opts.num_transfer_accounts,
                        shared_counter,
                        transfer_object,
                        delegation,
                        shared_counter_hotness_factor,
                        target_qps,
                        in_flight_ratio,
                        gas,
                        pay_coin,
                        pay_coin_type_tag,
                        proxy,
                        system_state_observer,
                        opts.gas_request_chunk_size,
                    )
                    .await
                }
            },
        }
    }

    async fn configure_combined_mode(
        &self,
        num_workers: u64,
        num_transfer_accounts: u64,
        shared_counter_weight: u32,
        transfer_object_weight: u32,
        delegation_weight: u32,
        shared_counter_hotness_factor: u32,
        target_qps: u64,
        in_flight_ratio: u64,
        gas: Gas,
        coin: Gas,
        coin_type_tag: TypeTag,
        proxy: Arc<dyn ValidatorProxy + Send + Sync>,
        system_state_observer: Arc<SystemStateObserver>,
        chunk_size: u64,
    ) -> Result<Vec<WorkloadInfo>> {
        let shared_counter_ratio =
            1.0 - (std::cmp::min(shared_counter_hotness_factor as u32, 100) as f32 / 100.0);
        let max_ops = target_qps * in_flight_ratio;
        let all_shared_counter_coin_configs = if shared_counter_weight == 0 {
            None
        } else {
            let num_shared_counters = (max_ops as f32 * shared_counter_ratio) as u64;
            let shared_counter_init_coin_configs =
                SharedCounterWorkload::generate_coin_config_for_init(num_shared_counters);
            let shared_counter_payload_coin_configs =
                SharedCounterWorkload::generate_coin_config_for_payloads(max_ops);
            Some((
                shared_counter_init_coin_configs,
                shared_counter_payload_coin_configs,
            ))
        };
        let all_transfer_object_coin_configs = if transfer_object_weight == 0 {
            None
        } else {
            Some(TransferObjectWorkload::generate_coin_config_for_payloads(
                max_ops,
                num_transfer_accounts,
                max_ops,
            ))
        };
        let delegation_gas_configs = if delegation_weight > 0 {
            DelegationWorkload::generate_gas_config_for_payloads(max_ops)
        } else {
            vec![]
        };
        let (shared_counter_workload_init_gas_config, shared_counter_workload_payload_gas_config) =
            all_shared_counter_coin_configs.unwrap_or((vec![], vec![]));
        let (transfer_object_workload_tokens, transfer_object_workload_payload_gas_config) =
            all_transfer_object_coin_configs.unwrap_or((vec![], vec![]));
        let (workload_init_gas, workload_payload_gas) = generate_all_gas_for_test(
            proxy.clone(),
            gas,
            coin,
            coin_type_tag,
            WorkloadGasConfig {
                shared_counter_workload_init_gas_config,
                shared_counter_workload_payload_gas_config,
                transfer_object_workload_tokens,
                transfer_object_workload_payload_gas_config,
                delegation_gas_configs,
            },
            *system_state_observer.reference_gas_price.borrow(),
            chunk_size,
        )
        .await?;
        let mut combination_workload = make_combination_workload(
            target_qps,
            num_workers,
            in_flight_ratio,
            num_transfer_accounts,
            shared_counter_weight,
            transfer_object_weight,
            delegation_weight,
            workload_payload_gas,
        );
        combination_workload
            .workload
            .init(workload_init_gas, proxy, system_state_observer.clone())
            .await;
        Ok(vec![combination_workload])
    }

    async fn configure_disjoint_mode(
        &self,
        num_workers: u64,
        num_transfer_accounts: u64,
        shared_counter_weight: u32,
        transfer_object_weight: u32,
        delegation_weight: u32,
        shared_counter_hotness_factor: u32,
        target_qps: u64,
        in_flight_ratio: u64,
        gas: Gas,
        coin: Gas,
        coin_type_tag: TypeTag,
        proxy: Arc<dyn ValidatorProxy + Send + Sync>,
        system_state_observer: Arc<SystemStateObserver>,
        chunk_size: u64,
    ) -> Result<Vec<WorkloadInfo>> {
        let mut workloads = vec![];
        let shared_counter_weight_ratio = shared_counter_weight as f32
            / (shared_counter_weight + transfer_object_weight + delegation_weight) as f32;
        let shared_counter_qps = (shared_counter_weight_ratio * target_qps as f32) as u64;
        let shared_counter_num_workers =
            (shared_counter_weight_ratio * num_workers as f32).ceil() as u64;
        let shared_counter_max_ops = (shared_counter_qps * in_flight_ratio) as u64;
        let shared_counter_ratio =
            1.0 - (std::cmp::min(shared_counter_hotness_factor as u32, 100) as f32 / 100.0);
        let num_shared_counters = (shared_counter_max_ops as f32 * shared_counter_ratio) as u64;
        let (shared_counter_workload_init_gas_config, shared_counter_workload_payload_gas_config) =
            if shared_counter_qps == 0
                || shared_counter_max_ops == 0
                || shared_counter_num_workers == 0
            {
                (vec![], vec![])
            } else {
                let shared_counter_init_coin_configs =
                    SharedCounterWorkload::generate_coin_config_for_init(num_shared_counters);
                let shared_counter_payload_coin_configs =
                    SharedCounterWorkload::generate_coin_config_for_payloads(
                        shared_counter_max_ops,
                    );
                (
                    shared_counter_init_coin_configs,
                    shared_counter_payload_coin_configs,
                )
            };

        let transfer_object_weight_ratio = transfer_object_weight as f32
            / (shared_counter_weight + transfer_object_weight + delegation_weight) as f32;
        let transfer_object_qps = (transfer_object_weight_ratio * target_qps as f32) as u64;
        let transfer_object_num_workers =
            (transfer_object_weight_ratio * num_workers as f32).ceil() as u64;
        let transfer_object_max_ops = (transfer_object_qps * in_flight_ratio) as u64;

        let delegate_weight_ratio = delegation_weight as f32
            / (shared_counter_weight + transfer_object_weight + delegation_weight) as f32;
        let delegate_qps = (delegate_weight_ratio * target_qps as f32) as u64;
        let delegate_num_workers = (delegate_weight_ratio * num_workers as f32).ceil() as u64;
        let delegate_max_ops = (delegate_qps * in_flight_ratio) as u64;

        let (transfer_object_workload_tokens, transfer_object_workload_payload_gas_config) =
            if transfer_object_qps == 0
                || transfer_object_max_ops == 0
                || transfer_object_num_workers == 0
            {
                (vec![], vec![])
            } else {
                TransferObjectWorkload::generate_coin_config_for_payloads(
                    transfer_object_max_ops,
                    num_transfer_accounts,
                    transfer_object_max_ops,
                )
            };
        let delegation_gas_configs = if delegation_weight > 0 {
            DelegationWorkload::generate_gas_config_for_payloads(delegate_max_ops)
        } else {
            vec![]
        };
        let (workload_init_gas, workload_payload_gas) = generate_all_gas_for_test(
            proxy.clone(),
            gas,
            coin,
            coin_type_tag,
            WorkloadGasConfig {
                shared_counter_workload_init_gas_config,
                shared_counter_workload_payload_gas_config,
                transfer_object_workload_tokens,
                transfer_object_workload_payload_gas_config,
                delegation_gas_configs,
            },
            *system_state_observer.reference_gas_price.borrow(),
            chunk_size,
        )
        .await?;
        if let Some(mut shared_counter_workload) = make_shared_counter_workload(
            shared_counter_qps,
            shared_counter_num_workers,
            shared_counter_max_ops,
            WorkloadPayloadGas {
                transfer_tokens: vec![],
                transfer_object_payload_gas: vec![],
                shared_counter_payload_gas: workload_payload_gas.shared_counter_payload_gas,
                delegation_payload_gas: vec![],
            },
        ) {
            shared_counter_workload
                .workload
                .init(
                    workload_init_gas,
                    proxy.clone(),
                    system_state_observer.clone(),
                )
                .await;
            workloads.push(shared_counter_workload);
        }
        if let Some(mut transfer_object_workload) = make_transfer_object_workload(
            transfer_object_qps,
            transfer_object_num_workers,
            transfer_object_max_ops,
            num_transfer_accounts,
            WorkloadPayloadGas {
                transfer_tokens: workload_payload_gas.transfer_tokens,
                transfer_object_payload_gas: workload_payload_gas.transfer_object_payload_gas,
                shared_counter_payload_gas: vec![],
                delegation_payload_gas: vec![],
            },
        ) {
            transfer_object_workload
                .workload
                .init(
                    WorkloadInitGas {
                        shared_counter_init_gas: vec![],
                    },
                    proxy.clone(),
                    system_state_observer.clone(),
                )
                .await;
            workloads.push(transfer_object_workload);
        }
        if let Some(delegation_workload) = make_delegation_workload(
            delegate_qps,
            delegate_num_workers,
            delegate_max_ops,
            WorkloadPayloadGas {
                transfer_tokens: vec![],
                transfer_object_payload_gas: vec![],
                shared_counter_payload_gas: vec![],
                delegation_payload_gas: workload_payload_gas.delegation_payload_gas,
            },
        ) {
            workloads.push(delegation_workload);
        }
        Ok(workloads)
    }
}
