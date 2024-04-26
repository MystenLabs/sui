// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module tto::M1 {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer::{Self, Receiving};
    use sui::dynamic_object_field;

    public struct A has key, store {
        id: UID,
    }

    public struct B has key, store {
        id: UID,
    }

    public struct C has key {
        id: UID, 
        wrapped: B,
    }

    public fun start(ctx: &mut TxContext) {
        let a = A { id: object::new(ctx) };
        let a_address = object::id_address(&a);
        let b = B { id: object::new(ctx) };
        let c = A { id: object::new(ctx) };
        let mut d = A { id: object::new(ctx) };
        let e = A { id: object::new(ctx) };
        dynamic_object_field::add(&mut d.id, 0, e);

        transfer::public_transfer(a, tx_context::sender(ctx));
        transfer::public_transfer(b, a_address);
        transfer::freeze_object(c);
        transfer::share_object(d);
    }

    public entry fun receiver(parent: &mut A, x: Receiving<B>) {
        let b = transfer::receive(&mut parent.id, x);
        transfer::public_transfer(b, @tto);
    }

    public entry fun send_back(parent: &mut A, x: Receiving<B>) {
        let b = transfer::receive(&mut parent.id, x);
        let parent_address = object::id_address(parent);
        transfer::public_transfer(b, parent_address);
    }

    public entry fun deleter(parent: &mut A, x: Receiving<B>) {
        let B { id } = transfer::receive(&mut parent.id, x);
        object::delete(id);
    }

    public entry fun wrapper(parent: &mut A, x: Receiving<B>, ctx: &mut TxContext) {
        let b = transfer::receive(&mut parent.id, x);
        let c = C { id: object::new(ctx), wrapped: b };
        transfer::transfer(c, @tto);
    }

    public fun call_immut_ref(_parent: &mut A, _x: &Receiving<B>) { }
    public fun call_mut_ref(_parent: &mut A, _x: &mut Receiving<B>) { }
}
