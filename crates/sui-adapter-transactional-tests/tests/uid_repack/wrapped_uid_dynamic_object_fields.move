// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that a dynamic object field child (and its own dynamic field) is read at the root's version
// while its parent UID is stored bare in a dynamic field, moved between parents, and revived as an
// object of a different type.

//# init --addresses ex=0x0 --accounts A --flavor core

//# publish
module ex::m {
    use sui::dynamic_field;
    use sui::dynamic_object_field;
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    public struct Parent has key { id: UID }
    public struct A has key { id: UID }
    public struct B has key { id: UID }
    public struct Child has key, store { id: UID, value: u64 }

    public fun setup(ctx: &mut TxContext) {
        transfer::transfer(Parent { id: object::new(ctx) }, tx_context::sender(ctx));
        transfer::transfer(Parent { id: object::new(ctx) }, tx_context::sender(ctx));
        let mut a = A { id: object::new(ctx) };
        let mut c = Child { id: object::new(ctx), value: 1 };
        dynamic_field::add(&mut c.id, 0u64, 10u64);
        dynamic_object_field::add(&mut a.id, 0u64, c);
        transfer::transfer(a, tx_context::sender(ctx))
    }

    fun bump(id: &mut UID) {
        let c: &mut Child = dynamic_object_field::borrow_mut(id, 0u64);
        c.value = c.value + 1;
        let v: &mut u64 = dynamic_field::borrow_mut(&mut c.id, 0u64);
        *v = *v + 1;
    }

    public fun stash(p: &mut Parent, a: A) {
        let A { id } = a;
        dynamic_field::add(&mut p.id, 0u64, id)
    }

    public fun bump_stashed(p: &mut Parent) {
        bump(dynamic_field::borrow_mut(&mut p.id, 0u64))
    }

    public fun move_stashed(p: &mut Parent, q: &mut Parent) {
        let mut id: UID = dynamic_field::remove(&mut p.id, 0u64);
        bump(&mut id);
        dynamic_field::add(&mut q.id, 0u64, id)
    }

    public fun unstash_as_b(q: &mut Parent, ctx: &TxContext) {
        let mut id: UID = dynamic_field::remove(&mut q.id, 0u64);
        bump(&mut id);
        transfer::transfer(B { id }, tx_context::sender(ctx))
    }

    public fun bump_b(b: &mut B) {
        bump(&mut b.id)
    }

    public fun check_b(b: &B, value: u64, inner: u64) {
        let c: &Child = dynamic_object_field::borrow(&b.id, 0u64);
        assert!(c.value == value, 0);
        assert!(*dynamic_field::borrow<u64, u64>(&c.id, 0u64) == inner, 1);
    }
}

//# run ex::m::setup --sender A

//# view-object 2,3

//# run ex::m::stash --args object(2,0) object(2,2) --sender A

//# run ex::m::bump_stashed --args object(2,0) --sender A

//# view-object 2,3

//# run ex::m::move_stashed --args object(2,0) object(2,1) --sender A

//# view-object 2,3

//# run ex::m::unstash_as_b --args object(2,1) --sender A

//# run ex::m::bump_b --args object(2,2) --sender A

//# view-object 2,3

//# run ex::m::check_b --args object(2,2) 5 14 --sender A
