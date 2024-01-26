// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::zklogin_verified_id_tests {
    use sui::zklogin_verified_id::{check_zklogin_id, verify_zklogin_id};
    use std::string::utf8;
    use sui::test_scenario;

    #[test]
    #[expected_failure(abort_code = sui::zklogin_verified_id::EFunctionDisabled)]
    fun test_check_zklogin_id() {
        let address = @0x1c6b623a2f2c91333df730c98d220f11484953b391a3818680f922c264cc0c6b;
        let kc_name = utf8(b"sub");
        let kc_value = utf8(b"106294049240999307923");
        let aud = utf8(b"575519204237-msop9ep45u2uo98hapqmngv8d84qdc8k.apps.googleusercontent.com");
        let iss = utf8(b"https://accounts.google.com");
        let salt_hash = 15232766888716517538274372547598053531354666056102343895255590477425668733026u256;
        check_zklogin_id(address, &kc_name, &kc_value, &iss, &aud, salt_hash);
    }

    #[test]
    #[expected_failure(abort_code = sui::zklogin_verified_id::EFunctionDisabled)]
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
        test_scenario::end(scenario_val);
    }
}
