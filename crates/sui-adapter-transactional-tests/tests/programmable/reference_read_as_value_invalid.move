// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Flag-on version of reference_read_as_value_invalid_v139: the Move call may return a
// reference, so the `Read` argument is reached.

//# init --addresses test=0x0 --accounts A

//# publish
module test::m {
    public fun id_ref(x: &u64): &u64 {
        x
    }

    public fun check(value: u64, expected: u64) {
        assert!(value == expected, 0);
    }
}

//# programmable --sender A --inputs 112u64
//> 0: test::m::id_ref(Input(0));
//> 1: test::m::check(Result(0), Input(0));
