// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use lru::LruCache;
use parking_lot::RwLock;
use std::num::NonZeroUsize;
use std::sync::Arc;
use sui_types::base_types::ObjectID;
use sui_types::error::{SuiError, SuiResult, UserInputError};
use sui_types::storage::{ObjectStore, PackageObject};

pub struct PackageObjectCache {
    cache: RwLock<LruCache<ObjectID, PackageObject>>,
}

const CACHE_CAP: usize = 1024 * 1024;

impl PackageObjectCache {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            cache: RwLock::new(LruCache::new(NonZeroUsize::new(CACHE_CAP).unwrap())),
        })
    }

    pub fn get_package_object(
        &self,
        package_id: &ObjectID,
        store: &impl ObjectStore,
    ) -> SuiResult<Option<PackageObject>> {
        // TODO: Here the use of `peek` doesn't update the internal use record,
        // and hence the LRU is really used as a capped map here.
        // This is OK because we won't typically have too many entries.
        // We cannot use `get` here because it requires a mut reference and that would
        // require unnecessary lock contention on the mutex, which defeats the purpose.
        if let Some(p) = self.cache.read().peek(package_id) {
            #[cfg(debug_assertions)]
            {
                assert_eq!(
                    store.get_object(package_id).unwrap().digest(),
                    p.object().digest(),
                    "Package object cache is inconsistent for package {:?}",
                    package_id
                )
            }
            return Ok(Some(p.clone()));
        }
        if let Some(p) = store.get_object(package_id) {
            if p.is_package() {
                let p = PackageObject::new(p);
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

    pub fn force_reload_system_packages(
        &self,
        system_package_ids: impl IntoIterator<Item = ObjectID>,
        store: &impl ObjectStore,
    ) {
        for package_id in system_package_ids {
            if let Some(p) = store.get_object(&package_id) {
                assert!(p.is_package());
                self.cache.write().push(package_id, PackageObject::new(p));
            }
            // It's possible that a package is not found if it's newly added system package ID
            // that hasn't got created yet. This should be very very rare though.
        }
    }
}
