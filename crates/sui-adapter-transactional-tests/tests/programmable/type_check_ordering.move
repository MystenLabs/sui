// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests ordering of multiple incorrect arguments

//# init --addresses test=0x0

//# publish
module test::m;

public struct A has key {
    id: UID,
}
public struct B has key {
    id: UID,
}
public struct C has key {
    id: UID,
}

public fun a(ctx: &mut TxContext) {
    transfer::share_object(A { id: object::new(ctx) });
}

public fun b(ctx: &mut TxContext) {
    transfer::share_object(B { id: object::new(ctx) });
}

public fun c(ctx: &mut TxContext) {
    transfer::share_object(C { id: object::new(ctx) });
}

public fun t0(_: &mut A, _: &B, _: &C, _: &TxContext)
{
}

//# programmable
//> test::m::a()

//# programmable
//> test::m::b()

//# programmable
//> test::m::c()


//# programmable --inputs object(2,0) object(3,0) object(4,0)
// Should fail with type error in the first argument, even though the third is also incorrect
//> test::m::t0(Input(2), Input(1), Input(0))
