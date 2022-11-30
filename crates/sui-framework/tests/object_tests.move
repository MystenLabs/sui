// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::object_tests {
    use std::vector;
    use sui::object;
    use sui::tx_context;

    const EDifferentAddress: u64 = 0xF000;
    const EDifferentBytes: u64 = 0xF001;
    const EAddressRoundTrip: u64 = 0xF002;

    #[test]
    fun test_bytes_address_roundtrip() {
        let ctx = tx_context::dummy();

        let uid0 = object::new(&mut ctx);
        let uid1 = object::new(&mut ctx);

        let addr0 = object::uid_to_address(&uid0);
        let byte0 = object::uid_to_bytes(&uid0);
        let addr1 = object::uid_to_address(&uid1);
        let byte1 = object::uid_to_bytes(&uid1);

        assert!(addr0 != addr1, EDifferentAddress);
        assert!(byte0 != byte1, EDifferentBytes);

        assert!(addr0 == object::address_from_bytes(byte0), EAddressRoundTrip);
        assert!(addr1 == object::address_from_bytes(byte1), EAddressRoundTrip);

        object::delete(uid0);
        object::delete(uid1);
    }

    #[test]
    #[expected_failure(abort_code = sui::object::EAddressParseError)]
    fun test_address_from_too_few_bytes() {
        let ctx = tx_context::dummy();
        let uid = object::new(&mut ctx);

        let bytes = object::uid_to_bytes(&uid);
        vector::pop_back(&mut bytes);

        let _ = object::address_from_bytes(bytes);

        object::delete(uid);
    }

    #[test]
    #[expected_failure(abort_code = sui::object::EAddressParseError)]
    fun test_address_from_too_many_bytes() {
        let ctx = tx_context::dummy();
        let uid = object::new(&mut ctx);

        let bytes = object::uid_to_bytes(&uid);
        vector::push_back(&mut bytes, 0x42);

        let _ = object::address_from_bytes(bytes);

        object::delete(uid);
    }
}
