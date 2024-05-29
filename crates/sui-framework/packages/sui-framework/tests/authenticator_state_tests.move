
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
#[allow(unused_use)]
module sui::authenticator_state_tests {
    use std::string::String;

    use sui::test_scenario;
    use sui::authenticator_state::{
        Self,
        AuthenticatorState,
        create_active_jwk,
        update_authenticator_state_for_testing,
        get_active_jwks_for_testing,
        expire_jwks_for_testing,
        ActiveJwk,
    };

    #[test]
    fun authenticator_state_tests_basic() {
        let mut scenario = test_scenario::begin(@0x0);

        authenticator_state::create_for_testing(scenario.ctx());
        scenario.next_tx(@0x0);

        let mut auth_state = scenario.take_shared<AuthenticatorState>();

        let jwk1 = create_active_jwk(b"iss1".to_string(), b"key1".to_string(), b"key1_payload".to_string(), 1);
        let jwk2 = create_active_jwk(b"iss1".to_string(), b"key2".to_string(), b"key2_payload".to_string(), 1);
        let jwk3 = create_active_jwk(b"iss1".to_string(), b"key3".to_string(), b"key3_payload".to_string(), 1);


        auth_state.update_authenticator_state_for_testing(vector[jwk1, jwk3], scenario.ctx());

        let recorded_jwks = auth_state.get_active_jwks_for_testing(scenario.ctx());
        assert!(&recorded_jwks[0] == &jwk1);
        assert!(&recorded_jwks[1] == &jwk3);

        auth_state.update_authenticator_state_for_testing(vector[jwk2], scenario.ctx());
        let recorded_jwks = auth_state.get_active_jwks_for_testing(scenario.ctx());
        assert!(&recorded_jwks[0] == &jwk1);
        assert!(&recorded_jwks[1] == &jwk2);
        assert!(&recorded_jwks[2] == &jwk3);

        auth_state.expire_jwks_for_testing(1, scenario.ctx());
        let recorded_jwks = auth_state.get_active_jwks_for_testing(scenario.ctx());
        assert!(recorded_jwks.length() == 3);

        let jwk1 = create_active_jwk(b"iss1".to_string(), b"key1".to_string(), b"key1_payload".to_string(), 2);
        auth_state.update_authenticator_state_for_testing(vector[jwk1], scenario.ctx());
        let recorded_jwks = auth_state.get_active_jwks_for_testing(scenario.ctx());
        assert!(recorded_jwks.length() == 3);
        assert!(&recorded_jwks[0] == &jwk1);

        auth_state.expire_jwks_for_testing(2, scenario.ctx());
        let recorded_jwks = auth_state.get_active_jwks_for_testing(scenario.ctx());
        assert!(recorded_jwks.length() == 1);
        assert!(&recorded_jwks[0] == &jwk1);

        test_scenario::return_shared(auth_state);
        scenario.end();
    }

    #[test]
    fun authenticator_state_tests_deduplication() {
        let mut scenario = test_scenario::begin(@0x0);

        authenticator_state::create_for_testing(scenario.ctx());
        scenario.next_tx(@0x0);

        let mut auth_state = scenario.take_shared<AuthenticatorState>();

        let jwk1 = create_active_jwk(b"https://accounts.google.com".to_string(), b"kid2".to_string(), b"k1".to_string(), 0);
        auth_state.update_authenticator_state_for_testing(vector[jwk1], scenario.ctx());

        let recorded_jwks = auth_state.get_active_jwks_for_testing(scenario.ctx());
        assert!(recorded_jwks.length() == 1);
        assert!(&recorded_jwks[0] == &jwk1);

        let jwk2 = create_active_jwk(b"https://www.facebook.com".to_string(), b"kid1".to_string(), b"k2".to_string(), 0);
        let jwk3 = create_active_jwk(b"https://accounts.google.com".to_string(), b"kid2".to_string(), b"k3".to_string(), 0);
        auth_state.update_authenticator_state_for_testing(vector[jwk2, jwk3], scenario.ctx());

        let recorded_jwks = auth_state.get_active_jwks_for_testing(scenario.ctx());
        assert!(recorded_jwks.length() == 2);
        // jwk2 sorts before 1, and 3 is dropped because its a duplicated
        assert!(&recorded_jwks[0] == &jwk2);
        assert!(&recorded_jwks[1] == &jwk1);

        let jwk4 = create_active_jwk(b"https://accounts.google.com".to_string(), b"kid4".to_string(), b"k4".to_string(), 0);
        auth_state.update_authenticator_state_for_testing(vector[jwk4], scenario.ctx());
        let recorded_jwks = auth_state.get_active_jwks_for_testing(scenario.ctx());
        assert!(recorded_jwks.length() == 3);
        assert!(&recorded_jwks[0] == &jwk2);
        assert!(&recorded_jwks[1] == &jwk1);
        assert!(&recorded_jwks[2] == &jwk4);

        test_scenario::return_shared(auth_state);
        scenario.end();
    }

