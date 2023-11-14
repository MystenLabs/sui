// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{ObjectID, ObjectRef, VersionNumber};
use crate::error::SuiError;
use crate::object::Object;
use crate::storage::WriteKind;
use std::collections::BTreeMap;
use std::sync::Arc;

pub trait ObjectStore {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Arc<Object>>, SuiError>;
    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Arc<Object>>, SuiError>;
}

impl ObjectStore for &[Arc<Object>] {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Arc<Object>>, SuiError> {
        Ok(self.iter().find(|o| o.id() == *object_id).cloned())
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Arc<Object>>, SuiError> {
        Ok(self
            .iter()
            .find(|o| o.id() == *object_id && o.version() == version)
            .cloned())
    }
}

impl ObjectStore for BTreeMap<ObjectID, (ObjectRef, Arc<Object>, WriteKind)> {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Arc<Object>>, SuiError> {
        Ok(self.get(object_id).map(|(_, obj, _)| obj).cloned())
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Arc<Object>>, SuiError> {
        Ok(self
            .get(object_id)
            .and_then(|(_, obj, _)| {
                if obj.version() == version {
                    Some(obj)
                } else {
                    None
                }
            })
            .cloned())
    }
}

impl ObjectStore for BTreeMap<ObjectID, Arc<Object>> {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Arc<Object>>, SuiError> {
        Ok(self.get(object_id).cloned())
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Arc<Object>>, SuiError> {
        Ok(self.get(object_id).and_then(|o| {
            if o.version() == version {
                Some(o.clone())
            } else {
                None
            }
        }))
    }
}

impl<T: ObjectStore> ObjectStore for Arc<T> {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Arc<Object>>, SuiError> {
        self.as_ref().get_object(object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Arc<Object>>, SuiError> {
        self.as_ref().get_object_by_key(object_id, version)
    }
}

impl<T: ObjectStore> ObjectStore for &T {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Arc<Object>>, SuiError> {
        ObjectStore::get_object(*self, object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Arc<Object>>, SuiError> {
        ObjectStore::get_object_by_key(*self, object_id, version)
    }
}
