// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::address_tests {
    use sui::address;

    #[test]
    fun from_bytes_ok() {
        assert!(address::from_bytes(x"0000000000000000000000000000000000000000000000000000000000000000") == @0x0);
        assert!(address::from_bytes(x"0000000000000000000000000000000000000000000000000000000000000001") == @0x1);
        assert!(address::from_bytes(x"0000000000000000000000000000000000000000000000000000000000000010") == @0x10);
        assert!(address::from_bytes(x"00000000000000000000000000000000000000000000000000000000000000ff") == @0xff);
        assert!(address::from_bytes(x"0000000000000000000000000000000000000000000000000000000000000100") == @0x100);
        assert!(address::from_bytes(x"fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe") == @0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe);
        assert!(address::from_bytes(x"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff") == @0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
    }

    #[test]
    #[expected_failure(abort_code = sui::address::EAddressParseError)]
    fun from_bytes_too_few_bytes() {
        let mut ctx = tx_context::dummy();
        let uid = object::new(&mut ctx);

        let mut bytes = uid.to_bytes();
        bytes.pop_back();

        let _ = address::from_bytes(bytes);

        uid.delete();
    }

    #[test]
    #[expected_failure(abort_code = sui::address::EAddressParseError)]
    fun test_from_bytes_too_many_bytes() {
        let mut ctx = tx_context::dummy();
        let uid = object::new(&mut ctx);

        let mut bytes = uid.to_bytes();
        bytes.push_back(0x42);

        let _ = address::from_bytes(bytes);

        uid.delete();
    }

    #[test]
    fun to_u256_ok() {
        assert!(address::from_bytes(x"0000000000000000000000000000000000000000000000000000000000000000").to_u256() == 0);
        assert!(address::from_bytes(x"0000000000000000000000000000000000000000000000000000000000000001").to_u256() == 1);
        assert!(address::from_bytes(x"0000000000000000000000000000000000000000000000000000000000000010").to_u256() == 16);
        assert!(address::from_bytes(x"00000000000000000000000000000000000000000000000000000000000000ff").to_u256() == 255);
        assert!(address::from_bytes(x"0000000000000000000000000000000000000000000000000000000000000100").to_u256() == 256);
        assert!(address::from_bytes(x"fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe").to_u256() == address::max() - 1);
        assert!(address::from_bytes(x"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").to_u256() == address::max());
    }

    #[test]
    fun from_u256_ok() {
        assert!(address::from_u256(0) == @0x0);
        assert!(address::from_u256(1) == @0x1);
        assert!(address::from_u256(16) == @0x10);
        assert!(address::from_u256(255) == @0xff);
        assert!(address::from_u256(256) == @0x100);
        assert!(address::from_u256(address::max() - 1) == @0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe);
        assert!(address::from_u256(address::max()) == @0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff);
    }

    #[test]
    fun from_u256_tests_max_bytes(): address {
        let u256_max = 115792089237316195423570985008687907853269984665640564039457584007913129639935;
        address::from_u256(u256_max)
    }

    #[test]
    fun to_bytes_ok() {
        assert!(@0x0.to_bytes() == x"0000000000000000000000000000000000000000000000000000000000000000");
        assert!(@0x1.to_bytes() == x"0000000000000000000000000000000000000000000000000000000000000001");
        assert!(@0x10.to_bytes() == x"0000000000000000000000000000000000000000000000000000000000000010");
        assert!(@0xff.to_bytes() == x"00000000000000000000000000000000000000000000000000000000000000ff");
        assert!(@0x101.to_bytes() == x"0000000000000000000000000000000000000000000000000000000000000101");
        assert!(@0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe.to_bytes() == x"fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe");
        assert!(@0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff.to_bytes() == x"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");
    }

    #[test]
    fun to_ascii_string_ok() {
        assert!(@0x0.to_ascii_string() == b"0000000000000000000000000000000000000000000000000000000000000000".to_ascii_string());
        assert!(@0x1.to_ascii_string() == b"0000000000000000000000000000000000000000000000000000000000000001".to_ascii_string());
        assert!(@0x10.to_ascii_string() == b"0000000000000000000000000000000000000000000000000000000000000010".to_ascii_string());
        assert!(@0xff.to_ascii_string() == b"00000000000000000000000000000000000000000000000000000000000000ff".to_ascii_string());
        assert!(@0x101.to_ascii_string() == b"0000000000000000000000000000000000000000000000000000000000000101".to_ascii_string());
        assert!(@0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe.to_ascii_string() == b"fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe".to_ascii_string());
        assert!(@0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff.to_ascii_string() == b"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_ascii_string());
    }

     #[test]
    fun to_string_ok() {
        assert!(@0x0.to_string() == b"0000000000000000000000000000000000000000000000000000000000000000".to_string());
        assert!(@0x1.to_string() == b"0000000000000000000000000000000000000000000000000000000000000001".to_string());
        assert!(@0x10.to_string() == b"0000000000000000000000000000000000000000000000000000000000000010".to_string());
        assert!(@0xff.to_string() == b"00000000000000000000000000000000000000000000000000000000000000ff".to_string());
        assert!(@0x101.to_string() == b"0000000000000000000000000000000000000000000000000000000000000101".to_string());
        assert!(@0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe.to_string() == b"fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe".to_string());
        assert!(@0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff.to_string() == b"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_string());
    }

    #[test]
    fun from_ascii_string_ok() {
        assert!(address::from_ascii_bytes(&b"0000000000000000000000000000000000000000000000000000000000000000") == @0x0);
        assert!(address::from_ascii_bytes(&b"0000000000000000000000000000000000000000000000000000000000000001") == @0x1);
        assert!(address::from_ascii_bytes(&b"0000000000000000000000000000000000000000000000000000000000000010") == @0x10);
        assert!(address::from_ascii_bytes(&b"00000000000000000000000000000000000000000000000000000000000000ff") == @0xff);
        assert!(address::from_ascii_bytes(&b"0000000000000000000000000000000000000000000000000000000000000101") == @0x101);
        assert!(address::from_ascii_bytes(&b"fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe") == @0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe);
        assert!(address::from_ascii_bytes(&b"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff") == @0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff);
    }

    #[test]
    #[expected_failure(abort_code = sui::address::EAddressParseError)]
    fun from_ascii_string_too_short() {
        address::from_ascii_bytes(&b"0");
    }

    #[test]
    #[expected_failure(abort_code = sui::address::EAddressParseError)]
    fun from_ascii_string_too_long() {
        address::from_ascii_bytes(&b"00000000000000000000000000000000000000000000000000000000000000001");
    }

    #[test]
    #[expected_failure(abort_code = sui::address::EAddressParseError)]
    fun from_ascii_string_non_hex_character() {
        address::from_ascii_bytes(&b"fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffg");
    }

}
