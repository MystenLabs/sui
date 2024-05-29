// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module object_wrapping::object_wrapping {
    use std::option::{Self, Option};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::object::{Self, UID};

    public struct Child has key, store {
        id: UID,
    }

    public struct Parent has key {
        id: UID,
        child: Option<Child>,
    }

    public entry fun create_child(ctx: &mut TxContext) {
        transfer::public_transfer(
            Child {
                id: object::new(ctx),
            },
            tx_context::sender(ctx),
        )
    }

    public entry fun create_parent(child: Child, ctx: &mut TxContext) {
        transfer::transfer(
            Parent {
                id: object::new(ctx),
                child: option::some(child),
            },
            tx_context::sender(ctx),
        )
    }

    public entry fun set_child(parent: &mut Parent, child: Child) {
        option::fill(&mut parent.child, child)
    }

    public entry fun extract_child(parent: &mut Parent, ctx: &mut TxContext) {
        let child = option::extract(&mut parent.child);
        transfer::public_transfer(
            child,
            tx_context::sender(ctx),
        )
    }

    public entry fun delete_parent(parent: Parent) {
        let Parent { id: parent_id, child: mut child_opt } = parent;
        object::delete(parent_id);
        if (option::is_some(&child_opt)) {
            let child = option::extract(&mut child_opt);
            let Child { id: child_id } = child;
            object::delete(child_id);
        };
        option::destroy_none(child_opt)
    }
}
