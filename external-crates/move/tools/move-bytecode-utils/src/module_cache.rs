// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use move_binary_format::CompiledModule;
use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};
use std::{
    borrow::Borrow,
    cell::RefCell,
    collections::{btree_map::Entry, BTreeMap},
    fmt::Debug,
    sync::{Arc, RwLock},
};

/// A persistent storage that can fetch the bytecode for a given module id
/// TODO: do we want to implement this in a way that allows clients to cache struct layouts?
pub trait GetModule {
    type Error: Debug;
    type Item: Borrow<CompiledModule>;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<Self::Item>, Self::Error>;
}

/// Simple in-memory module cache
pub struct ModuleCache<R: ModuleResolver> {
    cache: RefCell<BTreeMap<ModuleId, CompiledModule>>,
    resolver: R,
}

impl<R: ModuleResolver> ModuleCache<R> {
    pub fn new(resolver: R) -> Self {
        ModuleCache {
            cache: RefCell::new(BTreeMap::new()),
            resolver,
        }
    }

    pub fn add(&self, id: ModuleId, m: CompiledModule) {
        self.cache.borrow_mut().insert(id, m);
    }

    pub fn len(&self) -> usize {
        self.cache.borrow().len()
    }
}

impl<R: ModuleResolver> GetModule for ModuleCache<R> {
    type Error = anyhow::Error;
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<CompiledModule>, Self::Error> {
        Ok(Some(match self.cache.borrow_mut().entry(id.clone()) {
            Entry::Vacant(entry) => {
                let module_bytes = self
                    .resolver
                    .get_module(id)
                    .map_err(|_| anyhow!("Failed to get module {:?}", id))?
                    .ok_or_else(|| anyhow!("Module {:?} doesn't exist", id))?;
                let module = CompiledModule::deserialize_with_defaults(&module_bytes)
                    .map_err(|_| anyhow!("Failure deserializing module {:?}", id))?;
                entry.insert(module.clone());
                module
            }
            Entry::Occupied(entry) => entry.get().clone(),
        }))
    }
}

impl<R: ModuleResolver> GetModule for &R {
    type Error = R::Error;
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<CompiledModule>, Self::Error> {
        Ok(self
            .get_module(id)
            .unwrap()
            .map(|bytes| CompiledModule::deserialize_with_defaults(&bytes).unwrap()))
    }
}

impl<R: ModuleResolver> GetModule for &mut R {
    type Error = R::Error;
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<CompiledModule>, Self::Error> {
        Ok(self
            .get_module(id)
            .unwrap()
            .map(|bytes| CompiledModule::deserialize_with_defaults(&bytes).unwrap()))
    }
}

impl<T: GetModule> GetModule for Arc<T> {
    type Error = T::Error;
    type Item = T::Item;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<T::Item>, Self::Error> {
        self.as_ref().get_module_by_id(id)
    }
}

/// Simple in-memory module cache that implements Sync
pub struct SyncModuleCache<R: ModuleResolver> {
    cache: RwLock<BTreeMap<ModuleId, Arc<CompiledModule>>>,
    resolver: R,
}

impl<R: ModuleResolver> SyncModuleCache<R> {
    pub fn new(resolver: R) -> Self {
        SyncModuleCache {
            cache: RwLock::new(BTreeMap::new()),
            resolver,
        }
    }

    pub fn add(&self, id: ModuleId, m: CompiledModule) {
        self.cache.write().unwrap().insert(id, Arc::new(m));
    }

    pub fn len(&self) -> usize {
        self.cache.read().unwrap().len()
    }
}

impl<R: ModuleResolver> GetModule for SyncModuleCache<R> {
    type Error = anyhow::Error;
    type Item = Arc<CompiledModule>;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<Arc<CompiledModule>>, Self::Error> {
        if let Some(compiled_module) = self.cache.read().unwrap().get(id) {
            return Ok(Some(compiled_module.clone()));
        }

        if let Some(module_bytes) = self
            .resolver
            .get_module(id)
            .map_err(|_| anyhow!("Failed to get module {:?}", id))?
        {
            let module = Arc::new(
                CompiledModule::deserialize_with_defaults(&module_bytes)
                    .map_err(|_| anyhow!("Failure deserializing module {:?}", id))?,
            );

            self.cache
                .write()
                .unwrap()
                .insert(id.clone(), module.clone());
            Ok(Some(module))
        } else {
            Ok(None)
        }
    }
}
