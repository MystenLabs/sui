// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::zklogin_verified_issuer_tests {
    use sui::zklogin_verified_issuer::{check_zklogin_issuer, verify_zklogin_issuer, VerifiedIssuer};
    use std::string::utf8;
    use sui::test_scenario;

    #[test]
    fun test_check_zklogin_issuer() {
        let address = @0x1c6b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b;
        let iss = utf8(b"https://accounts.google.com");
        let address_seed = 3006596378422062745101035755700472756930796952630484939867684134047976874601u256;
        assert!(check_zklogin_issuer(address,  address_seed, &iss,), 0);

        let other_address = @0x006b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b;
        assert!(!check_zklogin_issuer(other_address, address_seed, &iss), 1);

        let other_address_seed = 1234u256;
        assert!(!check_zklogin_issuer(address, other_address_seed, &iss), 2);

        let other_iss = utf8(b"https://other.issuer.com");
        assert!(!check_zklogin_issuer(address, address_seed, &other_iss), 3);
    }

    #[test]
    fun test_verified_issuer() {
        let address = @0x1c6b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b;
        let iss = utf8(b"https://accounts.google.com");
        let address_seed = 3006596378422062745101035755700472756930796952630484939867684134047976874601u256;

        assert!(check_zklogin_issuer(address,  address_seed, &iss), 0);

        let scenario_val = test_scenario::begin(address);
        let scenario = &mut scenario_val;
        {
            verify_zklogin_issuer(address_seed, iss, test_scenario::ctx(scenario));
        };
        test_scenario::next_tx(scenario, address);
        {
            assert!(test_scenario::has_most_recent_for_sender<VerifiedIssuer>(scenario), 1);
        };
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = sui::zklogin_verified_issuer::EInvalidProof)]
    fun test_invalid_verified_issuer() {
        let other_address = @0x1;
        let iss = utf8(b"https://accounts.google.com");
        let address_seed = 3006596378422062745101035755700472756930796952630484939867684134047976874601u256;
        let scenario_val = test_scenario::begin(other_address);
        let scenario = &mut scenario_val;
        {
            verify_zklogin_issuer(address_seed, iss, test_scenario::ctx(scenario));
        };
        test_scenario::end(scenario_val);
    }
}