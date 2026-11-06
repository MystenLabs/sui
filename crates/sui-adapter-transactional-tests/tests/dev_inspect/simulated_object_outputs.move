// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Dry-run and dev-inspect must leave created and unwrapped simulated outputs unnumbered.
// Both must leave the wrapped parent unchanged
// so that subsequent committed execution can still unwrap its child.

//# init --addresses test=0x0 --accounts A

//# publish --sender A
module test::m;

public struct First has key, store { id: UID, value: u64 }
public struct Second has key, store { id: UID, value: u64 }
public struct Parent has key, store { id: UID, child: First }

public fun create(ctx: &mut TxContext) {
    let second = Second { id: object::new(ctx), value: 2 };
    let first = First { id: object::new(ctx), value: 1 };
    transfer::public_transfer(second, ctx.sender());
    transfer::public_transfer(first, ctx.sender())
}

public fun make_wrapped(ctx: &mut TxContext) {
    let child = First { id: object::new(ctx), value: 7 };
    transfer::public_transfer(Parent { id: object::new(ctx), child }, ctx.sender())
}

public fun unwrap(parent: Parent, ctx: &mut TxContext) {
    let Parent { id, child } = parent;
    object::delete(id);
    transfer::public_transfer(child, ctx.sender())
}

//# run test::m::make_wrapped --sender A

//# programmable --sender A --dev-inspect
// Created simulated outputs are unnumbered.
//> test::m::create();

//# programmable --sender A --dry-run
// Dry-run uses the same convention for multiple created objects.
//> test::m::create();

//# programmable --sender A --dev-inspect --inputs object(2,0)
// The simulated unwrapped output is also unnumbered.
//> test::m::unwrap(Input(0));

//# view-object 2,0
// Dev-inspect did not delete the parent or unwrap its child in persistent storage.

//# programmable --sender A --dry-run --inputs object(2,0)
// The same parent is still available; its simulated unwrapped output is unnumbered.
//> test::m::unwrap(Input(0));

//# view-object 2,0
// Dry-run also left the parent and its wrapped child unchanged.

//# run test::m::unwrap --sender A --args object(2,0)
// Committed execution can still unwrap the object after both simulations.
