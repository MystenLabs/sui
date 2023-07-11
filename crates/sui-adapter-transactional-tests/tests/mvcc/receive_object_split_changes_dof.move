// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses tto=0x0 --accounts A

//# publish
module tto::M1 {
    // use std::option::{Self, Option};
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer::{Self, Receiving};
    use sui::dynamic_object_field as dof;

    const KEY: u64 = 0;
    const BKEY: u64 = 1;

    struct A has key, store {
        id: UID,
        value: u64,
    }

    public fun start(ctx: &mut TxContext) {
        let a_parent = A { id: object::new(ctx), value: 0 };
        let a_child = A { id: object::new(ctx), value: 0 };

        let b_parent = A { id: object::new(ctx), value: 0 };
        let b_child = A { id: object::new(ctx), value: 0 };
        dof::add(&mut a_parent.id, KEY, a_child);
        dof::add(&mut b_parent.id, KEY, b_child);
        transfer::public_transfer(a_parent, tx_context::sender(ctx));
        transfer::public_transfer(b_parent, tx_context::sender(ctx));
    }

    public entry fun receive(a_parent: &mut A, x: Receiving<A>,apv: u64, acv: u64, bpv: u64, bcv: u64) {
        let b_parent = transfer::receive(&mut a_parent.id, x);
        dof::add(&mut a_parent.id, BKEY, b_parent);
        let b_parent: &A = dof::borrow(&a_parent.id, BKEY);
        let b_child: &A = dof::borrow(&b_parent.id, KEY);
        let a_child: &A = dof::borrow(&a_parent.id, KEY);
        assert!(a_parent.value == apv, 0);
        assert!(a_child.value == acv, 1);
        assert!(b_parent.value == bpv, 2);
        assert!(b_child.value == bcv, 3);
    }

    public entry fun mutate(b_parent: A, a_parent: &A) {
        let b_child: &mut A = dof::borrow_mut(&mut b_parent.id, KEY);
        b_parent.value = 40;
        b_child.value = 40;
        let a_address = object::id_address(a_parent);
        transfer::public_transfer(b_parent, a_address);
    }

}

//# run tto::M1::start --sender A

//# view-object 2,0

//# view-object 2,1

//# view-object 2,2

//# view-object 2,3

//# view-object 2,4

//# view-object 2,5

//# run tto::M1::mutate --args object(2,3) object(2,5) --sender A

//# view-object 2,0

//# view-object 2,1

//# view-object 2,2

//# view-object 2,3

//# view-object 2,4

//# view-object 2,5

//# run tto::M1::receive --args object(2,5) receiving(2,3) 0 0 40 40 --sender A
