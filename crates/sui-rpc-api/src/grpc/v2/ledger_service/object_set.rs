// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;

use mysten_common::ZipDebugEqIteratorExt;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_types::full_checkpoint_content::ObjectSet;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;

use crate::RpcError;
use crate::reader::{StateReader, TransactionRead};

/// Request-scoped memo of objects fetched for object-set rendering. One
/// instance serves a single request (a List stream or one Get/BatchGet call)
/// and drops with it. Entries are never evicted: `ObjectKey` is versioned, so
/// values are immutable, and the request deadline bounds the cache lifetime.
/// List chunk workers run sequentially, so the mutex callers wrap this in is
/// uncontended.
#[derive(Default)]
pub(crate) struct RequestObjectCache {
    objects: HashMap<ObjectKey, Object>,
}

/// Key accounting for one `RequestObjectCache::multi_get` call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ObjectFetchStats {
    /// Unique keys served from the request-scoped cache.
    pub(crate) cache_hits: usize,
    /// Unique keys forwarded to the backing store.
    pub(crate) store_keys: usize,
}

impl RequestObjectCache {
    /// Fetches keys absent from the cache and returns objects in input order.
    ///
    /// `keys` must be unique. Present objects are cached; missing objects are
    /// fetched again by later calls. `fetch` is not invoked when all keys are
    /// cached.
    pub(crate) fn multi_get(
        &mut self,
        keys: &[ObjectKey],
        fetch: impl FnOnce(&[ObjectKey]) -> Vec<Option<Object>>,
    ) -> (Vec<Option<Object>>, ObjectFetchStats) {
        let missing = keys
            .iter()
            .copied()
            .filter(|key| !self.objects.contains_key(key))
            .collect::<Vec<_>>();
        let stats = ObjectFetchStats {
            cache_hits: keys.len() - missing.len(),
            store_keys: missing.len(),
        };

        if !missing.is_empty() {
            for (key, object) in missing.iter().copied().zip_debug_eq(fetch(&missing)) {
                if let Some(object) = object {
                    self.objects.insert(key, object);
                }
            }
        }

        let objects = keys
            .iter()
            .map(|key| self.objects.get(key).cloned())
            .collect();
        (objects, stats)
    }
}

pub(crate) fn mask_requests_object_set(mask: &FieldMaskTree) -> bool {
    mask.contains(ExecutedTransaction::BALANCE_CHANGES_FIELD)
        || mask.contains(ExecutedTransaction::EFFECTS_FIELD)
}

/// Object keys read or written by this transaction, in the same order returned
/// by `get_transaction_object_set`.
pub(crate) fn transaction_object_keys(read: &TransactionRead) -> Vec<ObjectKey> {
    sui_types::storage::get_transaction_object_set(
        &read.transaction,
        &read.effects,
        read.unchanged_loaded_runtime_objects
            .as_deref()
            .unwrap_or_default(),
    )
    .into_iter()
    .collect()
}

/// Fetch the union of the requested keys once and reconstruct each transaction's object set.
pub(crate) fn build_object_sets(
    items: &[(sui_sdk_types::Digest, Vec<ObjectKey>)],
    fetch: impl FnOnce(&[ObjectKey]) -> Vec<Option<Object>>,
) -> Result<Vec<ObjectSet>, RpcError> {
    let unique_keys = items
        .iter()
        .flat_map(|(_, keys)| keys.iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let objects_by_key = if unique_keys.is_empty() {
        HashMap::new()
    } else {
        unique_keys
            .iter()
            .copied()
            .zip_debug_eq(fetch(&unique_keys))
            .filter_map(|(key, object)| object.map(|object| (key, object)))
            .collect::<HashMap<_, _>>()
    };

    let mut object_sets = Vec::with_capacity(items.len());
    for (digest, object_keys) in items {
        let mut object_set = ObjectSet::default();
        for object_key in object_keys {
            let object = objects_by_key.get(object_key).ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("unable to fetch object {object_key:?} for transaction {digest}"),
                )
            })?;
            object_set.insert(object.clone());
        }
        object_sets.push(object_set);
    }

    Ok(object_sets)
}

pub(crate) fn fetch_object_sets_for_chunk(
    reader: &StateReader,
    reads: &[TransactionRead],
    mask: &FieldMaskTree,
    cache: &Mutex<RequestObjectCache>,
) -> Result<(Vec<ObjectSet>, ObjectFetchStats), RpcError> {
    if !mask_requests_object_set(mask) {
        return Ok((
            vec![ObjectSet::default(); reads.len()],
            ObjectFetchStats::default(),
        ));
    }

    let items = reads
        .iter()
        .map(|read| (read.digest, transaction_object_keys(read)))
        .collect::<Vec<_>>();
    let mut stats = ObjectFetchStats::default();
    let object_sets = build_object_sets(&items, |unique_keys| {
        let (objects, fetch_stats) = cache
            .lock()
            .expect("request object cache mutex poisoned")
            .multi_get(unique_keys, |missing| {
                reader.inner().multi_get_objects_by_key(missing)
            });
        stats = fetch_stats;
        objects
    })?;
    Ok((object_sets, stats))
}

