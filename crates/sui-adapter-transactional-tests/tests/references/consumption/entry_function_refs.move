// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }
public struct Hot {}

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun inner(o: &Obj): &Inner { &o.inner }
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun heat(_: &Obj): Hot { Hot {} }
public fun cool(h: Hot) { let Hot {} = h; }
entry fun play(_: &Inner) {}
entry fun play_mut(_: &mut Inner) {}
entry fun play_val(_: Inner) {}
public entry fun public_play(_: &Inner) {}

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable --sender A --inputs object(2,0)
// VALID: reference results rooted in an input reach a private entry function
//> 0: test::m::inner(Input(0));
//> 1: test::m::play(Result(0));
//> 2: test::m::inner_mut(Input(0));
//> 3: test::m::play_mut(Result(2));

//# programmable --sender A --inputs object(2,0)
// INVALID: InvalidArgumentToPrivateEntryFunction at arg 0 of command 2, the reference's clique holds a hot potato
//> 0: test::m::heat(Input(0));
//> 1: test::m::inner(Input(0));
//> 2: test::m::play(Result(1));
//> 3: test::m::cool(Result(0));

//# programmable --sender A --inputs object(2,0)
// VALID: the same shape reaches a public entry function
//> 0: test::m::heat(Input(0));
//> 1: test::m::inner(Input(0));
//> 2: test::m::public_play(Result(1));
//> 3: test::m::cool(Result(0));

//# programmable --sender A --inputs object(2,0)
// VALID: the hot potato is cooled before the entry call
//> 0: test::m::heat(Input(0));
//> 1: test::m::cool(Result(0));
//> 2: test::m::inner(Input(0));
//> 3: test::m::play(Result(2));

//# programmable --sender A --inputs object(2,0)
// INVALID: InvalidArgumentToPrivateEntryFunction at arg 0 of command 2, a read of a reference whose clique holds a hot potato
//> 0: test::m::heat(Input(0));
//> 1: test::m::inner(Input(0));
//> 2: test::m::play_val(Result(1));
//> 3: test::m::cool(Result(0));

//# programmable --sender A --inputs object(2,0)
// VALID: the read reaches the private entry function once the hot potato is cooled
//> 0: test::m::heat(Input(0));
//> 1: test::m::cool(Result(0));
//> 2: test::m::inner(Input(0));
//> 3: test::m::play_val(Result(2));
