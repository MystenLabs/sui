// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests that pure arguments have distinct values/locals per type usage

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    use std::string::{Self, String};
    use std::ascii;

    public fun borrow_mut<T>(x: &mut T): &mut T {
        x
    }

    public fun modify_u8(s: &mut u8) {
        assert!(*s == 0);
        *s = 1;
    }
    public fun assert_u8(s: u8) {
        assert!(s == 1);
    }

    public fun modify_ascii(s: &mut ascii::String) {
        assert!(s.is_empty());
        s.append(ascii::string(b"ascii"));
    }
    public fun assert_ascii(s: ascii::String) {
        assert!(s.as_bytes() == b"ascii");
    }

    public fun modify_string(s: &mut String) {
        assert!(s.is_empty());
        s.append(string::utf8(b"utf8"));
    }
    public fun assert_string(s: String) {
        assert!(s.as_bytes() == b"utf8");
    }
}

// In statically checked PTBs, tests that each type usage gets its own value/locals
// Originally, this will fail for changing types
//# programmable --inputs 0u8
//> test::m1::modify_u8(Input(0));
//> test::m1::modify_ascii(Input(0));
//> test::m1::modify_string(Input(0));
//> test::m1::assert_u8(Input(0));
//> test::m1::assert_ascii(Input(0));
//> test::m1::assert_string(Input(0));

// Tests that locals of the same type are distinct even if they are the same value+type
// This should abort
//# programmable --inputs 0u8 0u8
//> test::m1::modify_u8(Input(0));
//> test::m1::assert_string(Input(1));

// In statically checked PTBs, tests that each type can be borrowed mutably separately
//# programmable --inputs 0u8 --dev-inspect
//> 0: test::m1::borrow_mut<u8>(Input(0));
//> 1: test::m1::borrow_mut<std::ascii::String>(Input(0));
//> test::m1::modify_ascii(Result(1));
//> test::m1::modify_u8(Result(0));

//# programmable --inputs 0u8 --dev-inspect
//> 0: test::m1::borrow_mut<u8>(Input(0));
//> 1: test::m1::borrow_mut<std::ascii::String>(Input(0));
//> test::m1::modify_u8(Result(0));
//> test::m1::modify_ascii(Result(1));
