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

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.cache.borrow().len()
    }
}

fn get_module_by_id<R: ModuleResolver>(
    resolver: &R,
    id: &ModuleId,
) -> Result<Option<CompiledModule>, R::Error> {
    let [Some(pkg)] = resolver.get_packages_static([*id.address()])? else {
        return Ok(None);
    };
    Ok(pkg
        .modules
        .iter()
        .map(|m| CompiledModule::deserialize_with_defaults(&m).unwrap())
        .find_map(|m| if m.self_id() == *id { Some(m) } else { None }))
}

impl<R: ModuleResolver> GetModule for ModuleCache<R> {
    type Error = anyhow::Error;
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<CompiledModule>, Self::Error> {
        Ok(Some(match self.cache.borrow_mut().entry(id.clone()) {
            Entry::Vacant(entry) => {
                let Some(module) = get_module_by_id(&self.resolver, id)
                    .map_err(|e| anyhow!("Failed to get module {:?}: {:?}", id, e))?
                else {
                    return Ok(None);
                };
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
        get_module_by_id(self, id)
    }
}

impl<R: ModuleResolver> GetModule for &mut R {
    type Error = R::Error;
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<CompiledModule>, Self::Error> {
        get_module_by_id(*self, id)
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

    #[allow(clippy::len_without_is_empty)]
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

        let Some(module) = get_module_by_id(&self.resolver, id)
            .map_err(|e| anyhow!("Failed to get module {:?}: {:?}", id, e))?
        else {
            return Ok(None);
        };

        let module = Arc::new(module);
        self.cache
            .write()
            .unwrap()
            .insert(id.clone(), module.clone());
        Ok(Some(module))
    }
}