    #[test]
    fun authenticator_state_tests_expiry_edge_cases() {
        let mut scenario = test_scenario::begin(@0x0);

        authenticator_state::create_for_testing(scenario.ctx());
        scenario.next_tx(@0x0);

        let mut auth_state = scenario.take_shared<AuthenticatorState>();

        // expire on an empty state
        auth_state.expire_jwks_for_testing(1, scenario.ctx());

        let jwk1 = create_active_jwk(b"iss1".to_string(), b"key1".to_string(), b"key1_payload".to_string(), 1);
        let jwk2 = create_active_jwk(b"iss2".to_string(), b"key2".to_string(), b"key2_payload".to_string(), 1);
        let jwk3 = create_active_jwk(b"iss3".to_string(), b"key3".to_string(), b"key3_payload".to_string(), 1);

        auth_state.update_authenticator_state_for_testing(
            vector[jwk1, jwk2, jwk3], scenario.ctx()
        );

        // because none of the issuers have a jwk in epoch 2, we expire nothing
        auth_state.expire_jwks_for_testing(2, scenario.ctx());
        let recorded_jwks = auth_state.get_active_jwks_for_testing(scenario.ctx());
        assert!(recorded_jwks.length() == 3);

        // now add a new jwk for iss1 in epoch 2
        let jwk4 = create_active_jwk(b"iss1".to_string(), b"key4".to_string(), b"key4_payload".to_string(), 2);
        auth_state.update_authenticator_state_for_testing(
            vector[jwk4], scenario.ctx()
        );
        let recorded_jwks = auth_state.get_active_jwks_for_testing(scenario.ctx());
        assert!(recorded_jwks.length() == 4);

        // now iss2 has one jwk in epoch 2, so we expire the one from epoch 1
        auth_state.expire_jwks_for_testing(2, scenario.ctx());
        let recorded_jwks = auth_state.get_active_jwks_for_testing(scenario.ctx());
        assert!(recorded_jwks.length() == 3);
        assert!(&recorded_jwks[0] == &jwk4);
        assert!(&recorded_jwks[1] == &jwk2);
        assert!(&recorded_jwks[2] == &jwk3);

        // now add two new keys in epoch 3
        let jwk5 = create_active_jwk(b"iss2".to_string(), b"key5".to_string(), b"key5_payload".to_string(), 3);
        let jwk6 = create_active_jwk(b"iss3".to_string(), b"key6".to_string(), b"key6_payload".to_string(), 3);
        auth_state.update_authenticator_state_for_testing(
            vector[jwk5, jwk6], scenario.ctx()
        );
        let recorded_jwks = auth_state.get_active_jwks_for_testing(scenario.ctx());
        assert!(recorded_jwks.length() == 5);
        assert!(&recorded_jwks[2] == &jwk5);
        assert!(&recorded_jwks[4] == &jwk6);

        // now iss2 and iss3 have one jwk in epoch 3, so we expire the one from epoch 1
        auth_state.expire_jwks_for_testing(3, scenario.ctx());
        let recorded_jwks = auth_state.get_active_jwks_for_testing(scenario.ctx());
        assert!(recorded_jwks.length() == 3);
        assert!(&recorded_jwks[0] == &jwk4);
        assert!(&recorded_jwks[1] == &jwk5);
        assert!(&recorded_jwks[2] == &jwk6);

        auth_state.expire_jwks_for_testing(3, scenario.ctx());
        let recorded_jwks = auth_state.get_active_jwks_for_testing(scenario.ctx());
        assert!(recorded_jwks.length() == 3);
        assert!(&recorded_jwks[0] == &jwk4);
        assert!(&recorded_jwks[1] == &jwk5);
        assert!(&recorded_jwks[2] == &jwk6);

        test_scenario::return_shared(auth_state);
        scenario.end();
    }
}
