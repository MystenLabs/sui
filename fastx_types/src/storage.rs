// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use crate::{base_types::ObjectID, object::Object};

/// An abstraction of the (possibly distributed) store for objects, and (soon) events and transactions
pub trait Storage {
    fn read_object(&self, id: &ObjectID) -> Option<Object>;

    fn write_object(&mut self, object: Object);

    fn delete_object(&mut self, id: &ObjectID);
}
