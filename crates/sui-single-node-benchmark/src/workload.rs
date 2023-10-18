// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::benchmark_context::BenchmarkContext;
use crate::command::WorkloadKind;
use crate::tx_generator::{MoveTxGenerator, NonMoveTxGenerator, TxGenerator};
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

    pub(crate) fn num_accounts(&self) -> u64 {
        self.tx_count
    }

    pub(crate) fn gas_object_num_per_account(&self) -> u64 {
        match self.workload_kind {
            WorkloadKind::NoMove => 1,
            WorkloadKind::Move {
                num_input_objects, ..
            } => num_input_objects as u64,
        }
    }

    pub(crate) async fn create_tx_generator(
        &self,
        ctx: &mut BenchmarkContext,
    ) -> Arc<dyn TxGenerator> {
        match self.workload_kind {
            WorkloadKind::NoMove => Arc::new(NonMoveTxGenerator::new()),
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
                let root_objects = ctx
                    .preparing_dynamic_fields(move_package.0, num_dynamic_fields)
                    .await;
                Arc::new(MoveTxGenerator::new(
                    move_package.0,
                    num_input_objects,
                    computation,
                    root_objects,
                ))
            }
        }
    }
}