pub(crate) fn fetch_transaction_object_set(
    reader: &StateReader,
    read: &TransactionRead,
    mask: &FieldMaskTree,
    cache: &Mutex<RequestObjectCache>,
) -> Result<ObjectSet, RpcError> {
    let (mut object_sets, _) =
        fetch_object_sets_for_chunk(reader, std::slice::from_ref(read), mask, cache)?;
    Ok(object_sets.pop().expect("one object set per read"))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use sui_types::base_types::{ObjectID, SuiAddress};

    use super::*;

    fn digest(byte: u8) -> sui_sdk_types::Digest {
        sui_sdk_types::Digest::new([byte; 32])
    }

    fn object(byte: u8) -> Object {
        Object::with_id_owner_for_testing(
            ObjectID::new([byte; ObjectID::LENGTH]),
            SuiAddress::random_for_testing_only(),
        )
    }

    fn object_key(object: &Object) -> ObjectKey {
        ObjectKey(object.id(), object.version())
    }

    #[test]
    fn cache_fetches_only_missing_keys_and_reports_stats() {
        let first = object(1);
        let second = object(2);
        let third = object(3);
        let first_key = object_key(&first);
        let second_key = object_key(&second);
        let third_key = object_key(&third);
        let fetch_calls = Cell::new(0);
        let mut cache = RequestObjectCache::default();

        let (objects, stats) = cache.multi_get(&[first_key, second_key], |keys| {
            fetch_calls.set(fetch_calls.get() + 1);
            assert_eq!(keys, &[first_key, second_key]);
            vec![Some(first.clone()), Some(second.clone())]
        });
        assert_eq!(fetch_calls.get(), 1);
        assert_eq!(
            stats,
            ObjectFetchStats {
                cache_hits: 0,
                store_keys: 2,
            }
        );
        assert_eq!(objects, vec![Some(first.clone()), Some(second.clone())]);

        let (objects, stats) = cache.multi_get(&[second_key, third_key], |keys| {
            fetch_calls.set(fetch_calls.get() + 1);
            assert_eq!(keys, &[third_key]);
            vec![Some(third.clone())]
        });
        assert_eq!(fetch_calls.get(), 2);
        assert_eq!(
            stats,
            ObjectFetchStats {
                cache_hits: 1,
                store_keys: 1,
            }
        );
        assert_eq!(objects, vec![Some(second), Some(third)]);
    }

    #[test]
    fn fully_cached_multi_get_skips_fetch() {
        let first = object(1);
        let second = object(2);
        let first_key = object_key(&first);
        let second_key = object_key(&second);
        let fetch_calls = Cell::new(0);
        let mut cache = RequestObjectCache::default();

        let (objects, stats) = cache.multi_get(&[first_key, second_key], |keys| {
            fetch_calls.set(fetch_calls.get() + 1);
            assert_eq!(keys, &[first_key, second_key]);
            vec![Some(first.clone()), Some(second.clone())]
        });
        assert_eq!(
            stats,
            ObjectFetchStats {
                cache_hits: 0,
                store_keys: 2,
            }
        );
        assert_eq!(objects, vec![Some(first.clone()), Some(second.clone())]);

        let (objects, stats) = cache.multi_get(&[first_key, second_key], |_| {
            fetch_calls.set(fetch_calls.get() + 1);
            panic!("fully cached keys must not be fetched")
        });
        assert_eq!(fetch_calls.get(), 1);
        assert_eq!(
            stats,
            ObjectFetchStats {
                cache_hits: 2,
                store_keys: 0,
            }
        );
        assert_eq!(objects, vec![Some(first), Some(second)]);
    }

    #[test]
    fn missing_objects_are_not_cached() {
        let missing = object(1);
        let missing_key = object_key(&missing);
        let fetch_calls = Cell::new(0);
        let mut cache = RequestObjectCache::default();

        let (objects, stats) = cache.multi_get(&[missing_key], |keys| {
            fetch_calls.set(fetch_calls.get() + 1);
            assert_eq!(keys, &[missing_key]);
            vec![None]
        });
        assert_eq!(fetch_calls.get(), 1);
        assert_eq!(
            stats,
            ObjectFetchStats {
                cache_hits: 0,
                store_keys: 1,
            }
        );
        assert_eq!(objects, vec![None]);

        let (objects, stats) = cache.multi_get(&[missing_key], |keys| {
            fetch_calls.set(fetch_calls.get() + 1);
            assert_eq!(keys, &[missing_key]);
            vec![Some(missing.clone())]
        });
        assert_eq!(fetch_calls.get(), 2);
        assert_eq!(
            stats,
            ObjectFetchStats {
                cache_hits: 0,
                store_keys: 1,
            }
        );
        assert_eq!(objects, vec![Some(missing)]);
    }

    #[test]
    fn shared_cache_dedupes_keys_across_successive_chunk_builds() {
        let first = object(1);
        let shared = object(2);
        let second = object(3);
        let first_key = object_key(&first);
        let shared_key = object_key(&shared);
        let second_key = object_key(&second);
        let objects_by_key = HashMap::from([
            (first_key, first.clone()),
            (shared_key, shared.clone()),
            (second_key, second.clone()),
        ]);
        let cache = Mutex::new(RequestObjectCache::default());
        let first_fetch_calls = Cell::new(0);
        let second_fetch_calls = Cell::new(0);

        let first_items = vec![(digest(1), vec![shared_key, first_key, shared_key])];
        let first_sets = build_object_sets(&first_items, |unique_keys| {
            let (objects, _) = cache
                .lock()
                .expect("request object cache mutex poisoned")
                .multi_get(unique_keys, |missing| {
                    first_fetch_calls.set(first_fetch_calls.get() + 1);
                    assert_eq!(missing, &[first_key, shared_key]);
                    missing
                        .iter()
                        .map(|key| objects_by_key.get(key).cloned())
                        .collect()
                });
            objects
        })
        .unwrap();

        let second_items = vec![(digest(2), vec![shared_key, second_key, shared_key])];
        let second_sets = build_object_sets(&second_items, |unique_keys| {
            let (objects, _) = cache
                .lock()
                .expect("request object cache mutex poisoned")
                .multi_get(unique_keys, |missing| {
                    second_fetch_calls.set(second_fetch_calls.get() + 1);
                    assert_eq!(missing, &[second_key]);
                    missing
                        .iter()
                        .map(|key| objects_by_key.get(key).cloned())
                        .collect()
                });
            objects
        })
        .unwrap();

        assert_eq!(first_fetch_calls.get(), 1);
        assert_eq!(second_fetch_calls.get(), 1);
        assert_eq!(first_sets.len(), 1);
        assert_eq!(first_sets[0].len(), 2);
        assert_eq!(first_sets[0].get(&first_key), Some(&first));
        assert_eq!(first_sets[0].get(&shared_key), Some(&shared));
        assert!(first_sets[0].get(&second_key).is_none());
        assert_eq!(second_sets.len(), 1);
        assert_eq!(second_sets[0].len(), 2);
        assert!(second_sets[0].get(&first_key).is_none());
        assert_eq!(second_sets[0].get(&shared_key), Some(&shared));
        assert_eq!(second_sets[0].get(&second_key), Some(&second));
    }

    #[test]
    fn shared_keys_are_fetched_once_and_reconstructed_per_item() {
        let first = object(1);
        let shared = object(2);
        let second = object(3);
        let first_key = object_key(&first);
        let shared_key = object_key(&shared);
        let second_key = object_key(&second);
        let items = vec![
            (digest(1), vec![shared_key, first_key]),
            (digest(2), vec![second_key, shared_key]),
        ];
        let expected_union = vec![first_key, shared_key, second_key];
        let objects_by_key = HashMap::from([
            (first_key, first),
            (shared_key, shared),
            (second_key, second),
        ]);
        let fetch_calls = Cell::new(0);

        let sets = build_object_sets(&items, |keys| {
            fetch_calls.set(fetch_calls.get() + 1);
            assert_eq!(keys, expected_union);
            keys.iter()
                .map(|key| objects_by_key.get(key).cloned())
                .collect()
        })
        .unwrap();

        assert_eq!(fetch_calls.get(), 1);
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0].len(), 2);
        assert!(sets[0].get(&first_key).is_some());
        assert!(sets[0].get(&shared_key).is_some());
        assert!(sets[0].get(&second_key).is_none());
        assert_eq!(sets[1].len(), 2);
        assert!(sets[1].get(&first_key).is_none());
        assert!(sets[1].get(&shared_key).is_some());
        assert!(sets[1].get(&second_key).is_some());
    }

    #[test]
    fn empty_inputs_do_not_fetch() {
        let fetch_calls = Cell::new(0);
        let no_items = Vec::new();
        let sets = build_object_sets(&no_items, |_| {
            fetch_calls.set(fetch_calls.get() + 1);
            Vec::new()
        })
        .unwrap();
        assert_eq!(fetch_calls.get(), 0);
        assert!(sets.is_empty());

        let empty_items = vec![(digest(1), Vec::new()), (digest(2), Vec::new())];
        let sets = build_object_sets(&empty_items, |_| {
            fetch_calls.set(fetch_calls.get() + 1);
            Vec::new()
        })
        .unwrap();
        assert_eq!(fetch_calls.get(), 0);
        assert_eq!(sets.len(), 2);
        assert!(sets.iter().all(ObjectSet::is_empty));
    }

    #[test]
    fn missing_object_error_names_key_and_owning_transaction() {
        let present = object(1);
        let missing = object(2);
        let present_key = object_key(&present);
        let missing_key = object_key(&missing);
        let owning_digest = digest(2);
        let items = vec![
            (digest(1), vec![present_key]),
            (owning_digest, vec![missing_key]),
        ];

        let error = build_object_sets(&items, |keys| {
            keys.iter()
                .map(|key| (*key == present_key).then(|| present.clone()))
                .collect()
        })
        .unwrap_err();
        let message = error.to_string();

        assert!(message.contains(&format!("{missing_key:?}")));
        assert!(message.contains(&owning_digest.to_string()));
    }
}
