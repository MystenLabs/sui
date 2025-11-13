// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Simple test of hot value rules for entry functions, where the heat is caused by taking a
// shared object by value

//# init --addresses test=0x0 --accounts A

//# publish

module test::m;

public struct A has key, store { id: UID }
public struct B has key, store { id: UID }

public struct Trade has key { id: UID }

public fun a(ctx: &mut TxContext): A {
    A { id: object::new(ctx) }
}

public fun init_trade(ctx: &mut TxContext) {
    sui::transfer::share_object(Trade { id: object::new(ctx) })
}

public fun trade(a: A, trade: Trade, _: &mut TxContext): Trade {
    let A { id } = a;
    object::delete(id);
    trade
}

public fun b(_: &mut Trade, ctx: &mut TxContext): B {
    B { id: object::new(ctx) }
}

public fun settle(t: Trade, _: &mut TxContext) {
    let Trade { id } = t;
    object::delete(id);
}

entry fun play(b: B, _: &mut TxContext) {
    let B { id } = b;
    object::delete(id);
}

public fun heat(_: &A, trade: Trade) {
    sui::transfer::share_object(trade)
}

entry fun delete_a(a: A) {
    let A { id } = a;
    object::delete(id);
}

//# programmable --sender A --inputs @A
//> test::m::a();
//> sui::transfer::public_transfer<test::m::A>(Result(0), Input(0));
//> test::m::a();
//> sui::transfer::public_transfer<test::m::A>(Result(2), Input(0));
//> test::m::init_trade();

//# programmable --sender A --inputs @A object(2,0) object(2,2)
// invalid in both
//> 0: test::m::trade(Input(1), Input(2));
//> 1: test::m::b(Result(0));
//> test::m::settle(Result(0));
//> test::m::play(Result(1));

//# programmable --sender A --inputs @A object(2,0) object(2,2)
// Invalid in v2
//> test::m::heat(Input(1), Input(2));
//> test::m::delete_a(Input(1));
