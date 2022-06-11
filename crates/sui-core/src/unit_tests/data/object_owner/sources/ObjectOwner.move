// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module ObjectOwner::ObjectOwner {
    use std::option::{Self, Option};
    use Sui::ID::{Self, VersionedID};
    use Sui::Transfer::{Self, ChildRef};
    use Sui::TxContext::{Self, TxContext};

    struct Parent has key {
        id: VersionedID,
        child: Option<ChildRef<Child>>,
    }

    struct Child has key {
        id: VersionedID,
    }

    struct AnotherParent has key {
        id: VersionedID,
        child: ChildRef<Child>,
    }

    public entry fun create_child(ctx: &mut TxContext) {
        Transfer::transfer(
            Child { id: TxContext::new_id(ctx) },
            TxContext::sender(ctx),
        );
    }

    public entry fun create_parent(ctx: &mut TxContext) {
        let parent = Parent {
            id: TxContext::new_id(ctx),
            child: option::none(),
        };
        Transfer::transfer(parent, TxContext::sender(ctx));
    }

    public entry fun create_parent_and_child(ctx: &mut TxContext) {
        let parent_id = TxContext::new_id(ctx);
        let child = Child { id: TxContext::new_id(ctx) };
        let (parent_id, child_ref) = Transfer::transfer_to_object_id(child, parent_id);
        let parent = Parent {
            id: parent_id,
            child: option::some(child_ref),
        };
        Transfer::transfer(parent, TxContext::sender(ctx));
    }

    public entry fun add_child(parent: &mut Parent, child: Child) {
        let child_ref = Transfer::transfer_to_object(child, parent);
        option::fill(&mut parent.child, child_ref);
    }

    // Call to mutate_child will fail if its owned by a parent,
    // since all owners must be in the arguments for authentication.
    public entry fun mutate_child(_child: &mut Child) {}

    // This should always succeeds, even when child is not owned by parent.
    public entry fun mutate_child_with_parent(_child: &mut Child, _parent: &mut Parent) {}

    public entry fun transfer_child(parent: &mut Parent, child: Child, new_parent: &mut Parent) {
        let child_ref = option::extract(&mut parent.child);
        let new_child_ref = Transfer::transfer_child_to_object(child, child_ref, new_parent);
        option::fill(&mut new_parent.child, new_child_ref);
    }

    public entry fun remove_child(parent: &mut Parent, child: Child, ctx: &mut TxContext) {
        let child_ref = option::extract(&mut parent.child);
        Transfer::transfer_child_to_address(child, child_ref, TxContext::sender(ctx));
    }

    // Call to delete_child can fail if it's still owned by a parent.
    public entry fun delete_child(child: Child, _parent: &mut Parent) {
        let Child { id } = child;
        ID::delete(id);
    }

    public entry fun delete_parent_and_child(parent: Parent, child: Child) {
        let Parent { id: parent_id, child: child_ref_opt } = parent;
        let child_ref = option::extract(&mut child_ref_opt);
        option::destroy_none(child_ref_opt);
        ID::delete(parent_id);

        let Child { id: child_id } = child;
        Transfer::delete_child_object(child_id, child_ref);
    }

    public entry fun create_another_parent(child: Child, ctx: &mut TxContext) {
        let id = TxContext::new_id(ctx);
        let (id, child_ref) = Transfer::transfer_to_object_id(child, id);
        let parent = AnotherParent {
            id,
            child: child_ref,
        };
        Transfer::transfer(parent, TxContext::sender(ctx));
    }
}
