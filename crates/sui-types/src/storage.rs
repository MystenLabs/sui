// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashSet};

use crate::{
    base_types::{ObjectID, SequenceNumber},
    error::SuiResult,
    event::Event,
    object::Object,
};

#[derive(Debug, PartialEq, Eq)]
pub enum DeleteKind {
    /// An object is provided in the call input, and gets deleted.
    Normal,
    /// An object is not provided in the call input, but gets unwrapped
    /// from another object, and then gets deleted.
    UnwrapThenDelete,
    /// An object is provided in the call input, and gets wrapped into another object.
    Wrap,
}

pub enum DeleteEvent {
    /// By-value object is deleted
    Normal {
        /// child count in Info at deletion
        child_count: u64,
    },
    /// Unwrapped from another object then deleted
    UnwrapThenDelete {
        /// child count in Info at deletion
        child_count: u64,
    },
    /// By-value object gets wrapped in another object
    Wrap,
}

/// An abstraction of the (possibly distributed) store for objects, and (soon) events and transactions
pub trait Storage {
    fn reset(&mut self);

    fn read_object(&self, id: &ObjectID) -> Option<&Object>;

    // Specify the list of object IDs created during the transaction.
    // This is needed to determine unwrapped objects at the end.
    fn set_create_object_ids(&mut self, ids: HashSet<ObjectID>);

    fn write_object(&mut self, object: Object);

    /// Record an event that happened during execution
    fn log_event(&mut self, event: Event);

    fn delete_object(&mut self, id: &ObjectID, version: SequenceNumber, kind: DeleteEvent);

    fn decrement_child_counts(&mut self, decrements: BTreeMap<ObjectID, u64>);

    fn deleted_objects_child_count(&mut self) -> Vec<(ObjectID, u64)>;
}

pub trait BackingPackageStore {
    fn get_package(&self, package_id: &ObjectID) -> SuiResult<Option<Object>>;
}

impl<S: BackingPackageStore> BackingPackageStore for std::sync::Arc<S> {
    fn get_package(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        BackingPackageStore::get_package(self.as_ref(), package_id)
    }
}

impl<S: BackingPackageStore> BackingPackageStore for &S {
    fn get_package(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        BackingPackageStore::get_package(*self, package_id)
    }
}
