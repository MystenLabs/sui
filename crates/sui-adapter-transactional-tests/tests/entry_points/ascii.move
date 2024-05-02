// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0 --accounts A

//# publish
module Test::M {
    use std::ascii;

    public entry fun ascii_arg(s: ascii::String, _: &mut TxContext) {
        assert!(ascii::length(&s) == 10, 0);
    }

    public entry fun ascii_vec_arg(mut v: vector<ascii::String>, _: &mut TxContext) {
        let mut l = 0;
        while (!vector::is_empty(&v)) {
            let s = vector::pop_back(&mut v);
            l = l + ascii::length(&s)
        };
        assert!(l == 10, 0);
    }

}

// string of ASCII characters as byte string

//# run Test::M::ascii_arg --sender A --args b"SomeString"


// string of ASCII characters as UTF8 string

//# run Test::M::ascii_arg --sender A --args "SomeString"


// vector of ASCII character strings as byte strings

//# run Test::M::ascii_vec_arg --sender A --args vector[b"Some",b"String"]


// vector of ASCII character strings as UTF8 strings

// run Test::M::ascii_vec_arg --sender A --args vector["Some","String"]


// error - a character out of ASCII range

//# run Test::M::ascii_arg --sender A --args "Some∫tring"
