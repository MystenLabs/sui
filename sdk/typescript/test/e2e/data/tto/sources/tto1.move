// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module tto::tto {
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
        transfer::public_transfer(b, @tto);
    }

    public entry fun deleter(parent: &mut A, x: Receiving<B>) {
        let B { id } = transfer::receive(&mut parent.id, x);
        object::delete(id);
    }

    public fun return_(parent: &mut A, x: Receiving<B>): B {
        transfer::receive(&mut parent.id, x)
    }

    public entry fun delete_(b: B) {
        let B { id } = b;
        object::delete(id);
    }

    public fun invalid_call_immut_ref(_parent: &mut A, _x: &Receiving<B>) { }
    public fun invalid_call_mut_ref(_parent: &mut A, _x: &mut Receiving<B>) { }
    public fun dropper(_parent: &mut A, _x: Receiving<B>) { }
}
