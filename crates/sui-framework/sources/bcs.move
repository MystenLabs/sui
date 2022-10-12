// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module implements BCS (de)serialization in Move.
/// Full specification can be found here: https://github.com/diem/bcs
///
/// Short summary (for Move-supported types):
///
/// - address - sequence of X bytes
/// - bool - byte with 0 or 1
/// - u8 - a single u8 byte
/// - u64 / u128 - LE bytes
/// - vector - ULEB128 length + LEN elements
/// - option - first byte bool: None (0) or Some (1), then value
module sui::bcs {
    use std::option::{Self, Option};
    use std::vector as v;
    use sui::object;
    use std::bcs;

    /// For when bytes length is less than required for deserialization.
    const EOutOfRange: u64 = 0;

    /// For when the boolean value different than `0` or `1`.
    const ENotBool: u64 = 1;

    /// For when ULEB byte is out of range (or not found).
    const ELenOutOfRange: u64 = 2;

    /// Address length in Sui is 20 bytes.
    const SUI_ADDRESS_LENGTH: u64 = 20;


    /// Get BCS serialized bytes for any value.
    /// Re-exports stdlib `bcs::to_bytes`.
    public fun to_bytes<T>(value: &T): vector<u8> {
        bcs::to_bytes(value)
    }

    /// Read address from the bcs-serialized bytes.
    public fun peel_address(bcs: &mut vector<u8>): address {
        assert!(v::length(bcs) >= SUI_ADDRESS_LENGTH, EOutOfRange);
        v::reverse(bcs);
        let (addr_bytes, i) = (v::empty(), 0);
        while (i < 20) {
            v::push_back(&mut addr_bytes, v::pop_back(bcs));
            i = i + 1;
        };
        v::reverse(bcs);
        object::address_from_bytes(addr_bytes)
    }

