// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module move_test_code::quasi_shared_objects {
    use Std::Option::{Self, Option};

    use Sui::ID::VersionedID;
    use Sui::Transfer::ChildRef;
    use Sui::TxContext::TxContext;
    use Sui::TxContext;
    use Sui::Transfer;

    struct Parent has key {
        id: VersionedID,
        child: Option<ChildRef<Child>>,
    }

    struct Child has key {
        id: VersionedID,
        counter: u64,
    }

    public(script) fun create_parent(ctx: &mut TxContext) {
        let parent = new_parent(ctx);
        Transfer::transfer(parent, TxContext::sender(ctx))
    }

    public(script) fun create_child(parent: &mut Parent, ctx: &mut TxContext) {
        create_child_impl(parent, ctx)
    }

    public(script) fun share_parent(parent: Parent) {
        Transfer::share_object(parent)
    }

    public(script) fun create_owned_parent_and_child(ctx: &mut TxContext) {
        let parent = new_parent(ctx);
        create_child_impl(&mut parent, ctx);
        Transfer::transfer(parent, TxContext::sender(ctx))
    }

    public(script) fun create_shared_parent_and_child(ctx: &mut TxContext) {
        let parent = new_parent(ctx);
        create_child_impl(&mut parent, ctx);
        share_parent(parent)
    }

    public(script) fun add_child(parent: &mut Parent, child: Child) {
        let child_ref = Transfer::transfer_to_object(child, parent);
        Option::fill(&mut parent.child, child_ref)
    }

    public(script) fun remove_child(parent: &mut Parent, child: Child, ctx: &mut TxContext) {
        let child_ref = Option::extract(&mut parent.child);
        Transfer::transfer_child_to_address(child, child_ref, TxContext::sender(ctx))
    }

    public(script) fun delete_child(parent: &mut Parent, child: Child) {
        let child_ref = Option::extract(&mut parent.child);
        let Child { id, counter: _ } = child;
        Transfer::delete_child_object(id, child_ref)
    }

    public(script) fun increment_counter(_parent: &Parent, child: &mut Child) {
        child.counter = child.counter + 1
    }

    fun new_parent(ctx: &mut TxContext): Parent {
        Parent {
            id: TxContext::new_id(ctx),
            child: Option::none(),
        }
    }

    fun create_child_impl(parent: &mut Parent, ctx: &mut TxContext) {
        let child = Child {
            id: TxContext::new_id(ctx),
            counter: 0,
        };
        add_child_impl(parent, child)
    }

    fun add_child_impl(parent: &mut Parent, child: Child) {
        let child_ref = Transfer::transfer_to_object(child, parent);
        Option::fill(&mut parent.child, child_ref)
    }
}
