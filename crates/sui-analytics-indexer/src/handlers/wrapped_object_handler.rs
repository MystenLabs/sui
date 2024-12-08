// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use std::collections::BTreeMap;
use std::path::Path;
use sui_data_ingestion_core::Worker;
use sui_types::SYSTEM_PACKAGE_ADDRESSES;
use tokio::sync::Mutex;

use sui_package_resolver::Resolver;
use sui_rpc_api::{CheckpointData, CheckpointTransaction};
use sui_types::object::Object;

use crate::handlers::{get_move_struct, parse_struct, AnalyticsHandler};

use crate::package_store::{LocalDBPackageStore, PackageCache};
use crate::tables::WrappedObjectEntry;
use crate::FileType;

pub struct WrappedObjectHandler {
    state: Mutex<State>,
}

struct State {
    wrapped_objects: Vec<WrappedObjectEntry>,
    package_store: LocalDBPackageStore,
    resolver: Resolver<PackageCache>,
}

#[async_trait::async_trait]
impl Worker for WrappedObjectHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: &CheckpointData) -> Result<()> {
        let CheckpointData {
            checkpoint_summary,
            transactions: checkpoint_transactions,
            ..
        } = checkpoint_data;
        let mut state = self.state.lock().await;
        for checkpoint_transaction in checkpoint_transactions {
            for object in checkpoint_transaction.output_objects.iter() {
                state.package_store.update(object)?;
            }
            self.process_transaction(
                checkpoint_summary.epoch,
                checkpoint_summary.sequence_number,
                checkpoint_summary.timestamp_ms,
                checkpoint_transaction,
                &mut state,
            )
            .await?;
            if checkpoint_summary.end_of_epoch_data.is_some() {
                state
                    .resolver
                    .package_store()
                    .evict(SYSTEM_PACKAGE_ADDRESSES.iter().copied());
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<WrappedObjectEntry> for WrappedObjectHandler {
    async fn read(&self) -> Result<Vec<WrappedObjectEntry>> {
        let mut state = self.state.lock().await;
        let cloned = state.wrapped_objects.clone();
        state.wrapped_objects.clear();
        Ok(cloned)
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::WrappedObject)
    }

    fn name(&self) -> &str {
        "wrapped_object"
    }
}

impl WrappedObjectHandler {
    pub fn new(store_path: &Path, rest_uri: &str) -> Self {
        let package_store = LocalDBPackageStore::new(&store_path.join("wrapped_object"), rest_uri);
        let state = Mutex::new(State {
            wrapped_objects: vec![],
            package_store: package_store.clone(),
            resolver: Resolver::new(PackageCache::new(package_store)),
        });
        WrappedObjectHandler { state }
    }
    async fn process_transaction(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
        state: &mut State,
    ) -> Result<()> {
        for object in checkpoint_transaction.output_objects.iter() {
            self.process_object(epoch, checkpoint, timestamp_ms, object, state)
                .await?;
        }
        Ok(())
    }

    async fn process_object(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        object: &Object,
        state: &mut State,
    ) -> Result<()> {
        let move_struct = if let Some((tag, contents)) = object
            .struct_tag()
            .and_then(|tag| object.data.try_as_move().map(|mo| (tag, mo.contents())))
        {
            let move_struct = get_move_struct(&tag, contents, &state.resolver).await?;
            Some(move_struct)
        } else {
            None
        };
        let mut wrapped_structs = BTreeMap::new();
        if let Some(move_struct) = move_struct {
            parse_struct("$", move_struct, &mut wrapped_structs);
        }
        for (json_path, wrapped_struct) in wrapped_structs.iter() {
            let entry = WrappedObjectEntry {
                object_id: wrapped_struct.object_id.map(|id| id.to_string()),
                root_object_id: object.id().to_string(),
                root_object_version: object.version().value(),
                checkpoint,
                epoch,
                timestamp_ms,
                json_path: json_path.to_string(),
                struct_tag: wrapped_struct.struct_tag.clone().map(|tag| tag.to_string()),
            };
            state.wrapped_objects.push(entry);
        }
        Ok(())
    }
}
