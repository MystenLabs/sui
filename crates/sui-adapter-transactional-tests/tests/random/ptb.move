// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --accounts A B --addresses test=0x0

//# publish --sender A
module test::random {
    use sui::clock::Clock;
    use sui::random::Random;
    use sui::transfer;
    use sui::object;
    use sui::tx_context:: TxContext;

    public struct Obj has key, store {
        id: object::UID,
    }

    public entry fun create(ctx: &mut TxContext) {
        transfer::public_share_object(Obj { id: object::new(ctx) })
    }

    public fun use_clock(_clock: &Clock) {}
    public fun use_random(_random: &Random) {}
    public fun use_clock_random(_clock: &Clock, _random: &Random) {}
    public fun use_value(_value: u64) {}
}

//# view-object 8

// bad tx - use_random, use_value,
//# programmable --sender A --inputs 16 immshared(8)
//> test::random::use_random(Input(1));
//> test::random::use_value(Input(0))

// bad tx - use_random, use_clock
//# programmable --sender A --inputs immshared(8) immshared(6) @A
//> test::random::use_random(Input(0));
//> test::random::use_clock(Input(1))

// bad tx - use_random, transfer, use_clock
//# programmable --sender A --inputs 10 immshared(8) immshared(6) @B
//> SplitCoins(Gas, [Input(0)]);
//> test::random::use_random(Input(1));
//> TransferObjects([Result(0)], Input(3));
//> test::random::use_clock(Input(0))


// TODO: Enable the following cases once execution with Random is working.

// good tx - use_random
// //# programmable --sender A --inputs immshared(8)
// //> test::random::use_random(Input(0))

// good tx - use_value, use_random
// //# programmable --sender A --inputs 16 object(8,1)
// //> test::random::use_value(Input(0));
// //> test::random::use_random(Input(1))

// good tx - use_clock, use_random, transfer
// //# programmable --sender A --inputs 10 immshared(6) immshared(8) @B
// //> SplitCoins(Gas, [Input(0)]);
// //> test::random::use_clock(Input(1));
// //> test::random::use_random(Input(2));
// //> TransferObjects([Result(0)], Input(3))

// good tx - use_clock, use_random, merge
// //# programmable --sender A --inputs 10 immshared(6) immshared(8) @A
// //> SplitCoins(Gas, [Input(0)]);
// //> test::random::use_clock(Input(1));
// //> test::random::use_random(Input(2));
// //> TransferObjects([Result(0)], Input(3));
// //> MergeCoins(Result(0), [Gas])