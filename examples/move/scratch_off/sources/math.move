// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module scratch_off::math {

    /// Hard coding this as 1 billion
    const MODULO_FACTOR: u256 = 1_000_000_000;

    public fun bytes_to_u256(hashed_beacon: &vector<u8>): u256 {
        let output: u256 = 0;
        let bytes_length: u64 = 32;
        let idx: u64 = 0;
        while (idx < bytes_length) {
            let current_byte = *std::vector::borrow(hashed_beacon, idx);
            output = (output << 8) | (current_byte as u256) ;
            idx = idx + 1;
        };
        output
    }

    public fun get_result(random_number: u256): u64 {
        ((random_number % MODULO_FACTOR) as u64)
    }
}