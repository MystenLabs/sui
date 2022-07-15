// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module object_owner::object_owner {
    use std::option::{Self, Option};
    use sui::id::{Self, ID, VersionedID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    struct Parent has key {
        id: VersionedID,
        child: Option<ID>,
    }

    struct Child has key {
        id: VersionedID,
    }

    struct AnotherParent has key {
        id: VersionedID,
        child: ID,
    }

    public entry fun create_child(ctx: &mut TxContext) {
        transfer::transfer(
            Child { id: tx_context::new_id(ctx) },
            tx_context::sender(ctx),
        );
    }

    public entry fun create_parent(ctx: &mut TxContext) {
        let parent = Parent {
            id: tx_context::new_id(ctx),
            child: option::none(),
        };
        transfer::transfer(parent, tx_context::sender(ctx));
    }

    public entry fun create_parent_and_child(ctx: &mut TxContext) {
        let parent_id = tx_context::new_id(ctx);
        let child = Child { id: tx_context::new_id(ctx) };
        let child_id = *id::id(&child);
        transfer::transfer_to_object_id(child, &parent_id);
        let parent = Parent {
            id: parent_id,
            child: option::some(child_id),
        };
        transfer::transfer(parent, tx_context::sender(ctx));
    }

    public entry fun add_child(parent: &mut Parent, child: Child) {
        let child_id = *id::id(&child);
        transfer::transfer_to_object(child, parent);
        option::fill(&mut parent.child, child_id);
    }

    // Call to mutate_child will fail if its owned by a parent,
    // since all owners must be in the arguments for authentication.
    public entry fun mutate_child(_child: &mut Child) {}

    // This should always succeeds, even when child is not owned by parent.
    public entry fun mutate_child_with_parent(_child: &mut Child, _parent: &mut Parent) {}

    public entry fun transfer_child(parent: &mut Parent, child: Child, new_parent: &mut Parent) {
        let child_id = option::extract(&mut parent.child);
        assert!(id::id(&child) == &child_id, 0);
        transfer::transfer_to_object(child, new_parent);
        option::fill(&mut new_parent.child, child_id);
    }

    public entry fun remove_child(parent: &mut Parent, child: Child, ctx: &mut TxContext) {
        let child_id = option::extract(&mut parent.child);
        assert!(id::id(&child) == &child_id, 0);
        transfer::transfer(child, tx_context::sender(ctx));
    }

    // Call to delete_child can fail if it's still owned by a parent.
    public entry fun delete_child(child: Child, _parent: &mut Parent) {
        let Child { id } = child;
        id::delete(id);
    }

    public entry fun delete_parent_and_child(parent: Parent, child: Child) {
        let Parent { id: parent_id, child: child_ref_opt } = parent;
        let child_id = option::extract(&mut child_ref_opt);
        assert!(id::id(&child) == &child_id, 0);
        id::delete(parent_id);

        let Child { id: child_id } = child;
        id::delete(child_id);
    }

    public entry fun create_another_parent(child: Child, ctx: &mut TxContext) {
        let id = tx_context::new_id(ctx);
        let child_id = *id::id(&child);
        transfer::transfer_to_object_id(child, &id);
        let parent = AnotherParent {
            id,
            child: child_id,
        };
        transfer::transfer(parent, tx_context::sender(ctx));
    }
}
