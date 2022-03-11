// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::{ObjectID, SequenceNumber},
    event::Event,
    object::Object,
};

#[derive(Debug, PartialEq)]
pub enum DeleteKind {
    /// An object is provided in the call input, and gets deleted.
    Normal,
    /// An object is not provided in the call input, but gets unwrapped
    /// from another object, and then gets deleted.
    UnwrapThenDelete,
    /// An object is provided in the call input, and gets wrapped into another object.
    Wrap,
}

/// An abstraction of the (possibly distributed) store for objects, and (soon) events and transactions
pub trait Storage {
    fn reset(&mut self);

    fn read_object(&self, id: &ObjectID) -> Option<Object>;

    // Indicate a new object ID is created, which may be used to create an object.
    // This is needed to determine unwrapped objects at the end.
    fn create_object_id(&mut self, id: ObjectID);

    fn write_object(&mut self, object: Object);

    /// Record an event that happened during execution
    fn log_event(&mut self, event: Event);

    fn delete_object(&mut self, id: &ObjectID, version: SequenceNumber, kind: DeleteKind);
}
