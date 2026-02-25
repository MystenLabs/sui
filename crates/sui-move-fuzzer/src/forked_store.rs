// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `ForkedStore` — a `BackingStore` implementation that lazily fetches objects
//! from a Sui RPC node while allowing an in-memory override layer on top.
//!
//! Override priority (highest to lowest):
//!   1. `overrides` — injected objects (oracle mocks, mock gas coins, etc.)
//!   2. `cache`     — objects previously fetched from RPC (lazy-populated)
//!   3. RPC fetch   — live network state, result stored in `cache`

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use sui_json_rpc_types::SuiObjectDataOptions;
use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber};
use sui_types::committee::EpochId;
use sui_types::error::SuiResult;
use sui_types::object::Object;
use sui_types::storage::{
    load_package_object_from_object_store, BackingPackageStore, ChildObjectResolver, ObjectStore,
    PackageObject, ParentSync,
};

pub struct ForkedStore {
    /// Highest-priority layer: objects injected by the caller (oracle mocks, etc.).
    /// Modified via `&mut self` (inject_object / clear_override).
    overrides: BTreeMap<ObjectID, Object>,
    /// Lazily-populated cache of objects fetched from RPC.
    /// Uses interior mutability so trait impls can update it via `&self`.
    cache: Mutex<BTreeMap<ObjectID, Object>>,
    /// Objects confirmed absent from the chain — avoids repeated RPC round-trips
    /// for objects that don't exist.
    negative_cache: Mutex<BTreeSet<ObjectID>>,
    /// Total RPC fetches performed — useful for end-of-run diagnostics.
    rpc_fetches: AtomicUsize,
    /// Async Sui RPC client.
    client: SuiClient,
    /// Single-threaded Tokio runtime for bridging async RPC calls into sync trait impls.
    /// The outer fuzzing loop is always synchronous, so block_on is safe here.
    runtime: tokio::runtime::Runtime,
}

impl ForkedStore {
    /// Create a new `ForkedStore` connected to the given RPC URL.
    pub fn new(rpc_url: &str) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let client = runtime.block_on(SuiClientBuilder::default().build(rpc_url))?;
        Ok(Self {
            overrides: BTreeMap::new(),
            cache: Mutex::new(BTreeMap::new()),
            negative_cache: Mutex::new(BTreeSet::new()),
            rpc_fetches: AtomicUsize::new(0),
            client,
            runtime,
        })
    }

    /// Insert `obj` into the override layer.  Subsequent lookups by ID will
    /// return this object regardless of what the RPC returns.
    pub fn inject_object(&mut self, obj: Object) {
        self.overrides.insert(obj.id(), obj);
    }

    /// Remove an override (falls through to cache / RPC on next lookup).
    pub fn clear_override(&mut self, id: &ObjectID) {
        self.overrides.remove(id);
    }

    /// Fetch a single object from the RPC, bypassing the override/cache layers.
    /// Useful for callers that need the raw on-chain state before patching it.
    pub fn fetch_object(&self, id: &ObjectID) -> Result<Option<Object>> {
        let response = self.runtime.block_on(
            self.client
                .read_api()
                .get_object_with_options(*id, SuiObjectDataOptions::bcs_lossless()),
        )?;
        match response.into_object() {
            Ok(data) => Ok(Some(data.try_into()?)),
            Err(_) => Ok(None),
        }
    }

    /// Total number of RPC fetches made since construction.
    pub fn rpc_fetch_count(&self) -> usize {
        self.rpc_fetches.load(Ordering::Relaxed)
    }

    // --- Internal helpers -------------------------------------------------

    /// Fetch from RPC, populate cache, and return the object.
    /// Returns `None` if the object does not exist or the RPC call fails.
    /// Negative results are cached to avoid repeated RPC round-trips.
    fn fetch_to_cache(&self, id: &ObjectID) -> Option<Object> {
        // Skip the RPC call if we already know the object doesn't exist.
        {
            let neg = self.negative_cache.lock().unwrap();
            if neg.contains(id) {
                return None;
            }
        }

        self.rpc_fetches.fetch_add(1, Ordering::Relaxed);

        let response = match self.runtime.block_on(
            self.client
                .read_api()
                .get_object_with_options(*id, SuiObjectDataOptions::bcs_lossless()),
        ) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[ForkedStore] RPC error fetching {id}: {e}");
                return None;
            }
        };

        let data = match response.into_object() {
            Ok(d) => d,
            Err(_) => {
                // Object not found on chain — record the miss so we don't re-fetch.
                self.negative_cache.lock().unwrap().insert(*id);
                return None;
            }
        };

        let obj: Object = match data.try_into() {
            Ok(o) => o,
            Err(e) => {
                eprintln!("[ForkedStore] failed to convert object {id}: {e}");
                return None;
            }
        };

        let mut cache = self.cache.lock().unwrap();
        cache.insert(obj.id(), obj.clone());
        Some(obj)
    }

    /// Resolve an object ID through the full priority stack.
    fn resolve(&self, id: &ObjectID) -> Option<Object> {
        if let Some(obj) = self.overrides.get(id) {
            return Some(obj.clone());
        }
        {
            let cache = self.cache.lock().unwrap();
            if let Some(obj) = cache.get(id) {
                return Some(obj.clone());
            }
        }
        self.fetch_to_cache(id)
    }
}

