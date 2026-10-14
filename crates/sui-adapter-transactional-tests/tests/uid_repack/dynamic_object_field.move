// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests changing the type of a dynamic object field child, keeping its dynamic fields, and
// moving it out of its parent with a different type.

//# init --addresses ex=0x0 --accounts A --flavor core

//# publish
module ex::m {
    use sui::dynamic_field;
    use sui::dynamic_object_field;
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    public struct Parent has key { id: UID }
    public struct A has key, store { id: UID }
    public struct B has key, store { id: UID }

    public fun setup(ctx: &mut TxContext) {
        let mut a = A { id: object::new(ctx) };
        dynamic_field::add(&mut a.id, b"value", 42u64);
        let mut p = Parent { id: object::new(ctx) };
        dynamic_object_field::add(&mut p.id, 0u64, a);
        transfer::transfer(p, tx_context::sender(ctx))
    }

    public fun child_a_to_b(p: &mut Parent) {
        let A { id } = dynamic_object_field::remove(&mut p.id, 0u64);
        dynamic_object_field::add(&mut p.id, 0u64, B { id })
    }

    public fun borrow_a(p: &Parent) {
        let a: &A = dynamic_object_field::borrow(&p.id, 0u64);
        assert!(*dynamic_field::borrow<vector<u8>, u64>(&a.id, b"value") == 42, 0);
    }

    public fun borrow_b(p: &Parent) {
        let b: &B = dynamic_object_field::borrow(&p.id, 0u64);
        assert!(*dynamic_field::borrow<vector<u8>, u64>(&b.id, b"value") == 42, 0);
    }

    public fun child_b_to_a_transfer(p: &mut Parent, ctx: &TxContext) {
        let B { id } = dynamic_object_field::remove(&mut p.id, 0u64);
        transfer::transfer(A { id }, tx_context::sender(ctx))
    }

    public fun use_a(a: &A) {
        assert!(*dynamic_field::borrow<vector<u8>, u64>(&a.id, b"value") == 42, 0);
    }
}

//# run ex::m::setup --sender A

//# view-object 2,0

//# run ex::m::borrow_a --args object(2,1) --sender A

//# run ex::m::child_a_to_b --args object(2,1) --sender A

//# view-object 2,0

// The child is now a B
//# run ex::m::borrow_a --args object(2,1) --sender A

//# run ex::m::borrow_b --args object(2,1) --sender A

// The child leaves the parent as an A
//# run ex::m::child_b_to_a_transfer --args object(2,1) --sender A

//# view-object 2,0

//# run ex::m::use_a --args object(2,0) --sender A
