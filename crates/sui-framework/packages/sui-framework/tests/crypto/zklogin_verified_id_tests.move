// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::zklogin_verified_id_tests {
    use sui::zklogin_verified_id::{check_zklogin_id, verify_zklogin_id, VerifiedID};
    use sui::address;
    use std::string::utf8;
    use sui::test_scenario;

    #[test]
    fun test_check_zklogin_id() {
        let address = @0x1c6b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b;
        let kc_name = utf8(b"sub");
        let kc_value = utf8(b"106294049240999307923");
        let aud = utf8(b"575519204237-msop9ep45u2uo98hapqmngv8d84qdc8k.apps.googleusercontent.com");
        let iss = utf8(b"https://accounts.google.com");
        let salt_hash = 15232766888716517538274372547598053531354666056102343895255590477425668733026u256;
        assert!(check_zklogin_id(address, &kc_name, &kc_value, &iss, &aud, salt_hash), 0);

        // Negative tests
        let other_address = address::from_bytes(x"016b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b");
        assert!(!check_zklogin_id(other_address, &kc_name, &kc_value, &iss, &aud, salt_hash), 1);

        let other_kc_name = utf8(b"name");
        assert!(!check_zklogin_id(address, &other_kc_name, &kc_value, &iss, &aud, salt_hash), 2);

        let other_kc_value = utf8(b"106294049240999307924");
        assert!(!check_zklogin_id(address, &other_kc_name, &other_kc_value, &iss, &aud, salt_hash), 3);

        let other_iss = utf8(b"https://other.issuer.com");
        assert!(!check_zklogin_id(address, &kc_name, &kc_value, &other_iss, &aud, salt_hash), 4);

        let other_aud = utf8(b"other.wallet.com");
        assert!(!check_zklogin_id(address, &kc_name, &kc_value, &iss, &other_aud, salt_hash), 5);

        let other_salt_hash = 15232766888716517538274372547598053531354666056102343895255590477425668733027u256;
        assert!(!check_zklogin_id(address, &kc_name, &kc_value, &iss, &aud, other_salt_hash), 6);
    }

    #[test]
    fun test_check_zklogin_id_upper_bounds() {
        // Set kc_name, kc_value and aud to be as long as they can be (32, 115 and 145 bytes respectively) and verify
        // that the check function doesn't abort.
        let address = @0x1c6b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b;
        let kc_name = utf8(b"qvKbuwvu6LTnYocFPwz1EiIClFUAuMC3");
        let kc_value = utf8(b"BA7SREzYLKG5opithujfrU8SFaSpKI4zezu8Vb2GBPVpZsMUpYVeXl6oEAo84ryIlbHOqrMWpI7mlTfvr7HYxiF70jdyIY4rPOOpuJIYWwN3o1olTP2");
        let aud = utf8(b"munO2fnn2XnBNq5fXRmSmhC4GPL3yX4Rv9fGoECXTsmniR8dwkPTefbmLF08zh7BnVcaxriii4dEPNwzEF2puLIHmJoeuYbQxV84J9of4NRaL3IhwImVGubgPHWfMfCuGuedCdLn6KUdJsgG1");
        let iss = utf8(b"https://issuer.com");
        let salt_hash = 1234u256;
        assert!(!check_zklogin_id(address, &kc_name, &kc_value, &iss, &aud, salt_hash), 0);
    }

    #[test]
    #[expected_failure(abort_code = sui::zklogin_verified_id::EInvalidInput)]
    fun test_check_zklogin_id_long_kc_name() {
        let address = @0x1c6b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b;
        // Should at most be 32 bytes
        let long_kc_name = utf8(b"qvKbuwvu6LTnYocFPwz1EiIClFUAuMC3G");
        let kc_value = utf8(b"106294049240999307923");
        let aud = utf8(b"575519204237-msop9ep45u2uo98hapqmngv8d84qdc8k.apps.googleusercontent.com");
        let iss = utf8(b"https://accounts.google.com");
        let salt_hash = 15232766888716517538274372547598053531354666056102343895255590477425668733026u256;
        check_zklogin_id(address, &long_kc_name, &kc_value, &iss, &aud, salt_hash);
    }

    #[test]
    #[expected_failure(abort_code = sui::zklogin_verified_id::EInvalidInput)]
    fun test_check_zklogin_id_long_aud() {
        let address = @0x1c6b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b;
        let kc_name = utf8(b"sub");
        // Should at most be 115 bytes
        let long_kc_value = utf8(b"BA7SREzYLKG5opithujfrU8SFaSpKI4zezu8Vb2GBPVpZsMUpYVeXl6oEAo84ryIlbHOqrMWpI7mlTfvr7HYxiF70jdyIY4rPOOpuJIYWwN3o1olTP2c");
        let aud = utf8(b"575519204237-msop9ep45u2uo98hapqmngv8d84qdc8k.apps.googleusercontent.com");
        let iss = utf8(b"https://accounts.google.com");
        let salt_hash = 15232766888716517538274372547598053531354666056102343895255590477425668733026u256;
        check_zklogin_id(address, &kc_name, &long_kc_value, &iss, &aud, salt_hash);
    }

    #[test]
    #[expected_failure(abort_code = sui::zklogin_verified_id::EInvalidInput)]
    fun test_check_zklogin_id_long_kc_value() {
        let address = @0x1c6b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b;
        let kc_name = utf8(b"sub");
        let kc_value = utf8(b"106294049240999307923");
        // Should be at most 145 bytes
        let long_aud = utf8(b"munO2fnn2XnBNq5fXRmSmhC4GPL3yX4Rv9fGoECXTsmniR8dwkPTefbmLF08zh7BnVcaxriii4dEPNwzEF2puLIHmJoeuYbQxV84J9of4NRaL3IhwImVGubgPHWfMfCuGuedCdLn6KUdJsgG1S");
        let iss = utf8(b"https://accounts.google.com");
        let salt_hash = 15232766888716517538274372547598053531354666056102343895255590477425668733026u256;
        check_zklogin_id(address, &kc_name, &kc_value, &iss, &long_aud, salt_hash);
    }

    #[test]
    fun test_verified_id() {
        let address = @0x1c6b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b;

        let kc_name = utf8(b"sub");
        let kc_value = utf8(b"106294049240999307923");
        let aud = utf8(b"575519204237-msop9ep45u2uo98hapqmngv8d84qdc8k.apps.googleusercontent.com");
        let iss = utf8(b"https://accounts.google.com");
        let salt_hash = 15232766888716517538274372547598053531354666056102343895255590477425668733026u256;

        let scenario_val = test_scenario::begin(address);
        let scenario = &mut scenario_val;
        {
            verify_zklogin_id(kc_name, kc_value, iss, aud, salt_hash, test_scenario::ctx(scenario));
        };
        test_scenario::next_tx(scenario, address);
        {
            assert!(test_scenario::has_most_recent_for_sender<VerifiedID>(scenario), 0);
        };
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = sui::zklogin_verified_id::EInvalidProof)]
    fun test_invalid_verified_issuer() {
        let other_address = @0x1;

        let kc_name = utf8(b"sub");
        let kc_value = utf8(b"106294049240999307923");
        let aud = utf8(b"575519204237-msop9ep45u2uo98hapqmngv8d84qdc8k.apps.googleusercontent.com");
        let iss = utf8(b"https://accounts.google.com");
        let salt_hash = 15232766888716517538274372547598053531354666056102343895255590477425668733026u256;

        let scenario_val = test_scenario::begin(other_address);
        let scenario = &mut scenario_val;
        {
            verify_zklogin_id(kc_name, kc_value, iss, aud, salt_hash, test_scenario::ctx(scenario));
        };
        test_scenario::end(scenario_val);
    }
}
