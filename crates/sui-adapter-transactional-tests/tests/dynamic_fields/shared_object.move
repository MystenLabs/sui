// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests adding a shared object as a dynamic field

//# init --addresses test=0x0 --accounts A --shared-object-deletion true

//# publish

module test::m {
    use sui::dynamic_object_field as ofield;

    public struct Outer has key {
        id: UID,
        inner: Inner,
    }

    public struct Inner has key, store {
        id: UID,
    }

    public struct Child has key, store {
        id: UID,
        value: u64,
    }

    public fun new_parent(ctx: &mut TxContext): Outer {
        let inner = Inner { id: object::new(ctx) };
        Outer { id: object::new(ctx), inner }
    }

    public entry fun parent(ctx: &mut TxContext) {
        transfer::share_object(new_parent(ctx))
    }

    public entry fun child(ctx: &mut TxContext) {
        let child = Child { id: object::new(ctx), value: 0 };
        transfer::share_object(child)
    }

    public entry fun add_field(parent: &mut Outer, child: Child) {
        ofield::add(&mut parent.inner.id, 0u64, child);
    }

    public fun buy(parent: &mut Outer, ctx: &mut TxContext) {
        let mut new_parent = new_parent(ctx);
        swap(parent, &mut new_parent);
        give(&mut new_parent, tx_context::sender(ctx));
        transfer::share_object(new_parent)
    }

    public fun swap(old_parent: &mut Outer, new_parent: &mut Outer) {
        let child: Child = ofield::remove(&mut old_parent.inner.id, 0u64);
        ofield::add(&mut new_parent.inner.id, 0u64, child);
    }

    public fun give(parent: &mut Outer, recipient: address) {
        let child: Child = ofield::remove(&mut parent.inner.id, 0u64);
        transfer::transfer(child, recipient)
    }

    public fun make_dynamic_remove_and_then_share(ctx: &mut TxContext): Outer {
        let child = Child { id: object::new(ctx), value: 0 };

        let mut parent = new_parent(ctx);

        add_field(&mut parent, child);
        let c: Child = ofield::remove(&mut parent.id, 0u64);
        transfer::share_object(c);
        parent
    }
}

//# run test::m::parent --sender A

//# run test::m::child --sender A

// This aborts because shared objects cannot be added as dynamic fields
//# run test::m::add_field --sender A --args object(2,0) object(3,0)

//# view-object 3,0

//# run test::m::buy --sender A --args object(2,0)

//# view-object 3,0

//# run test::m::make_dynamic_remove_and_then_share
