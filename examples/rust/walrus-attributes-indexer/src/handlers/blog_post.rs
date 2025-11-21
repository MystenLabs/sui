// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use anyhow::{self, bail, Context};
use diesel::query_dsl::methods::FilterDsl;
use diesel::upsert::excluded;
use diesel::ExpressionMethods;
use diesel_async::RunQueryDsl;
use move_core_types::language_storage::StructTag;
use sui_indexer_alt_framework::pipeline::{sequential::Handler, Processor};
use sui_indexer_alt_framework::postgres;
use sui_indexer_alt_framework::types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_indexer_alt_framework::types::effects::TransactionEffectsAPI;
use sui_indexer_alt_framework::types::full_checkpoint_content::CheckpointData;
use sui_indexer_alt_framework::types::object::Object;
use sui_indexer_alt_framework::types::parse_sui_struct_tag;
use sui_indexer_alt_framework::FieldCount;
use sui_indexer_alt_framework::Result;

use crate::schema::blog_post;
use crate::storage::StoredBlogPost;
use crate::types::{extract_content_from_metadata, BlogPostMetadata};

// ============================================================================
// PROCESSING TYPES
// ============================================================================
// These types represent intermediate data structures used during processing.
// They bridge between the raw on-chain data and the database storage format.

/// Enum representing the data of interest transformed from processing the checkpoint to be passed
/// to the committer implementation.
#[derive(Debug, Clone)]
pub enum ProcessedWalrusMetadata {
    Upsert {
        /// The ID of the Metadata dynamic field.
        dynamic_field_id: ObjectID,
        /// The version of the Metadata dynamic field.
        df_version: u64,
        blog_post_metadata: BlogPostMetadata,
        /// ID of the Blob object on Sui, used during reads to fetch the actual blob content. If
        /// this object has been wrapped or deleted, it will not be present on the live object set,
        /// which means the corresponding content on Walrus is also not accessible.
        blob_obj_id: SuiAddress,
    },
    /// Tracks the deletion of a Metadata dynamic field. When committing, this will delete the
    /// existing row.
    Delete(ObjectID),
}

pub struct BlogPostPipeline {
    metadata_type: StructTag,
}

impl Processor for BlogPostPipeline {
    const NAME: &'static str = "blog_post";

    type Value = ProcessedWalrusMetadata;

    /// This pipeline operates on a checkpoint granularity to produce a set of values reflecting the
    /// state of relevant Metadata dynamic fields at checkpoint boundary.
    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let checkpoint_input_objects = checkpoint_input_objects(checkpoint)?;
        let latest_live_output_objects = checkpoint_output_objects(checkpoint)?;
        // Collect values to be passed to committer. This map is keyed on the dynamic field id.
        let mut values: BTreeMap<ObjectID, Self::Value> = BTreeMap::new();

        // Process relevant Metadata dynamic fields that were wrapped or deleted in this checkpoint.
        for (object_id, object) in &checkpoint_input_objects {
            // If an object appears in both maps, it is still alive at the end of the checkpoint.
            if latest_live_output_objects.contains_key(object_id) {
                continue;
            }

            // Check the checkpoint input state of the Metadata dynamic field to see if it's
            // relevant to our indexing.
            let Some((_, _)) = extract_content_from_metadata(&self.metadata_type, object)? else {
                continue;
            };

            // Since the table is keyed on the dynamic field id, this is all the information we need
            // to delete the correct entry in the commit fn.
            values.insert(*object_id, ProcessedWalrusMetadata::Delete(*object_id));
        }

        for (object_id, object) in &latest_live_output_objects {
            let Some((blog_post_metadata, blob_obj_id)) =
                extract_content_from_metadata(&self.metadata_type, object)?
            else {
                continue;
            };

            values.insert(
                *object_id,
                ProcessedWalrusMetadata::Upsert {
                    df_version: object.version().into(),
                    dynamic_field_id: *object_id,
                    blog_post_metadata,
                    blob_obj_id,
                },
            );
        }

        Ok(values.into_values().collect())
    }
}

#[async_trait::async_trait]
impl Handler for BlogPostPipeline {
    type Store = postgres::Db;
    type Batch = BTreeMap<ObjectID, Self::Value>;

    fn batch(
        batch: &mut Self::Batch,
        values: impl IntoIterator<Item = Self::Value>,
    ) -> sui_indexer_alt_framework::pipeline::BatchStatus {
        for value in values {
            match value {
                ProcessedWalrusMetadata::Upsert {
                    dynamic_field_id, ..
                } => {
                    batch.insert(dynamic_field_id, value);
                }
                ProcessedWalrusMetadata::Delete(dynamic_field_id) => {
                    batch.insert(dynamic_field_id, value);
                }
            }
        }
        sui_indexer_alt_framework::pipeline::BatchStatus::Pending
    }

