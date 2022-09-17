// SPDX-License-Identifier: Apache-2.0
// Source: https://github.com/starcoinorg/starcoin-framework-commons/blob/main/sources/PseudoRandom.move

/// @title bcd
/// @notice BCD = Binary canoncial DEserialization.
/// Copied from movemate
module lucky_capy::bcd {
    use std::vector;

    public fun bytes_to_u128(bytes: vector<u8>): u128 {
        let value = 0u128;
        let i = 0u64;
        while (i < 16) {
            value = value | ((*vector::borrow(&bytes, i) as u128) << ((8 * (15 - i)) as u8));
            i = i + 1;
        };
        return value
    }

    public fun bytes_to_u64(bytes: vector<u8>): u64 {
        let value = 0u64;
        let i = 0u64;
        while (i < 8) {
            value = value | ((*vector::borrow(&bytes, i) as u64) << ((8 * (7 - i)) as u8));
            i = i + 1;
        };
        return value
    }

    #[test]
    fun test_bytes_to_u64() {
        // binary: 01010001 11010011 10101111 11001100 11111101 00001001 10001110 11001101
        // bytes = [81, 211, 175, 204, 253, 9, 142, 205];
        let dec = 5896249632111562445;

        let bytes = vector::empty<u8>();
        vector::push_back(&mut bytes, 81);
        vector::push_back(&mut bytes, 211);
        vector::push_back(&mut bytes, 175);
        vector::push_back(&mut bytes, 204);
        vector::push_back(&mut bytes, 253);
        vector::push_back(&mut bytes, 9);
        vector::push_back(&mut bytes, 142);
        vector::push_back(&mut bytes, 205);

        let value = bytes_to_u64(bytes);
        assert!(value == dec, 101);
    }
}
