// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A shared object whose entries are dynamic fields of its own id, for exercising child-object
/// reads and writes on a fork.
module registry::registry;

use sui::dynamic_field as df;

public struct Registry has key {
    id: UID,
    size: u64,
}

/// Create an empty shared registry.
public fun create(ctx: &mut TxContext) {
    transfer::share_object(Registry { id: object::new(ctx), size: 0 })
}

/// Add an entry under `key`.
public fun add(registry: &mut Registry, key: u64, value: u64) {
    df::add(&mut registry.id, key, value);
    registry.size = registry.size + 1;
}

/// Increment the entry under `key`, which must exist.
public fun bump(registry: &mut Registry, key: u64) {
    let value: &mut u64 = df::borrow_mut(&mut registry.id, key);
    *value = *value + 1;
}
