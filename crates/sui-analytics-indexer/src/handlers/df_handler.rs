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
use sui_indexer_alt_framework::pipeline::concurrent::{BatchStatus, Handler};
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_object_store::ObjectStore;
use sui_json_rpc_types::SuiMoveValue;
use sui_types::TypeTag;
use sui_types::base_types::ObjectID;
use sui_types::dynamic_field::visitor as DFV;
use sui_types::dynamic_field::{DynamicFieldName, DynamicFieldType};
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::object::Object;
use sui_types::object::bounded_visitor::BoundedVisitor;
use tap::tap::TapFallible;
use tracing::warn;

use crate::FileType;
use crate::package_store::PackageCache;
use crate::tables::DynamicFieldEntry;
use crate::writers::AnalyticsWriter;

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
                let object = all_written_objects.get(&object_id).ok_or(
                    IndexerError::UncategorizedError(anyhow::anyhow!(
                        "Failed to find object_id {:?} when trying to create dynamic field info",
                        object_id
                    )),
                )?;
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

#[async_trait]
impl Processor for DynamicFieldHandler {
    const NAME: &'static str = "dynamic_field";
    const FANOUT: usize = 10;
    type Value = DynamicFieldEntry;

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
                if let Some(entry) = self
                    .process_dynamic_field(
                        epoch,
                        checkpoint_seq,
                        timestamp_ms,
                        object,
                        &all_objects,
                    )
                    .await?
                {
                    entries.push(entry);
                }
            }
        }

        Ok(entries)
    }
}

#[async_trait]
impl Handler for DynamicFieldHandler {
    type Store = ObjectStore;
    type Batch = Vec<DynamicFieldEntry>;

    const MIN_EAGER_ROWS: usize = 100_000;
    const MAX_PENDING_ROWS: usize = 500_000;

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        batch.extend(values);

        if batch.len() >= Self::MIN_EAGER_ROWS {
            BatchStatus::Ready
        } else {
            BatchStatus::Pending
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> Result<usize> {
        if batch.is_empty() {
            return Ok(0);
        }

        // Get the checkpoint range from the batch
        let first_checkpoint = batch.first().unwrap().checkpoint;
        let last_checkpoint = batch.last().unwrap().checkpoint;
        let epoch = batch.first().unwrap().epoch;

        // Create a temporary Parquet file
        use crate::parquet::ParquetWriter;
        use tempfile::TempDir;

        let temp_dir = TempDir::new()?;
        let mut writer: ParquetWriter =
            ParquetWriter::new(temp_dir.path(), FileType::DynamicField, first_checkpoint)?;

        // Collect into a vec to satisfy 'static lifetime requirement
        let rows = batch.to_vec();
        AnalyticsWriter::<DynamicFieldEntry>::write(&mut writer, Box::new(rows.into_iter()))?;
        AnalyticsWriter::<DynamicFieldEntry>::flush(&mut writer, last_checkpoint + 1)?;

        // Build the object store path
        let file_path = FileType::DynamicField.file_path(
            crate::FileFormat::PARQUET,
            epoch,
            first_checkpoint..(last_checkpoint + 1),
        );

        // Read the file and upload
        let local_file = temp_dir
            .path()
            .join(FileType::DynamicField.dir_prefix().as_ref())
            .join(format!("epoch_{}", epoch))
            .join(format!(
                "{}_{}.parquet",
                first_checkpoint,
                last_checkpoint + 1
            ));

        let file_bytes = tokio::fs::read(&local_file).await?;

        conn.object_store()
            .put(&file_path, file_bytes.into())
            .await?;

        Ok(batch.len())
    }
}
