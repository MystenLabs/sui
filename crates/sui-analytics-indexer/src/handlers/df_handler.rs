// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use fastcrypto::encoding::{Base64, Encoding};
use std::collections::HashMap;
use std::sync::Arc;
use sui_indexer::errors::IndexerError;
use sui_types::object::bounded_visitor::BoundedVisitor;
use sui_types::TypeTag;
use tap::tap::TapFallible;
use tracing::warn;

use sui_indexer::types::owner_to_owner_info;
use sui_json_rpc_types::SuiMoveValue;
use sui_types::base_types::ObjectID;
use sui_types::dynamic_field::visitor as DFV;
use sui_types::dynamic_field::{DynamicFieldName, DynamicFieldType};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::object::Object;

use crate::handlers::{process_transactions, AnalyticsHandler, TransactionProcessor};
use crate::package_store::PackageCache;
use crate::tables::DynamicFieldEntry;
use crate::FileType;

use super::wait_for_cache;

#[derive(Clone)]
pub struct DynamicFieldHandler {
    package_cache: Arc<PackageCache>,
}

impl DynamicFieldHandler {
    pub fn new(package_cache: Arc<PackageCache>) -> Self {
        Self { package_cache }
    }
    async fn process_dynamic_field(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        object: &Object,
        all_written_objects: &HashMap<ObjectID, Object>,
    ) -> Result<Option<DynamicFieldEntry>> {
        let move_obj_opt = object.data.try_as_move();
        let Some(move_object) = move_obj_opt else {
            return Ok(None);
        };
        if !move_object.type_().is_dynamic_field() {
            return Ok(None);
        }

        let layout = self
            .package_cache
            .resolver_for_epoch(epoch)
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
            return Ok(None);
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
        Ok(Some(entry))
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<DynamicFieldEntry> for DynamicFieldHandler {
    async fn process_checkpoint(
        &self,
        checkpoint_data: &Arc<CheckpointData>,
    ) -> Result<Vec<DynamicFieldEntry>> {
        wait_for_cache(&checkpoint_data, &self.package_cache).await;
        Ok(process_transactions(checkpoint_data.clone(), Arc::new(self.clone())).await?)
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::DynamicField)
    }

    fn name(&self) -> &'static str {
        "dynamic_field"
    }
}

#[async_trait::async_trait]
impl TransactionProcessor<DynamicFieldEntry> for DynamicFieldHandler {
    async fn process_transaction(
        &self,
        tx_idx: usize,
        checkpoint: &CheckpointData,
    ) -> Result<Vec<DynamicFieldEntry>> {
        let checkpoint_transaction = &checkpoint.transactions[tx_idx];
        for object in checkpoint_transaction.output_objects.iter() {
            self.package_cache.update(object)?;
        }

        let all_objects: HashMap<_, _> = checkpoint_transaction
            .output_objects
            .iter()
            .map(|x| (x.id(), x.clone()))
            .collect();

        let mut entries = Vec::new();
        for object in checkpoint_transaction.output_objects.iter() {
            if let Some(entry) = self
                .process_dynamic_field(
                    checkpoint.checkpoint_summary.epoch,
                    checkpoint.checkpoint_summary.sequence_number,
                    checkpoint.checkpoint_summary.timestamp_ms,
                    object,
                    &all_objects,
                )
                .await?
            {
                entries.push(entry);
            }
        }

        Ok(entries)
    }
}
