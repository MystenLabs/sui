// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::base_types::ObjectID;

use crate::benchmark_context::BenchmarkContext;
use crate::command::WorkloadKind;
use crate::tx_generator::{CounterTxGenerator, MoveTxGenerator, NonMoveTxGenerator, TxGenerator};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct Workload {
    pub tx_count: u64,
    pub workload_kind: WorkloadKind,
}

impl Workload {
    pub fn new(tx_count: u64, workload_kind: WorkloadKind) -> Self {
        Self {
            tx_count,
            workload_kind,
        }
    }

    pub fn num_accounts(&self) -> u64 {
        match self.workload_kind {
            WorkloadKind::Counter { txs_per_counter } => self.tx_count / txs_per_counter,
            _ => self.tx_count,
        }
    }

    pub fn gas_object_num_per_account(&self) -> u64 {
        match self.workload_kind {
            WorkloadKind::NoMove => 1,
            WorkloadKind::Move {
                num_input_objects, ..
            } => num_input_objects as u64,
            WorkloadKind::Counter { txs_per_counter } => txs_per_counter,
        }
    }

    pub async fn create_tx_generator(
        &self,
        ctx: &mut BenchmarkContext,
    ) -> (Arc<dyn TxGenerator>, Option<ObjectID>) {
        match self.workload_kind {
            WorkloadKind::NoMove => (Arc::new(NonMoveTxGenerator::new()), None),
            WorkloadKind::Move {
                num_input_objects,
                num_dynamic_fields,
                computation,
            } => {
                assert!(
                    num_input_objects >= 1,
                    "Each transaction requires at least 1 input object"
                );
                let move_package = ctx.publish_package().await;
                println!("move_package: {:?}", move_package.0);
                let root_objects = ctx
                    .preparing_dynamic_fields(move_package.0, num_dynamic_fields)
                    .await;

                (
                    Arc::new(MoveTxGenerator::new(
                        move_package.0,
                        num_input_objects,
                        computation,
                        root_objects,
                    )),
                    Some(move_package.0),
                )
            }
            WorkloadKind::Counter { txs_per_counter } => {
                let move_package = ctx.publish_package().await;
                println!("move_package: {:?}", move_package.0);
                // generate counter objects
                let counter_objects = ctx.preparing_counter_objects(move_package.0).await;
                (
                    Arc::new(CounterTxGenerator::new(
                        move_package.0,
                        counter_objects,
                        txs_per_counter,
                    )),
                    Some(move_package.0),
                )
            }
        }
    }
}
