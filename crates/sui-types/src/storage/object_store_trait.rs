// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::error::Result;
use super::ObjectKey;
use crate::base_types::{ObjectID, ObjectRef, VersionNumber};
use crate::object::Object;
use crate::storage::WriteKind;
use std::collections::BTreeMap;
use std::sync::Arc;

pub trait ObjectStore {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>>;

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>>;

    fn multi_get_objects(&self, object_ids: &[ObjectID]) -> Result<Vec<Option<Object>>> {
        object_ids
            .iter()
            .map(|digest| self.get_object(digest))
            .collect::<Result<Vec<_>, _>>()
    }

    fn multi_get_objects_by_key(&self, object_keys: &[ObjectKey]) -> Result<Vec<Option<Object>>> {
        object_keys
            .iter()
            .map(|k| self.get_object_by_key(&k.0, k.1))
            .collect::<Result<Vec<_>, _>>()
    }
}

impl<T: ObjectStore + ?Sized> ObjectStore for &T {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>> {
        (*self).get_object(object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>> {
        (*self).get_object_by_key(object_id, version)
    }

    fn multi_get_objects(&self, object_ids: &[ObjectID]) -> Result<Vec<Option<Object>>> {
        (*self).multi_get_objects(object_ids)
    }

    fn multi_get_objects_by_key(&self, object_keys: &[ObjectKey]) -> Result<Vec<Option<Object>>> {
        (*self).multi_get_objects_by_key(object_keys)
    }
}

impl<T: ObjectStore + ?Sized> ObjectStore for Box<T> {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>> {
        (**self).get_object(object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>> {
        (**self).get_object_by_key(object_id, version)
    }

    fn multi_get_objects(&self, object_ids: &[ObjectID]) -> Result<Vec<Option<Object>>> {
        (**self).multi_get_objects(object_ids)
    }

    fn multi_get_objects_by_key(&self, object_keys: &[ObjectKey]) -> Result<Vec<Option<Object>>> {
        (**self).multi_get_objects_by_key(object_keys)
    }
}

impl<T: ObjectStore + ?Sized> ObjectStore for Arc<T> {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>> {
        (**self).get_object(object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>> {
        (**self).get_object_by_key(object_id, version)
    }

    fn multi_get_objects(&self, object_ids: &[ObjectID]) -> Result<Vec<Option<Object>>> {
        (**self).multi_get_objects(object_ids)
    }

    fn multi_get_objects_by_key(&self, object_keys: &[ObjectKey]) -> Result<Vec<Option<Object>>> {
        (**self).multi_get_objects_by_key(object_keys)
    }
}

impl ObjectStore for &[Object] {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>> {
        Ok(self.iter().find(|o| o.id() == *object_id).cloned())
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>> {
        Ok(self
            .iter()
            .find(|o| o.id() == *object_id && o.version() == version)
            .cloned())
    }
}

impl ObjectStore for BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)> {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>> {
        Ok(self.get(object_id).map(|(_, obj, _)| obj).cloned())
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>> {
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

impl ObjectStore for BTreeMap<ObjectID, Object> {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>> {
        Ok(self.get(object_id).cloned())
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>> {
        Ok(self.get(object_id).and_then(|o| {
            if o.version() == version {
                Some(o.clone())
            } else {
                None
            }
        }))
    }
}
