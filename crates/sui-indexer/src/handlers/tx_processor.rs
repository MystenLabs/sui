// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use move_binary_format::CompiledModule;
use move_core_types::language_storage::ModuleId;
use mysten_metrics::monitored_scope;
use mysten_metrics::spawn_monitored_task;
use sui_sdk::SuiClient;
use tokio::sync::watch;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use sui_types::object::Object;
use tokio::time::Duration;
use tokio::time::Instant;

use sui_json_rpc::get_balance_changes_from_effect;
use sui_json_rpc::get_object_changes;
use sui_json_rpc::ObjectProvider;
use sui_json_rpc_types::SuiObjectDataOptions;
use sui_types::base_types::SequenceNumber;
use sui_types::digests::TransactionDigest;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::transaction::{TransactionData, TransactionDataAPI};
use tracing::debug;

use sui_types::base_types::ObjectID;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::errors::IndexerError;
use crate::metrics::IndexerMetrics;
use crate::store::IndexerStoreV2;

use crate::types_v2::IndexedPackage;
use crate::types_v2::{IndexedObjectChange, IndexerResult};

pub struct InMemObjectCache {
    id_map: HashMap<ObjectID, (Arc<Object>, CheckpointSequenceNumber)>,
    seq_map: HashMap<(ObjectID, SequenceNumber), (Arc<Object>, CheckpointSequenceNumber)>,
    packages: HashMap<(ObjectID, String), (Arc<CompiledModule>, CheckpointSequenceNumber)>,
}

impl InMemObjectCache {
    // FIXME: rename and remoev arc<mutex<
    pub fn start(
        commit_watcher: watch::Receiver<Option<CheckpointSequenceNumber>>,
    ) -> Arc<Mutex<Self>> {
        let cache = Arc::new(Mutex::new(Self {
            id_map: HashMap::new(),
            seq_map: HashMap::new(),
            packages: HashMap::new(),
        }));
        let cache_clone = cache.clone();
        spawn_monitored_task!(Self::remove_committed(cache_clone, commit_watcher));
        cache
    }

    pub async fn remove_committed(
        cache: Arc<Mutex<Self>>,
        commit_watcher: watch::Receiver<Option<CheckpointSequenceNumber>>,
    ) {

        // GC every 10 minutes
        let mut interval = tokio::time::interval_at(Instant::now(), Duration::from_secs(600));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let _scope = monitored_scope("InMemObjectCache::remove_committed");
            let Some(committed_checkpoint) = *commit_watcher.borrow() else {
                continue;
            };
            debug!("About to GC packages older than: {committed_checkpoint}");

            let mut cache = cache.lock().unwrap();
            let mut to_remove = vec![];
            for (id, (_, checkpoint_seq)) in cache.packages.iter() {
                if *checkpoint_seq <= committed_checkpoint {
                    to_remove.push(id.clone());
                }
            }
            for id in to_remove {
                cache.packages.remove(&id);
            }
        }
    }

    pub fn insert_object(&mut self, object: Object, checkpoint_seq: CheckpointSequenceNumber) {
        let obj = Arc::new(object);
        self.id_map.insert(obj.id(), (obj.clone(), checkpoint_seq));
        self.seq_map
            .insert((obj.id(), obj.version()), (obj, checkpoint_seq));
    }

    pub fn insert_packages(
        &mut self,
        new_packages: &[IndexedPackage],
        checkpoint_seq: CheckpointSequenceNumber,
    ) {
        let new_packages = new_packages
            .iter()
            .flat_map(|p| {
                p.move_package
                    .serialized_module_map()
                    .iter()
                    .map(|(module_name, bytes)| {
                        let module = CompiledModule::deserialize_with_defaults(bytes).unwrap();
                        (
                            (p.package_id, module_name.clone()),
                            (Arc::new(module), checkpoint_seq),
                        )
                    })
            })
            .collect::<HashMap<_, _>>();
        self.packages.extend(new_packages);
    }

    pub fn get(&self, id: &ObjectID, version: Option<&SequenceNumber>) -> Option<&Object> {
        if let Some(version) = version {
            self.seq_map.get(&(*id, *version)).map(|o| o.0.as_ref())
        } else {
            self.id_map.get(id).map(|o| o.0.as_ref())
        }
    }

    pub fn get_module_by_id(&self, id: &ModuleId) -> Option<Arc<CompiledModule>> {
        let package_id = ObjectID::from(*id.address());
        let name = id.name().to_string();
        self.packages
            .get(&(package_id, name))
            .as_ref()
            .map(|(m, _)| m.clone())
    }
}

pub struct TxChangesProcessor {
    // state: &'a S,
    object_cache: Arc<Mutex<InMemObjectCache>>,
    // sui_client: Arc<SuiClient>,
    metrics: IndexerMetrics,
}

impl TxChangesProcessor {
    pub fn new(
        // state: &'a S,
        objects: &[&Object],
        object_cache: Arc<Mutex<InMemObjectCache>>,
        // sui_client: Arc<SuiClient>,
        // FIXME remove this
        checkpoint_seq: CheckpointSequenceNumber,
        metrics: IndexerMetrics,
    ) -> Self {
        for obj in objects {
            object_cache
                .lock()
                .unwrap()
                .insert_object(<&Object>::clone(obj).clone(), checkpoint_seq);
        }
        Self {
            // state,
            object_cache,
            // sui_client,
            metrics,
        }
    }

