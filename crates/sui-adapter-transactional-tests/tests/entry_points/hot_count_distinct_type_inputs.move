// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test of hot value rules for entry functions respect separate counts for distinct types
// of pure and receiving inputs

//# init --addresses test=0x0 --accounts A

//# publish
module test::m;

public struct A has key, store { id: UID }
public struct B has key, store { id: UID }

public struct Hot {}

public fun a(ctx: &mut TxContext): A {
    A { id: object::new(ctx) }
}

public fun id_imm<T>(r: &T): &T {
    r
}
public fun id_mut<T>(r: &mut T): &mut T {
    r
}

public fun heat_val<T>(t: T): (T, Hot) {
    (t, Hot {})
}
public fun heat_imm<T>(_: &T): Hot {
    Hot {}
}
public fun heat_mut<T>(_: &mut T): Hot {
    Hot {}
}

public fun cool(hot: Hot) {
    let Hot {} = hot;
}

entry fun play_u256(_: u256) {}
entry fun play_receiving(_: sui::transfer::Receiving<A>) {}

public fun delete(a: A) {
    let A { id } = a;
    object::delete(id);
}

//# programmable --sender A --inputs @A
//> test::m::a();
//> sui::transfer::public_transfer<test::m::A>(Result(0), Input(0));

//# set-address a object(2,0)

//# programmable --sender A --inputs @a
//> test::m::a();
//> sui::transfer::public_transfer<test::m::A>(Result(0), Input(0));

//# programmable --sender A --inputs @0
// valid since pure args are distinct per type usage
//> 0: test::m::heat_val<address>(Input(0));
//> 1: test::m::heat_imm<address>(Input(0));
//> 2: test::m::heat_mut<address>(Input(0));
//> test::m::play_u256(Input(0));
//> test::m::cool(NestedResult(0, 1));
//> test::m::cool(Result(1));
//> test::m::cool(Result(2));

//# programmable --sender A --inputs receiving(4,0)
// valid since receiving args are distinct per type usage
//> 0: test::m::heat_imm<sui::transfer::Receiving<test::m::B>>(Input(0));
//> test::m::play_receiving(Input(0));
//> test::m::cool(Result(0));
