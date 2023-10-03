// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::zklogin_verified_issuer_tests {
    use sui::zklogin_verified_issuer::check_zklogin_issuer;
    use sui::address;
    use std::string::utf8;

    #[test]
    fun test_check_zklogin_issuer() {
        let address = address::from_bytes(x"1c6b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b");
        let iss = utf8(b"https://accounts.google.com");
        let address_seed = 3006596378422062745101035755700472756930796952630484939867684134047976874601u256;
        assert!(check_zklogin_issuer(address,  address_seed, &iss,), 0);

        let other_address = address::from_bytes(x"006b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b");
        assert!(!check_zklogin_issuer(other_address, address_seed, &iss), 1);

        let other_address_seed = 1234u256;
        assert!(!check_zklogin_issuer(address, other_address_seed, &iss), 2);

        let other_iss = utf8(b"https://other.issuer.com");
        assert!(!check_zklogin_issuer(address, address_seed, &other_iss), 3);
    }
}