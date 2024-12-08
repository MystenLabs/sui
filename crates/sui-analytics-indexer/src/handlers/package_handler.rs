// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use fastcrypto::encoding::{Base64, Encoding};
use sui_data_ingestion_core::Worker;
use sui_rpc_api::CheckpointData;
use sui_types::full_checkpoint_content::CheckpointTransaction;
use sui_types::object::Object;
use tokio::sync::Mutex;

use crate::handlers::AnalyticsHandler;
use crate::tables::MovePackageEntry;
use crate::FileType;

pub struct PackageHandler {
    state: Mutex<State>,
}

struct State {
    packages: Vec<MovePackageEntry>,
}

#[async_trait::async_trait]
impl Worker for PackageHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: &CheckpointData) -> Result<()> {
        let CheckpointData {
            checkpoint_summary,
            transactions: checkpoint_transactions,
            ..
        } = checkpoint_data;
        let mut state = self.state.lock().await;
        for checkpoint_transaction in checkpoint_transactions {
            self.process_transaction(
                checkpoint_summary.epoch,
                checkpoint_summary.sequence_number,
                checkpoint_summary.timestamp_ms,
                checkpoint_transaction,
                &mut state,
            )?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<MovePackageEntry> for PackageHandler {
    async fn read(&self) -> Result<Vec<MovePackageEntry>> {
        let mut state = self.state.lock().await;
        let cloned = state.packages.clone();
        state.packages.clear();
        Ok(cloned)
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::MovePackage)
    }

    fn name(&self) -> &str {
        "package"
    }
}

impl PackageHandler {
    pub fn new() -> Self {
        let state = Mutex::new(State { packages: vec![] });
        PackageHandler { state }
    }
    fn process_transaction(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
        state: &mut State,
    ) -> Result<()> {
        for object in checkpoint_transaction.output_objects.iter() {
            self.process_package(epoch, checkpoint, timestamp_ms, object, state)?;
        }
        Ok(())
    }
    fn process_package(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        object: &Object,
        state: &mut State,
    ) -> Result<()> {
        if let sui_types::object::Data::Package(p) = &object.data {
            let package_id = p.id();
            let package_version = p.version().value();
            let original_package_id = p.original_package_id();
            let package = MovePackageEntry {
                package_id: package_id.to_string(),
                package_version: Some(package_version),
                checkpoint,
                epoch,
                timestamp_ms,
                bcs: Base64::encode(bcs::to_bytes(p).unwrap()),
                transaction_digest: object.previous_transaction.to_string(),
                original_package_id: Some(original_package_id.to_string()),
            };
            state.packages.push(package)
        }
        Ok(())
    }
}
