// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Support for fork testing - loading objects from live networks into tests.
//! This module provides thread-safe global storage for fork-loaded objects that
//! can be accessed from both sui-move and sui-move-natives crates.

use crate::base_types::{MoveObjectType, ObjectID};
use crate::object::Owner;
use once_cell::sync::Lazy;
use std::sync::Mutex;

/// Represents a fork-loaded object: (ObjectID, MoveObjectType, Owner, BCS bytes).
pub type ForkLoadedObject = (ObjectID, MoveObjectType, Owner, Vec<u8>);

/// Global storage for fork-loaded objects that works across threads.
static FORK_LOADED_OBJECTS: Lazy<Mutex<Vec<ForkLoadedObject>>> =
    Lazy::new(|| Mutex::new(Vec::new()));

/// Get a copy of all fork-loaded objects.
pub fn get_fork_loaded_objects() -> Vec<ForkLoadedObject> {
    FORK_LOADED_OBJECTS.lock().unwrap().clone()
}

/// Store fork-loaded objects for use in tests.
pub fn set_fork_loaded_objects(objects: Vec<ForkLoadedObject>) {
    *FORK_LOADED_OBJECTS.lock().unwrap() = objects;
}

/// Clear all fork-loaded objects.
pub fn clear_fork_loaded_objects() {
    FORK_LOADED_OBJECTS.lock().unwrap().clear();
}
