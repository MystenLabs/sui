// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A generated `Read` releases its reference immediately, so reading one reference more times than
// `max_ptb_live_references` in a single command stays under the limit.
// This should be updated if the limit ever changes.

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m {
    public fun id_ref(x: &u64): &u64 {
        x
    }

    public fun check_vec(v: vector<u64>, len: u64, value: u64) {
        assert!(v.length() == len, 0);
        v.do!(|x| assert!(x == value, 0));
    }
}

//# programmable --sender A --inputs 7u64 65u64
//> 0: test::m::id_ref(Input(0));
//> 1: MakeMoveVec<u64>([Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0), Result(0)]);
//> 2: test::m::check_vec(Result(1), Input(1), Input(0));