    /// Read a `bool` value from bcs-serialized bytes.
    public fun peel_bool(bcs: &mut vector<u8>): bool {
        let value = peel_u8(bcs);
        if (value == 0) {
            false
        } else if (value == 1) {
            true
        } else {
            abort ENotBool
        }
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

    // === Vector<T> ===

    /// Read ULEB bytes expecting a vector length. Result should
    /// then be used to perform `peel_*` operation LEN times.
    ///
    /// In BCS `vector` length is implemented with ULEB128;
    /// See more here: https://en.wikipedia.org/wiki/LEB128
    public fun peel_vec_length(bcs: &mut vector<u8>): u64 {
        v::reverse(bcs);
        let (total, shift, len) = (0u64, 0, 0);
        while (true) {
            assert!(len <= 4, ELenOutOfRange);
            let byte = (v::pop_back(bcs) as u64);
            len = len + 1;
            total = total | ((byte & 0x7f) << shift);
            if ((byte & 0x80) == 0) {
                break
            };
            shift = shift + 7;
        };
        v::reverse(bcs);
        total
    }

    /// Peel a vector of `address` from serialized bytes.
    public fun peel_vec_address(bcs: &mut vector<u8>): vector<address> {
        let (len, i, res) = (peel_vec_length(bcs), 0, vector[]);
        while (i < len) {
            v::push_back(&mut res, peel_address(bcs));
            i = i + 1;
        };
        res
    }

    /// Peel a vector of `address` from serialized bytes.
    public fun peel_vec_bool(bcs: &mut vector<u8>): vector<bool> {
        let (len, i, res) = (peel_vec_length(bcs), 0, vector[]);
        while (i < len) {
            v::push_back(&mut res, peel_bool(bcs));
            i = i + 1;
        };
        res
    }

    /// Peel a vector of `u8` (eg string) from serialized bytes.
    public fun peel_vec_u8(bcs: &mut vector<u8>): vector<u8> {
        let (len, i, res) = (peel_vec_length(bcs), 0, vector[]);
        while (i < len) {
            v::push_back(&mut res, peel_u8(bcs));
            i = i + 1;
        };
        res
    }

    /// Peel a vector of `u64` from serialized bytes.
    public fun peel_vec_u64(bcs: &mut vector<u8>): vector<u64> {
        let (len, i, res) = (peel_vec_length(bcs), 0, vector[]);
        while (i < len) {
            v::push_back(&mut res, peel_u64(bcs));
            i = i + 1;
        };
        res
    }

    /// Peel a vector of `u128` from serialized bytes.
    public fun peel_vec_u128(bcs: &mut vector<u8>): vector<u128> {
        let (len, i, res) = (peel_vec_length(bcs), 0, vector[]);
        while (i < len) {
            v::push_back(&mut res, peel_u128(bcs));
            i = i + 1;
        };
        res
    }

    // === Option<T> ===

    /// Peel `Option<address>` from serialized bytes.
    public fun peel_option_address(bcs: &mut vector<u8>): Option<address> {
        if (peel_bool(bcs)) {
            option::some(peel_address(bcs))
        } else {
            option::none()
        }
    }

    /// Peel `Option<bool>` from serialized bytes.
    public fun peel_option_bool(bcs: &mut vector<u8>): Option<bool> {
        if (peel_bool(bcs)) {
            option::some(peel_bool(bcs))
        } else {
            option::none()
        }
    }

    /// Peel `Option<u8>` from serialized bytes.
    public fun peel_option_u8(bcs: &mut vector<u8>): Option<u8> {
        if (peel_bool(bcs)) {
            option::some(peel_u8(bcs))
        } else {
            option::none()
        }
    }

    /// Peel `Option<u64>` from serialized bytes.
    public fun peel_option_u64(bcs: &mut vector<u8>): Option<u64> {
        if (peel_bool(bcs)) {
            option::some(peel_u64(bcs))
        } else {
            option::none()
        }
    }

    /// Peel `Option<u128>` from serialized bytes.
    public fun peel_option_u128(bcs: &mut vector<u8>): Option<u128> {
        if (peel_bool(bcs)) {
            option::some(peel_u128(bcs))
        } else {
            option::none()
        }
    }

    // === Tests ===

    #[test_only]
    struct Info has drop { a: bool, b: u8, c: u64, d: u128, k: vector<bool>, s: address }

    #[test]
    #[expected_failure(abort_code = 2)]
    fun test_uleb_len_fail() {
        let value = vector[0xff, 0xff, 0xff, 0xff, 0xff];
        let bytes = &mut to_bytes(&value);
        let _fail = peel_vec_length(bytes);
        abort 2 // TODO: make this test fail
    }

    #[test]
    #[expected_failure(abort_code = 1)]
    fun test_bool_fail() {
        let bytes = to_bytes(&10u8);
        let _fail = peel_bool(&mut bytes);
    }

    #[test]
    fun test_option() {
        {
            let value = option::some(true);
            let bytes = &mut to_bytes(&value);
            assert!(value == peel_option_bool(bytes), 0);
        };

        {
            let value = option::some(10u8);
            let bytes = &mut to_bytes(&value);
            assert!(value == peel_option_u8(bytes), 0);
        };

        {
            let value = option::some(10000u64);
            let bytes = &mut to_bytes(&value);
            assert!(value == peel_option_u64(bytes), 0);
        };

        {
            let value = option::some(10000999999u128);
            let bytes = &mut to_bytes(&value);
            assert!(value == peel_option_u128(bytes), 0);
        };

        {
            let value = option::some(@0xC0FFEE);
            let bytes = &mut to_bytes(&value);
            assert!(value == peel_option_address(bytes), 0);
        };

        {
            let value: Option<bool> = option::none();
            let bytes = &mut to_bytes(&value);
            assert!(value == peel_option_bool(bytes), 0);
        };
    }

    #[test]
    fun test_bcs() {
        {
            let value = @0xC0FFEE;
            let bytes = to_bytes(&value);
            assert!(value == peel_address(&mut bytes), 0);
        };

        { // boolean: true
            let value = true;
            let bytes = to_bytes(&value);
            assert!(value == peel_bool(&mut bytes), 0);
        };

        { // boolean: false
            let value = false;
            let bytes = to_bytes(&value);
            assert!(value == peel_bool(&mut bytes), 0);
        };

        { // u8
            let value = 100u8;
            let bytes = to_bytes(&value);
            assert!(value == peel_u8(&mut bytes), 0);
        };

        { // u64 (4 bytes)
            let value = 1000100u64;
            let bytes = to_bytes(&value);
            assert!(value == peel_u64(&mut bytes), 0);
        };

        { // u64 (8 bytes)
            let value = 100000000000000u64;
            let bytes = to_bytes(&value);
            assert!(value == peel_u64(&mut bytes), 0);
        };

        { // u128 (16 bytes)
            let value = 100000000000000000000000000u128;
            let bytes = to_bytes(&value);
            assert!(value == peel_u128(&mut bytes), 0);
        };

        { // vector length
            let value = vector[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0];
            let bytes = to_bytes(&value);
            assert!(v::length(&value) == peel_vec_length(&mut bytes), 0);
        };

        { // vector length (more data)
            let value = vector[
                0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
                0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
                0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
                0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
                0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
                0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0
            ];

            let bytes = to_bytes(&value);
            assert!(v::length(&value) == peel_vec_length(&mut bytes), 0);
        };

        { // full deserialization test (ordering)
            let info = Info { a: true, b: 100, c: 9999, d: 112333, k: vector[true, false, true, false], s: @0xAAAAAAAAAAA };
            let bytes = &mut to_bytes(&info);

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

            assert!(info.s == peel_address(bytes), 0);
        };

        { // read vector of bytes directly
            let value = vector[1,2,3,4,5];
            let bytes = &mut to_bytes(&value);
            assert!(value == peel_vec_u8(bytes), 0);
        };

        { // read vector of bytes directly
            let value = vector[1,2,3,4,5];
            let bytes = &mut to_bytes(&value);
            assert!(value == peel_vec_u64(bytes), 0);
        };

        { // read vector of bytes directly
            let value = vector[1,2,3,4,5];
            let bytes = &mut to_bytes(&value);
            assert!(value == peel_vec_u128(bytes), 0);
        };

        { // read vector of bytes directly
            let value = vector[true, false, true, false];
            let bytes = &mut to_bytes(&value);
            assert!(value == peel_vec_bool(bytes), 0);
        };

        { // read vector of address directly
            let value = vector[@0x0, @0x1, @0x2, @0x3];
            let bytes = &mut to_bytes(&value);
            assert!(value == peel_vec_address(bytes), 0);
        };
    }
}
