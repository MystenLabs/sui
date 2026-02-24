// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sod::sod;

public struct A has key, store {
	id: UID,
}

public fun start(ctx: &mut TxContext) {
	let a = A { id: object::new(ctx) };
	transfer::public_share_object(a);
}

public fun delete(a: A) {
	let A { id } = a;
	id.delete();
}
