// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module move_test_code::quasi_shared_objects {
    use std::option::{Self, Option};
    use sui::object::{Self, UID, ID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    struct Parent has key {
        id: UID,
        child: Option<ID>,
    }

    struct Child has key {
        id: UID,
        counter: u64,
    }

    public entry fun create_parent(ctx: &mut TxContext) {
        let parent = Parent { id: object::new(ctx), child: option::none() };
        transfer::transfer(parent, tx_context::sender(ctx))
    }

    public entry fun create_child(parent: &mut Parent, ctx: &mut TxContext) {
        let child = Child {
            id: object::new(ctx),
            counter: 0,
        };
        add_child(parent, child)
    }

    public entry fun share_parent(parent: Parent) {
        transfer::share_object(parent)
    }

    public entry fun create_owned_parent_and_child(ctx: &mut TxContext) {
        let parent = Parent { id: object::new(ctx), child: option::none() };
        create_child(&mut parent, ctx);
        transfer::transfer(parent, tx_context::sender(ctx))
    }

    public entry fun create_shared_parent_and_child(ctx: &mut TxContext) {
        let parent = Parent { id: object::new(ctx), child: option::none() };
        create_child(&mut parent, ctx);
        transfer::share_object(parent)
    }

    public entry fun create_immutable_parent(ctx: &mut TxContext) {
        let parent = Parent { id: object::new(ctx), child: option::none() };
        transfer::freeze_object(parent);
    }

    public entry fun add_child(parent: &mut Parent, child: Child) {
        option::fill(&mut parent.child, object::id(&child));
        transfer::transfer_to_object(child, parent);
    }

    public entry fun remove_child(parent: &mut Parent, child: Child, ctx: &mut TxContext) {
        option::extract(&mut parent.child);
        transfer::transfer(child, tx_context::sender(ctx))
    }

    public entry fun delete_child(parent: &mut Parent, child: Child) {
        option::extract(&mut parent.child);
        let Child { id, counter: _ } = child;
        object::delete(id)
    }

    public entry fun increment_counter(_parent: &Parent, child: &mut Child) {
        child.counter = child.counter + 1
    }

    public entry fun use_parent(_parent: &Parent) {}

}
