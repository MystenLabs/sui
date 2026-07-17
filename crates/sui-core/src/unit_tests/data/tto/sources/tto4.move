// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module tto::M4;

use sui::transfer::Receiving;

public struct A has key, store {
    id: UID,
}

public struct B has key, store {
    id: UID,
}

public fun start1(ctx: &mut TxContext) {
    let a = A { id: object::new(ctx) };
    transfer::public_transfer(a, ctx.sender());
}

public fun start2(ctx: &mut TxContext) {
    let b = B { id: object::new(ctx) };
    transfer::public_transfer(b, ctx.sender());
}

public fun transfer(addr: address, b: B) {
    transfer::public_transfer(b, addr);
}

public fun receiver(parent: &mut A, x: Receiving<B>) {
    let b = transfer::receive(&mut parent.id, x);
    transfer::public_transfer(b, @tto);
}

public fun deleter(parent: &mut A, x: Receiving<B>) {
    let B { id } = transfer::receive(&mut parent.id, x);
    id.delete();
}

public fun nop(_parent: &mut A, _x: Receiving<B>) {}

public fun aborter(_parent: &mut A, _x: Receiving<B>) { abort 0 }

public fun receive_abort(parent: &mut A, x: Receiving<B>) {
    let _b = transfer::receive(&mut parent.id, x);
    abort 0
}

public fun receive_type_mismatch(parent: &mut A, x: Receiving<A>) {
    let _b: A = transfer::receive(&mut parent.id, x);
    abort 0
}
