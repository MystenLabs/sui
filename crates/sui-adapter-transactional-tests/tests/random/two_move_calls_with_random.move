// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --accounts A B --addresses test=0x0

//# publish --sender A
module test::random {
    use sui::random::Random;

    public fun use_random(_random: &Random) {}
}

// bad tx - use_random twice
//# programmable --sender A --inputs immshared(8) @A
//> test::random::use_random(Input(0));
//> test::random::use_random(Input(0));
