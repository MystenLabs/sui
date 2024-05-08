// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0 --accounts A

//# publish
module Test::M {
    use std::string;

    public entry fun utf8_arg(s: string::String, _: &mut TxContext) {
        assert!(string::length(&s) == 24, 0);
    }

    public entry fun utf8_vec_arg(mut v: vector<string::String>, _: &mut TxContext) {
        let mut concat = string::utf8(vector::empty());
        while (!vector::is_empty(&v)) {
            let s = vector::pop_back(&mut v);
            string::append(&mut concat, s);
        };
        assert!(string::length(&concat) == 24, 0);
    }
}

// string of ASCII characters as byte string

//# run Test::M::utf8_arg --sender A --args b"SomeStringPlusSomeString"


// string of ASCII characters as UTF8 string

//# run Test::M::utf8_arg --sender A --args "SomeStringPlusSomeString"


// string of UTF8 characters as UTF8 string

//# run Test::M::utf8_arg --sender A --args "çå∞≠¢õß∂ƒ∫"


// vector of ASCII character strings as byte strings

//# run Test::M::utf8_vec_arg --sender A --args vector[b"SomeStringPlus",b"SomeString"]


// vector of ASCII character strings as UTF8 strings

//# run Test::M::utf8_vec_arg --sender A --args vector["SomeStringPlus","SomeString"]


// vector of UTF8 character strings as UTF8 strings

//# run Test::M::utf8_vec_arg --sender A --args vector["çå∞≠¢","õß∂ƒ∫"]
