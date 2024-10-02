// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module tto::M5 {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer::{Self, Receiving};
    use sui::dynamic_object_field as dof;

    public struct A has key, store {
        id: UID,
    }

    public struct B has key, store {
        id: UID,
    }

    // step 1 and 2
    public fun start(ctx: &mut TxContext) {
        let a = A { id: object::new(ctx) };
        transfer::share_object(a);
        let b = B { id: object::new(ctx) };
        transfer::public_transfer(b, tx_context::sender(ctx));
    }

    // Step 3
    // Now sign deleter with parent (result of start1) and child (result of start2).
    // Don't execute this transaction though.


    // Step 4: sign and execute this transaction
    public fun add_dof(parent: &mut A, obj: B) {
        dof::add(&mut parent.id, 1, obj);
    }

    // Step 5: now execute what was signed in step 3

    public fun deleter(parent: &mut A, _x: Receiving<B>) {
        let b = dof::remove(&mut parent.id, 1);
        let B { id } = b;
        object::delete(id);
    }

}
