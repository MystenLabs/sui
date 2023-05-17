// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use lru::LruCache;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::language_storage::ModuleId;
use move_core_types::resolver::ModuleResolver;
use parking_lot::RwLock;
use std::num::NonZeroUsize;
use std::sync::Arc;
use sui_types::base_types::ObjectID;
use sui_types::error::{SuiError, SuiResult, UserInputError};
use sui_types::object::Object;
use sui_types::storage::{get_module, get_module_by_id, BackingPackageStore, ObjectStore};

pub struct PackageObjectCache<S> {
    cache: RwLock<LruCache<ObjectID, Object>>,
    store: Arc<S>,
}

const CACHE_CAP: usize = 1024 * 1024;

impl<S> PackageObjectCache<S> {
    pub fn new(store: Arc<S>) -> Arc<Self> {
        Arc::new(Self {
            cache: RwLock::new(LruCache::new(NonZeroUsize::new(CACHE_CAP).unwrap())),
            store,
        })
    }
}

impl<S: ObjectStore> GetModule for PackageObjectCache<S> {
    type Error = SuiError;
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> anyhow::Result<Option<Self::Item>, Self::Error> {
        get_module_by_id(self, id)
    }
}

impl<S: BackingPackageStore> ModuleResolver for PackageObjectCache<S> {
    type Error = SuiError;

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        get_module(&self.store, id)
    }
}

// impl<S: ObjectStore + BackingPackageStore + ModuleResolver<Error = SuiError>> BackingPackageStore for PackageObjectCache<S> {
impl<S: ObjectStore> BackingPackageStore for PackageObjectCache<S> {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        // TODO: Here the use of `peek` doesn't update the internal use record,
        // and hence the LRU is really used as a capped map here.
        // This is OK because we won't typically have too many entries.
        // We cannot use `get` here because it requires a mut reference and that would
        // require unnecessary lock contention on the mutex, which defeats the purpose.
        if let Some(p) = self.cache.read().peek(package_id) {
            return Ok(Some(p.clone()));
        }
        if let Some(p) = self.store.get_object(package_id)? {
            if p.is_package() {
                self.cache.write().push(*package_id, p.clone());
                Ok(Some(p))
            } else {
                Err(SuiError::UserInputError {
                    error: UserInputError::MoveObjectAsPackage {
                        object_id: *package_id,
                    },
                })
            }
        } else {
            Ok(None)
        }
    }
}
