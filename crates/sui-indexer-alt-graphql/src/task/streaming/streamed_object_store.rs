// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::object::Object as NativeObject;

use crate::scope::ExecutionObjectMap;
use crate::task::watermark::LEDGER_GRPC_PIPELINE;

use super::streamed_cache_eviction::EvictableCache;
use super::streamed_store::StreamedStore;

/// Object store for streaming subscriptions that holds object contents not yet available from the KV
/// backend.
///
/// A streamed checkpoint runs ahead of the KV backend that `KvLoader` reads. The current checkpoint's
/// own objects are served from the scope's execution map, but an object introduced by an *earlier*
/// streamed checkpoint (e.g. reached via `Object.previousTransaction` on live data) is neither in the
/// current checkpoint's execution map nor yet in the backend, so it would resolve empty. Objects from
/// streamed checkpoints are indexed here by `(id, version)` so those lookups resolve from memory;
/// once the backend catches up, the eviction task removes them and the backend serves them instead.
pub(crate) struct StreamedObjectStore {
    cache: StreamedStore<(ObjectID, SequenceNumber), NativeObject>,
}

impl StreamedObjectStore {
    pub(crate) fn new() -> Self {
        Self {
            cache: StreamedStore::new(),
        }
    }

    /// Index a streamed checkpoint's execution objects by `(id, version)`. Deleted/wrapped entries
    /// (`None`) are skipped: they have no contents to serve and fall through to the KV backend.
    // Called only from the staging-gated population path (the object source, `execution_objects`, is
    // staging-only) and from tests.
    #[cfg_attr(not(feature = "staging"), allow(dead_code))]
    pub(crate) fn index_objects(&self, checkpoint_seq: u64, objects: &ExecutionObjectMap) {
        for ((id, version), object) in objects.iter() {
            if let Some(object) = object {
                self.cache
                    .insert(checkpoint_seq, (*id, *version), object.clone());
            }
        }
    }

    /// The contents of the object at `(id, version)`, if held.
    pub(crate) fn get(&self, id: ObjectID, version: SequenceNumber) -> Option<NativeObject> {
        self.cache.get(&(id, version))
    }
}

impl EvictableCache for StreamedObjectStore {
    fn watermark_pipeline(&self) -> &'static str {
        LEDGER_GRPC_PIPELINE
    }

    fn evict_up_to(&self, indexed_checkpoint: u64) {
        self.cache.evict_up_to(indexed_checkpoint);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use super::super::streamed_cache_eviction::EvictableCache;
    use super::*;

    fn store() -> StreamedObjectStore {
        StreamedObjectStore::new()
    }

    fn oid(n: u8) -> ObjectID {
        ObjectID::from_single_byte(n)
    }

    fn native(n: u8) -> NativeObject {
        NativeObject::with_id_owner_for_testing(oid(n), sui_types::base_types::SuiAddress::ZERO)
    }

    fn map(entries: Vec<((ObjectID, SequenceNumber), Option<NativeObject>)>) -> ExecutionObjectMap {
        Arc::new(BTreeMap::from_iter(entries))
    }

    #[test]
    fn index_then_get_hits_by_id_and_version() {
        let store = store();
        let v = SequenceNumber::from_u64(3);
        store.index_objects(5, &map(vec![((oid(1), v), Some(native(1)))]));

        assert!(store.get(oid(1), v).is_some());
        assert!(store.get(oid(2), v).is_none());
        // A different version of the same id is a distinct key.
        assert!(store.get(oid(1), SequenceNumber::from_u64(4)).is_none());
    }

    #[test]
    fn deleted_or_wrapped_entries_are_skipped() {
        let store = store();
        let v = SequenceNumber::from_u64(3);
        store.index_objects(5, &map(vec![((oid(1), v), None)]));
        assert!(store.get(oid(1), v).is_none());
    }

    #[test]
    fn evict_up_to_removes_indexed_object() {
        let store = store();
        let v = SequenceNumber::from_u64(3);
        store.index_objects(5, &map(vec![((oid(1), v), Some(native(1)))]));

        store.evict_up_to(5);

        assert!(store.get(oid(1), v).is_none());
    }

    #[test]
    fn evict_up_to_keeps_object_reindexed_at_later_checkpoint() {
        let store = store();
        let v = SequenceNumber::from_u64(3);
        store.index_objects(5, &map(vec![((oid(1), v), Some(native(1)))]));
        store.index_objects(10, &map(vec![((oid(1), v), Some(native(1)))]));

        store.evict_up_to(5);

        assert!(store.get(oid(1), v).is_some());
    }
}
