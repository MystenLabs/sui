// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use fastcrypto::encoding::{Base64, Encoding};
use std::collections::HashMap;
use std::path::Path;
use sui_data_ingestion_core::Worker;
use sui_indexer::errors::IndexerError;
use sui_types::object::bounded_visitor::BoundedVisitor;
use sui_types::{TypeTag, SYSTEM_PACKAGE_ADDRESSES};
use tap::tap::TapFallible;
use tokio::sync::Mutex;
use tracing::warn;

use sui_indexer::types::owner_to_owner_info;
use sui_json_rpc_types::SuiMoveValue;
use sui_package_resolver::Resolver;
use sui_rpc_api::{CheckpointData, CheckpointTransaction};
use sui_types::base_types::ObjectID;
use sui_types::dynamic_field::visitor as DFV;
use sui_types::dynamic_field::{DynamicFieldName, DynamicFieldType};
use sui_types::object::Object;

use crate::handlers::AnalyticsHandler;
use crate::package_store::{LocalDBPackageStore, PackageCache};
use crate::tables::DynamicFieldEntry;
use crate::FileType;

pub struct DynamicFieldHandler {
    state: Mutex<State>,
}

struct State {
    dynamic_fields: Vec<DynamicFieldEntry>,
    package_store: LocalDBPackageStore,
    resolver: Resolver<PackageCache>,
}

#[async_trait::async_trait]
impl Worker for DynamicFieldHandler {
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
impl AnalyticsHandler<DynamicFieldEntry> for DynamicFieldHandler {
    async fn read(&self) -> Result<Vec<DynamicFieldEntry>> {
        let mut state = self.state.lock().await;
        let cloned = state.dynamic_fields.clone();
        state.dynamic_fields.clear();
        Ok(cloned)
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::DynamicField)
    }

    fn name(&self) -> &str {
        "dynamic_field"
    }
}

impl DynamicFieldHandler {
    pub fn new(store_path: &Path, rest_uri: &str) -> Self {
        let package_store = LocalDBPackageStore::new(&store_path.join("dynamic_field"), rest_uri);
        let state = State {
            dynamic_fields: vec![],
            package_store: package_store.clone(),
            resolver: Resolver::new(PackageCache::new(package_store)),
        };
        Self {
            state: Mutex::new(state),
        }
    }
    async fn process_dynamic_field(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        object: &Object,
        all_written_objects: &HashMap<ObjectID, Object>,
        state: &mut State,
    ) -> Result<()> {
        let move_obj_opt = object.data.try_as_move();
        // Skip if not a move object
        let Some(move_object) = move_obj_opt else {
            return Ok(());
        };
        if !move_object.type_().is_dynamic_field() {
            return Ok(());
        }

        let layout = state
            .resolver
            .type_layout(move_object.type_().clone().into())
            .await?;
        let object_id = object.id();

        let field = DFV::FieldVisitor::deserialize(move_object.contents(), &layout)?;

        let type_ = field.kind;
        let name_type: TypeTag = field.name_layout.into();
        let bcs_name = field.name_bytes.to_owned();

        let name_value = BoundedVisitor::deserialize_value(field.name_bytes, field.name_layout)
            .tap_err(|e| {
                warn!("{e}");
            })?;
        let name = DynamicFieldName {
            type_: name_type,
            value: SuiMoveValue::from(name_value).to_json_value(),
        };
        let name_json = serde_json::to_string(&name)?;
        let (_owner_type, owner_id) = owner_to_owner_info(&object.owner);
        let Some(parent_id) = owner_id else {
            return Ok(());
        };
        let entry = match type_ {
            DynamicFieldType::DynamicField => DynamicFieldEntry {
                parent_object_id: parent_id.to_string(),
                transaction_digest: object.previous_transaction.base58_encode(),
                checkpoint,
                epoch,
                timestamp_ms,
                name: name_json,
                bcs_name: Base64::encode(bcs_name),
                type_,
                object_id: object.id().to_string(),
                version: object.version().value(),
                digest: object.digest().to_string(),
                object_type: move_object.clone().into_type().into_type_params()[1]
                    .to_canonical_string(/* with_prefix */ true),
            },
            DynamicFieldType::DynamicObject => {
                let object =
                    all_written_objects
                        .get(&object_id)
                        .ok_or(IndexerError::UncategorizedError(anyhow::anyhow!(
                    "Failed to find object_id {:?} when trying to create dynamic field info",
                    object_id
                )))?;
                let version = object.version().value();
                let digest = object.digest().to_string();
                let object_type = object.data.type_().unwrap().clone();
                DynamicFieldEntry {
                    parent_object_id: parent_id.to_string(),
                    transaction_digest: object.previous_transaction.base58_encode(),
                    checkpoint,
                    epoch,
                    timestamp_ms,
                    name: name_json,
                    bcs_name: Base64::encode(bcs_name),
                    type_,
                    object_id: object.id().to_string(),
                    digest,
                    version,
                    object_type: object_type.to_canonical_string(true),
                }
            }
        };
        state.dynamic_fields.push(entry);
        Ok(())
    }

    async fn process_transaction(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
        state: &mut State,
    ) -> Result<()> {
        let all_objects: HashMap<_, _> = checkpoint_transaction
            .output_objects
            .iter()
            .map(|x| (x.id(), x.clone()))
            .collect();
        for object in checkpoint_transaction.output_objects.iter() {
            self.process_dynamic_field(
                epoch,
                checkpoint,
                timestamp_ms,
                object,
                &all_objects,
                state,
            )
            .await?;
        }
        Ok(())
    }
}
