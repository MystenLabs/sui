// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use sui_data_ingestion_core::Worker;
use sui_types::{TypeTag, SYSTEM_PACKAGE_ADDRESSES};
use tokio::sync::Mutex;

use sui_json_rpc_types::SuiMoveStruct;
use sui_package_resolver::Resolver;
use sui_types::base_types::ObjectID;
use sui_types::effects::TransactionEffects;
use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};
use sui_types::object::Object;

use crate::handlers::{
    get_move_struct, get_owner_address, get_owner_type, initial_shared_version, AnalyticsHandler,
    ObjectStatusTracker,
};
use crate::AnalyticsMetrics;

use crate::package_store::{LocalDBPackageStore, PackageCache};
use crate::tables::{ObjectEntry, ObjectStatus};
use crate::FileType;

pub struct ObjectHandler {
    state: Mutex<State>,
    package_filter: Option<ObjectID>,
    metrics: AnalyticsMetrics,
}

struct State {
    objects: Vec<ObjectEntry>,
    package_store: LocalDBPackageStore,
    resolver: Resolver<PackageCache>,
}

#[async_trait::async_trait]
impl Worker for ObjectHandler {
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
                &checkpoint_transaction.effects,
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
impl AnalyticsHandler<ObjectEntry> for ObjectHandler {
    async fn read(&self) -> Result<Vec<ObjectEntry>> {
        let mut state = self.state.lock().await;
        let cloned = state.objects.clone();
        state.objects.clear();
        Ok(cloned)
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::Object)
    }

    fn name(&self) -> &str {
        "object"
    }
}

impl ObjectHandler {
    pub fn new(
        package_store: LocalDBPackageStore,
        package_filter: &Option<String>,
        metrics: AnalyticsMetrics,
    ) -> Self {
        let state = State {
            objects: vec![],
            package_store: package_store.clone(),
            resolver: Resolver::new(PackageCache::new(package_store)),
        };
        Self {
            state: Mutex::new(state),
            package_filter: package_filter
                .clone()
                .map(|x| ObjectID::from_hex_literal(&x).unwrap()),
            metrics,
        }
    }
    async fn process_transaction(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
        effects: &TransactionEffects,
        state: &mut State,
    ) -> Result<()> {
        let object_status_tracker = ObjectStatusTracker::new(effects);
        for object in checkpoint_transaction.output_objects.iter() {
            self.process_object(
                epoch,
                checkpoint,
                timestamp_ms,
                object,
                &object_status_tracker,
                state,
            )
            .await?;
        }
        for (object_ref, _) in effects.all_removed_objects().iter() {
            let entry = ObjectEntry {
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
            state.objects.push(entry);
        }
        Ok(())
    }

    async fn check_type_hierarchy(
        &self,
        type_tag: &TypeTag,
        original_package_id: ObjectID,
        state: &mut State,
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
            let package_store = state.package_store.clone();
            original_ids.spawn(async move { package_store.get_original_package_id(id).await });
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
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        object: &Object,
        object_status_tracker: &ObjectStatusTracker,
        state: &mut State,
    ) -> Result<()> {
        let move_obj_opt = object.data.try_as_move();
        let has_public_transfer = move_obj_opt
            .map(|o| o.has_public_transfer())
            .unwrap_or(false);
        let move_struct = if let Some((tag, contents)) = object
            .struct_tag()
            .and_then(|tag| object.data.try_as_move().map(|mo| (tag, mo.contents())))
        {
            match get_move_struct(&tag, contents, &state.resolver).await {
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
                        .with_label_values(&[self.name()])
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
                let original_package_id = state
                    .package_store
                    .get_original_package_id(package_id.into())
                    .await?;

                // Check if any type parameter matches the package filter
                let type_tag: TypeTag = object_type.clone().into();
                self.check_type_hierarchy(&type_tag, original_package_id, state)
                    .await?
            } else {
                false
            }
        } else {
            true
        };

        if !is_match {
            return Ok(());
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
        state.objects.push(entry);
        Ok(())
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
        let package_store = LocalDBPackageStore::new(temp_dir.path(), "http://localhost:9000");
        let handler = ObjectHandler::new(package_store, &Some("0xabc".to_string()), metrics);
        let mut state = handler.state.lock().await;

        // 1. Direct match
        let type_tag = create_struct_tag("0xabc", "module", "Type", vec![]);
        assert!(handler
            .check_type_hierarchy(
                &type_tag,
                ObjectID::from_hex_literal("0xabc").unwrap(),
                &mut state
            )
            .await
            .unwrap());

        // 2. Match in type parameter
        let inner_type = create_struct_tag("0xabc", "module", "Inner", vec![]);
        let type_tag = create_struct_tag("0xcde", "module", "Type", vec![inner_type]);
        assert!(handler
            .check_type_hierarchy(
                &type_tag,
                ObjectID::from_hex_literal("0xabc").unwrap(),
                &mut state
            )
            .await
            .unwrap());

        // 3. Match in nested vector
        let inner_type = create_struct_tag("0xabc", "module", "Inner", vec![]);
        let vector_type = TypeTag::Vector(Box::new(inner_type));
        let type_tag = create_struct_tag("0xcde", "module", "Type", vec![vector_type]);
        assert!(handler
            .check_type_hierarchy(
                &type_tag,
                ObjectID::from_hex_literal("0xabc").unwrap(),
                &mut state
            )
            .await
            .unwrap());

        // 4. No match
        let type_tag = create_struct_tag("0xcde", "module", "Type", vec![]);
        assert!(!handler
            .check_type_hierarchy(
                &type_tag,
                ObjectID::from_hex_literal("0xabc").unwrap(),
                &mut state
            )
            .await
            .unwrap());

        // 5. Primitive type
        let type_tag = TypeTag::U64;
        assert!(!handler
            .check_type_hierarchy(
                &type_tag,
                ObjectID::from_hex_literal("0xabc").unwrap(),
                &mut state
            )
            .await
            .unwrap());
    }
}
