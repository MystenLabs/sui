// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_json_rpc_types::SuiMoveStruct;
use sui_types::TypeTag;
use sui_types::base_types::{EpochId, ObjectID};
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::object::Object;

use super::{
    ObjectStatusTracker, get_is_consensus, get_move_struct, get_owner_address, get_owner_type,
    initial_shared_version,
};
use crate::Row;
use crate::metrics::Metrics;
use crate::package_store::PackageCache;
use crate::tables::{ObjectRow, ObjectStatus};

pub struct ObjectProcessor {
    package_cache: Arc<PackageCache>,
    package_filter: Option<ObjectID>,
    metrics: Metrics,
}

impl ObjectProcessor {
    pub fn new(
        package_cache: Arc<PackageCache>,
        package_filter: &Option<String>,
        metrics: Metrics,
    ) -> Self {
        Self {
            package_cache,
            package_filter: package_filter
                .clone()
                .map(|x| ObjectID::from_hex_literal(&x).unwrap()),
            metrics,
        }
    }

    async fn check_type_hierarchy(
        &self,
        type_tag: &TypeTag,
        original_package_id: ObjectID,
    ) -> Result<bool> {
        use std::collections::BTreeSet;
        use tokio::task::JoinSet;

        // Collect all package IDs using stack-based traversal
        let mut types = vec![type_tag];
        let mut package_ids = BTreeSet::new();

        while let Some(type_) = types.pop() {
            match type_ {
                TypeTag::Struct(s) => {
                    package_ids.insert(s.address);
                    types.extend(s.type_params.iter());
                }
                TypeTag::Vector(inner) => types.push(inner.as_ref()),
                _ => {}
            }
        }

        // Resolve original package IDs in parallel
        let mut original_ids = JoinSet::new();

        for id in package_ids {
            let package_cache = self.package_cache.clone();
            original_ids.spawn(async move { package_cache.get_original_package_id(id).await });
        }

        // Check if any resolved ID matches our target
        while let Some(result) = original_ids.join_next().await {
            if result?? == original_package_id {
                return Ok(true);
            }
        }

        Ok(false)
    }

    async fn process_object(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        object: &Object,
        object_status_tracker: &ObjectStatusTracker,
    ) -> Result<Option<ObjectRow>> {
        let move_obj_opt = object.data.try_as_move();
        let has_public_transfer = move_obj_opt
            .map(|o| o.has_public_transfer())
            .unwrap_or(false);
        let move_struct = if let Some((tag, contents)) = object
            .struct_tag()
            .and_then(|tag| object.data.try_as_move().map(|mo| (tag, mo.contents())))
        {
            match get_move_struct(
                &tag,
                contents,
                &self.package_cache.resolver_for_epoch(epoch),
            )
            .await
            {
                Ok(move_struct) => Some(move_struct),
                Err(err)
                    if err
                        .downcast_ref::<sui_types::object::bounded_visitor::Error>()
                        .filter(|e| {
                            matches!(e, sui_types::object::bounded_visitor::Error::OutOfBudget)
                        })
                        .is_some() =>
                {
                    self.metrics
                        .total_too_large_to_deserialize
                        .with_label_values(&["object"])
                        .inc();
                    tracing::warn!(
                        "Skipping struct with type {} because it was too large.",
                        tag
                    );
                    None
                }
                Err(err) => return Err(err),
            }
        } else {
            None
        };
        let (struct_tag, sui_move_struct) = if let Some(move_struct) = move_struct {
            match move_struct.into() {
                SuiMoveStruct::WithTypes { type_, fields } => {
                    (Some(type_), Some(SuiMoveStruct::WithFields(fields)))
                }
                fields => (object.struct_tag(), Some(fields)),
            }
        } else {
            (None, None)
        };

        let object_type = move_obj_opt.map(|o| o.type_());

        let is_match = if let Some(package_id) = self.package_filter {
            if let Some(object_type) = object_type {
                let original_package_id = self
                    .package_cache
                    .get_original_package_id(package_id.into())
                    .await?;

                let type_tag: TypeTag = object_type.clone().into();
                self.check_type_hierarchy(&type_tag, original_package_id)
                    .await?
            } else {
                false
            }
        } else {
            true
        };

        if !is_match {
            return Ok(None);
        }

        let object_id = object.id();
        let row = ObjectRow {
            object_id: object_id.to_string(),
            digest: object.digest().to_string(),
            version: object.version().value(),
            type_: object_type.map(|t| t.to_string()),
            checkpoint,
            epoch,
            timestamp_ms,
            owner_type: Some(get_owner_type(object)),
            owner_address: get_owner_address(object),
            is_consensus: get_is_consensus(object),
            object_status: object_status_tracker
                .get_object_status(&object_id)
                .expect("Object must be in output objects"),
            initial_shared_version: initial_shared_version(object),
            previous_transaction: object.previous_transaction.base58_encode(),
            has_public_transfer,
            storage_rebate: Some(object.storage_rebate),
            bcs: "".to_string(),
            bcs_length: bcs::to_bytes(object).unwrap().len() as u64,
            coin_type: object.coin_type_maybe().map(|t| t.to_string()),
            coin_balance: if object.coin_type_maybe().is_some() {
                Some(object.get_coin_value_unsafe())
            } else {
                None
            },
            struct_tag: struct_tag.map(|x| x.to_string()),
            object_json: sui_move_struct.map(|x| x.to_json_value().to_string()),
        };
        Ok(Some(row))
    }
}

impl Row for ObjectRow {
    fn get_epoch(&self) -> EpochId {
        self.epoch
    }

    fn get_checkpoint(&self) -> u64 {
        self.checkpoint
    }
}

#[async_trait]
impl Processor for ObjectProcessor {
    const NAME: &'static str = "objects";
    const FANOUT: usize = 10;
    type Value = ObjectRow;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let epoch = checkpoint.summary.data().epoch;
        let checkpoint_num = checkpoint.summary.data().sequence_number;
        let timestamp_ms = checkpoint.summary.data().timestamp_ms;

        let mut entries = Vec::new();

        for checkpoint_transaction in &checkpoint.transactions {
            let effects = &checkpoint_transaction.effects;
            let object_status_tracker = ObjectStatusTracker::new(effects);

            for object in checkpoint_transaction.output_objects(&checkpoint.object_set) {
                if let Some(object_row) = self
                    .process_object(
                        epoch,
                        checkpoint_num,
                        timestamp_ms,
                        object,
                        &object_status_tracker,
                    )
                    .await?
                {
                    entries.push(object_row);
                }
            }

            for (object_ref, _) in effects.all_removed_objects().iter() {
                let object_row = ObjectRow {
                    object_id: object_ref.0.to_string(),
                    digest: object_ref.2.to_string(),
                    version: u64::from(object_ref.1),
                    type_: None,
                    checkpoint: checkpoint_num,
                    epoch,
                    timestamp_ms,
                    owner_type: None,
                    owner_address: None,
                    object_status: ObjectStatus::Deleted,
                    initial_shared_version: None,
                    previous_transaction: checkpoint_transaction
                        .transaction
                        .digest()
                        .base58_encode(),
                    has_public_transfer: false,
                    is_consensus: false,
                    storage_rebate: None,
                    bcs: "".to_string(),
                    coin_type: None,
                    coin_balance: None,
                    struct_tag: None,
                    object_json: None,
                    bcs_length: 0,
                };
                entries.push(object_row);
            }
        }

        Ok(entries)
    }
}
