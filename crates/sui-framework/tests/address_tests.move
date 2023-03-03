// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::address_tests {
    use std::ascii;
    use std::string;
    use std::vector;
    use sui::address;
    use sui::object;
    use sui::tx_context;

    #[test]
    fun from_bytes_ok() {
        assert!(address::from_bytes(x"0000000000000000000000000000000000000000000000000000000000000000") == @0x0, 0);
        assert!(address::from_bytes(x"0000000000000000000000000000000000000000000000000000000000000001") == @0x1, 0);
        assert!(address::from_bytes(x"0000000000000000000000000000000000000000000000000000000000000010") == @0x10, 0);
        assert!(address::from_bytes(x"00000000000000000000000000000000000000000000000000000000000000ff") == @0xff, 0);
        assert!(address::from_bytes(x"0000000000000000000000000000000000000000000000000000000000000100") == @0x100, 0);
        assert!(address::from_bytes(x"fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe") == @0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 0);
        assert!(address::from_bytes(x"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff") == @0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0)
    }

    #[test]
    #[expected_failure(abort_code = sui::address::EAddressParseError)]
    fun from_bytes_too_few_bytes() {
        let ctx = tx_context::dummy();
        let uid = object::new(&mut ctx);

        let bytes = object::uid_to_bytes(&uid);
        vector::pop_back(&mut bytes);

        let _ = address::from_bytes(bytes);

        object::delete(uid);
    }

    #[test]
    #[expected_failure(abort_code = sui::address::EAddressParseError)]
    fun test_from_bytes_too_many_bytes() {
        let ctx = tx_context::dummy();
        let uid = object::new(&mut ctx);

        let bytes = object::uid_to_bytes(&uid);
        vector::push_back(&mut bytes, 0x42);

        let _ = address::from_bytes(bytes);

        object::delete(uid);
    }

    #[test]
    fun to_u256_ok() {
        assert!(address::to_u256(address::from_bytes(x"0000000000000000000000000000000000000000000000000000000000000000")) == 0, 0);
        assert!(address::to_u256(address::from_bytes(x"0000000000000000000000000000000000000000000000000000000000000001")) == 1, 0);
        assert!(address::to_u256(address::from_bytes(x"0000000000000000000000000000000000000000000000000000000000000010")) == 16, 0);
        assert!(address::to_u256(address::from_bytes(x"00000000000000000000000000000000000000000000000000000000000000ff")) == 255, 0);
        assert!(address::to_u256(address::from_bytes(x"0000000000000000000000000000000000000000000000000000000000000100")) == 256, 0);
        assert!(address::to_u256(address::from_bytes(x"fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe")) == address::max() - 1, 0);
        assert!(address::to_u256(address::from_bytes(x"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")) == address::max(), 0);
    }

    #[test]
    fun from_u256_ok() {
        assert!(address::from_u256(0) == @0x0, 0);
        assert!(address::from_u256(1) == @0x1, 0);
        assert!(address::from_u256(16) == @0x10, 0);
        assert!(address::from_u256(255) == @0xff, 0);
        assert!(address::from_u256(256) == @0x100, 0);
        assert!(address::from_u256(address::max() - 1) == @0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 0);
        assert!(address::from_u256(address::max()) == @0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0);
    }

    #[test]
    fun from_u256_tests_max_bytes(): address {
        let u256_max = 115792089237316195423570985008687907853269984665640564039457584007913129639935;
        address::from_u256(u256_max)
    }

    #[test]
    fun to_bytes_ok() {
        assert!(address::to_bytes(@0x0) == x"0000000000000000000000000000000000000000000000000000000000000000", 0);
        assert!(address::to_bytes(@0x1) == x"0000000000000000000000000000000000000000000000000000000000000001", 0);
        assert!(address::to_bytes(@0x10) == x"0000000000000000000000000000000000000000000000000000000000000010", 0);
        assert!(address::to_bytes(@0xff) == x"00000000000000000000000000000000000000000000000000000000000000ff", 0);
        assert!(address::to_bytes(@0x101) == x"0000000000000000000000000000000000000000000000000000000000000101", 0);
        assert!(address::to_bytes(@0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe) == x"fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe", 0);
        assert!(address::to_bytes(@0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff) == x"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff", 0);
    }

    #[test]
    fun to_ascii_string_ok() {
        assert!(address::to_ascii_string(@0x0) == ascii::string(b"0000000000000000000000000000000000000000000000000000000000000000"), 0);
        assert!(address::to_ascii_string(@0x1) == ascii::string(b"0000000000000000000000000000000000000000000000000000000000000001"), 0);
        assert!(address::to_ascii_string(@0x10) == ascii::string(b"0000000000000000000000000000000000000000000000000000000000000010"), 0);
        assert!(address::to_ascii_string(@0xff) == ascii::string(b"00000000000000000000000000000000000000000000000000000000000000ff"), 0);
        assert!(address::to_ascii_string(@0x101) == ascii::string(b"0000000000000000000000000000000000000000000000000000000000000101"), 0);
        assert!(address::to_ascii_string(@0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe) == ascii::string(b"fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe"), 0);
        assert!(address::to_ascii_string(@0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff) == ascii::string(b"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"), 0);
    }

     #[test]
    fun to_string_ok() {
        assert!(address::to_string(@0x0) == string::utf8(b"0000000000000000000000000000000000000000000000000000000000000000"), 0);
        assert!(address::to_string(@0x1) == string::utf8(b"0000000000000000000000000000000000000000000000000000000000000001"), 0);
        assert!(address::to_string(@0x10) == string::utf8(b"0000000000000000000000000000000000000000000000000000000000000010"), 0);
        assert!(address::to_string(@0xff) == string::utf8(b"00000000000000000000000000000000000000000000000000000000000000ff"), 0);
        assert!(address::to_string(@0x101) == string::utf8(b"0000000000000000000000000000000000000000000000000000000000000101"), 0);
        assert!(address::to_string(@0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe) == string::utf8(b"fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe"), 0);
        assert!(address::to_string(@0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff) == string::utf8(b"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"), 0);
    }
}
