// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --accounts A B --addresses test=0x0

//# publish --sender A
module test::random {
    use sui::random::Random;

    public fun use_random(_random: &Random) {}
    public fun use_value(_value: u64) {}
}

// bad tx - use_random, use_value,
//# programmable --sender A --inputs 16 immshared(8)
//> test::random::use_random(Input(1));
//> test::random::use_value(Input(0))
