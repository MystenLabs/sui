// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Simple test of hot value rules for entry functions

//# init --addresses test=0x0 --accounts A

//# publish

module test::m;

public struct A has key, store { id: UID }
public struct B has key, store { id: UID }

public struct Trade {}

public fun a(ctx: &mut TxContext): A {
    A { id: object::new(ctx) }
}

public fun trade(a: A, _: &mut TxContext): Trade {
    let A { id } = a;
    object::delete(id);
    Trade {}
}

public fun b(_: &mut Trade, ctx: &mut TxContext): B {
    B { id: object::new(ctx) }
}

public fun settle(t: Trade, _: &mut TxContext) {
    let Trade {} = t;
}

entry fun play(b: B, _: &mut TxContext) {
    let B { id } = b;
    object::delete(id);
}

//# programmable --sender A --inputs @A
//> test::m::a();
//> sui::transfer::public_transfer<test::m::A>(Result(0), Input(0));
//> test::m::a();
//> sui::transfer::public_transfer<test::m::A>(Result(2), Input(0));

//# programmable --sender A --inputs @A object(2,0)
// Valid in V2 (but not the original adapter)
//> 0: test::m::trade(Input(1));
//> 1: test::m::b(Result(0));
//> test::m::settle(Result(0));
//> test::m::play(Result(1));

//# programmable --sender A --inputs @A object(2,1)
// invalid in both
//> 0: test::m::trade(Input(1));
//> 1: test::m::b(Result(0));
// not valid since Trade is not yet consumed
//> test::m::play(Result(1));
//> test::m::settle(Result(0));
