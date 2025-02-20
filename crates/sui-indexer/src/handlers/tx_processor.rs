// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use async_trait::async_trait;
use sui_json_rpc::get_balance_changes_from_effect;
use sui_json_rpc::get_object_changes;
use sui_json_rpc::ObjectProvider;
use sui_rpc_api::CheckpointData;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::digests::TransactionDigest;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::object::Object;
use sui_types::transaction::{TransactionData, TransactionDataAPI};

use crate::errors::IndexerError;
use crate::metrics::IndexerMetrics;
use crate::types::{IndexedObjectChange, IndexerResult};

pub struct InMemObjectCache {
    id_map: HashMap<ObjectID, Object>,
    seq_map: HashMap<(ObjectID, SequenceNumber), Object>,
}

impl InMemObjectCache {
    pub fn new() -> Self {
        Self {
            id_map: HashMap::new(),
            seq_map: HashMap::new(),
        }
    }

    pub fn insert_object(&mut self, obj: Object) {
        self.id_map.insert(obj.id(), obj.clone());
        self.seq_map.insert((obj.id(), obj.version()), obj);
    }

    pub fn get(&self, id: &ObjectID, version: Option<&SequenceNumber>) -> Option<&Object> {
        if let Some(version) = version {
            self.seq_map.get(&(*id, *version))
        } else {
            self.id_map.get(id)
        }
    }
}

impl Default for InMemObjectCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Along with InMemObjectCache, TxChangesProcessor implements ObjectProvider
/// so it can be used in indexing write path to get object/balance changes.
/// Its lifetime is per checkpoint.
pub struct TxChangesProcessor {
    object_cache: InMemObjectCache,
    metrics: IndexerMetrics,
}

impl TxChangesProcessor {
    pub fn new(objects: &[&Object], metrics: IndexerMetrics) -> Self {
        let mut object_cache = InMemObjectCache::new();
        for obj in objects {
            object_cache.insert_object(<&Object>::clone(obj).clone());
        }
        Self {
            object_cache,
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
            effects,
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
                    "Checkpointed tx {:?} has invalid input objects: {e}",
                    tx_digest,
                )
            }),
            None,
        )
        .await?;
        Ok((balance_change, object_change))
    }
}

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
            .get(id, Some(version))
            .as_ref()
            .map(|o| <&Object>::clone(o).clone());
        if let Some(o) = object {
            self.metrics.indexing_get_object_in_mem_hit.inc();
            return Ok(o);
        }

        panic!(
            "Object {} is not found in TxChangesProcessor as an ObjectProvider (fn get_object)",
            id
        );
    }

    async fn find_object_lt_or_eq_version(
        &self,
        id: &ObjectID,
        version: &SequenceNumber,
    ) -> Result<Option<Object>, Self::Error> {
        // First look up the exact version in object_cache.
        let object = self
            .object_cache
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
            .get(id, None)
            .as_ref()
            .map(|o| <&Object>::clone(o).clone());
        if let Some(o) = object {
            if o.version() > *version {
                panic!(
                    "Found a higher version {} for object {}, expected lt_or_eq {}",
                    o.version(),
                    id,
                    *version
                );
            }
            if o.version() <= *version {
                self.metrics.indexing_get_object_in_mem_hit.inc();
                return Ok(Some(o));
            }
        }

        panic!("Object {} is not found in TxChangesProcessor as an ObjectProvider (fn find_object_lt_or_eq_version)", id);
    }
}

// This is a struct that is used to extract SuiSystemState and its dynamic children
// for end-of-epoch indexing.
pub(crate) struct EpochEndIndexingObjectStore<'a> {
    objects: Vec<&'a Object>,
}

impl<'a> EpochEndIndexingObjectStore<'a> {
    pub fn new(data: &'a CheckpointData) -> Self {
        Self {
            objects: data.latest_live_output_objects(),
        }
    }
}

impl<'a> sui_types::storage::ObjectStore for EpochEndIndexingObjectStore<'a> {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.objects
            .iter()
            .find(|o| o.id() == *object_id)
            .cloned()
            .cloned()
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> Option<Object> {
        self.objects
            .iter()
            .find(|o| o.id() == *object_id && o.version() == version)
            .cloned()
            .cloned()
    }
}
