// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module ObjectWrapping::ObjectWrapping {
    use std::option::{Self, Option};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};
    use Sui::ID::{Self, VersionedID};

    struct Child has key, store {
        id: VersionedID,
    }

    struct Parent has key {
        id: VersionedID,
        child: Option<Child>,
    }

    public entry fun create_child(ctx: &mut TxContext) {
        Transfer::transfer(
            Child {
                id: TxContext::new_id(ctx),
            },
            TxContext::sender(ctx),
        )
    }

    public entry fun create_parent(child: Child, ctx: &mut TxContext) {
        Transfer::transfer(
            Parent {
                id: TxContext::new_id(ctx),
                child: option::some(child),
            },
            TxContext::sender(ctx),
        )
    }

    public entry fun set_child(parent: &mut Parent, child: Child) {
        option::fill(&mut parent.child, child)
    }

    public entry fun extract_child(parent: &mut Parent, ctx: &mut TxContext) {
        let child = option::extract(&mut parent.child);
        Transfer::transfer(
            child,
            TxContext::sender(ctx),
        )
    }

    public entry fun delete_parent(parent: Parent) {
        let Parent { id: parent_id, child: child_opt } = parent;
        ID::delete(parent_id);
        if (option::is_some(&child_opt)) {
            let child = option::extract(&mut child_opt);
            let Child { id: child_id } = child;
            ID::delete(child_id);
        };
        option::destroy_none(child_opt)
    }
}
