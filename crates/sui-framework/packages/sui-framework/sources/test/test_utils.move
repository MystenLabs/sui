// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::test_utils;

#[deprecated(note = b"Use `std::unit_test::assert_eq!` for better error messages")]
public fun assert_eq<T: drop>(t1: T, t2: T) {
    assert_ref_eq(&t1, &t2)
}

#[deprecated(note = b"Use `std::unit_test::assert_ref_eq!` for better error messages")]
public fun assert_ref_eq<T>(t1: &T, t2: &T) {
    let res = t1 == t2;
    if (!res) {
        print(b"Assertion failed:");
        std::debug::print(t1);
        print(b"!=");
        std::debug::print(t2);
        abort (0)
    }
}

#[deprecated(note = b"Use `std::debug::print` instead")]
public fun print(str: vector<u8>) {
    std::debug::print(&str.to_ascii_string())
}

#[deprecated(note = b"Use `std::unit_test::destroy` instead")]
public fun destroy<T>(x: T) { std::unit_test::destroy(x) }

public native fun create_one_time_witness<T: drop>(): T;