    async fn commit<'a>(batch: &Self::Batch, conn: &mut postgres::Connection<'a>) -> Result<usize> {
        // Partition the batch into items to delete and items to upsert
        let (to_delete, to_upsert): (Vec<_>, Vec<_>) = batch
            .values()
            .partition(|item| matches!(item, ProcessedWalrusMetadata::Delete(_)));

        let to_upsert: Vec<StoredBlogPost> = to_upsert
            .into_iter()
            .map(|item| item.to_stored())
            .collect::<Result<Vec<_>>>()?;

        let to_delete: Vec<ObjectID> = to_delete
            .into_iter()
            .map(|item| Ok(item.dynamic_field_id()))
            .collect::<Result<Vec<_>>>()?;

        let mut total_affected = 0;

        if !to_delete.is_empty() {
            let deleted_count = diesel::delete(blog_post::table)
                .filter(blog_post::dynamic_field_id.eq_any(to_delete.iter().map(|id| id.to_vec())))
                .execute(conn)
                .await?;

            total_affected += deleted_count;
        }

        if !to_upsert.is_empty() {
            let upserted_count = diesel::insert_into(blog_post::table)
                .values(&to_upsert)
                .on_conflict(blog_post::dynamic_field_id)
                .do_update()
                .set((
                    blog_post::df_version.eq(excluded(blog_post::df_version)),
                    blog_post::title.eq(excluded(blog_post::title)),
                    blog_post::view_count.eq(excluded(blog_post::view_count)),
                    blog_post::blob_obj_id.eq(excluded(blog_post::blob_obj_id)),
                ))
                .filter(blog_post::df_version.lt(excluded(blog_post::df_version)))
                .execute(conn)
                .await?;

            total_affected += upserted_count;
        }

        Ok(total_affected)
    }
}

impl FieldCount for ProcessedWalrusMetadata {
    const FIELD_COUNT: usize = StoredBlogPost::FIELD_COUNT;
}

impl BlogPostPipeline {
    pub fn new(type_string: &str) -> Result<Self> {
        let metadata_type = parse_sui_struct_tag(type_string)?;
        Ok(BlogPostPipeline { metadata_type })
    }
}

impl ProcessedWalrusMetadata {
    fn dynamic_field_id(&self) -> ObjectID {
        match self {
            ProcessedWalrusMetadata::Upsert {
                dynamic_field_id, ..
            } => *dynamic_field_id,
            ProcessedWalrusMetadata::Delete(dynamic_field_id) => *dynamic_field_id,
        }
    }

    /// Attempt to convert into a `StoredBlogPost` only if the variant is `Upsert`.
    fn to_stored(&self) -> Result<StoredBlogPost> {
        match self {
            ProcessedWalrusMetadata::Upsert {
                dynamic_field_id,
                df_version,
                blog_post_metadata,
                blob_obj_id,
            } => Ok(StoredBlogPost {
                // This is meant to validate that the publisher address stored is a valid SuiAddress
                publisher: blog_post_metadata.publisher.clone(),
                dynamic_field_id: dynamic_field_id.to_vec(),
                df_version: *df_version as i64,
                view_count: blog_post_metadata.view_count as i64,
                title: blog_post_metadata.title.clone(),
                blob_obj_id: blob_obj_id.to_vec(),
            }),
            ProcessedWalrusMetadata::Delete(_) => {
                bail!("Cannot convert Delete variant to StoredBlogPost")
            }
        }
    }
}

/// Returns the first appearance of all objects that were used as inputs to the transactions in the
/// checkpoint. These are objects that existed prior to the checkpoint, and excludes objects that
/// were created or unwrapped within the checkpoint.
pub fn checkpoint_input_objects(
    checkpoint: &CheckpointData,
) -> anyhow::Result<BTreeMap<ObjectID, &Object>> {
    let mut output_objects_seen = HashSet::new();
    let mut checkpoint_input_objects = BTreeMap::new();
    for tx in &checkpoint.transactions {
        let input_objects_map: BTreeMap<(ObjectID, SequenceNumber), &Object> = tx
            .input_objects
            .iter()
            .map(|obj| ((obj.id(), obj.version()), obj))
            .collect();

        for change in tx.effects.object_changes() {
            let id = change.id;

            let Some(version) = change.input_version else {
                continue;
            };

            // This object was previously modified, created, or unwrapped in the checkpoint, so
            // this version is not a checkpoint input.
            if output_objects_seen.contains(&id) {
                continue;
            }

            // Make sure this object has not already been recorded as an input.
            let Entry::Vacant(entry) = checkpoint_input_objects.entry(id) else {
                continue;
            };

            let input_obj = input_objects_map
                .get(&(id, version))
                .copied()
                .with_context(|| format!(
                    "Object {id} at version {version} referenced in effects not found in input_objects"
                ))?;

            entry.insert(input_obj);
        }

        for change in tx.effects.object_changes() {
            if change.output_version.is_some() {
                output_objects_seen.insert(change.id);
            }
        }
    }
    Ok(checkpoint_input_objects)
}

/// Returns all versions of objects that were output by transactions in the checkpoint, and are
/// still live at the end of the checkpoint.
pub(crate) fn checkpoint_output_objects(
    checkpoint: &CheckpointData,
) -> anyhow::Result<BTreeMap<ObjectID, &Object>> {
    let mut output_objects = BTreeMap::new();
    for tx in &checkpoint.transactions {
        let output_objects_map: BTreeMap<_, _> = tx
            .output_objects
            .iter()
            .map(|obj| ((obj.id(), obj.version()), obj))
            .collect();

        for change in tx.effects.object_changes() {
            let id = change.id;

            // Clear the previous entry, in case it was created within this checkpoint.
            output_objects.remove(&id);

            let Some(version) = change.output_version else {
                continue;
            };

            let output_object = output_objects_map
                .get(&(id, version))
                .copied()
                .with_context(|| format!("{id} at {version} in effects, not in output_objects"))?;

            output_objects.insert(id, output_object);
        }
    }

    Ok(output_objects)
}
