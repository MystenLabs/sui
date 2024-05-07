// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::benchmark_context::BenchmarkContext;
use crate::command::WorkloadKind;
use crate::tx_generator::{MoveTxGenerator, PackagePublishTxGenerator, TxGenerator};
use std::path::PathBuf;
use std::sync::Arc;
use sui_test_transaction_builder::PublishData;

#[derive(Clone)]
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
        self.workload_kind.gas_object_num_per_account()
    }

    pub(crate) async fn create_tx_generator(
        &self,
        ctx: &mut BenchmarkContext,
    ) -> Arc<dyn TxGenerator> {
        match &self.workload_kind {
            WorkloadKind::PTB {
                num_transfers,
                use_native_transfer,
                num_dynamic_fields,
                computation,
                num_shared_objects,
                num_mints,
                nft_size,
                use_batch_mint,
            } => {
                let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                path.extend(["move_package"]);
                let move_package = ctx.publish_package(PublishData::Source(path, false)).await;
                let root_objects = ctx
                    .preparing_dynamic_fields(move_package.0, *num_dynamic_fields)
                    .await;
                let shared_objects = ctx
                    .prepare_shared_objects(move_package.0, *num_shared_objects)
                    .await;
                Arc::new(MoveTxGenerator::new(
                    move_package.0,
                    *num_transfers,
                    *use_native_transfer,
                    *computation,
                    root_objects,
                    shared_objects,
                    *num_mints,
                    *nft_size,
                    *use_batch_mint,
                ))
            }
            WorkloadKind::Publish {
                manifest_file: manifest_path,
            } => Arc::new(PackagePublishTxGenerator::new(ctx, manifest_path.clone()).await),
        }
    }
}
