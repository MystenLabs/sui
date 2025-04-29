// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use sui_types::{TypeTag, SYSTEM_PACKAGE_ADDRESSES};

use sui_json_rpc_types::SuiMoveStruct;
use sui_types::base_types::ObjectID;
use sui_types::effects::TransactionEffects;
use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};
use sui_types::object::Object;

use crate::handlers::{
    get_move_struct, get_owner_address, get_owner_type, initial_shared_version,
    process_transactions, AnalyticsHandler, ObjectStatusTracker, TransactionProcessor,
};
use crate::package_store::PackageCache;
use crate::tables::{ObjectEntry, ObjectStatus};
use crate::AnalyticsMetrics;
use crate::FileType;

const NAME: &str = "object";

// Context contains shared resources
struct Context {
    package_filter: Option<ObjectID>,
    metrics: AnalyticsMetrics,
    package_cache: Arc<PackageCache>,
}

#[derive(Clone)]
pub struct ObjectHandler {
    context: Arc<Context>,
}


#[async_trait::async_trait]
impl TransactionProcessor<ObjectEntry> for ObjectHandler {
    async fn process_transaction(
        &self,
        tx_idx: usize,
        checkpoint: &CheckpointData,
    ) -> Result<Vec<ObjectEntry>> {
        let transaction = &checkpoint.transactions[tx_idx];
        Self::process_transaction(
            checkpoint.checkpoint_summary.epoch,
            checkpoint.checkpoint_summary.sequence_number,
            checkpoint.checkpoint_summary.timestamp_ms,
            transaction,
            &transaction.effects,
            self.context.clone(),
        )
        .await
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<ObjectEntry> for ObjectHandler {
    async fn process_checkpoint(
        &self,
        checkpoint_data: Arc<CheckpointData>,
    ) -> Result<Box<dyn Iterator<Item = ObjectEntry>>> {
        for checkpoint_transaction in &checkpoint_data.transactions {
            for object in checkpoint_transaction.output_objects.iter() {
                self.context.package_cache.update(object)?;
            }
        }

        // Run parallel processing
        let results = process_transactions(checkpoint_data.clone(), Arc::new(self.clone())).await?;

        // If end of epoch, evict package store
        if checkpoint_data
            .checkpoint_summary
            .end_of_epoch_data
            .is_some()
        {
            self.context
                .package_cache
                .resolver
                .package_store()
                .evict(SYSTEM_PACKAGE_ADDRESSES.iter().copied());
        }

        Ok(Box::new(results.into_iter()))
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::Object)
    }

    fn name(&self) -> &'static str {
        NAME
    }
}

impl ObjectHandler {
    pub fn new(
        package_cache: Arc<PackageCache>,
        package_filter: &Option<String>,
        metrics: AnalyticsMetrics,
    ) -> Self {
        let context = Arc::new(Context {
            package_filter: package_filter
                .clone()
                .map(|x| ObjectID::from_hex_literal(&x).unwrap()),
            metrics,
            package_cache,
        });

        Self {
            context,
        }
    }
    async fn process_transaction(
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
        effects: &TransactionEffects,
        context: Arc<Context>,
    ) -> Result<Vec<ObjectEntry>> {
        let object_status_tracker = ObjectStatusTracker::new(effects);
        let mut vec = Vec::new();
        for object in checkpoint_transaction.output_objects.iter() {
            if let Some(object_entry) = Self::process_object(
                epoch,
                checkpoint,
                timestamp_ms,
                object,
                &object_status_tracker,
                context.clone(),
            )
            .await?
            {
                vec.push(object_entry);
            }
        }
        for (object_ref, _) in effects.all_removed_objects().iter() {
            let object_entry = ObjectEntry {
                object_id: object_ref.0.to_string(),
                digest: object_ref.2.to_string(),
                version: u64::from(object_ref.1),
                type_: None,
                checkpoint,
                epoch,
                timestamp_ms,
                owner_type: None,
                owner_address: None,
                object_status: ObjectStatus::Deleted,
                initial_shared_version: None,
                previous_transaction: checkpoint_transaction.transaction.digest().base58_encode(),
                has_public_transfer: false,
                storage_rebate: None,
                bcs: "".to_string(),
                coin_type: None,
                coin_balance: None,
                struct_tag: None,
                object_json: None,
                bcs_length: 0,
            };
            vec.push(object_entry);
        }
        Ok(vec)
    }

