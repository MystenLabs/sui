// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// entry checks maintained during dev inspect

//# init --addresses test=0x0 --accounts A

//# publish

module test::m {

    public fun public_take_and_return_pure(input: u64): u64 {
        input
    }

    public fun public_take_pure(_input: u64) {
        // Do nothing with input
    }

    entry fun entry_take_pure(_input: u64) {}


    public fun public_take_and_return_object(obj: A): A {
        obj
    }

    entry fun entry_take_object(obj: A) {
        let A { id } = obj;
        id.delete();
    }

    public fun public_pure_return_value(): u64 {
        0
    }

    entry fun entry_return_u64(): u64 {
        0
    }

    public struct A has key, store {
        id: UID,
    }

    public fun create_object(ctx: &mut TxContext): A {
        A { id: object::new(ctx) }
    }
}


// Basic pure tainted input
//# programmable --sender A --inputs 0 --dev-inspect
//> 0: test::m::public_take_and_return_pure(Input(0));
//> 1: test::m::entry_take_pure(Result(0));

// Object tainted
//# programmable --sender A --inputs 0 --dev-inspect
//> 0: test::m::create_object();
//> 1: test::m::entry_take_object(Result(0));

// Pass a value created by a public function
//# programmable --sender A --dev-inspect
//> 0: test::m::public_pure_return_value();
//> 1: test::m::entry_take_pure(Result(0));

// Use input on each call instead of result is allowed for pure inputs
//# programmable --sender A --inputs 0 --dev-inspect
//> 0: test::m::public_take_pure(Input(0));
//> 1: test::m::entry_take_pure(Input(0));

// Entry to entry
//# programmable --sender A --dev-inspect
//> 0: test::m::entry_return_u64();
//> 1: test::m::entry_take_pure(Result(0));
