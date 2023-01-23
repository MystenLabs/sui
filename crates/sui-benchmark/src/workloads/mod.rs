// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod delegation;
pub mod payload;
pub mod shared_counter;
pub mod transfer_object;
pub mod workload;
pub mod workload_configuration;

use std::collections::HashMap;
use std::sync::Arc;

use crate::workloads::payload::Payload;
use delegation::DelegationWorkload;
use shared_counter::SharedCounterWorkload;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::crypto::AccountKeyPair;
use sui_types::object::Owner;
use transfer_object::TransferObjectWorkload;
use workload::*;

pub type Gas = (ObjectRef, Owner, Arc<AccountKeyPair>);

#[derive(Clone)]
pub struct GasCoinConfig {
    // amount of SUI to transfer to this gas coin
    pub amount: u64,
    // recipient of this gas coin
    pub address: SuiAddress,
    // recipient account key pair (useful for signing txns)
    pub keypair: Arc<AccountKeyPair>,
}

#[derive(Clone)]
pub struct WorkloadInitGas {
    // Gas coins to initialize shared counter workload
    // This includes the coins to publish the package and create
    // shared counters
    pub shared_counter_init_gas: Vec<Gas>,
}

#[derive(Clone)]
pub struct WorkloadPayloadGas {
    // Gas coins to be used as transfer tokens
    // These are the objects which get transferred
    // between different accounts during the course of benchmark
    pub transfer_tokens: Vec<Gas>,
    // Gas coins needed to run transfer transactions during
    // the course of benchmark
    pub transfer_object_payload_gas: Vec<Gas>,
    // Gas coins needed to run shared counter increment during
    // the course of benchmark
    pub shared_counter_payload_gas: Vec<Gas>,
    // Gas coins needed to run delegation flow
    pub delegation_payload_gas: Vec<Gas>,
}

#[derive(Clone)]
pub struct WorkloadGasConfig {
    pub shared_counter_workload_init_gas_config: Vec<GasCoinConfig>,
    pub shared_counter_workload_payload_gas_config: Vec<GasCoinConfig>,
    pub transfer_object_workload_tokens: Vec<GasCoinConfig>,
    pub transfer_object_workload_payload_gas_config: Vec<GasCoinConfig>,
    pub delegation_gas_configs: Vec<GasCoinConfig>,
}

pub fn make_combination_workload(
    target_qps: u64,
    num_workers: u64,
    in_flight_ratio: u64,
    num_transfer_accounts: u64,
    shared_counter_weight: u32,
    transfer_object_weight: u32,
    delegation_weight: u32,
    payload_config: WorkloadPayloadGas,
) -> WorkloadInfo {
    let mut workloads = HashMap::<WorkloadType, (u32, Box<dyn Workload<dyn Payload>>)>::new();
    if shared_counter_weight > 0 {
        let workload = SharedCounterWorkload::new_boxed(None, vec![]);
        workloads
            .entry(WorkloadType::SharedCounter)
            .or_insert((shared_counter_weight, workload));
    }
    if transfer_object_weight > 0 {
        let workload = TransferObjectWorkload::new_boxed(num_transfer_accounts);
        workloads
            .entry(WorkloadType::TransferObject)
            .or_insert((transfer_object_weight, workload));
    }
    if delegation_weight > 0 {
        let workload = DelegationWorkload::new_boxed();
        workloads
            .entry(WorkloadType::Delegation)
            .or_insert((delegation_weight, workload));
    }
    let workload = CombinationWorkload::new_boxed(workloads);
    WorkloadInfo {
        target_qps,
        num_workers,
        max_in_flight_ops: in_flight_ratio * target_qps,
        workload,
        payload_config,
    }
}

pub fn make_shared_counter_workload(
    target_qps: u64,
    num_workers: u64,
    max_in_flight_ops: u64,
    payload_config: WorkloadPayloadGas,
) -> Option<WorkloadInfo> {
    if target_qps == 0 || max_in_flight_ops == 0 || num_workers == 0 {
        None
    } else {
        let workload = SharedCounterWorkload::new_boxed(None, vec![]);
        Some(WorkloadInfo {
            target_qps,
            num_workers,
            max_in_flight_ops,
            workload,
            payload_config,
        })
    }
}

pub fn make_transfer_object_workload(
    target_qps: u64,
    num_workers: u64,
    max_in_flight_ops: u64,
    num_transfer_accounts: u64,
    payload_config: WorkloadPayloadGas,
) -> Option<WorkloadInfo> {
    if target_qps == 0 || max_in_flight_ops == 0 || num_workers == 0 {
        None
    } else {
        let workload = TransferObjectWorkload::new_boxed(num_transfer_accounts);
        Some(WorkloadInfo {
            target_qps,
            num_workers,
            max_in_flight_ops,
            workload,
            payload_config,
        })
    }
}

pub fn make_delegation_workload(
    target_qps: u64,
    num_workers: u64,
    max_in_flight_ops: u64,
    payload_config: WorkloadPayloadGas,
) -> Option<WorkloadInfo> {
    if target_qps == 0 || max_in_flight_ops == 0 || num_workers == 0 {
        None
    } else {
        Some(WorkloadInfo {
            target_qps,
            num_workers,
            max_in_flight_ops,
            workload: DelegationWorkload::new_boxed(),
            payload_config,
        })
    }
}