    async fn check_type_hierarchy(
        context: Arc<Context>,
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
                _ => {} // Primitive types can't match a package ID
            }
        }

        // Resolve original package IDs in parallel
        let mut original_ids = JoinSet::new();

        for id in package_ids {
            let package_cache = context.package_cache.clone();
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

    // Object data. Only called if there are objects in the transaction.
    // Responsible to build the live object table.
    async fn process_object(
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        object: &Object,
        object_status_tracker: &ObjectStatusTracker,
        context: Arc<Context>,
    ) -> Result<Option<ObjectEntry>> {
        let move_obj_opt = object.data.try_as_move();
        let has_public_transfer = move_obj_opt
            .map(|o| o.has_public_transfer())
            .unwrap_or(false);
        let move_struct = if let Some((tag, contents)) = object
            .struct_tag()
            .and_then(|tag| object.data.try_as_move().map(|mo| (tag, mo.contents())))
        {
            match get_move_struct(&tag, contents, &context.package_cache.resolver).await {
                Ok(move_struct) => Some(move_struct),
                Err(err)
                    if err
                        .downcast_ref::<sui_types::object::bounded_visitor::Error>()
                        .filter(|e| {
                            matches!(e, sui_types::object::bounded_visitor::Error::OutOfBudget)
                        })
                        .is_some() =>
                {
                    context
                        .metrics
                        .total_too_large_to_deserialize
                        .with_label_values(&[NAME])
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

        let is_match = if let Some(package_id) = context.package_filter {
            if let Some(object_type) = object_type {
                let original_package_id = context
                    .package_cache
                    .get_original_package_id(package_id.into())
                    .await?;

                // Check if any type parameter matches the package filter
                let type_tag: TypeTag = object_type.clone().into();
                Self::check_type_hierarchy(context, &type_tag, original_package_id).await?
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
        let entry = ObjectEntry {
            object_id: object_id.to_string(),
            digest: object.digest().to_string(),
            version: object.version().value(),
            type_: object_type.map(|t| t.to_string()),
            checkpoint,
            epoch,
            timestamp_ms,
            owner_type: Some(get_owner_type(object)),
            owner_address: get_owner_address(object),
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
        Ok(Some(entry))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use move_core_types::{
        account_address::AccountAddress, identifier::Identifier, language_storage::StructTag,
    };
    use prometheus::Registry;
    use std::str::FromStr;
    use sui_types::TypeTag;

    fn create_struct_tag(
        addr: &str,
        module: &str,
        name: &str,
        type_params: Vec<TypeTag>,
    ) -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: AccountAddress::from_str(addr).unwrap(),
            module: Identifier::new(module).unwrap(),
            name: Identifier::new(name).unwrap(),
            type_params,
        }))
    }

    #[tokio::test]
    async fn test_check_type_hierarchy() {
        let temp_dir = tempfile::tempdir().unwrap();
        let registry = Registry::new();
        let metrics = AnalyticsMetrics::new(&registry);
        let package_cache = Arc::new(PackageCache::new(temp_dir.path(), "http://localhost:9000"));

        // Create context directly instead of using the handler
        let context = Arc::new(Context {
            package_filter: Some(ObjectID::from_hex_literal("0xabc").unwrap()),
            metrics,
            package_cache,
        });

        // 1. Direct match
        let type_tag = create_struct_tag("0xabc", "module", "Type", vec![]);
        assert!(ObjectHandler::check_type_hierarchy(
            context.clone(),
            &type_tag,
            ObjectID::from_hex_literal("0xabc").unwrap(),
        )
        .await
        .unwrap());

        // 2. Match in type parameter
        let inner_type = create_struct_tag("0xabc", "module", "Inner", vec![]);
        let type_tag = create_struct_tag("0xcde", "module", "Type", vec![inner_type]);
        assert!(ObjectHandler::check_type_hierarchy(
            context.clone(),
            &type_tag,
            ObjectID::from_hex_literal("0xabc").unwrap(),
        )
        .await
        .unwrap());

        // 3. Match in nested vector
        let inner_type = create_struct_tag("0xabc", "module", "Inner", vec![]);
        let vector_type = TypeTag::Vector(Box::new(inner_type));
        let type_tag = create_struct_tag("0xcde", "module", "Type", vec![vector_type]);
        assert!(ObjectHandler::check_type_hierarchy(
            context.clone(),
            &type_tag,
            ObjectID::from_hex_literal("0xabc").unwrap(),
        )
        .await
        .unwrap());

        // 4. No match
        let type_tag = create_struct_tag("0xcde", "module", "Type", vec![]);
        assert!(!ObjectHandler::check_type_hierarchy(
            context.clone(),
            &type_tag,
            ObjectID::from_hex_literal("0xabc").unwrap(),
        )
        .await
        .unwrap());

        // 5. Primitive type
        let type_tag = TypeTag::U64;
        assert!(!ObjectHandler::check_type_hierarchy(
            context.clone(),
            &type_tag,
            ObjectID::from_hex_literal("0xabc").unwrap(),
        )
        .await
        .unwrap());
    }
}
