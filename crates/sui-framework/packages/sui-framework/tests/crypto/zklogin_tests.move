// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::zklogin_tests {
    use sui::zklogin::{check_zklogin_id, check_zklogin_iss};
    use sui::address;
    use std::string::utf8;

    #[test]
    fun test_check_zklogin_id() {
        let address = address::from_bytes(x"1c6b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b");
        let kc_name = utf8(b"sub");
        let kc_value = utf8(b"106294049240999307923");
        let aud = utf8(b"575519204237-msop9ep45u2uo98hapqmngv8d84qdc8k.apps.googleusercontent.com");
        let iss = utf8(b"https://accounts.google.com");
        let salt_hash = 15232766888716517538274372547598053531354666056102343895255590477425668733026u256;
        assert!(check_zklogin_id(address, &kc_name, &kc_value, &iss, &aud, salt_hash), 0);

        let iss = utf8(b"https://other.issuer.com");
        assert!(!check_zklogin_id(address, &kc_name, &kc_value, &iss, &aud, salt_hash), 1);
    }

    #[test]
    #[expected_failure(abort_code = sui::zklogin::EInvalidInput)]
    fun test_check_zklogin_id_with_too_long_input() {
        let address = address::from_bytes(x"1c6b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b");
        let kc_name = utf8(b"subsubsubsubsubsubsubsubsubsubsubsub"); // Should at most be 32 bytes
        let kc_value = utf8(b"106294049240999307923");
        let aud = utf8(b"575519204237-msop9ep45u2uo98hapqmngv8d84qdc8k.apps.googleusercontent.com");
        let iss = utf8(b"https://accounts.google.com");
        let salt_hash = 15232766888716517538274372547598053531354666056102343895255590477425668733026u256;
        check_zklogin_id(address, &kc_name, &kc_value, &iss, &aud, salt_hash);
    }

    #[test]
    fun test_check_zklogin_iss() {
        let address = address::from_bytes(x"1c6b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b");
        let iss = utf8(b"https://accounts.google.com");
        let address_seed = 3006596378422062745101035755700472756930796952630484939867684134047976874601u256;
        assert!(check_zklogin_iss(address,  address_seed, &iss,), 0);

        let iss = utf8(b"https://other.issuer.com");
        assert!(!check_zklogin_iss(address, address_seed, &iss), 1);
    }
}