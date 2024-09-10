// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::zklogin_verified_issuer_tests {
    use sui::zklogin_verified_issuer::{check_zklogin_issuer, delete, verify_zklogin_issuer, VerifiedIssuer};
    use sui::test_scenario;

    #[test]
    fun test_check_zklogin_issuer() {
        let address = @0x1c6b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b;
        let iss = b"https://accounts.google.com".to_string();
        let address_seed = 3006596378422062745101035755700472756930796952630484939867684134047976874601u256;
        assert!(check_zklogin_issuer(address,  address_seed, &iss,));

        let other_address = @0x006b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b;
        assert!(!check_zklogin_issuer(other_address, address_seed, &iss));

        let other_address_seed = 1234u256;
        assert!(!check_zklogin_issuer(address, other_address_seed, &iss));

        let other_iss = b"https://other.issuer.com".to_string();
        assert!(!check_zklogin_issuer(address, address_seed, &other_iss));
    }

    #[test]
    fun test_verified_issuer() {
        let address = @0x1c6b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b;
        let iss = b"https://accounts.google.com".to_string();
        let address_seed = 3006596378422062745101035755700472756930796952630484939867684134047976874601u256;

        assert!(check_zklogin_issuer(address,  address_seed, &iss));

        let mut scenario = test_scenario::begin(address);
        {
            verify_zklogin_issuer(address_seed, iss, scenario.ctx());
        };
        scenario.next_tx(address);
        {
            assert!(scenario.has_most_recent_for_sender<VerifiedIssuer>());
            delete(scenario.take_from_sender<VerifiedIssuer>());
            assert!(!scenario.has_most_recent_for_sender<VerifiedIssuer>());
        };
        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = sui::zklogin_verified_issuer::EInvalidProof)]
    fun test_invalid_verified_issuer() {
        let other_address = @0x1;
        let iss = b"https://accounts.google.com".to_string();
        let address_seed = 3006596378422062745101035755700472756930796952630484939867684134047976874601u256;
        let mut scenario = test_scenario::begin(other_address);
        {
            verify_zklogin_issuer(address_seed, iss, scenario.ctx());
        };
        scenario.end();
    }
}
