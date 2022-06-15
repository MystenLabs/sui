// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module object_wrapping::object_wrapping {
    use std::option::{Self, Option};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::id::{Self, VersionedID};

    struct Child has key, store {
        id: VersionedID,
    }

    struct Parent has key {
        id: VersionedID,
        child: Option<Child>,
    }

    public entry fun create_child(ctx: &mut TxContext) {
        transfer::transfer(
            Child {
                id: tx_context::new_id(ctx),
            },
            tx_context::sender(ctx),
        )
    }

    public entry fun create_parent(child: Child, ctx: &mut TxContext) {
        transfer::transfer(
            Parent {
                id: tx_context::new_id(ctx),
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
        transfer::transfer(
            child,
            tx_context::sender(ctx),
        )
    }

    public entry fun delete_parent(parent: Parent) {
        let Parent { id: parent_id, child: child_opt } = parent;
        id::delete(parent_id);
        if (option::is_some(&child_opt)) {
            let child = option::extract(&mut child_opt);
            let Child { id: child_id } = child;
            id::delete(child_id);
        };
        option::destroy_none(child_opt)
    }
}