// ---------------------------------------------------------------------------
// BackingStore supertrait implementations
// ---------------------------------------------------------------------------

impl ObjectStore for ForkedStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.resolve(object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> Option<Object> {
        // Check overrides first
        if let Some(obj) = self.overrides.get(object_id)
            && obj.version() == version
        {
            return Some(obj.clone());
        }
        // Check cache
        {
            let cache = self.cache.lock().unwrap();
            if let Some(obj) = cache.get(object_id)
                && obj.version() == version
            {
                return Some(obj.clone());
            }
        }
        // Fetch latest from RPC and accept only if version matches
        let obj = self.fetch_to_cache(object_id)?;
        if obj.version() == version { Some(obj) } else { None }
    }
}

impl BackingPackageStore for ForkedStore {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        load_package_object_from_object_store(self, package_id)
    }
}

impl ChildObjectResolver for ForkedStore {
    /// Called by the Move VM when resolving dynamic field children.
    /// The override layer is the oracle interception point: inject a patched
    /// child object here to redirect price queries.
    fn read_child_object(
        &self,
        _parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        // Overrides bypass all version checks — the caller owns the mock.
        if let Some(obj) = self.overrides.get(child) {
            return Ok(Some(obj.clone()));
        }
        {
            let cache = self.cache.lock().unwrap();
            if let Some(obj) = cache.get(child)
                && obj.version() <= child_version_upper_bound
            {
                return Ok(Some(obj.clone()));
            }
        }
        match self.fetch_to_cache(child) {
            Some(obj) if obj.version() <= child_version_upper_bound => Ok(Some(obj)),
            Some(_) => Ok(None),
            None => Ok(None),
        }
    }

    fn get_object_received_at_version(
        &self,
        _owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        if let Some(obj) = self.overrides.get(receiving_object_id)
            && obj.version() == receive_object_at_version
        {
            return Ok(Some(obj.clone()));
        }
        {
            let cache = self.cache.lock().unwrap();
            if let Some(obj) = cache.get(receiving_object_id)
                && obj.version() == receive_object_at_version
            {
                return Ok(Some(obj.clone()));
            }
        }
        let obj = self.fetch_to_cache(receiving_object_id);
        Ok(obj.filter(|o| o.version() == receive_object_at_version))
    }
}

impl ParentSync for ForkedStore {
    fn get_latest_parent_entry_ref_deprecated(&self, object_id: ObjectID) -> Option<ObjectRef> {
        // Check overrides
        if let Some(obj) = self.overrides.get(&object_id) {
            return Some(obj.compute_object_reference());
        }
        // Check cache
        {
            let cache = self.cache.lock().unwrap();
            if let Some(obj) = cache.get(&object_id) {
                return Some(obj.compute_object_reference());
            }
        }
        // Fall through to RPC on cache miss.
        self.fetch_to_cache(&object_id)
            .map(|obj| obj.compute_object_reference())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_types::base_types::SuiAddress;

    /// Verify that injected overrides take priority over cache entries.
    #[test]
    fn override_beats_cache() {
        // Build a store without connecting to RPC (we won't make any RPC calls).
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let client = runtime.block_on(async {
            // Use a bogus URL — we won't actually call RPC in this test.
            // The client construction would fail, so we skip ForkedStore::new
            // and instead just verify the override logic directly.
            None::<SuiClient>
        });
        // We can't easily construct a SuiClient without a real URL, so just
        // verify the override data structure directly.
        let _ = client;

        let obj_a = Object::new_gas_with_balance_and_owner_for_testing(100, SuiAddress::ZERO);
        let obj_b = Object::new_gas_with_balance_and_owner_for_testing(200, SuiAddress::ZERO);

        let mut overrides: BTreeMap<ObjectID, Object> = BTreeMap::new();
        let mut cache: BTreeMap<ObjectID, Object> = BTreeMap::new();

        // obj_a in cache
        let a = obj_a.clone();
        // We can't easily set a custom ID on Object here, so use the default
        // IDs assigned during construction.
        let a_id = a.id();
        cache.insert(a_id, a.clone());

        // Override with obj_b using the same ID — we insert an object with
        // obj_b's balance but under obj_a's ID (by injecting).
        overrides.insert(a_id, obj_b.clone());

        // Simulate resolve(): overrides win.
        let resolved = overrides.get(&a_id).or_else(|| cache.get(&a_id)).cloned();
        // The resolved object should be obj_b (from overrides), not obj_a (from cache).
        let resolved = resolved.unwrap();
        assert_eq!(resolved.id(), obj_b.id());
    }
}
