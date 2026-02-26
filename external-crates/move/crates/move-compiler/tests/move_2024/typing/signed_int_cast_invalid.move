// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Casting signed to bool should error
    fun i8_to_bool() {
        let x: i8 = 1i8;
        let _y = (x as bool);
    }

    // Casting bool to signed should error
    fun bool_to_i8() {
        let x = true;
        let _y = (x as i8);
    }

    // Casting signed to address should error
    fun i64_to_address() {
        let x: i64 = 1i64;
        let _y = (x as address);
    }

    // Casting address to signed should error
    fun address_to_i64() {
        let x = @0x42;
        let _y = (x as i64);
    }

    // Casting signed to vector should error
    fun i8_to_vector() {
        let x: i8 = 1i8;
        let _y = (x as vector<u8>);
    }
}
