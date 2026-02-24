// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module tto::tto;

use sui::transfer::Receiving;

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
	transfer::public_transfer(a, ctx.sender());
	transfer::public_transfer(b, a_address);
}

public fun receiver(parent: &mut A, x: Receiving<B>) {
	let b = transfer::receive(&mut parent.id, x);
	transfer::public_transfer(b, @tto);
}

public fun invalid_call_immut_ref(_parent: &mut A, _x: &Receiving<B>) {}

public fun invalid_call_mut_ref(_parent: &mut A, _x: &mut Receiving<B>) {}
