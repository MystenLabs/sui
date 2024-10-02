// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module move_test_code::tto {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer::{Self, Receiving};

    public struct A has key, store {
        id: UID,
    }

    public struct B has key, store {
        id: UID,
    }

    public fun start(ctx: &mut TxContext) {
        let a = A { id: object::new(ctx) };
        let a_address = object::id_address(&a);
        let b = B { id: object::new(ctx) };
        let c = B { id: object::new(ctx) };
        transfer::share_object(c);
        transfer::public_transfer(a, tx_context::sender(ctx));
        transfer::public_transfer(b, a_address);
    }

    public entry fun receiver(parent: &mut A, x: Receiving<B>) {
        let b = transfer::receive(&mut parent.id, x);
        // transfer back to the parent so we can reuse
        transfer::public_transfer(b, object::id_address(parent));
    }

    public entry fun deleter(parent: &mut A, x: Receiving<B>) {
        let B { id } = transfer::receive(&mut parent.id, x);
        object::delete(id);
    }
}
