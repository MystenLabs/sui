// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Simple test of hot value via shared overrides any hot count

//# init --addresses test=0x0 --accounts A

//# publish

module test::m;

public struct A has key, store { id: UID }

public struct Shared has key { id: UID }

public struct Hot {}

public fun a(ctx: &mut TxContext): A {
    A { id: object::new(ctx) }
}

public fun share(ctx: &mut TxContext) {
    sui::transfer::share_object(Shared { id: object::new(ctx) })
}

public fun heat_via_shared(_: &A, obj: Shared, _: &mut TxContext): Shared {
    obj
}

public fun heat(_: &A): Hot {
    Hot {}
}

public fun reshare(obj: Shared, _: &mut TxContext) {
    sui::transfer::share_object(obj)
}

public fun cool(hot: Hot) {
    let Hot {} = hot;
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

//# programmable --sender A
//> test::m::share();

//# programmable --sender A --inputs object(2,0) object(3,0)
// invalid even though the shared object and hot potato are gone
//> 0: test::m::heat(Input(0));
//> 1: test::m::heat_via_shared(Input(0), Input(1));
//> test::m::reshare(Result(1));
//> test::m::cool(Result(0));
//> test::m::delete_a(Input(0));

//# programmable --sender A --inputs object(2,1) object(3,0)
// invalid even though the shared object and hot potato are gone
//> 0: test::m::heat_via_shared(Input(0), Input(1));
//> 1: test::m::heat(Input(0));
//> test::m::cool(Result(1));
//> test::m::reshare(Result(0));
//> test::m::delete_a(Input(0));
