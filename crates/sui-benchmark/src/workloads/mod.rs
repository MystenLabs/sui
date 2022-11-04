// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod shared_counter;
pub mod transfer_object;
pub mod workload;

use std::collections::HashMap;
use std::sync::Arc;

use shared_counter::SharedCounterWorkload;
use transfer_object::TransferObjectWorkload;
use workload::*;

use sui_types::{
    base_types::{ObjectID, SuiAddress},
    crypto::AccountKeyPair,
};

pub fn make_combination_workload(
    target_qps: u64,
    num_workers: u64,
    in_flight_ratio: u64,
    primary_gas_id: ObjectID,
    primary_gas_account_owner: SuiAddress,
    primary_gas_account_keypair: Arc<AccountKeyPair>,
    num_transfer_accounts: u64,
    shared_counter_weight: u32,
    transfer_object_weight: u32,
) -> WorkloadInfo {
    let mut workloads = HashMap::<WorkloadType, (u32, Box<dyn Workload<dyn Payload>>)>::new();
    if shared_counter_weight > 0 {
        let workload = SharedCounterWorkload::new_boxed(
            primary_gas_id,
            primary_gas_account_owner,
            primary_gas_account_keypair.clone(),
            None,
            vec![],
        );
        workloads
            .entry(WorkloadType::SharedCounter)
            .or_insert((shared_counter_weight, workload));
    }
    if transfer_object_weight > 0 {
        let workload = TransferObjectWorkload::new_boxed(
            num_transfer_accounts,
            primary_gas_id,
            primary_gas_account_owner,
            primary_gas_account_keypair,
        );
        workloads
            .entry(WorkloadType::TransferObject)
            .or_insert((transfer_object_weight, workload));
    }
    let workload = CombinationWorkload::new_boxed(workloads);
    WorkloadInfo {
        target_qps,
        num_workers,
        max_in_flight_ops: in_flight_ratio * target_qps,
        workload,
    }
}

pub fn make_shared_counter_workload(
    target_qps: u64,
    num_workers: u64,
    max_in_flight_ops: u64,
    primary_gas_id: ObjectID,
    owner: SuiAddress,
    keypair: Arc<AccountKeyPair>,
) -> Option<WorkloadInfo> {
    if target_qps == 0 || max_in_flight_ops == 0 || num_workers == 0 {
        None
    } else {
        let workload =
            SharedCounterWorkload::new_boxed(primary_gas_id, owner, keypair, None, vec![]);
        Some(WorkloadInfo {
            target_qps,
            num_workers,
            max_in_flight_ops,
            workload,
        })
    }
}

pub fn make_transfer_object_workload(
    target_qps: u64,
    num_workers: u64,
    max_in_flight_ops: u64,
    num_transfer_accounts: u64,
    primary_gas_id: &ObjectID,
    owner: SuiAddress,
    keypair: Arc<AccountKeyPair>,
) -> Option<WorkloadInfo> {
    if target_qps == 0 || max_in_flight_ops == 0 || num_workers == 0 {
        None
    } else {
        let workload = TransferObjectWorkload::new_boxed(
            num_transfer_accounts,
            *primary_gas_id,
            owner,
            keypair,
        );
        Some(WorkloadInfo {
            target_qps,
            num_workers,
            max_in_flight_ops,
            workload,
        })
    }
}
