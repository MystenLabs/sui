// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::u64 {
    use std::ascii;
    use std::string;
    use std::vector;

    public fun to_string(value: u64): string::String {
        if (value == 0) {
            return string::utf8(b"0")
        };
        string::utf8(u64_to_bytes(value))
    }

    public fun to_ascii_string(value: u64): ascii::String {
        if (value == 0) {
            return ascii::string(b"0")
        };
        ascii::string(u64_to_bytes(value))
    }

    fun u64_to_bytes(value: u64): vector<u8> {
        let bytes = vector::empty<u8>();
        while (value != 0) {
            vector::push_back(&mut bytes, ((48 + value % 10) as u8));
            value = value / 10;
        };
        vector::reverse(&mut bytes);
        bytes
    }
}
