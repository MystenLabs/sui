// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    digests::ObjectDigest,
    object::Object,
};

struct ObjectStore {
    /// Keeps track of the latest version of the object
    live_objects: HashMap<ObjectID, SequenceNumber>,
    /// Contains objects and their versions
    objects: HashMap<ObjectID, BTreeMap<SequenceNumber, Object>>,
}

pub(crate) trait ObjectStoreApi {
    pub fn get_object_by_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Option<&Object> {
        self.objects
            .get(&object_id)
            .and_then(|versions| versions.get(&version))
    }

    pub fn get_object(&self, object_id: ObjectID) -> Option<(&Object, SequenceNumber)> {
        self.live_objects.get(&object_id).and_then(|&version| {
            self.objects
                .get(&object_id)
                .and_then(|versions| versions.get(&version))
                .map(|obj| (obj, version))
        })
    }

    pub fn get_owned_objects(&self, owner: SuiAddress) -> Vec<&Object> {
        self.objects
            .filter(|_, objects| objects.values().any(|obj| obj.owner() == &owner))
    }

    pub fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    );
}
