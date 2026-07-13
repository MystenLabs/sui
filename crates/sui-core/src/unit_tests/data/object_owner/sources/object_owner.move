// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module object_owner::object_owner;

use std::option::{Self, Option};
use sui::dynamic_field;
use sui::dynamic_object_field;
use sui::object::{Self, ID, UID};
use sui::transfer;
use sui::tx_context::{Self, TxContext};

public struct Parent has key {
    id: UID,
    child: Option<ID>,
}

public struct Child has key, store {
    id: UID,
}

public struct AnotherParent has key {
    id: UID,
    child: ID,
}

public fun create_child(ctx: &mut TxContext) {
    transfer::public_transfer(
        Child { id: object::new(ctx) },
        ctx.sender(),
    );
}

public fun create_parent(ctx: &mut TxContext) {
    let parent = Parent {
        id: object::new(ctx),
        child: option::none(),
    };
    transfer::transfer(parent, ctx.sender());
}

public fun create_parent_and_child(ctx: &mut TxContext) {
    let mut parent_id = object::new(ctx);
    let child = Child { id: object::new(ctx) };
    let child_id = object::id(&child);
    dynamic_object_field::add(&mut parent_id, 0, child);
    let parent = Parent {
        id: parent_id,
        child: option::some(child_id),
    };
    transfer::transfer(parent, ctx.sender());
}

public fun add_child(parent: &mut Parent, child: Child) {
    let child_id = object::id(&child);
    parent.child.fill(child_id);
    dynamic_object_field::add(&mut parent.id, 0, child);
}

public fun add_child_wrapped(parent: &mut Parent, child: Child) {
    let child_id = object::id(&child);
    parent.child.fill(child_id);
    dynamic_field::add(&mut parent.id, 0, child);
}

// Call to mutate_child will fail if its owned by a parent,
// since all owners must be in the arguments for authentication.
public fun mutate_child(_child: &mut Child) {}

public fun mutate_child_with_parent(_child: &mut Child, _parent: &mut Parent) {}

public fun transfer_child(parent: &mut Parent, new_parent: &mut Parent) {
    let child_id = parent.child.extract();
    let child: Child = dynamic_object_field::remove(&mut parent.id, 0);
    assert!(object::id(&child) == child_id, 0);
    new_parent.child.fill(child_id);
    dynamic_object_field::add(&mut new_parent.id, 0, child);
}

public fun remove_child(parent: &mut Parent, ctx: &mut TxContext) {
    let child_id = parent.child.extract();
    let child: Child = dynamic_object_field::remove(&mut parent.id, 0);
    assert!(object::id(&child) == child_id, 0);
    transfer::public_transfer(child, ctx.sender());
}

public fun remove_wrapped_child(parent: &mut Parent, ctx: &mut TxContext) {
    let child_id = parent.child.extract();
    let child: Child = dynamic_field::remove(&mut parent.id, 0);
    assert!(object::id(&child) == child_id, 0);
    transfer::public_transfer(child, ctx.sender());
}

// Call to delete_child
public fun delete_child(child: Child) {
    let Child { id } = child;
    id.delete();
}

public fun delete_parent_and_child(parent: Parent) {
    let Parent { id: mut parent_id, child: mut child_ref_opt } = parent;
    let child_id = child_ref_opt.extract();
    let child: Child = dynamic_object_field::remove(&mut parent_id, 0);
    assert!(object::id(&child) == child_id, 0);
    parent_id.delete();
    let Child { id: child_id } = child;
    child_id.delete();
}

public fun create_another_parent(child: Child, ctx: &mut TxContext) {
    let mut id = object::new(ctx);
    let child_id = object::id(&child);
    dynamic_object_field::add(&mut id, 0, child);
    let parent = AnotherParent {
        id,
        child: child_id,
    };
    transfer::transfer(parent, ctx.sender());
}

public fun create_parent_and_child_wrapped(ctx: &mut TxContext) {
    let mut parent_id = object::new(ctx);
    let child = Child { id: object::new(ctx) };
    let child_id = object::id(&child);
    dynamic_field::add(&mut parent_id, 0, child);
    let parent = Parent {
        id: parent_id,
        child: option::some(child_id),
    };
    transfer::transfer(parent, ctx.sender());
}

public fun delete_parent_and_child_wrapped(parent: Parent) {
    let Parent { id: mut parent_id, child: mut child_ref_opt } = parent;
    let child_id = child_ref_opt.extract();
    let child: Child = dynamic_field::remove(&mut parent_id, 0);
    assert!(object::id(&child) == child_id, 0);
    parent_id.delete();
    let Child { id: child_id } = child;
    child_id.delete();
}
