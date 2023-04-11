// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use sui_types::base_types::ObjectID;
use sui_types::error::{SuiError, SuiResult, UserInputError};
use sui_types::object::Object;
use sui_types::storage::{BackingPackageStore, ObjectStore};

pub struct PackageObjectCache<S> {
    cache: RwLock<HashMap<ObjectID, Object>>,
    store: Arc<S>,
}

impl<S> PackageObjectCache<S> {
    pub fn new(store: Arc<S>) -> Arc<Self> {
        Arc::new(Self {
            cache: RwLock::new(HashMap::new()),
            store,
        })
    }
}

impl<S: ObjectStore> BackingPackageStore for PackageObjectCache<S> {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        if let Some(p) = self.cache.read().get(package_id) {
            return Ok(Some(p.clone()));
        }
        if let Some(p) = self.store.get_object(package_id)? {
            if p.is_package() {
                self.cache.write().insert(*package_id, p.clone());
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
