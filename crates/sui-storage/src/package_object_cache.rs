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
use sui_types::move_package::MovePackage;
use sui_types::object::{Data, Object};
use sui_types::storage::{
    get_module, get_module_by_id, BackingPackageCache, BackingPackageStore, ObjectStore,
};

type Item = (Arc<Object>, Arc<MovePackage>);

pub struct PackageObjectCache<S> {
    // TODO: Eventually we should make it only need to keep MovePackage instead of Object.
    cache: RwLock<LruCache<ObjectID, Item>>,
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

impl<S: ObjectStore> BackingPackageCache for PackageObjectCache<S> {
    fn get_package(&self, package_id: &ObjectID) -> SuiResult<Option<Arc<MovePackage>>> {
        self.load_package_entry(package_id)
            .map(|o| o.map(|(_, p)| p.clone()))
    }

    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Arc<Object>>> {
        self.load_package_entry(package_id)
            .map(|o| o.map(|(o, _)| o.clone()))
    }
}

impl<S: ObjectStore> PackageObjectCache<S> {
    fn load_package_entry(&self, package_id: &ObjectID) -> SuiResult<Option<Item>> {
        // TODO: Here the use of `peek` doesn't update the internal use record,
        // and hence the LRU is really used as a capped map here.
        // This is OK because we won't typically have too many entries.
        // We cannot use `get` here because it requires a mut reference and that would
        // require unnecessary lock contention on the mutex, which defeats the purpose.
        if let Some(p) = self.cache.read().peek(package_id) {
            return Ok(Some(p.clone()));
        }
        if let Some(o) = self.store.get_object(package_id)? {
            match &o.data {
                Data::Package(p) => {
                    let p = Arc::new(p.clone());
                    let o = Arc::new(o);
                    self.cache.write().push(*package_id, (o.clone(), p.clone()));
                    Ok(Some((o, p)))
                }
                Data::Move(_) => Err(SuiError::UserInputError {
                    error: UserInputError::MoveObjectAsPackage {
                        object_id: *package_id,
                    },
                }),
            }
        } else {
            Ok(None)
        }
    }
}
