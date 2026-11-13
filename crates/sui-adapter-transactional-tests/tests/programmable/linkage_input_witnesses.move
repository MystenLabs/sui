// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Statically known input types contribute to transaction-wide linkage even when unused.
// Tests nested phantom types, type-package dependencies, shared inputs, and inputs supplying
// packages absent from all commands. Also covers dev-inspect enforcement, abort attribution
// with shifted function indices after upgrade, and independent selection across transactions.

//# init --addresses DepV1=0x0 DepV2=0x0 Consumer=0x0 Wrapper=0x0 WitnessV1=0x0 WitnessV2=0x0 --accounts A

//# publish --upgradeable --sender A
module DepV1::m {
    public fun check(expected: u64) { assert!(expected == 1, 0) }

    public fun fail() { abort 11 }
}

//# publish --dependencies DepV1 --sender A
module Consumer::m {
    public fun check(expected: u64) { DepV1::m::check(expected) }

    public fun fail() { DepV1::m::fail() }
}

//# publish --sender A
module Wrapper::m {
    public struct Wrapper<phantom T> has key, store { id: UID }

    public fun make<T>(ctx: &mut TxContext) {
        transfer::public_transfer(Wrapper<T> { id: object::new(ctx) }, ctx.sender())
    }
}

//# upgrade --package DepV1 --upgrade-capability 1,1 --sender A
module DepV2::m {
    public struct New has drop {}
    public struct Shared has key, store { id: UID }

    // This v2-only function shifts `fail` from definition index 1 to 2.
    public fun added() {}

    public fun check(expected: u64) { assert!(expected == 2, 0) }

    public fun fail() { abort 22 }

    public fun make_shared(ctx: &mut TxContext) {
        transfer::share_object(Shared { id: object::new(ctx) })
    }
}

//# run DepV2::m::make_shared --sender A

//# run Wrapper::m::make --type-args DepV2::m::New --sender A

//# publish --upgradeable --dependencies DepV1 --sender A
module WitnessV1::witness {
    public fun ping() { DepV1::m::check(1) }
}

//# upgrade --package WitnessV1 --upgrade-capability 7,1 --dependencies DepV2 --sender A
module WitnessV2::witness {
    public struct Witness has key, store { id: UID }

    public fun ping() { DepV2::m::check(2) }

    public fun make(ctx: &mut TxContext) {
        transfer::public_transfer(Witness { id: object::new(ctx) }, ctx.sender())
    }
}

//# run WitnessV2::witness::make --sender A

//# programmable --sender A --inputs 1u64
// Without an input witness, the public consumer remains on v1.
//> Consumer::m::check(Input(0));

//# programmable --sender A --inputs object(6,0) 2u64
// Only the old wrapper's phantom type argument introduces the v2 requirement.
//> Consumer::m::check(Input(1));

//# programmable --sender A --inputs object(6,0) 1u64
// The same unused nested type conflicts with an exact v1 target.
//> DepV1::m::check(Input(1));

//# programmable --sender A --inputs immshared(5,0) 2u64
// Unused shared inputs contribute before typing can discard them.
//> Consumer::m::check(Input(1));

//# programmable --sender A --inputs immshared(5,0) 1u64
// The shared type was introduced in v2 and conflicts with an exact v1 target.
//> DepV1::m::check(Input(1));

//# programmable --sender A --inputs object(9,0) 2u64
// The unused type's package is different from Dep; its dependency table supplies Dep v2.
//> Consumer::m::check(Input(1));

//# programmable --sender A --inputs object(9,0) 1u64
// Transitive input requirements also conflict with an exact v1 target.
//> DepV1::m::check(Input(1));

//# programmable --sender A --inputs object(6,0)
// No user-package command supplies either Wrapper or Dep: both must come from the input.
//> std::option::none<u64>();

//# programmable --sender A --dev-inspect --inputs object(6,0) 2u64
// Dev-inspect uses the same input-driven dependency selection.
//> Consumer::m::check(Input(1));

//# programmable --sender A --dev-inspect --inputs object(6,0) 1u64
// Relaxed execution checks do not bypass linkage conflicts.
//> DepV1::m::check(Input(1));

//# programmable --sender A
// The baseline abort identifies the v1 dependency.
//> Consumer::m::fail();

//# programmable --sender A --inputs object(6,0) 2u64
// Input-selected v2 must report its shifted function index, package, name, and code at command 1.
//> Consumer::m::check(Input(1));
//> Consumer::m::fail();

//# programmable --sender A --inputs 2u64
// A later contribution selects v2 even though command 0 aborts before it executes.
//> Consumer::m::fail();
//> DepV2::m::check(Input(0));

//# programmable --sender A --inputs 1u64
// Warming v2 must not change the next transaction's independent v1 selection.
//> Consumer::m::check(Input(0));
