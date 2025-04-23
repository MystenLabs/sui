// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use sui_data_ingestion_core::Worker;
use sui_types::{TypeTag, SYSTEM_PACKAGE_ADDRESSES};
use tokio::sync::{Mutex, Semaphore};

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

use std::sync::Arc;
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct ObjectHandler {
    /// Shared mutable state protected by a mutex and wrapped in `Arc` so it can be
    /// cloned into the async tasks.
    state: Arc<Mutex<State>>,
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

        // ──────────────────────────────────────────────────────────────────────────
        // Build a Semaphore chain so we can push results to `state.objects` in the
        // same order as `checkpoint_transactions`, while allowing *everything*
        // else to run in parallel.
        // ──────────────────────────────────────────────────────────────────────────
        let txn_count = checkpoint_transactions.len();
        let semaphores: Vec<_> = (0..txn_count)
            .map(|i| {
                if i == 0 {
                    Arc::new(Semaphore::new(1)) // first txn proceeds immediately
                } else {
                    Arc::new(Semaphore::new(0))
                }
            })
            .collect();

        let mut handles: Vec<JoinHandle<Result<()>>> = Vec::with_capacity(txn_count);

        for (idx, checkpoint_transaction) in checkpoint_transactions.iter().cloned().enumerate() {
            let handler = self.clone();
            let start_sem = semaphores[idx].clone();
            let next_sem = semaphores.get(idx + 1).cloned();

            // Snapshot any data we need from the summary (Copy types, cheap).
            let epoch = checkpoint_summary.epoch;
            let checkpoint_seq = checkpoint_summary.sequence_number;
            let timestamp_ms = checkpoint_summary.timestamp_ms;
            let end_of_epoch = checkpoint_summary.end_of_epoch_data.is_some();

            let handle = tokio::spawn(async move {
                // ───── 1. Heavy work off‑mutex ───────────────────────────────────
                // Clone the package store so we can mutate it freely in parallel.
                let package_store = {
                    let guard = handler.state.lock().await;
                    guard.package_store.clone()
                };

                let mut local_state = State {
                    objects: Vec::new(),
                    package_store: package_store.clone(),
                    resolver: Resolver::new(PackageCache::new(package_store)),
                };

                // Update local package store & compute ObjectEntry rows.
                for object in checkpoint_transaction.output_objects.iter() {
                    local_state.package_store.update(object)?;
                }

                handler
                    .process_transaction(
                        epoch,
                        checkpoint_seq,
                        timestamp_ms,
                        &checkpoint_transaction,
                        &checkpoint_transaction.effects,
                        &mut local_state,
                    )
                    .await?;

                // ───── 2. Append results in order ────────────────────────────────
                // Wait for our turn.
                let _ = start_sem.acquire().await?;

                {
                    let mut shared_state = handler.state.lock().await;
                    shared_state.objects.extend(local_state.objects.into_iter());

                    if end_of_epoch {
                        shared_state
                            .resolver
                            .package_store()
                            .evict(SYSTEM_PACKAGE_ADDRESSES.iter().copied());
                    }
                }

                // Signal the next task.
                if let Some(next) = next_sem {
                    next.add_permits(1);
                }

                Ok(())
            });

            handles.push(handle);
        }

        // Propagate any error.
        for h in handles {
            h.await??;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<ObjectEntry> for ObjectHandler {
    async fn read(&self) -> Result<Vec<ObjectEntry>> {
        let mut state = self.state.lock().await;
        Ok(std::mem::take(&mut state.objects))
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
            state: Arc::new(Mutex::new(state)),
            package_filter: package_filter
                .clone()
                .map(|x| ObjectID::from_hex_literal(&x).unwrap()),
            metrics,
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Per‑transaction processing (unchanged, but now called in parallel).
    // ─────────────────────────────────────────────────────────────────────────────
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

        // Removed objects.
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
                bcs: String::new(),
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

        // Collect all package IDs via stack traversal.
        let mut types = vec![type_tag];
        let mut package_ids = BTreeSet::new();

        while let Some(tt) = types.pop() {
            match tt {
                TypeTag::Struct(s) => {
                    package_ids.insert(s.address);
                    types.extend(s.type_params.iter());
                }
                TypeTag::Vector(inner) => types.push(inner.as_ref()),
                _ => {}
            }
        }

        // Resolve in parallel.
        let mut set = JoinSet::new();
        for id in package_ids {
            let ps = state.package_store.clone();
            set.spawn(async move { ps.get_original_package_id(id).await });
        }

        while let Some(res) = set.join_next().await {
            if res?? == original_package_id {
                return Ok(true);
            }
        }
        Ok(false)
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Object‑level processing (unchanged logic).
    // ─────────────────────────────────────────────────────────────────────────────
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

        // Attempt to build SuiMoveStruct, but handle budget errors gracefully.
        let move_struct = if let Some((tag, contents)) = object
            .struct_tag()
            .and_then(|tag| object.data.try_as_move().map(|mo| (tag, mo.contents())))
        {
            match get_move_struct(&tag, contents, &state.resolver).await {
                Ok(ms) => Some(ms),
                Err(err)
                    if err
                        .downcast_ref::<sui_types::object::bounded_visitor::Error>()
                        .filter(|e| {
                            matches!(e, sui_types::object::bounded_visitor::Error::OutOfBudget)
                        })
                        .is_some() =>
                {
                    // Too large, skip deserializing.
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

        let (struct_tag, sui_move_struct) = if let Some(ms) = move_struct {
            match ms.into() {
                SuiMoveStruct::WithTypes { type_, fields } => {
                    (Some(type_), Some(SuiMoveStruct::WithFields(fields)))
                }
                fields => (object.struct_tag(), Some(fields)),
            }
        } else {
            (None, None)
        };

        let object_type = move_obj_opt.map(|o| o.type_());

        // Filter by package, if requested.
        let is_match = if let Some(package_id) = self.package_filter {
            if let Some(obj_type) = &object_type {
                let original_package_id = state
                    .package_store
                    .get_original_package_id(package_id.into())
                    .await?;

                let type_tag: TypeTag = (*obj_type).clone().into();
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

        // Build row.
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
            bcs: String::new(),
            bcs_length: bcs::to_bytes(object).unwrap().len() as u64,
            coin_type: object.coin_type_maybe().map(|t| t.to_string()),
            coin_balance: object
                .coin_type_maybe()
                .map(|_| object.get_coin_value_unsafe()),
            struct_tag: struct_tag.map(|s| s.to_string()),
            object_json: sui_move_struct.map(|s| s.to_json_value().to_string()),
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
        let package_store =
            LocalDBPackageStore::new(temp_dir.path(), "http://localhost:9000", metrics.clone());
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
