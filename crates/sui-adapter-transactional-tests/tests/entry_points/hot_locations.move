// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Simple test of hot value rules each location and usage

//# init --addresses test=0x0 a=0x0 --accounts A --allow-references-in-ptbs

//# publish

module test::m;

use sui::coin::Coin;
use sui::sui::SUI;

public struct A has key, store { id: UID }

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

entry fun play_a(_: &A) {}
entry fun play_coin(_: &Coin<SUI>) {}
entry fun play_u64(_: u64) {}
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

//# programmable --sender A --inputs object(2,0)
// object input by-ref
//> 0: test::m::heat_imm<test::m::A>(Input(0));
//> test::m::cool(Result(0));
//> test::m::play_a(Input(0));

//# programmable --sender A --inputs object(2,0)
// object input by-mut
//> 0: test::m::heat_mut<test::m::A>(Input(0));
//> test::m::cool(Result(0));
//> test::m::play_a(Input(0));

//# programmable --sender A
// gas coin by-ref
//> test::m::heat_imm<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> test::m::cool(Result(0));
//> test::m::play_coin(Gas);

//# programmable --sender A
// gas coin by-mut
//> test::m::heat_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> test::m::cool(Result(0));
//> test::m::play_coin(Gas)

//# programmable --sender A --inputs object(2,0)
// result ref copy
//> 0: test::m::id_imm<test::m::A>(Input(0));
//> 1: test::m::heat_imm<test::m::A>(Result(0));
//> test::m::cool(Result(1));
//> test::m::play_a(Result(0));
//> test::m::play_a(Result(0));
//> test::m::play_a(Input(0));

//# programmable --sender A --inputs object(2,0)
// result ref move
//> 0: test::m::id_imm<test::m::A>(Input(0));
//> 1: test::m::heat_imm<test::m::A>(Result(0));
//> test::m::cool(Result(1));
//> test::m::play_a(Input(0));

//# programmable --sender A --inputs object(2,0)
// result mut ref copy
//> 0: test::m::id_mut<test::m::A>(Input(0));
//> 1: test::m::heat_mut<test::m::A>(Result(0));
//> test::m::cool(Result(1));
//> test::m::play_a(Result(0));
//> test::m::play_a(Result(0));
//> test::m::play_a(Input(0));

//# programmable --sender A --inputs object(2,0)
// result mut ref freeze
//> 0: test::m::id_mut<test::m::A>(Input(0));
//> 1: test::m::heat_imm<test::m::A>(Result(0));
//> test::m::cool(Result(1));
//> test::m::play_a(Result(0));
//> test::m::play_a(Result(0));
//> test::m::play_a(Input(0));

//# programmable --sender A --inputs object(2,0)
// result mut ref move
//> 0: test::m::id_mut<test::m::A>(Input(0));
//> 1: test::m::heat_mut<test::m::A>(Result(0));
//> test::m::cool(Result(1));
//> test::m::play_a(Input(0));

//# programmable --sender A --inputs 0u64
// pure input by-value
//> 0: test::m::heat_val<u64>(Input(0));
//> test::m::cool(NestedResult(0,1));
//> test::m::play_u64(Input(0));

//# programmable --sender A --inputs 0u64
// pure input by-ref
//> 0: test::m::heat_imm<u64>(Input(0));
//> test::m::cool(Result(0));
//> test::m::play_u64(Input(0));

//# programmable --sender A --inputs 0u64
// pure input by-mut
//> 0: test::m::heat_mut<u64>(Input(0));
//> test::m::cool(Result(0));
//> test::m::play_u64(Input(0));

//# programmable --sender A --inputs receiving(4,0)
// receiving input by-ref
//> 0: test::m::heat_imm<sui::transfer::Receiving<test::m::A>>(Input(0));
//> test::m::cool(Result(0));
//> test::m::play_receiving(Input(0));

//# programmable --sender A --inputs receiving(4,0)
// receiving input by-mut
//> 0: test::m::heat_mut<sui::transfer::Receiving<test::m::A>>(Input(0));
//> test::m::cool(Result(0));
//> test::m::play_receiving(Input(0));

//# programmable --sender A --inputs receiving(4,0)
// receiving input by-val
//> 0: test::m::heat_val<sui::transfer::Receiving<test::m::A>>(Input(0));
//> test::m::cool(NestedResult(0,1));
//> test::m::play_receiving(NestedResult(0,0));
