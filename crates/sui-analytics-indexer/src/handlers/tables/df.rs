// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use fastcrypto::encoding::{Base64, Encoding};
use sui_indexer::errors::IndexerError;
use sui_indexer::types::owner_to_owner_info;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_json_rpc_types::SuiMoveValue;
use sui_types::TypeTag;
use sui_types::base_types::{EpochId, ObjectID};
use sui_types::dynamic_field::visitor as DFV;
use sui_types::dynamic_field::{DynamicFieldName, DynamicFieldType};
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::object::Object;
use sui_types::object::bounded_visitor::BoundedVisitor;
use tap::tap::TapFallible;
use tracing::warn;

use crate::Row;
use crate::package_store::PackageCache;
use crate::tables::DynamicFieldRow;

pub struct DynamicFieldProcessor {
    package_cache: Arc<PackageCache>,
}

impl DynamicFieldProcessor {
    pub fn new(package_cache: Arc<PackageCache>) -> Self {
        Self { package_cache }
    }
}

impl DynamicFieldProcessor {
    async fn process_dynamic_field(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        object: &Object,
        all_written_objects: &HashMap<ObjectID, Object>,
    ) -> Result<Option<DynamicFieldRow>> {
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
        let row = match type_ {
            DynamicFieldType::DynamicField => DynamicFieldRow {
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
                let object = all_written_objects.get(&object_id).ok_or(
                    IndexerError::UncategorizedError(anyhow::anyhow!(
                        "Failed to find object_id {:?} when trying to create dynamic field info",
                        object_id
                    )),
                )?;
                let version = object.version().value();
                let digest = object.digest().to_string();
                let object_type = object.data.type_().unwrap().clone();
                DynamicFieldRow {
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
        Ok(Some(row))
    }
}

#[async_trait]
impl Processor for DynamicFieldProcessor {
    const NAME: &'static str = "dynamic_field";
    const FANOUT: usize = 10;
    type Value = DynamicFieldRow;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let epoch = checkpoint.summary.data().epoch;
        let checkpoint_seq = checkpoint.summary.data().sequence_number;
        let timestamp_ms = checkpoint.summary.data().timestamp_ms;

        let mut entries = Vec::new();

        for checkpoint_transaction in &checkpoint.transactions {
            let output_objects: Vec<&Object> = checkpoint_transaction
                .output_objects(&checkpoint.object_set)
                .collect();

            let all_objects: HashMap<_, _> = output_objects
                .iter()
                .map(|x| (x.id(), (*x).clone()))
                .collect();

            for object in &output_objects {
                if let Some(row) = self
                    .process_dynamic_field(
                        epoch,
                        checkpoint_seq,
                        timestamp_ms,
                        object,
                        &all_objects,
                    )
                    .await?
                {
                    entries.push(row);
                }
            }
        }

        Ok(entries)
    }
}

impl Row for DynamicFieldRow {
    fn get_epoch(&self) -> EpochId {
        self.epoch
    }

    fn get_checkpoint(&self) -> u64 {
        self.checkpoint
    }
}
