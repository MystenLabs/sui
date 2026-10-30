// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A shared object whose entries are dynamic fields of its own id, for exercising child-object
/// reads and writes on a fork.
module registry::registry;

use sui::dynamic_field as df;
use sui::dynamic_object_field as dof;

public struct Registry has key {
    id: UID,
    size: u64,
}

public struct Node has key, store {
    id: UID,
}

/// Create an empty shared registry.
public fun create(ctx: &mut TxContext) {
    transfer::share_object(Registry { id: object::new(ctx), size: 0 })
}

/// Create a shared registry containing nested dynamic object fields.
public fun create_nested(ctx: &mut TxContext) {
    let mut registry = Registry {
        id: object::new(ctx),
        size: 0,
    };
    let mut node = Node {
        id: object::new(ctx),
    };
    df::add(&mut node.id, 2u64, 0u64);
    dof::add(&mut registry.id, 1u64, node);
    transfer::share_object(registry);
}

/// Increment the nested entry under `node_key` and `leaf_key`.
public fun bump_nested(registry: &mut Registry, node_key: u64, leaf_key: u64) {
    let node = dof::borrow_mut<u64, Node>(&mut registry.id, node_key);
    let leaf = df::borrow_mut<u64, u64>(&mut node.id, leaf_key);
    *leaf = *leaf + 1;
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
