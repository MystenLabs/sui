// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module scratch_off::math {

    /// Largest u64 integer
    const MAX_U64: u128 = 0xFFFFFFFFFFFFFFFF; // 18446744073709551615
    const BYTES_LENGTH: u64 = 32;

    public fun bytes_to_u128(random_32b: &vector<u8>): u128 {
        let output = 0;
        assert!(std::vector::length(random_32b) == BYTES_LENGTH, 0);
        let idx = 0;
        while (idx < BYTES_LENGTH) {
            let current_byte = *std::vector::borrow(random_32b, idx);
            output = (output << 8) | (current_byte as u128) ;
            idx = idx + 1;
        };
        output
    }

    public fun get_random_u64(random_32b: &vector<u8>): u64 {
        let output = bytes_to_u128(random_32b);
        ((output % MAX_U64) as u64)
    }

    public fun get_random_u64_in_range(random_32b: &vector<u8>, max_range: u64): u64 {
        let output = bytes_to_u128(random_32b);
        ((output % (max_range as u128)) as u64)
    }
}