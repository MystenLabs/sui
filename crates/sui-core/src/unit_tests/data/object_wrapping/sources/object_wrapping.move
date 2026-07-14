// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module object_wrapping::object_wrapping;

public struct Child has key, store {
    id: UID,
}

public struct Parent has key {
    id: UID,
    child: Option<Child>,
}

public fun create_child(ctx: &mut TxContext) {
    transfer::public_transfer(
        Child {
            id: object::new(ctx),
        },
        ctx.sender(),
    )
}

public fun create_parent(child: Child, ctx: &mut TxContext) {
    transfer::transfer(
        Parent {
            id: object::new(ctx),
            child: option::some(child),
        },
        ctx.sender(),
    )
}

public fun set_child(parent: &mut Parent, child: Child) {
    parent.child.fill(child)
}

public fun extract_child(parent: &mut Parent, ctx: &mut TxContext) {
    let child = parent.child.extract();
    transfer::public_transfer(
        child,
        ctx.sender(),
    )
}

public fun delete_parent(parent: Parent) {
    let Parent { id: parent_id, child: mut child_opt } = parent;
    parent_id.delete();
    if (child_opt.is_some()) {
        let child = child_opt.extract();
        let Child { id: child_id } = child;
        child_id.delete();
    };
    child_opt.destroy_none()
}
