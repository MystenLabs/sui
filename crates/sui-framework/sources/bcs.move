// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::bcs {
    use std::vector as v;

    /// For when bytes length is less than required for deserialization.
    const EOutOfRange: u64 = 0;

    /// Read address from the bcs-serialized bytes.
    public fun peel_address(bcs: &mut vector<u8>): address {
        assert!(v::length(bcs) >= 20, EOutOfRange);
        let (_value, i) = (0, 0);
        while (i < 20) {
            let _ = v::remove(bcs, 0);
            i = i + 1;
        };
        @0x0
    }

    /// Read a `bool` value from bcs-serialized bytes.
    public fun peel_bool(bcs: &mut vector<u8>): bool {
        let value = peel_u8(bcs);
        (value != 0)
    }

    /// Read `u8` value from bcs-serialized bytes.
    public fun peel_u8(bcs: &mut vector<u8>): u8 {
        assert!(v::length(bcs) >= 1, EOutOfRange);
        v::remove(bcs, 0)
    }

    /// Read `u64` value from bcs-serialized bytes.
    public fun peel_u64(bcs: &mut vector<u8>): u64 {
        assert!(v::length(bcs) >= 8, EOutOfRange);
        let (l_value, r_value, i) = (0u64, 0u64, 0);

        // Read first 4 LE bytes (u32)
        while (i < 4) {
            let l_byte = (v::remove(bcs, 0) as u64);
            let r_byte = (v::remove(bcs, 3 - i) as u64);

            l_value = l_value + (l_byte << ((8 * (i)) as u8));
            r_value = r_value + (r_byte << ((8 * (i)) as u8));

            i = i + 1;
        };

        // Swap LHS and RHS of initial bytes
        (r_value << 32) | l_value
    }

    /// Read `u128` value from bcs-serialized bytes.
    public fun peel_u128(bcs: &mut vector<u8>): u128 {
        assert!(v::length(bcs) >= 16, EOutOfRange);

        let (l_value, r_value) = (peel_u64(bcs), peel_u64(bcs));

        ((r_value as u128) << 64) | (l_value as u128)
    }

    /// Read ULEB bytes expecting a vector length. Result should
    /// then be used to perform `peel_*` operation LEN times.
    public fun peel_vec_length(bcs: &mut vector<u8>): u64 {
        v::reverse(bcs);
        let (total, shift) = (0u64, 0);
        while (true) {
            let byte = (v::pop_back(bcs) as u64);
            total = total | ((byte & 0x7f) << shift);
            if ((byte & 0x80) == 0) {
                break
            };
            shift = shift + 7;
        };
        v::reverse(bcs);
        total
    }

    #[test_only]
    struct Info has drop { a: bool, b: u8, c: u64, d: u128, k: vector<bool> }

    #[test]
    fun test_bcs() {
        use std::bcs;

        { // boolean: true
            let value = true;
            let bytes = bcs::to_bytes(&value);
            assert!(value == peel_bool(&mut bytes), 0);
        };

        { // boolean: false
            let value = false;
            let bytes = bcs::to_bytes(&value);
            assert!(value == peel_bool(&mut bytes), 0);
        };

        { // u8
            let value = 100u8;
            let bytes = bcs::to_bytes(&value);
            assert!(value == peel_u8(&mut bytes), 0);
        };

        { // u64 (4 bytes)
            let value = 1000100u64;
            let bytes = bcs::to_bytes(&value);
            assert!(value == peel_u64(&mut bytes), 0);
        };

        { // u64 (8 bytes)
            let value = 100000000000000u64;
            let bytes = bcs::to_bytes(&value);
            assert!(value == peel_u64(&mut bytes), 0);
        };

        { // u128 (16 bytes)
            let value = 100000000000000000000000000u128;
            let bytes = bcs::to_bytes(&value);
            assert!(value == peel_u128(&mut bytes), 0);
        };

        { // vector length
            let value = vector[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0];
            let bytes = bcs::to_bytes(&value);
            assert!(v::length(&value) == peel_vec_length(&mut bytes), 0);
        };

        { // full deserialization test (ordering)
            let info = Info { a: true, b: 100, c: 9999, d: 112333, k: vector[true, false, true, false] };
            let bytes = &mut bcs::to_bytes(&info);

            assert!(info.a == peel_bool(bytes), 0);
            assert!(info.b == peel_u8(bytes), 0);
            assert!(info.c == peel_u64(bytes), 0);
            assert!(info.d == peel_u128(bytes), 0);

            let len = peel_vec_length(bytes);

            assert!(v::length(&info.k) == len, 0);

            let i = 0;
            while (i < v::length(&info.k)) {
                assert!(*v::borrow(&info.k, i) == peel_bool(bytes), 0);
                i = i + 1;
            };
        };
    }
}
