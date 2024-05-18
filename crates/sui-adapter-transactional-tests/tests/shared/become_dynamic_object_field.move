// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests that shared objects cannot become dynamic fields and that a shared object
// dynamic field added and removed in the same transaction does not error

//# init --addresses a=0x0 --accounts A --shared-object-deletion true

//# publish
module a::m {
    use sui::dynamic_object_field::{add, remove};

    public struct Outer has key, store {
        id: object::UID,
    }

    public struct Inner has key, store {
        id: object::UID,
    }

    public entry fun create_shared(ctx: &mut TxContext) {
        transfer::public_share_object(Inner { id: object::new(ctx) })
    }

    public entry fun add_dynamic_object_field(inner: Inner, ctx: &mut TxContext) {
        let mut outer = Outer {id: object::new(ctx)};
        add(&mut outer.id, 0, inner);
        transfer::transfer(outer, ctx.sender());
    }

    public entry fun add_and_remove_dynamic_object_field(inner: Inner, ctx: &mut TxContext) {
        let mut outer = Outer {id: object::new(ctx)};
        add(&mut outer.id, 0, inner);
        let removed: Inner = remove(&mut outer.id, 0);
        transfer::public_share_object(removed);
        transfer::transfer(outer, ctx.sender());
    }

}

//# run a::m::create_shared --sender A

//# view-object 2,0

//# run a::m::add_dynamic_object_field --sender A --args object(2,0)

//# run a::m::create_shared --sender A

//# view-object 5,0

//# run a::m::add_and_remove_dynamic_object_field --sender A --args object(5,0)
