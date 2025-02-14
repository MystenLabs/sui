// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_cache::cache_types::CacheResult;
use sui_types::error::SuiResult;

/// do_fallback_lookup is a helper function for multi-get operations.
/// It takes a list of keys and first attempts to look up each key in the cache.
/// The cache can return a hit, a miss, or a negative hit (if the object is known to not exist).
/// Any keys that result in a miss are then looked up in the store.
///
/// The "get from cache" and "get from store" behavior are implemented by the caller and provided
/// via the get_cached_key and multiget_fallback functions.
pub fn do_fallback_lookup<K: Clone, V: Default + Clone>(
    keys: &[K],
    get_cached_key: impl Fn(&K) -> CacheResult<V>,
    multiget_fallback: impl Fn(&[K]) -> Vec<V>,
) -> Vec<V> {
    do_fallback_lookup_fallible(
        keys,
        |key| Ok(get_cached_key(key)),
        |keys| Ok(multiget_fallback(keys)),
    )
    .expect("cannot fail")
}

pub fn do_fallback_lookup_fallible<K: Clone, V: Default + Clone>(
    keys: &[K],
    get_cached_key: impl Fn(&K) -> SuiResult<CacheResult<V>>,
    multiget_fallback: impl Fn(&[K]) -> SuiResult<Vec<V>>,
) -> SuiResult<Vec<V>> {
    let mut results = vec![V::default(); keys.len()];
    let mut fallback_keys = Vec::with_capacity(keys.len());
    let mut fallback_indices = Vec::with_capacity(keys.len());

    for (i, key) in keys.iter().enumerate() {
        match get_cached_key(key)? {
            CacheResult::Miss => {
                fallback_keys.push(key.clone());
                fallback_indices.push(i);
            }
            CacheResult::NegativeHit => (),
            CacheResult::Hit(value) => {
                results[i] = value;
            }
        }
    }

    let fallback_results = multiget_fallback(&fallback_keys)?;
    assert_eq!(fallback_results.len(), fallback_indices.len());
    assert_eq!(fallback_results.len(), fallback_keys.len());

    for (i, result) in fallback_indices
        .into_iter()
        .zip(fallback_results.into_iter())
    {
        results[i] = result;
    }
    Ok(results)
}
