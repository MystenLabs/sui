// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module entry_point_string::entry_point_string {
    use std::ascii;
    use std::string;
    use sui::tx_context::TxContext;
    use std::vector;


    public entry fun ascii_arg(s: ascii::String, _: &mut TxContext) {
        assert!(ascii::length(&s) == 10, 0);
    }

    public entry fun utf8_arg(s: string::String, _: &mut TxContext) {
        assert!(string::length(&s) == 24, 0);
    }

    public entry fun utf8_vec_arg(v: vector<string::String>, _: &mut TxContext) {
        let concat = string::utf8(vector::empty());
        while (!vector::is_empty(&v)) {
            let s = vector::pop_back(&mut v);
            string::append(&mut concat, s);
        };
        assert!(string::length(&concat) == 24, 0);
    }
}
