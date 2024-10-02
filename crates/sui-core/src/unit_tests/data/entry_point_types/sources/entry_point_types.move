// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module entry_point_types::entry_point_types {
    use std::ascii;
    use std::string;
    use sui::tx_context::TxContext;
    use std::vector;
    use std::option::Option;


    public entry fun ascii_arg(s: ascii::String, len: u64, _: &mut TxContext) {
        assert!(ascii::length(&s) == len, 0);
    }

    public entry fun utf8_arg(s: string::String, len: u64, _: &mut TxContext) {
        assert!(string::length(&s) == len, 0);
    }

    public entry fun utf8_vec_arg(
        mut v: vector<string::String>,
        total_len: u64,
        _: &mut TxContext
    ) {
        let mut concat = string::utf8(vector::empty());
        while (!vector::is_empty(&v)) {
            let s = vector::pop_back(&mut v);
            string::append(&mut concat, s);
        };
        assert!(string::length(&concat) == total_len, 0);
    }

    public entry fun option_ascii_arg(_: Option<ascii::String>) {
    }

    public entry fun option_utf8_arg(_: Option<string::String>) {
    }

    public entry fun vec_option_utf8_arg(_: vector<Option<string::String>>) {
    }

    public entry fun option_vec_option_utf8_arg(
        _: Option<vector<Option<string::String>>>
    ) {
    }

    public fun drop_all<T: drop>(mut v: vector<T>, expected_len: u64) {
        let mut actual = 0;
        while ((!vector::is_empty(&v))) {
            vector::pop_back(&mut v);
            actual = actual + 1;
        };
        vector::destroy_empty(v);
        assert!(actual == expected_len, 0);
    }

    public fun id<T>(x: T): T { x }
}
