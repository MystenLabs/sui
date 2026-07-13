// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module entry_point_types::entry_point_types;

use std::ascii;
use std::option::Option;
use std::string;
use std::vector;
use sui::tx_context::TxContext;

public fun ascii_arg(s: ascii::String, len: u64, _: &mut TxContext) {
    assert!(s.length() == len, 0);
}

public fun utf8_arg(s: string::String, len: u64, _: &mut TxContext) {
    assert!(s.length() == len, 0);
}

public fun utf8_vec_arg(mut v: vector<string::String>, total_len: u64, _: &mut TxContext) {
    let mut concat = string::utf8(vector[]);
    while (!v.is_empty()) {
        let s = v.pop_back();
        concat.append(s);
    };
    assert!(concat.length() == total_len, 0);
}

public fun option_ascii_arg(_: Option<ascii::String>) {}

public fun option_utf8_arg(_: Option<string::String>) {}

public fun vec_option_utf8_arg(_: vector<Option<string::String>>) {}

public fun option_vec_option_utf8_arg(_: Option<vector<Option<string::String>>>) {}

public fun drop_all<T: drop>(mut v: vector<T>, expected_len: u64) {
    let mut actual = 0;
    while ((!v.is_empty())) {
        v.pop_back();
        actual = actual + 1;
    };
    v.destroy_empty();
    assert!(actual == expected_len, 0);
}

public fun id<T>(x: T): T { x }