    pub(crate) async fn get_changes(
        &self,
        tx: &TransactionData,
        effects: &TransactionEffects,
        tx_digest: &TransactionDigest,
    ) -> IndexerResult<(
        Vec<sui_json_rpc_types::BalanceChange>,
        Vec<IndexedObjectChange>,
    )> {
        let _timer = self
            .metrics
            .indexing_tx_object_changes_latency
            .start_timer();
        let object_change: Vec<_> = get_object_changes(
            self,
            tx.sender(),
            effects.modified_at_versions(),
            effects.all_changed_objects(),
            effects.all_removed_objects(),
        )
        .await?
        .into_iter()
        .map(IndexedObjectChange::from)
        .collect();
        let balance_change = get_balance_changes_from_effect(
            self,
            effects,
            tx.input_objects().unwrap_or_else(|e| {
                panic!(
                    "Checkpointed tx {:?} has inavlid input objects: {e}",
                    tx_digest,
                )
            }),
            None,
        )
        .await?;
        Ok((balance_change, object_change))
    }
}

impl Drop for TxChangesProcessor {
    fn drop(&mut self) {
        let _scope = monitored_scope("TxChangesProcessor::drop");
        let mut cache = self.object_cache.lock().unwrap();
        cache.id_map.clear();
        cache.seq_map.clear();
    }
}


// Note: the implementation of `ObjectProvider` for `TxChangesProcessor`
// is NOT trivial. It needs to be a ObjectProvider to do
// `try_create_dynamic_field_info`. So the logic below is tailored towards that.

#[async_trait]
impl ObjectProvider for TxChangesProcessor {
    type Error = IndexerError;

    async fn get_object(
        &self,
        id: &ObjectID,
        version: &SequenceNumber,
    ) -> Result<Object, Self::Error> {
        let object = self
            .object_cache
            .lock()
            .unwrap()
            .get(id, Some(version))
            .as_ref()
            .map(|o| <&Object>::clone(o).clone());
        if let Some(o) = object {
            self.metrics.indexing_get_object_in_mem_hit.inc();
            return Ok(o);
        }

        panic!("Object {} is not found in TxChangesProcessor as an ObjectProvider (fn get_object)", id);
        // if let Some(object) = self.state.get_object(*id, Some(*version)).await? {
        //     self.metrics.indexing_get_object_db_hit.inc();
        //     return Ok(object);
        // }

        // Last resort - read the version from remote. Here's an edge case why this may be needed:
        // Say object O is at version V1 at Checkpoint C1, and then updated to V2 at Checkpoint C2.
        // When we process C2, we calculate the Object/BalanceChange and what not, all go well.
        // But the DB commits takes two steps, 1. commit txes, objects, etc and 2. commit checkpoints.
        // If the system crashed between these two steps, when it restarts, only V2 can be found in DB.
        // It needs to reprocess C2 because checkpoint data is not committed yet. Now it will find
        // difficulty in getting V1.
        // If we always commits everything in one DB transactions, then this is a non-issue. However:
        // 1. this is a big commitment that comes with performance trade-offs
        // 2. perhaps one day we will use a system that has no transaction support.
        // let object = self
        //     .sui_client
        //     .read_api()
        //     .try_get_parsed_past_object(*id, *version, SuiObjectDataOptions::bcs_lossless())
        //     .await
        //     .map_err(|e| IndexerError::FullNodeReadingError(e.to_string()))?
        //     .into_object()
        //     .map_err(|e| IndexerError::DataTransformationError(e.to_string()))?
        //     .try_into()
        //     .map_err(|e: anyhow::Error| IndexerError::DataTransformationError(e.to_string()))?;

        // self.metrics.indexing_get_object_remote_hit.inc();
        // Ok(object)
    }

    async fn find_object_lt_or_eq_version(
        &self,
        id: &ObjectID,
        version: &SequenceNumber,
    ) -> Result<Option<Object>, Self::Error> {
        // First look up the exact version in object_cache.
        let object = self
            .object_cache
            .lock()
            .unwrap()
            .get(id, Some(version))
            .as_ref()
            .map(|o| <&Object>::clone(o).clone());
        if let Some(o) = object {
            self.metrics.indexing_get_object_in_mem_hit.inc();
            return Ok(Some(o));
        }

        // Second look up the latest version in object_cache. This may be
        // called when the object is deleted hence the version at deletion
        // is given.
        let object = self
            .object_cache
            .lock()
            .unwrap()
            .get(id, None)
            .as_ref()
            .map(|o| <&Object>::clone(o).clone());
        if let Some(o) = object {
            if o.version() > *version {
                panic!("Found a higher version {} for object {}, expected lt_or_eq {}", o.version(), id, *version);
            }
            if o.version() <= *version {
                self.metrics.indexing_get_object_in_mem_hit.inc();
                return Ok(Some(o));
            }
        }

        panic!("Object {} is not found in TxChangesProcessor as an ObjectProvider (fn find_object_lt_or_eq_version)", id);

        // // Second, look up the object with the latest version and make sure the version is lt_or_eq
        // match self.state.get_object(*id, None).await? {
        //     None => {
        //         // TODO: can we not panic here but in the callsite of these functions?
        //         panic!(
        //             "Object {} is not found in TxChangesProcessor as an ObjectProvider",
        //             id
        //         );
        //     }
        //     Some(object) => {
        //         self.metrics.indexing_get_object_db_hit.inc();
        //         assert!(object.version() <= *version);
        //         Ok(Some(object))
        //     }
        // }
    }
}
