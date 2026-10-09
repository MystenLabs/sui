// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that dynamic fields of an object follow its UID while the UID is stored bare in a dynamic
// field, and after it is revived as an object of a different type.

//# init --addresses ex=0x0 --accounts A --flavor core

//# publish
module ex::m {
    use sui::dynamic_field;
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    public struct Parent has key { id: UID }
    public struct A has key { id: UID }
    public struct B has key { id: UID }

    public fun setup(ctx: &mut TxContext) {
        let mut a = A { id: object::new(ctx) };
        dynamic_field::add(&mut a.id, 0u64, 1u64);
        transfer::transfer(a, tx_context::sender(ctx));
        transfer::transfer(Parent { id: object::new(ctx) }, tx_context::sender(ctx))
    }

    public fun stash(p: &mut Parent, a: A) {
        let A { id } = a;
        dynamic_field::add(&mut p.id, 0u64, id)
    }

    public fun bump_stashed(p: &mut Parent) {
        let id: &mut UID = dynamic_field::borrow_mut(&mut p.id, 0u64);
        let v: &mut u64 = dynamic_field::borrow_mut(id, 0u64);
        *v = *v + 1;
    }

    public fun unstash_as_b(p: &mut Parent, ctx: &TxContext) {
        let id: UID = dynamic_field::remove(&mut p.id, 0u64);
        transfer::transfer(B { id }, tx_context::sender(ctx))
    }

    public fun check_b(b: &B, expected: u64) {
        assert!(*dynamic_field::borrow<u64, u64>(&b.id, 0u64) == expected, 0);
    }
}

//# run ex::m::setup --sender A

//# run ex::m::stash --args object(2,1) object(2,0) --sender A

//# run ex::m::bump_stashed --args object(2,1) --sender A

//# run ex::m::unstash_as_b --args object(2,1) --sender A

//# view-object 2,0

//# run ex::m::check_b --args object(2,0) 2 --sender A
