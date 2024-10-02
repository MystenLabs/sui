// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module tto::M3 {
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
        let fake_a = A { id: object::new(ctx) };
        let a_address = object::id_address(&a);
        let b = B { id: object::new(ctx) };
        transfer::public_transfer(fake_a, tx_context::sender(ctx));
        transfer::public_transfer(a, tx_context::sender(ctx));
        transfer::public_transfer(b, a_address);
    }

    public entry fun receiver(parent: &mut A, x: Receiving<B>) {
        let b = transfer::receive(&mut parent.id, x);
        transfer::public_transfer(b, @tto);
    }

    public entry fun simple(_parent: &mut A) { }


    public entry fun send_back(parent: &mut A, x: Receiving<B>) {
        let b = transfer::receive(&mut parent.id, x);
        let parent_address = object::id_address(parent);
        transfer::public_transfer(b, parent_address);
    }

    public entry fun deleter(parent: &mut A, x: Receiving<B>) {
        let B { id } = transfer::receive(&mut parent.id, x);
        object::delete(id);
    }
}

